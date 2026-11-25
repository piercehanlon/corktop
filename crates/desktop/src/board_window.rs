use corktop_board::Board;
use eframe::egui::{self, Color32, Key, Pos2, Rect, Sense, Vec2, vec2};
use std::{path::Path, time::Duration};

mod file_drop;
mod fonts;
mod hud;
mod images;
mod list;
mod material;
mod onboarding;
mod opener;
mod placard;
mod settings;
mod spatial;
use settings::SettingsTab;

const CARD_SIZE: Vec2 = vec2(220.0, 112.0);

struct Draft {
    original: Option<(usize, String)>,
    position: [i32; 2],
    text: String,
    focus: bool,
}

#[derive(Clone, Copy, PartialEq)]
enum Thread {
    Cards([usize; 2]),
    Divider(usize),
    Anchor(usize),
}

struct TackDrag {
    source: usize,
    color: usize,
}

struct CardOptions {
    file: String,
    tone: Option<u8>,
    show_pin: bool,
    position: [i32; 2],
}

struct Canvas {
    #[cfg(windows)]
    window_handle: isize,
    board: Board,
    spatial: spatial::Spatial,
    enter: Option<usize>,
    edit_title: Option<(usize, String)>,
    options: Option<CardOptions>,
    confirm_empty: bool,
    request_settings: bool,
    display_only: bool,
    drop_action: file_drop::Action,
    pending_drop: file_drop::Pending,
    ghost_days: u8,
    images: images::Images,
    reveal: Option<(String, std::time::Instant, bool)>,
    pasted_text: bool,
    typed_v: bool,
    next_refresh: std::time::Instant,
    open_error_tx: std::sync::mpsc::Sender<std::io::Error>,
    open_error_rx: std::sync::mpsc::Receiver<std::io::Error>,
    draft: Option<Draft>,
    moving: Option<(usize, [i32; 2])>,
    tack_drag: Option<TackDrag>,
    pending_copy: Option<(usize, f64)>,
    copied: Option<(usize, f64)>,
    paste_at: Option<[i32; 2]>,
    menu_pin_position: [i32; 2],
    cut_menu: Option<(Thread, Pos2)>,
}

#[derive(Default)]
pub struct BoardWindow {
    pub control: Option<crate::shell::control::Control>,
    pub panel: bool,
    #[cfg(windows)]
    pub hwnd: isize,
    placard: placard::Placard,
    materials: material::Materials,
    fonts_dirty: bool,
    smart_until: Option<std::time::Instant>,
    #[cfg(target_os = "linux")]
    pub change_host: bool,
    #[cfg(target_os = "linux")]
    tray: Option<ksni::blocking::Handle<crate::shell::tray::Tray>>,
    #[cfg(windows)]
    tray: Option<crate::shell::tray::Handle>,
    showing_settings: bool,
    export: Option<settings::ExportDialog>,
    settings_tab: SettingsTab,
    show_bin: bool,
    setup: onboarding::Setup,
    tutorial: bool,
    flat_list: Option<list::FlatList>,
    picker_open: bool,
    hidden: bool,
    pub machine: crate::shell::MachineState,
    layer: crate::shell::Layer,
    folder: String,
    parents: Vec<Canvas>,
    preferences: Option<corktop_board::Preferences>,
    occupied: Option<(std::path::PathBuf, usize)>,
    canvas: Option<Canvas>,
    error: String,
}

impl BoardWindow {
    #[cfg(target_os = "linux")]
    pub fn layer_unavailable(&mut self, error: std::io::Error) {
        self.error = format!("Desktop layer unavailable: {error}. Standard window is active.");
    }

    pub fn attach(&mut self, ctx: &egui::Context) {
        fonts::install(ctx, self.materials.custom_font.clone());
        self.fonts_dirty = false;
        self.materials.reset();
        self.placard.invalidate();
        for canvas in self.parents.iter_mut().chain(self.canvas.iter_mut()) {
            canvas.images.reset_textures();
        }
        if let Some(control) = &self.control {
            control.attach(ctx);
            #[cfg(windows)]
            if !self.panel && self.tray.is_none() {
                self.tray = crate::shell::tray::start(control.sender.clone()).ok();
            }
            #[cfg(target_os = "linux")]
            if !self.panel && self.tray.is_none() {
                use ksni::blocking::TrayMethods;
                self.tray = crate::shell::tray::Tray(control.sender.clone())
                    .assume_sni_available(true)
                    .spawn()
                    .ok();
            }
        }
    }

    fn choose(&mut self) {
        if !self.picker_open
            && let Some(control) = &self.control
        {
            match crate::shell::picker::choose(control.sender.clone()) {
                Ok(()) => self.picker_open = true,
                Err(e) => self.error = e.to_string(),
            }
        }
    }

    pub(crate) fn process_controls(&mut self, ctx: &egui::Context) {
        let events: Vec<_> = self.control.iter().flat_map(|c| c.events()).collect();
        for event in events {
            self.apply_control(ctx, event);
        }
    }

    #[cfg(target_os = "linux")]
    pub fn wait_for_visibility(&mut self) -> bool {
        let ctx = egui::Context::default();
        while self.hidden {
            let Some(event) = self.control.as_ref().and_then(|c| c.wait()) else {
                return false;
            };
            if matches!(event, crate::shell::control::Action::Quit) {
                return false;
            }
            self.apply_control(&ctx, event);
        }
        true
    }

    fn set_visible(&mut self, ctx: &egui::Context, visible: bool) {
        self.hidden = !visible;
        #[cfg(windows)]
        if self.hwnd != 0 {
            use windows_sys::Win32::UI::WindowsAndMessaging::*;
            crate::shell::tray_visibility(visible);
            unsafe {
                let window = self.hwnd as _;
                ShowWindow(
                    window,
                    if !visible {
                        SW_HIDE
                    } else if IsIconic(window) != 0 {
                        SW_RESTORE
                    } else {
                        SW_SHOW
                    },
                );
                if visible {
                    SetForegroundWindow(window);
                }
            }
            return;
        }
        ctx.send_viewport_cmd(egui::ViewportCommand::Visible(visible));
        if visible {
            ctx.send_viewport_cmd(egui::ViewportCommand::Minimized(false));
            ctx.send_viewport_cmd(egui::ViewportCommand::Focus);
        }
    }

    fn control_layer(&mut self, mode: bool) {
        self.machine.desktop = mode;
        #[cfg(windows)]
        if let Err(error) = self.layer.set_windows(self.hwnd, mode) {
            self.machine.desktop = self.layer.active;
            self.error = error.to_string();
        }
    }

    fn apply_control(&mut self, ctx: &egui::Context, event: crate::shell::control::Action) {
        use crate::shell::control::{Action, Launch};
        match event {
            Action::Launch(Launch::Open { path, desktop }) => {
                if let Some(mode) = desktop {
                    self.control_layer(mode);
                }
                if let Some(path) = path {
                    self.switch_folder(&path);
                }
                self.set_visible(ctx, true);
            }
            Action::Chosen(path) => {
                self.picker_open = false;
                self.switch_folder(&path);
            }
            Action::PickerClosed => self.picker_open = false,
            Action::Error(message) => {
                self.picker_open = false;
                self.error = message;
            }
            Action::ToggleVisible => {
                #[cfg(windows)]
                let visible = unsafe {
                    use windows_sys::Win32::UI::WindowsAndMessaging::*;
                    IsWindowVisible(self.hwnd as _) != 0 && IsIconic(self.hwnd as _) == 0
                };
                #[cfg(not(windows))]
                let visible =
                    !self.hidden && !ctx.input(|i| i.viewport().minimized.unwrap_or(false));
                #[cfg(windows)]
                if !visible && self.layer.active {
                    self.control_layer(false);
                }
                self.set_visible(ctx, !visible);
            }
            Action::ToggleLayer => {
                self.control_layer(!self.machine.desktop);
                #[cfg(windows)]
                self.set_visible(ctx, true);
            }
            Action::ChooseFolder => self.choose(),
            Action::Panel => {
                if let Err(e) = std::env::current_exe()
                    .and_then(|exe| std::process::Command::new(exe).arg("--panel").spawn())
                {
                    self.error = e.to_string();
                }
            }
            Action::Settings => {
                self.showing_settings = true;
                self.set_visible(ctx, true);
            }
            Action::Quit => {
                #[cfg(windows)]
                crate::shell::window_controls::quit(self.hwnd);
                #[cfg(not(windows))]
                ctx.send_viewport_cmd(egui::ViewportCommand::Close);
            }
        }
    }

    fn switch_folder(&mut self, path: &Path) {
        if let Ok(full) = path.canonicalize() {
            if self.canvas.as_ref().is_some_and(|c| c.board.root() == full) {
                return;
            }
            if let Some(i) = self.parents.iter().position(|c| c.board.root() == full) {
                self.parents.truncate(i + 1);
                self.canvas = self.parents.pop();
                return;
            }
        }
        match Board::open(path) {
            Ok(board) => self.show_root(Ok(board)),
            Err(e) if e.kind() == std::io::ErrorKind::DirectoryNotEmpty => self.open(path),
            Err(e) => self.error = e.to_string(),
        }
    }

    fn reveal_card(&mut self, board: &Path, file: String) -> std::io::Result<()> {
        let Some(current) = &self.canvas else {
            return Ok(());
        };
        let root = current.board.root();
        let rel = board.strip_prefix(root).map_err(std::io::Error::other)?;
        let mut path = root.to_owned();
        let mut chain = Vec::new();
        for part in rel.components() {
            let std::path::Component::Normal(name) = part else {
                return Err(std::io::Error::other("invalid child folder"));
            };
            path.push(name);
            if !path.symlink_metadata()?.is_dir() {
                return Err(std::io::Error::other("folder moved"));
            }
            if path.join(".board.toml").try_exists()? {
                chain.push(Board::open(&path)?);
            }
        }
        let target = chain.last().unwrap_or(&current.board);
        if target.root() != board || !target.cards().iter().any(|c| c.file == file) {
            return Err(std::io::Error::other("card moved since search"));
        }
        for board in chain {
            self.parents.push(self.canvas.take().unwrap());
            self.show_board(Ok(board));
        }
        self.canvas.as_mut().unwrap().reveal = Some((
            file,
            std::time::Instant::now() + Duration::from_millis(1100),
            true,
        ));
        Ok(())
    }

    pub fn open(&mut self, path: &Path) {
        match Board::open(path) {
            Err(e) if e.kind() == std::io::ErrorKind::DirectoryNotEmpty => {
                match std::fs::read_dir(path) {
                    Ok(entries) => self.occupied = Some((path.into(), entries.count())),
                    Err(error) => self.error = error.to_string(),
                }
            }
            result => self.show_root(result),
        }
    }

    fn show_root(&mut self, result: std::io::Result<Board>) {
        let result = result.and_then(|board| {
            let mut preferences = corktop_board::Preferences::load(board.root())?;
            if self.setup.active {
                if self.setup.styled {
                    preferences.store("appearance", &self.setup.style)?;
                }
                if let Some(action) = self.setup.drop_action {
                    preferences.store("file_drop_action", &action)?;
                }
                if preferences.get::<bool>("tutorial_seen")?.is_none() {
                    preferences.set("tutorial_seen", true.into())?;
                }
                self.setup = Default::default();
            }
            self.tutorial = !preferences.flag("tutorial_seen", true);
            self.materials.style = preferences.get("appearance")?.unwrap_or_default();
            self.materials.files = preferences.override_files().unwrap_or_default();
            if let Err(e) = self.materials.load_assets(&preferences) {
                eprintln!("Appearance override: {e}");
            }
            self.fonts_dirty = true;
            self.preferences = Some(preferences);
            self.parents.clear();
            Ok(board)
        });
        self.show_board(result);
    }

    fn show_board(&mut self, result: std::io::Result<Board>) {
        match result {
            Ok(board) => {
                let (open_error_tx, open_error_rx) = std::sync::mpsc::channel();
                self.canvas = Some(Canvas {
                    #[cfg(windows)]
                    window_handle: 0,
                    open_error_tx,
                    open_error_rx,
                    board,
                    spatial: Default::default(),
                    enter: None,
                    edit_title: None,
                    options: None,
                    confirm_empty: false,
                    request_settings: false,
                    display_only: false,
                    drop_action: Default::default(),
                    pending_drop: Default::default(),
                    ghost_days: 0,
                    images: Default::default(),
                    reveal: None,
                    pasted_text: false,
                    typed_v: false,
                    next_refresh: std::time::Instant::now(),
                    draft: None,
                    moving: None,
                    tack_drag: None,
                    cut_menu: None,
                    pending_copy: None,
                    copied: None,
                    paste_at: None,
                    menu_pin_position: [0, 0],
                });
                self.occupied = None;
                self.flat_list = None;
                self.error.clear();
            }
            Err(e) => self.error = format!("Could not open folder: {e}"),
        }
    }
}

impl eframe::App for BoardWindow {
    fn on_exit(&mut self, _: Option<&eframe::glow::Context>) {
        if !self.panel
            && let Err(error) = self.machine.save()
        {
            eprintln!("Could not save local window state: {error}");
        }
    }
    fn update(&mut self, ctx: &egui::Context, frame: &mut eframe::Frame) {
        self.process_controls(ctx);
        #[cfg(target_os = "linux")]
        if self.machine.desktop || self.hidden {
            use raw_window_handle::{HasWindowHandle, RawWindowHandle};
            if frame
                .window_handle()
                .is_ok_and(|h| matches!(h.as_raw(), RawWindowHandle::Wayland(_)))
            {
                self.change_host = true;
                ctx.send_viewport_cmd(egui::ViewportCommand::Close);
                return;
            }
        }
        if self.machine.desktop != self.layer.active
            && let Err(e) = self.layer.set(frame, self.machine.desktop)
        {
            self.machine.desktop = self.layer.active;
            self.error = e.to_string();
        }
        #[cfg(windows)]
        if let Some(canvas) = &mut self.canvas {
            use raw_window_handle::{HasWindowHandle, RawWindowHandle};
            if let Ok(h) = frame.window_handle()
                && let RawWindowHandle::Win32(handle) = h.as_raw()
            {
                canvas.window_handle = handle.hwnd.get();
            }
        }
        self.draw(ctx);
    }
}

impl BoardWindow {
    pub(crate) fn draw(&mut self, ctx: &egui::Context) {
        #[cfg(windows)]
        crate::shell::caption::clear();
        self.machine.observe(
            ctx,
            self.parents
                .first()
                .or(self.canvas.as_ref())
                .map(|c| c.board.root()),
        );
        if self.fonts_dirty {
            fonts::install(ctx, self.materials.custom_font.clone());
            self.placard.invalidate();
            self.fonts_dirty = false;
        }
        let enabled = self.canvas.is_some()
            && self
                .preferences
                .as_ref()
                .is_some_and(|p| p.flag("true_believer_mode", false));
        let visible = !self.hidden && !ctx.input(|i| i.viewport().minimized.unwrap_or(false));
        if self.placard.show(ctx, enabled && visible) {
            return;
        }
        if self.canvas.is_some() && self.occupied.is_none() {
            self.hud(ctx);
        }
        if !self.machine.desktop && !ctx.input(|i| i.viewport().maximized.unwrap_or(false)) {
            let screen = ctx.screen_rect();
            for (anchor, direction, cursor, size) in [
                (
                    egui::Align2::CENTER_TOP,
                    egui::ResizeDirection::North,
                    egui::CursorIcon::ResizeVertical,
                    vec2((screen.width() - 16.).max(0.), 8.),
                ),
                (
                    egui::Align2::CENTER_BOTTOM,
                    egui::ResizeDirection::South,
                    egui::CursorIcon::ResizeVertical,
                    vec2((screen.width() - 16.).max(0.), 8.),
                ),
                (
                    egui::Align2::LEFT_CENTER,
                    egui::ResizeDirection::West,
                    egui::CursorIcon::ResizeHorizontal,
                    vec2(8., (screen.height() - 16.).max(0.)),
                ),
                (
                    egui::Align2::RIGHT_CENTER,
                    egui::ResizeDirection::East,
                    egui::CursorIcon::ResizeHorizontal,
                    vec2(8., (screen.height() - 16.).max(0.)),
                ),
                (
                    egui::Align2::LEFT_TOP,
                    egui::ResizeDirection::NorthWest,
                    egui::CursorIcon::ResizeNwSe,
                    vec2(8., 8.),
                ),
                (
                    egui::Align2::RIGHT_TOP,
                    egui::ResizeDirection::NorthEast,
                    egui::CursorIcon::ResizeNeSw,
                    vec2(8., 8.),
                ),
                (
                    egui::Align2::LEFT_BOTTOM,
                    egui::ResizeDirection::SouthWest,
                    egui::CursorIcon::ResizeNeSw,
                    vec2(8., 8.),
                ),
                (
                    egui::Align2::RIGHT_BOTTOM,
                    egui::ResizeDirection::SouthEast,
                    egui::CursorIcon::ResizeNwSe,
                    vec2(8., 8.),
                ),
            ] {
                egui::Area::new(egui::Id::new(("resize", anchor)))
                    .order(egui::Order::Foreground)
                    .default_size(size)
                    .anchor(anchor, Vec2::ZERO)
                    .movable(false)
                    .show(ctx, |ui| {
                        let (_, r) = ui.allocate_exact_size(size, Sense::drag());
                        if r.on_hover_cursor(cursor).is_pointer_button_down_on()
                            && ui.input(|i| i.pointer.primary_pressed())
                        {
                            ctx.send_viewport_cmd(egui::ViewportCommand::BeginResize(direction));
                        }
                    });
            }
        }
        if !self.error.is_empty() {
            egui::TopBottomPanel::bottom("board-error").show(ctx, |ui| {
                ui.horizontal_wrapped(|ui| {
                    ui.colored_label(ui.visuals().error_fg_color, &self.error);
                    if ui.small_button("Dismiss").clicked() {
                        self.error.clear();
                    }
                });
            });
        }
        if self.tutorial
            && self.canvas.is_some()
            && self.occupied.is_none()
            && onboarding::tutorial(ctx, &self.materials.style, "Close tutorial")
            && let Some(p) = &mut self.preferences
        {
            match p.set("tutorial_seen", true.into()) {
                Ok(()) => self.tutorial = false,
                Err(e) => self.error = e.to_string(),
            }
        }
        if let Some(list) = &mut self.flat_list {
            let mut open = true;
            let selected = list.show(ctx, &self.materials.style, &mut open);
            if let Some((board, file)) = selected {
                self.flat_list = None;
                if let Err(e) = self.reveal_card(&board, file) {
                    self.error = e.to_string();
                }
            } else if !open {
                self.flat_list = None;
            }
        }
        if self.showing_settings {
            self.settings(ctx);
        }
        self.export_dialog(ctx);
        let panel = egui::CentralPanel::default();
        let panel = if self.canvas.is_some() && self.occupied.is_none() {
            panel.frame(egui::Frame::NONE)
        } else {
            panel
        };
        panel.show(ctx, |ui| {
            if let Some(canvas) = &mut self.canvas
                && self.occupied.is_none()
            {
                self.materials
                    .paint(ui, ui.max_rect(), ui.min_rect().min, 1.);
                let scroll = egui::scroll_area::ScrollSource {
                    drag: false,
                    ..Default::default()
                };
                egui::ScrollArea::both()
                    .scroll_source(scroll)
                    .show_viewport(ui, |ui, viewport| {
                        if let Err(e) = canvas.show(
                            ui,
                            viewport.size(),
                            &mut self.materials,
                            self.preferences.as_mut(),
                        ) {
                            self.error = format!("Board action failed: {e}");
                            ctx.request_repaint();
                        }
                    });
                if std::mem::take(&mut canvas.request_settings) {
                    self.showing_settings = true;
                }
                if let Some(idx) = canvas.enter.take() {
                    let child = canvas
                        .board
                        .child_path(idx)
                        .and_then(|p| Board::stage_folder(&p));
                    if child.is_ok()
                        && let Some(parent) = self.canvas.take()
                    {
                        self.parents.push(parent);
                    }
                    self.show_board(child);
                }
            } else if let Some((path, count)) = self.occupied.clone() {
                ui.label(format!("This folder contains {count} entries."));
                if ui.button("Choose an empty folder").clicked() {
                    self.occupied = None;
                    self.choose();
                }
                if ui.button("Stage files in Inbox").clicked() {
                    self.show_root(Board::stage_folder(&path));
                }
                if ui.button("Auto-pin files in a grid").clicked() {
                    let result = Board::stage_folder(&path).and_then(|mut board| {
                        let files = board.inbox().to_vec();
                        board.pin_files(&files)?;
                        Ok(board)
                    });
                    self.show_root(result);
                }
            } else {
                self.materials.style = self.setup.style.clone();
                self.materials
                    .paint(ui, ui.max_rect(), ui.min_rect().min, 1.);
                self.setup_card(ctx);
            }
        });
    }
}

// Count content crossing each viewport edge and reveal the nearest target on demand.
fn offscreen_controls(ui: &mut egui::Ui, view: Rect, items: impl Iterator<Item = Rect>) -> bool {
    if view.width() < 80. || view.height() < 64. {
        return false;
    }
    let mut edges = [(0usize, Rect::NOTHING, f32::INFINITY); 4];
    for item in items {
        let distance = item.center().distance_sq(view.center());
        for (index, overflow) in [
            view.left() - item.left(),
            item.right() - view.right(),
            view.top() - item.top(),
            item.bottom() - view.bottom(),
        ]
        .into_iter()
        .enumerate()
        {
            if overflow > 2. {
                let edge = &mut edges[index];
                edge.0 += 1;
                if distance < edge.2 {
                    edge.1 = item;
                    edge.2 = distance;
                }
            }
        }
    }
    if edges.iter().all(|e| e.0 == 0) {
        return false;
    }
    let wide = view.width() >= 260.;
    let width = if wide { 102. } else { 46. };
    for ((count, target, _), (direction, center)) in edges.into_iter().zip([
        (-Vec2::X, view.left_center() + vec2(width * 0.5 + 6., 0.)),
        (Vec2::X, view.right_center() - vec2(width * 0.5 + 6., 0.)),
        (-Vec2::Y, view.center_top() + vec2(0., 16.)),
        (Vec2::Y, view.center_bottom() - vec2(0., 16.)),
    ]) {
        if count == 0 {
            continue;
        }
        let rect = Rect::from_center_size(center, vec2(width, 23.));
        let hit = ui.interact(
            rect,
            ui.id()
                .with(("offscreen", direction.x as i8, direction.y as i8)),
            Sense::click(),
        );
        let painter = ui.painter();
        painter.rect_filled(
            rect,
            6.,
            if hit.hovered() {
                Color32::from_rgb(126, 56, 44)
            } else {
                Color32::from_rgb(91, 54, 42)
            },
        );
        let arrow = rect.left_center() + vec2(12., 0.);
        let head = arrow + direction * 4.;
        let stroke = egui::Stroke::new(1.6, Color32::WHITE);
        painter.line_segment([arrow - direction * 4., head], stroke);
        for side in [-1., 1.] {
            painter.line_segment([head, arrow + direction.rot90() * 3.5 * side], stroke);
        }
        painter.text(
            rect.left_center() + vec2(22., 0.),
            egui::Align2::LEFT_CENTER,
            if wide {
                format!("{count} off-screen")
            } else {
                count.to_string()
            },
            egui::FontId::proportional(12.),
            Color32::WHITE,
        );
        if hit
            .on_hover_text("Show nearest off-screen item")
            .on_hover_cursor(egui::CursorIcon::PointingHand)
            .clicked()
        {
            ui.scroll_to_rect(target, Some(egui::Align::Center));
        }
    }
    let refit = Rect::from_min_size(view.left_bottom() + vec2(10., -30.), vec2(53., 23.));
    ui.put(refit, egui::Button::new("Refit"))
        .on_hover_text("Fit the board to this window · Ctrl+Z to undo")
        .clicked()
}

fn logical(p: Vec2) -> [i32; 2] {
    [
        p.x.round().clamp(0.0, 1_000_000.0) as i32,
        p.y.round().clamp(0.0, 1_000_000.0) as i32,
    ]
}

fn free_paste_position(board: &Board, start: [i32; 2]) -> [i32; 2] {
    let extent = board.canvas_size();
    let mut pos = start;
    for _ in 0..board.cards().len() {
        if !board.cards().iter().any(|c| c.position == pos) {
            break;
        }
        pos[1] = (pos[1] + 26) % (extent[1] - 111).max(1);
        pos[0] = (pos[0] + 8) % (extent[0] - 219).max(1);
    }
    pos
}

fn offset(p: [i32; 2]) -> Vec2 {
    vec2(p[0] as f32, p[1] as f32)
}

fn tack(rect: Rect) -> Pos2 {
    rect.center_top() + vec2(0.0, 10.0)
}

fn strand(a: Pos2, b: Pos2) -> [Pos2; 17] {
    let sag = (a.distance(b) * 0.06).min(18.0);
    std::array::from_fn(|i| {
        let t = i as f32 / 16.0;
        a.lerp(b, t) + vec2(0.0, 4.0 * sag * t * (1.0 - t))
    })
}

impl Canvas {
    fn show(
        &mut self,
        ui: &mut egui::Ui,
        viewport: Vec2,
        materials: &mut material::Materials,
        prefs: Option<&mut corktop_board::Preferences>,
    ) -> std::io::Result<()> {
        if let Ok(error) = self.open_error_rx.try_recv() {
            return Err(error);
        }
        let time = std::time::Instant::now();
        if time >= self.next_refresh {
            self.next_refresh = time + Duration::from_secs(2);
            ui.ctx().request_repaint_after(Duration::from_secs(2));
            self.board.refresh()?;
            if self.edit_title.is_none()
                && self.tack_drag.is_none()
                && self.moving.is_none()
                && self.pending_copy.is_none()
                && self.options.is_none()
                && self.cut_menu.is_none()
                && self
                    .board
                    .expire_missing(self.ghost_days, std::time::SystemTime::now())?
            {
                self.copied = None;
            }
        }
        ui.ctx()
            .request_repaint_after(self.next_refresh.saturating_duration_since(time));
        if self.draft.is_none()
            && self.edit_title.is_none()
            && ui.input_mut(|i| i.consume_key(egui::Modifiers::CTRL, Key::Z))
        {
            self.board.undo_layout()?;
            ui.ctx().request_repaint();
        }
        // Size paper for readability independently of the stored board bounds.
        let scale = (viewport.x / 764.).min(viewport.y / 499.).clamp(0.9, 1.1);
        let extent = offset(self.board.canvas_size()).max(viewport / scale);
        let (canvas, background) = ui.allocate_exact_size(extent * scale, Sense::click());
        let origin = canvas.min;
        let card_position = |p: Vec2| logical((p / scale).min(extent - CARD_SIZE));
        materials.paint(ui, ui.clip_rect(), origin, scale);
        let style = &materials.style;
        let clip = ui.clip_rect().intersect(canvas);
        let files = ui.input(|i| i.raw.dropped_files.clone());
        let hovering = ui.input(|i| !i.raw.hovered_files.is_empty());
        let (cursor, mods) = ui.input(|i| (i.pointer.hover_pos(), i.modifiers));
        #[cfg(windows)]
        let (cursor, mods) = {
            let (mut cursor, mut mods) = (cursor, mods);
            if hovering || !files.is_empty() {
                // Explorer owns the drag and winit does not forward DragOver positions or keys.
                use windows_sys::Win32::{
                    Foundation::POINT,
                    Graphics::Gdi::ScreenToClient,
                    UI::{
                        Input::KeyboardAndMouse::{GetAsyncKeyState, VK_CONTROL, VK_SHIFT},
                        WindowsAndMessaging::GetCursorPos,
                    },
                };
                let mut pt = POINT::default();
                unsafe {
                    if self.window_handle != 0
                        && GetCursorPos(&mut pt) != 0
                        && ScreenToClient(self.window_handle as _, &mut pt) != 0
                    {
                        cursor = Some(
                            egui::pos2(pt.x as f32, pt.y as f32) / ui.ctx().pixels_per_point(),
                        );
                    }
                    mods.ctrl = GetAsyncKeyState(VK_CONTROL.into()) < 0;
                    mods.shift = GetAsyncKeyState(VK_SHIFT.into()) < 0;
                }
            }
            (cursor, mods)
        };
        let action = self.drop_action.with_keys(mods);
        if hovering && action == file_drop::Action::Ask {
            let painter = ui.ctx().layer_painter(egui::LayerId::new(
                egui::Order::Foreground,
                "ask-drop-highlight".into(),
            ));
            let edge = clip.shrink(2.);
            painter.rect_filled(edge, 4., Color32::from_rgba_unmultiplied(255, 220, 154, 22));
            painter.rect_stroke(
                edge,
                4.,
                egui::Stroke::new(3., Color32::from_rgb(255, 224, 158)),
                egui::StrokeKind::Inside,
            );
        } else if hovering {
            let label = action.badge();
            let screen = ui.ctx().screen_rect();
            let width = 182.0_f32.min(screen.width());
            let pos = screen.center_top() + vec2(-width / 2., 44.);
            let painter = ui.ctx().layer_painter(egui::LayerId::new(
                egui::Order::Tooltip,
                "drop-feedback".into(),
            ));
            let badge = Rect::from_min_size(pos, vec2(width, 36.));
            painter.rect_filled(badge, 5., Color32::from_rgb(32, 30, 28));
            painter.rect_stroke(
                badge,
                5.,
                egui::Stroke::new(2., Color32::WHITE),
                egui::StrokeKind::Inside,
            );
            painter.text(
                badge.center(),
                egui::Align2::CENTER_CENTER,
                label,
                egui::FontId::proportional(19.),
                Color32::WHITE,
            );
            ui.ctx().request_repaint_after(Duration::from_millis(50));
        }
        for (n, file) in files.iter().enumerate() {
            if let Some(path) = &file.path {
                let pos =
                    card_position(cursor.unwrap_or(origin) - origin + vec2(0.0, n as f32 * 26.0));
                self.pending_drop.add(path.clone(), pos);
            }
        }
        if let Some(chosen) = self.pending_drop.apply(ui, &mut self.board, action)?
            && let Some(prefs) = prefs
        {
            prefs.store("file_drop_action", &chosen)?;
            self.drop_action = chosen;
        }
        let now = ui.input(|i| i.time);
        let delay = ui.ctx().options(|o| o.input_options.max_double_click_delay);
        let mut dropped = None;
        enum CardAction {
            Tone(Option<u8>),
            Inline,
            Recycle,
            Location,
        }
        let mut card_action = None;
        let mut pin_edit = None;
        let mut reveal_outline = None;
        let mut rects = Vec::with_capacity(self.board.cards().len());
        for (idx, card) in self.board.cards().iter().enumerate() {
            let position = self
                .moving
                .filter(|(i, _)| *i == idx)
                .map_or(card.position, |(_, pos)| pos);
            let rect = Rect::from_min_size(origin + offset(position) * scale, CARD_SIZE * scale);
            let response = ui.interact(rect, ui.id().with(("card", idx)), Sense::click_and_drag());
            let mut edit = false;
            if self.draft.is_none() {
                response.context_menu(|ui| {
                    ui.horizontal(|ui| {
                        ui.label("Paper");
                        if ui.small_button("Default").clicked() {
                            card_action = Some((idx, CardAction::Tone(None)));
                            ui.close();
                        }
                        for tone in 0..4 {
                            if ui
                                .add(egui::Button::new("  ").fill(style.card_paper(
                                    &card.file,
                                    card.folder,
                                    Some(tone),
                                )))
                                .on_hover_text(format!("Paper color {}", tone + 1))
                                .clicked()
                            {
                                card_action = Some((idx, CardAction::Tone(Some(tone))));
                                ui.close();
                            }
                        }
                    });
                    if !card.missing
                        && !card.folder
                        && Path::new(&card.file).extension().is_some_and(|e| {
                            e.eq_ignore_ascii_case("md") || e.eq_ignore_ascii_case("txt")
                        })
                        && ui.button("Edit inline").clicked()
                    {
                        card_action = Some((idx, CardAction::Inline));
                        ui.close();
                    }
                    if !card.show_pin && ui.button("Show tack").clicked() {
                        pin_edit = Some((idx, false));
                        ui.close();
                    }
                    if ui.button("Rename / options…").clicked() {
                        edit = true;
                        ui.close();
                    }
                    if ui.button("Open file location").clicked() {
                        card_action = Some((idx, CardAction::Location));
                        ui.close();
                    }
                    if !card.folder
                        && ui
                            .button(if card.missing {
                                "Dismiss missing card"
                            } else {
                                "Move to recycle bin"
                            })
                            .clicked()
                    {
                        card_action = Some((idx, CardAction::Recycle));
                        ui.close();
                    }
                });
            }
            if self.options.is_none()
                && !edit
                && self.draft.is_none()
                && (response.dragged() || response.drag_stopped())
            {
                self.pending_copy = None;
                let delta = ui.input(|i| {
                    i.pointer
                        .interact_pos()
                        .zip(i.pointer.press_origin())
                        .map(|(p, start)| p - start)
                });
                if let Some(delta) = delta {
                    self.moving = Some((idx, card_position(offset(card.position) * scale + delta)));
                }
                if response.drag_stopped() {
                    dropped = self.moving.take();
                }
                ui.ctx().request_repaint();
            }
            if self.options.is_none() && !edit && self.draft.is_none() && response.clicked() {
                if response.double_clicked() && self.pending_copy.is_some_and(|(id, _)| id == idx) {
                    self.pending_copy = None;
                    if card.folder {
                        self.enter = Some(idx);
                    } else {
                        opener::launch(
                            self.board.open_target(idx)?,
                            self.open_error_tx.clone(),
                            ui.ctx().clone(),
                        )?;
                    }
                } else {
                    self.pending_copy = Some((idx, now + delay));
                }
            }
            if let Some((file, until, scroll)) = &mut self.reveal
                && *file == card.file
                && let Some(left) = until.checked_duration_since(std::time::Instant::now())
            {
                if std::mem::take(scroll) {
                    ui.scroll_to_rect(rect, Some(egui::Align::Center));
                }
                let pulse = (left.as_secs_f32() * 10.).sin() * 0.5 + 0.5;
                reveal_outline = Some((rect.expand(3. + pulse * 3.), pulse));
                ui.ctx().request_repaint();
            }
            rects.push(rect);
            if !ui.is_rect_visible(rect) {
                continue;
            }
            let painter = ui.painter().with_clip_rect(rect.intersect(ui.clip_rect()));
            let paper = if card.missing {
                Color32::from_gray(165)
            } else {
                style.card_paper(&card.file, card.folder, card.tone)
            };
            let ink = if paper.intensity() < 0.45 {
                Color32::WHITE
            } else {
                Color32::from_gray(32)
            };
            if card.folder {
                let tab = [
                    rect.min + vec2(0., 8.),
                    rect.min + vec2(0., 3.),
                    rect.min + vec2(44., 3.) * scale,
                    rect.min + vec2(53., 11.) * scale,
                ];
                painter.add(egui::Shape::convex_polygon(
                    tab.to_vec(),
                    paper.gamma_multiply(0.82),
                    egui::Stroke::NONE,
                ));
                painter.rect_filled(
                    Rect::from_min_max(rect.min + vec2(0., 8.) * scale, rect.max),
                    4.,
                    paper,
                );
            } else {
                painter.rect_filled(rect, 4., paper);
            }
            if edit {
                self.pending_copy = None;
                self.moving = None;
                self.options = Some(CardOptions {
                    file: card.file.clone(),
                    tone: card.tone,
                    show_pin: card.show_pin,
                    position: card.position,
                });
            }
            let pin = card.show_pin.then(|| {
                ui.interact(
                    Rect::from_center_size(tack(rect), vec2(22.0, 22.0)),
                    ui.id().with(("tack", idx)),
                    Sense::click_and_drag(),
                )
                .on_hover_text("Drag to pull yarn · Right-click for tack options")
            });
            if let Some(response) = &pin {
                response.context_menu(|ui| {
                    if ui.button("Hide tack").clicked() {
                        pin_edit = Some((idx, false));
                        ui.close();
                    }
                    if ui.button("Cut attached strings").clicked() {
                        pin_edit = Some((idx, true));
                        ui.close();
                    }
                });
            }
            if self.draft.is_none()
                && pin.is_some_and(|p| p.is_pointer_button_down_on())
                && ui.input(|i| i.pointer.primary_pressed())
            {
                self.pending_copy = None;
                self.tack_drag = Some(TackDrag {
                    source: idx,
                    color: 0,
                });
            }
            let mut lines = card.text.lines();
            lines.next();
            let mut body: String = lines
                .take(4)
                .flat_map(|line| line.chars().chain(['\n']))
                .take(480)
                .collect();
            if body.is_empty()
                && !card.folder
                && !images::supported(&card.file)
                && card.text == card.file
                && let Some(ext) = Path::new(&card.file).extension()
            {
                body = format!("{} file", ext.to_string_lossy().to_uppercase());
            }
            let title = if card.missing {
                format!("Missing: {}", card.display_title())
            } else {
                card.display_title().to_owned()
            };
            let mut job =
                egui::text::LayoutJob::simple_singleline(title, style.title(18. * scale), ink);
            job.wrap = egui::text::TextWrapping::truncate_at_width((CARD_SIZE.x - 20.) * scale);
            painter.galley(
                rect.min + vec2(10., 21.) * scale,
                painter.layout_job(job),
                ink,
            );
            let text = painter.layout(
                body,
                style.body(14.0 * scale),
                ink,
                (CARD_SIZE.x - 20.0) * scale,
            );
            if !card.missing && images::supported(&card.file) {
                self.images.preview(&self.board, idx, ui, rect);
            }
            painter.galley(rect.min + vec2(10.0, 47.0) * scale, text, ink);
        }
        if let Some((idx, action)) = card_action {
            self.pending_copy = None;
            let card = &self.board.cards()[idx];
            match action {
                CardAction::Inline => {
                    let text = self.board.note_text(idx)?;
                    self.draft = Some(Draft {
                        original: Some((idx, text.clone())),
                        position: card.position,
                        text,
                        focus: true,
                    });
                }
                CardAction::Tone(tone) => {
                    self.board
                        .configure_card(idx, tone, card.show_pin, card.position)?
                }
                CardAction::Location => opener::launch(
                    self.board.root().into(),
                    self.open_error_tx.clone(),
                    ui.ctx().clone(),
                )?,
                CardAction::Recycle => {
                    self.board.remove_card(idx)?;
                    self.copied = None;
                    self.tack_drag = None;
                    self.cut_menu = None;
                    ui.ctx().request_repaint();
                    return Ok(());
                }
            }
        }
        if let Some((idx, cut)) = pin_edit {
            if cut {
                self.board.cut_card_strings(idx)?;
            } else {
                let c = &self.board.cards()[idx];
                self.board
                    .configure_card(idx, c.tone, !c.show_pin, c.position)?;
            }
            self.tack_drag = None;
        }
        if self.confirm_empty {
            let mut empty = false;
            egui::Window::new("Permanently empty this board’s recycle bin?")
                .collapsible(false)
                .resizable(false)
                .show(ui.ctx(), |ui| {
                    if ui.button("Cancel").clicked() {
                        self.confirm_empty = false;
                    }
                    empty = ui.button("Delete recycled files").clicked();
                });
            if empty {
                self.confirm_empty = false;
                self.board.empty_trash()?;
            }
        }
        self.card_options(ui, style, extent)?;
        if rects.len() != self.board.cards().len() {
            ui.ctx().request_repaint();
            return Ok(());
        }
        if let Some((idx, text)) = &mut self.edit_title {
            let mut done = false;
            let mut commit = false;
            egui::Window::new("Edit card title")
                .collapsible(false)
                .resizable(false)
                .show(ui.ctx(), |ui| {
                    ui.text_edit_singleline(text);
                    commit = ui.button("Save title").clicked();
                    done = ui.button("Cancel").clicked();
                });
            if commit {
                self.board.edit_title(
                    *idx,
                    text,
                    self.display_only || self.board.cards()[*idx].folder,
                )?;
            }
            if done || commit {
                self.edit_title = None;
            }
        }
        self.finish_copy(ui, &rects, now)?;
        if background.clicked()
            && let Some(p) = background.interact_pointer_pos()
        {
            self.paste_at = Some(card_position(p - origin));
        }
        if self.draft.is_none()
            && self.options.is_none()
            && self.edit_title.is_none()
            && self.cut_menu.is_none()
            && self.moving.is_none()
            && self.tack_drag.is_none()
        {
            self.paste_notes(ui, origin, scale, extent)?;
        }
        if let Some((idx, pos)) = dropped {
            let target = ui.input(|i| i.pointer.interact_pos()).and_then(|p| {
                rects
                    .iter()
                    .enumerate()
                    .rfind(|(n, r)| *n != idx && self.board.cards()[*n].folder && r.contains(p))
                    .map(|(n, _)| n)
            });
            if let Some(folder) = target
                && !self.board.cards()[idx].folder
            {
                self.board.move_into_folder(idx, folder)?;
                self.copied = None;
                self.pending_copy = None;
                self.tack_drag = None;
                self.cut_menu = None;
                ui.ctx().request_repaint();
                return Ok(());
            }
            self.board.move_card(idx, pos)?;
        }
        if let Some((outline, pulse)) = reveal_outline {
            ui.painter().rect_stroke(
                outline,
                6.,
                egui::Stroke::new(4. + pulse * 2., Color32::from_rgb(200, 12, 51)),
                egui::StrokeKind::Outside,
            );
        }
        self.show_strings(ui, &rects, origin, scale, style, extent)?;
        let moving_tack = matches!(self.spatial.placement, spatial::Placement::Move(_));
        self.spatial
            .show(&mut self.board, ui, origin, scale, &background, style)?;
        if moving_tack || matches!(self.spatial.placement, spatial::Placement::Move(_)) {
            return Ok(());
        }
        {
            let pins = self
                .board
                .geometry()
                .pins
                .iter()
                .map(|p| Rect::from_center_size(origin + offset(*p) * scale, vec2(12., 12.)));
            if offscreen_controls(ui, clip, rects.iter().copied().chain(pins)) {
                self.board.refit(logical(viewport / scale))?;
            }
        }
        if self.board.cards().len() != rects.len() {
            self.copied = None;
            self.pending_copy = None;
            self.cut_menu = None;
            self.tack_drag = None;
        }
        if background.secondary_clicked()
            && let Some(p) = background.interact_pointer_pos()
        {
            self.paste_at = Some(card_position(p - origin));
            self.menu_pin_position = logical(((p - origin) / scale).min(extent - vec2(6., 6.)));
        }
        if self.cut_menu.is_none() {
            let pos = self.paste_at.unwrap_or([0, 0]);
            let mut add_pin = false;
            let mut paste = false;
            let mut refit = false;
            background.context_menu(|ui| {
                if ui.button("New note").clicked() {
                    self.draft = Some(Draft {
                        original: None,
                        position: pos,
                        text: String::new(),
                        focus: true,
                    });
                    ui.close();
                }
                if ui.button("Place bare tack").clicked() {
                    add_pin = true;
                    ui.close();
                }
                if ui.button("Paste").clicked() {
                    paste = true;
                    ui.close();
                }
                if ui.button("Settings").clicked() {
                    self.request_settings = true;
                    ui.close();
                }
                if ui.button("Refit to Current Frame").clicked() {
                    refit = true;
                    ui.close();
                }
            });
            if add_pin {
                self.spatial.placement = spatial::Placement::Idle;
                let id = self.board.add_pin(self.menu_pin_position)?;
                spatial::trace_tack("board menu add", id, self.menu_pin_position);
            }
            if paste {
                self.images.paste_menu(
                    &mut self.board,
                    pos,
                    self.drop_action == file_drop::Action::Link,
                    ui.ctx(),
                )?;
            }
            if refit {
                self.board.refit(logical(viewport / scale))?;
            }
        }
        if self.draft.is_none()
            && background.double_clicked()
            && let Some(p) = background.interact_pointer_pos()
        {
            self.draft = Some(Draft {
                original: None,
                position: card_position(p - origin),
                text: String::new(),
                focus: true,
            });
        }
        if let Some(draft) = &mut self.draft {
            let rect =
                Rect::from_min_size(origin + offset(draft.position) * scale, CARD_SIZE * scale);
            let commit = ui.input_mut(|i| i.consume_key(egui::Modifiers::CTRL, Key::Enter));
            let outside = ui.input(|i| {
                i.pointer.any_pressed()
                    && i.pointer.interact_pos().is_some_and(|p| !rect.contains(p))
            });
            let cancel = (draft.original.is_some() || draft.text.trim().is_empty())
                && ui.input(|i| i.key_pressed(Key::Escape));
            let field = ui.put(
                rect,
                egui::TextEdit::multiline(&mut draft.text)
                    .hint_text("Title on line one\nCtrl+Enter to save")
                    .id(ui.id().with("draft")),
            );
            if draft.focus {
                draft.focus = false;
                field.request_focus();
            }
            if commit || outside || cancel {
                if !cancel {
                    if let Some((idx, original)) = &draft.original {
                        self.board.edit_note(*idx, original, &draft.text)?;
                    } else if !draft.text.trim().is_empty() {
                        self.board.create(&draft.text, draft.position)?;
                    }
                }
                self.draft = None;
                ui.ctx().request_repaint();
            }
        }
        Ok(())
    }
    fn card_options(
        &mut self,
        ui: &egui::Ui,
        style: &material::Style,
        extent: Vec2,
    ) -> std::io::Result<()> {
        let Some(options) = &mut self.options else {
            return Ok(());
        };
        let Some(idx) = self
            .board
            .cards()
            .iter()
            .position(|c| c.file == options.file)
        else {
            self.options = None;
            return Ok(());
        };
        let card = &self.board.cards()[idx];
        let max = logical((extent - CARD_SIZE).max(Vec2::ZERO));
        let mut close = false;
        let mut apply = false;
        let mut dismiss = false;
        egui::Window::new("Card options")
            .default_pos(ui.ctx().screen_rect().center() - vec2(100., 80.))
            .collapsible(false)
            .resizable(false)
            .show(ui.ctx(), |ui| {
                ui.label(&card.file);
                ui.horizontal(|ui| {
                    ui.selectable_value(&mut options.tone, None, "Default");
                    for tone in 0..4 {
                        let color = style.card_paper(&card.file, card.folder, Some(tone));
                        if ui
                            .add(
                                egui::Button::new("   ")
                                    .fill(color)
                                    .selected(options.tone == Some(tone)),
                            )
                            .on_hover_text(format!("Paper color {}", tone + 1))
                            .clicked()
                        {
                            options.tone = Some(tone);
                        }
                    }
                });
                ui.checkbox(&mut options.show_pin, "Show tack");
                ui.horizontal(|ui| {
                    for (axis, label) in ["X ", "Y "].into_iter().enumerate() {
                        ui.add(
                            egui::DragValue::new(&mut options.position[axis])
                                .prefix(label)
                                .range(0..=max[axis]),
                        );
                    }
                });
                if !card.folder
                    && ui
                        .button(if card.missing {
                            "Dismiss missing card"
                        } else {
                            "Move to recycle bin"
                        })
                        .clicked()
                {
                    dismiss = true;
                }
                if ui.button("Edit card title").clicked() {
                    self.edit_title = Some((
                        idx,
                        card.title.clone().unwrap_or_else(|| {
                            card.text.lines().next().unwrap_or(&card.file).into()
                        }),
                    ));
                    close = true;
                }
                if ui.button("Open file location").clicked() {
                    if let Err(err) = opener::launch(
                        self.board.root().to_owned(),
                        self.open_error_tx.clone(),
                        ui.ctx().clone(),
                    ) {
                        let _ = self.open_error_tx.send(err);
                    }
                    close = true;
                }
                ui.horizontal(|ui| {
                    apply = ui.button("Apply").clicked();
                    close |= ui.button("Cancel").clicked();
                });
            });
        if apply {
            self.board
                .configure_card(idx, options.tone, options.show_pin, options.position)?;
        }
        if dismiss {
            self.board.remove_card(idx)?;
            self.pending_copy = None;
            self.copied = None;
        }
        if close || apply || dismiss {
            self.options = None;
        }
        Ok(())
    }

    fn finish_copy(&mut self, ui: &egui::Ui, cards: &[Rect], now: f64) -> std::io::Result<()> {
        if let Some((_, deadline)) = self.pending_copy
            && deadline > now
        {
            ui.ctx()
                .request_repaint_after(Duration::from_secs_f64(deadline - now));
        }
        if let Some((idx, deadline)) = self.pending_copy
            && now >= deadline
            && !ui.input(|i| i.pointer.primary_down())
        {
            self.pending_copy = None;
            #[cfg(windows)]
            {
                let card = &self.board.cards()[idx];
                let payload = if images::supported(&card.file) {
                    images::CopyPayload::Image(self.board.note_path(idx)?)
                } else {
                    images::CopyPayload::Text(self.board.note_text(idx)?)
                };
                self.images
                    .enqueue_copy(card.file.clone(), payload, ui.ctx())?;
            }
            #[cfg(not(windows))]
            {
                if images::supported(&self.board.cards()[idx].file) {
                    self.images.copy(&self.board.note_path(idx)?)?;
                } else {
                    ui.ctx().copy_text(self.board.note_text(idx)?);
                }
                self.copied = Some((idx, now + 0.45));
            }
        }
        #[cfg(windows)]
        while let Some(result) = self.images.copy_result() {
            let file = result?;
            self.copied = self
                .board
                .cards()
                .iter()
                .position(|c| c.file == file)
                .map(|idx| (idx, now + 0.45));
        }
        if let Some((idx, until)) = self.copied
            && idx < cards.len()
            && now < until
        {
            let alpha = ((until - now) / 0.45 * 175.).clamp(0., 175.) as u8;
            ui.ctx().request_repaint_after(Duration::from_millis(24));
            ui.painter().rect_stroke(
                cards[idx].shrink(2.),
                4.,
                egui::Stroke::new(2., Color32::from_black_alpha(alpha)),
                egui::StrokeKind::Inside,
            );
        }
        Ok(())
    }

    fn paste_notes(
        &mut self,
        ui: &egui::Ui,
        origin: Pos2,
        scale: f32,
        extent: Vec2,
    ) -> std::io::Result<()> {
        let pastes = ui.input(|i| {
            i.events
                .iter()
                .filter_map(|e| match e {
                    egui::Event::Paste(text) if !text.trim().is_empty() => Some(text.clone()),
                    _ => None,
                })
                .collect::<Vec<_>>()
        });
        if !pastes.is_empty() {
            self.pasted_text = true;
        }
        // egui-winit consumes the press for Ctrl+V even when the clipboard contains
        // only an image. Ctrl can already be up by the time V is released.
        let release = ui.input(|i| {
            let mut paste_released = false;
            for event in &i.events {
                if let egui::Event::Key {
                    key: Key::V,
                    pressed,
                    ..
                } = event
                {
                    if *pressed {
                        self.typed_v = true;
                    } else {
                        paste_released = !std::mem::take(&mut self.typed_v);
                    }
                }
            }
            paste_released
        });
        let binary_paste = release && !std::mem::take(&mut self.pasted_text);
        if pastes.is_empty() && !binary_paste {
            return Ok(());
        }
        let start = self.paste_at.unwrap_or_else(|| {
            logical(
                ((ui.clip_rect().min - origin + vec2(12.0, 12.0)) / scale).min(extent - CARD_SIZE),
            )
        });
        // File managers can offer both text and a native file list for one copy.
        // Some compositors expose text events without a data-control clipboard.
        let files = match self.images.clipboard_files() {
            Ok(files) => files,
            Err(_) if !pastes.is_empty() => Vec::new(),
            Err(e) => return Err(e),
        };
        if !files.is_empty() {
            for path in files {
                self.board.import_file(
                    &path,
                    free_paste_position(&self.board, start),
                    self.drop_action == file_drop::Action::Link,
                    false,
                )?;
            }
            ui.ctx().request_repaint();
            return Ok(());
        }
        if binary_paste {
            let pos = free_paste_position(&self.board, start);
            self.images.paste(&mut self.board, pos)?;
        }
        for text in pastes {
            let pos = free_paste_position(&self.board, start);
            self.board.paste_text(&text, pos)?;
            ui.ctx().request_repaint();
        }
        Ok(())
    }

    fn show_strings(
        &mut self,
        ui: &mut egui::Ui,
        cards: &[Rect],
        origin: Pos2,
        scale: f32,
        style: &material::Style,
        extent: Vec2,
    ) -> std::io::Result<()> {
        let pointer = ui.input(|i| i.pointer.interact_pos());

        let right_click = self.spatial.placement == spatial::Placement::Idle
            && ui.input(|i| i.pointer.button_clicked(egui::PointerButton::Secondary))
            && ui.ui_contains_pointer()
            && self.draft.is_none();
        let mut closest = None;
        let mut distance = 12.0;
        let mut threads = Vec::new();
        for (i, &pair @ [a, b]) in self.board.strings().iter().enumerate() {
            threads.push((
                Thread::Cards(pair),
                strand(tack(cards[a]), tack(cards[b])),
                self.board.thread_color(i),
            ));
        }
        let g = self.board.geometry();
        let pin = |i: usize| {
            let pos = origin + offset(g.pins[i]) * scale;
            if self.spatial.placement == spatial::Placement::Move(i) {
                pointer.unwrap_or(pos)
            } else {
                pos
            }
        };
        let straight = |a: Pos2, b: Pos2| std::array::from_fn(|j| a.lerp(b, j as f32 / 16.0));
        for (i, d) in g.dividers.iter().enumerate() {
            threads.push((
                Thread::Divider(i),
                straight(pin(d.pins[0]), pin(d.pins[1])),
                usize::from(d.color),
            ));
        }
        for (i, a) in g.anchors.iter().enumerate() {
            if let Some(idx) = self.board.cards().iter().position(|c| c.file == a.file) {
                threads.push((
                    Thread::Anchor(i),
                    straight(tack(cards[idx]), pin(a.pin)),
                    usize::from(a.color),
                ));
            }
        }
        let on_handle = pointer.is_some_and(|p| {
            let on_pin = cards.iter().zip(self.board.cards()).any(|(r, c)| {
                c.show_pin && Rect::from_center_size(tack(*r), vec2(22., 22.)).contains(p)
            }) || g.pins.iter().any(|&q| {
                Rect::from_center_size(origin + offset(q) * scale, vec2(22., 22.)).contains(p)
            });
            on_pin
                || g.zones.iter().enumerate().any(|(n, z)| {
                    let center = origin + offset(g.center(n)) * scale;
                    let label = ui.painter().layout_no_wrap(
                        z.title.clone(),
                        egui::FontId::proportional(17.),
                        Color32::WHITE,
                    );
                    Rect::from_center_size(center, label.size() + vec2(20., 10.)).contains(p)
                })
        });
        for (target, path, _) in &threads {
            if !on_handle
                && ui.ui_contains_pointer()
                && self.draft.is_none()
                && let Some(p) = pointer
            {
                for segment in path.windows(2) {
                    let v = segment[1] - segment[0];
                    let t =
                        ((p - segment[0]).dot(v) / v.length_sq().max(f32::EPSILON)).clamp(0.0, 1.0);
                    let d = (segment[0] + t * v).distance(p);
                    if d <= distance {
                        distance = d;
                        closest = Some((*target, p));
                    }
                }
            }
        }
        let selected = self.cut_menu.or(closest).map(|(pair, _)| pair);
        for (target, path, color) in threads {
            let ink = if selected == Some(target) {
                egui::Stroke::new(4.5, Color32::from_rgb(248, 99, 64))
            } else {
                egui::Stroke::new(2.5, style.thread(color))
            };
            paint_yarn(ui, path, ink);
        }
        if let Some(drag) = &mut self.tack_drag {
            let source = drag.source;
            let start = tack(cards[source]);
            let released = ui.input(|i| i.pointer.primary_released());
            if let Some(p) = pointer {
                let wheel = ui.input(|i| i.raw_scroll_delta.y);
                if wheel != 0.0 {
                    drag.color = (drag.color as i32 + wheel.signum() as i32).rem_euclid(6) as usize;
                }
                paint_yarn(
                    ui,
                    strand(start, p),
                    egui::Stroke::new(2.5, style.thread(drag.color)),
                );
                if released
                    && let Some(target) = cards.iter().rposition(|rect| rect.contains(p))
                    && source != target
                {
                    let color = drag.color as u8;
                    self.tack_drag = None;
                    self.board.connect_colored(source, target, color)?;
                } else if released && !cards[source].contains(p) {
                    let pos = logical(((p - origin) / scale).min(extent - vec2(6.0, 6.0)));
                    let pin = self
                        .board
                        .geometry()
                        .pins
                        .iter()
                        .position(|pt| (origin + offset(*pt) * scale).distance(p) <= 12.0);
                    let color = drag.color as u8;
                    self.tack_drag = None;
                    self.board.anchor_card(source, pin, pos, color)?;
                }
            }
            if !ui.input(|i| i.pointer.primary_down()) {
                self.tack_drag = None;
            }
            if released {
                ui.ctx().request_repaint();
            }
        }
        for (rect, card) in cards.iter().zip(self.board.cards()) {
            if card.show_pin && ui.is_rect_visible(*rect) {
                style.pin(ui.painter(), tack(*rect));
            }
        }
        let id = ui.id().with("cut-string");
        if right_click {
            self.cut_menu = closest;
            if closest.is_some() {
                egui::Popup::open_id(ui.ctx(), id);
            }
        }
        if let Some((target, pos)) = self.cut_menu {
            let menu = egui::Popup::new(id, ui.ctx().clone(), pos, ui.layer_id())
                .open_memory(None)
                .show(|ui| {
                    if ui.button("Cut String").clicked() {
                        ui.close();
                        return Some(None);
                    }
                    let mut action = None;
                    ui.menu_button("Change color", |ui| {
                        for n in 0..6 {
                            if ui
                                .add(
                                    egui::Button::new(format!("Color {}", n + 1))
                                        .fill(style.thread(n)),
                                )
                                .clicked()
                            {
                                action = Some(Some(n as u8));
                                ui.close();
                            }
                        }
                    });
                    action
                });
            if menu.is_none() {
                self.cut_menu = None;
                ui.ctx().request_repaint();
            }
            if let Some(action) = menu.and_then(|r| r.inner) {
                if let Some(color) = action {
                    match target {
                        Thread::Cards(pair) => self.board.recolor_string(pair, color)?,
                        Thread::Divider(i) => self.board.recolor_pin_string(false, i, color)?,
                        Thread::Anchor(i) => self.board.recolor_pin_string(true, i, color)?,
                    }
                    self.cut_menu = None;
                    egui::Popup::close_id(ui.ctx(), id);
                    return Ok(());
                }
                self.cut_menu = None;
                match target {
                    Thread::Cards([a, b]) => self.board.cut(a, b)?,
                    Thread::Divider(i) => self.board.cut_divider(i)?,
                    Thread::Anchor(i) => self.board.cut_anchor(i)?,
                };
                ui.ctx().request_repaint();
            }
        }
        Ok(())
    }
}

fn paint_yarn(ui: &egui::Ui, path: [Pos2; 17], stroke: egui::Stroke) {
    let shadow = path.map(|p| p + vec2(0.0, 3.0));
    ui.painter().add(egui::Shape::line(
        shadow.to_vec(),
        egui::Stroke::new(3.0, Color32::from_black_alpha(75)),
    ));
    ui.painter().add(egui::Shape::line(path.to_vec(), stroke));
}
