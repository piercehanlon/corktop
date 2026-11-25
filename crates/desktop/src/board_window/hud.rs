use super::*;

#[derive(Clone, Copy)]
enum RailIcon {
    Pin,
    Search,
    Trash,
    Settings,
    Maximize,
    Restore,
    Close,
}

fn rail_button(
    ui: &mut egui::Ui,
    icon: RailIcon,
    label: &str,
    width: f32,
    active: bool,
) -> egui::Response {
    let (rect, r) = ui.allocate_exact_size(vec2(width, 24.), Sense::click());
    if r.hovered() || active {
        ui.painter().rect_filled(
            rect,
            3.,
            Color32::from_white_alpha(if active { 38 } else { 22 }),
        );
    }
    let c = rect.center();
    let p = ui.painter();
    let stroke = egui::Stroke::new(
        1.5,
        Color32::from_gray(if ui.is_enabled() { 223 } else { 143 }),
    );
    let line = |a: [f32; 2], b: [f32; 2]| {
        p.line_segment([c + Vec2::from(a), c + Vec2::from(b)], stroke);
    };
    let box_at = |dx: f32, dy: f32, w: f32, h: f32| {
        p.rect_stroke(
            Rect::from_min_size(c + vec2(dx, dy), vec2(w, h)),
            1.,
            stroke,
            egui::StrokeKind::Inside,
        );
    };
    match icon {
        RailIcon::Pin => {
            line([-4., -6.], [4., -6.]);
            line([-3., -6.], [-2., -1.]);
            line([3., -6.], [2., -1.]);
            line([-2., -1.], [-4., 2.]);
            line([2., -1.], [4., 2.]);
            line([-4., 2.], [4., 2.]);
            line([0., 2.], [0., 8.]);
        }
        RailIcon::Search => {
            p.circle_stroke(c - vec2(1.5, 1.5), 5., stroke);
            line([2., 2.], [7., 7.]);
        }
        RailIcon::Trash => {
            box_at(-4., -3., 8., 10.);
            line([-6., -5.], [6., -5.]);
            line([-2., -7.], [2., -7.]);
        }
        RailIcon::Settings => {
            p.circle_stroke(c, 5., stroke);
            p.circle_stroke(c, 1.8, stroke);
            for i in 0..8 {
                let d = Vec2::angled(i as f32 * std::f32::consts::TAU / 8.);
                p.line_segment([c + d * 5., c + d * 7.5], stroke);
            }
        }
        RailIcon::Maximize => box_at(-6., -6., 12., 12.),
        RailIcon::Restore => {
            box_at(-3., -7., 10., 10.);
            box_at(-7., -3., 10., 10.);
        }
        RailIcon::Close => {
            line([-5., -5.], [5., 5.]);
            line([-5., 5.], [5., -5.]);
        }
    }
    r.on_hover_text(label)
        .on_hover_cursor(egui::CursorIcon::PointingHand)
}

impl BoardWindow {
    pub(super) fn hud(&mut self, ctx: &egui::Context) {
        let rect = ctx.screen_rect();
        let pointer = ctx.pointer_hover_pos();
        let canvas = self.canvas.as_mut().unwrap();
        if let Some(p) = &self.preferences {
            canvas.display_only = p.flag("display_only_titles", false);
            canvas.ghost_days = p
                .get::<u8>("ghost_retention_days")
                .ok()
                .flatten()
                .unwrap_or(0);
            canvas.drop_action = file_drop::Action::saved(p);
        }
        let auto_hide = self
            .preferences
            .as_ref()
            .is_some_and(|p| p.flag("auto_hide_controls", false));
        let reveal = !auto_hide
            || pointer.is_some_and(|p| p.y <= rect.top() + 40.)
            || egui::Popup::is_any_open(ctx);
        let mut parent = None;
        let count = canvas.board.inbox().len();
        let maximized = ctx.input(|i| i.viewport().maximized.unwrap_or(false));
        #[cfg(windows)]
        let maximized = if canvas.window_handle != 0 {
            unsafe {
                windows_sys::Win32::UI::WindowsAndMessaging::IsZoomed(canvas.window_handle as _)
                    != 0
            }
        } else {
            maximized
        };
        let toggle_maximize = || {
            #[cfg(windows)]
            if canvas.window_handle != 0 {
                crate::shell::caption::toggle_maximize(canvas.window_handle);
                return;
            }
            ctx.send_viewport_cmd(egui::ViewportCommand::Maximized(!maximized));
        };
        if reveal {
            egui::TopBottomPanel::top("board-banner")
                .exact_height(32.)
                .frame(egui::Frame::NONE.fill(Color32::from_rgb(57, 51, 44)))
                .show(ctx, |ui| {
                    let rect = ui.max_rect();
                    let drag = ui.interact(rect, ui.id().with("caption"), Sense::click_and_drag());
                    if !self.machine.desktop {
                        if drag.drag_started_by(egui::PointerButton::Primary) {
                            ctx.send_viewport_cmd(egui::ViewportCommand::StartDrag);
                        }
                        if drag.double_clicked() {
                            toggle_maximize();
                        }
                    }
                    let width = ((rect.width() - 48.) / 8.).clamp(15., 28.);
                    ui.scope_builder(
                        egui::UiBuilder::new()
                            .max_rect(rect.shrink2(vec2(9., 3.)))
                            .layout(egui::Layout::right_to_left(egui::Align::Center)),
                        |ui| {
                            ui.spacing_mut().item_spacing.x = 1.;
                            if rail_button(ui, RailIcon::Close, "Close window", width, false)
                                .clicked()
                            {
                                ctx.send_viewport_cmd(egui::ViewportCommand::Close);
                            }
                            if !self.machine.desktop
                                && rail_button(
                                    ui,
                                    if maximized {
                                        RailIcon::Restore
                                    } else {
                                        RailIcon::Maximize
                                    },
                                    "Maximize / restore",
                                    width,
                                    false,
                                )
                                .clicked()
                            {
                                toggle_maximize();
                            }
                            ui.separator();
                            if rail_button(ui, RailIcon::Settings, "Settings", width, false)
                                .clicked()
                            {
                                self.showing_settings = true;
                            }
                            if rail_button(ui, RailIcon::Trash, "Recycle bin", width, false)
                                .clicked()
                            {
                                self.show_bin = true;
                            }
                            if rail_button(ui, RailIcon::Search, "Search cards", width, false)
                                .clicked()
                            {
                                self.flat_list = Some(list::FlatList::open(&canvas.board));
                            }
                            if rail_button(
                                ui,
                                RailIcon::Pin,
                                "Place bare tack",
                                width,
                                canvas.spatial.placing,
                            )
                            .clicked()
                            {
                                canvas.spatial.placing = !canvas.spatial.placing;
                            }
                            let available = ui.available_rect_before_wrap();
                            #[cfg(windows)]
                            if !self.machine.desktop
                                && !self.showing_settings
                                && !self.show_bin
                                && self.flat_list.is_none()
                                && !self.tutorial
                                && !egui::Popup::is_any_open(ctx)
                            {
                                let mut caption = rect;
                                caption.max.x = available.right();
                                if !self.parents.is_empty() {
                                    caption.min.x = available.left() + 140.;
                                }
                                if !maximized {
                                    caption.min.y += 8.;
                                    caption.min.x = caption.min.x.max(rect.left() + 8.);
                                }
                                crate::shell::caption::set(
                                    canvas.window_handle,
                                    caption,
                                    ctx.pixels_per_point(),
                                );
                            }
                            if available.width() > 8. {
                                let title = ui.painter().with_clip_rect(available);
                                title.text(
                                    available.left_center(),
                                    egui::Align2::LEFT_CENTER,
                                    folder(&canvas.board),
                                    egui::FontId::proportional(14.),
                                    Color32::from_gray(232),
                                );
                                if !self.parents.is_empty() {
                                    let r = ui.interact(
                                        available.intersect(Rect::from_min_size(
                                            available.min,
                                            vec2(140., available.height()),
                                        )),
                                        ui.id().with("ancestors"),
                                        Sense::click(),
                                    );
                                    egui::Popup::menu(&r).show(|ui| {
                                        for (i, a) in self.parents.iter().enumerate() {
                                            if ui.button(folder(&a.board)).clicked() {
                                                parent = Some(i);
                                                ui.close();
                                            }
                                        }
                                    });
                                }
                            }
                        },
                    );
                });
        }
        if let Some(n) = parent {
            self.parents.truncate(n + 1);
            self.canvas = self.parents.pop();
            self.flat_list = None;
            return;
        }
        if count > 0 {
            egui::Area::new("inbox-corner".into())
                .anchor(
                    egui::Align2::RIGHT_TOP,
                    vec2(-12., if reveal { 42. } else { 12. }),
                )
                .movable(false)
                .show(ctx, |ui| {
                    egui::Frame::popup(ui.style()).show(ui, |ui| {
                        ui.horizontal(|ui| {
                            let phase = (ui.input(|i| i.time) as f32 * 2.4).sin() * 0.5 + 0.5;
                            let (r, _) = ui.allocate_exact_size(vec2(10., 12.), Sense::hover());
                            ui.painter().circle_filled(
                                r.center(),
                                4.,
                                Color32::from_rgb(
                                    (155. + 100. * phase) as u8,
                                    (46. + 79. * phase) as u8,
                                    35,
                                ),
                            );
                            let inbox = ui.button(format!("Inbox +{count}"));
                            egui::Popup::menu(&inbox).gap(5.).show(|ui| {
                                let mut pin = None;
                                egui::ScrollArea::vertical()
                                    .max_height(240.)
                                    .show(ui, |ui| {
                                        for file in canvas.board.inbox() {
                                            if ui.button(file).clicked() {
                                                pin = Some(file.clone());
                                                ui.close();
                                            }
                                        }
                                    });
                                if let Some(file) = pin
                                    && let Err(e) = canvas.board.pin_files(&[file])
                                {
                                    self.error = e.to_string();
                                }
                            });
                        });
                    });
                });
            ctx.request_repaint_after(Duration::from_millis(180));
        }
        if self.show_bin {
            egui::Window::new("Recycle bin")
                .default_pos(rect.center() - vec2(110., 70.))
                .open(&mut self.show_bin)
                .show(ctx, |ui| {
                    let trash = canvas.board.trashed_files();
                    if trash.is_empty() {
                        ui.label("The recycle bin is empty.");
                    } else {
                        if ui.button("Empty bin…").clicked() {
                            canvas.confirm_empty = true;
                        }
                        egui::ScrollArea::vertical()
                            .max_height(260.)
                            .show(ui, |ui| {
                                for name in trash {
                                    if ui.button(format!("Restore {name}")).clicked()
                                        && let Err(e) = canvas.board.restore(&name)
                                    {
                                        self.error = e.to_string();
                                    }
                                }
                            });
                    }
                });
        }
    }
}
fn folder(board: &Board) -> String {
    board
        .root()
        .file_name()
        .unwrap_or(board.root().as_os_str())
        .to_string_lossy()
        .into_owned()
}
