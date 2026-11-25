use super::*;
#[derive(Default, Clone, Copy, PartialEq)]
pub(super) enum SettingsTab {
    #[default]
    Board,
    Appearance,
    Behavior,
    Help,
}

impl BoardWindow {
    pub(super) fn settings(&mut self, ctx: &egui::Context) {
        let mut show = true;
        egui::Window::new("Settings")
            .default_width(360.)
            .resizable(false)
            .collapsible(false)
            .open(&mut show)
            .show(ctx, |ui| {
                ui.horizontal_wrapped(|ui| {
                    for (tab, label) in [
                        (SettingsTab::Board, "Board"),
                        (SettingsTab::Appearance, "Appearance"),
                        (SettingsTab::Behavior, "Behavior"),
                    ] {
                        ui.selectable_value(&mut self.settings_tab, tab, label);
                    }
                    ui.separator();
                    if ui.button("Help").clicked() {
                        self.settings_tab = SettingsTab::Help;
                    }
                });
                ui.separator();
                egui::ScrollArea::vertical()
                    .max_height((ctx.screen_rect().height() - 115.).max(75.))
                    .show(ui, |ui| {
                        ui.spacing_mut().item_spacing.y = 8.;
                        match self.settings_tab {
                            SettingsTab::Board => self.settings_board(ui),
                            SettingsTab::Appearance => self.settings_appearance(ui),
                            SettingsTab::Behavior => self.settings_behavior(ui),
                            SettingsTab::Help => self.settings_help(ui),
                        }
                    });
            });
        self.showing_settings &= show;
    }
    fn settings_board(&mut self, ui: &mut egui::Ui) {
        ui.strong("Board folder");
        if let Some(canvas) = &self.canvas {
            ui.label(crate::shell::display_path(canvas.board.root()));
        }
        ui.horizontal_wrapped(|ui| {
            if ui.button("Switch folder…").clicked() {
                self.choose();
            }
            if ui.button("New panel").clicked()
                && let Some(c) = &self.control
            {
                c.sender.send(crate::shell::control::Action::Panel);
            }
        });
        ui.separator();
        ui.strong("Files");
        if let Some(prefs) = &mut self.preferences {
            let saved = prefs
                .get::<u8>("ghost_retention_days")
                .ok()
                .flatten()
                .unwrap_or(0);
            let mut days = saved;
            ui.horizontal_wrapped(|ui| {
                ui.label("Dismiss missing cards after");
                for (v, label) in [(0, "Never"), (1, "24 hours"), (7, "7 days")] {
                    ui.selectable_value(&mut days, v, label);
                }
            });
            if days != saved
                && let Err(e) = prefs.store("ghost_retention_days", &days)
            {
                self.error = e.to_string();
            }
            let mut action = file_drop::Action::saved(prefs);
            if action.setting(ui)
                && let Err(e) = prefs.store("file_drop_action", &action)
            {
                self.error = e.to_string();
            }
            let mut enabled = prefs.flag("display_only_titles", false);
            if ui
                .checkbox(&mut enabled, "Keep filenames when renaming cards")
                .changed()
                && let Err(e) = prefs.set("display_only_titles", enabled.into())
            {
                self.error = e.to_string();
            }
        }
    }
    fn settings_appearance(&mut self, ui: &mut egui::Ui) {
        if let Some(prefs) = &mut self.preferences {
            let mut style = self.materials.style.clone();
            style.controls(ui, &self.materials.files);
            if ui
                .button("Reload overrides")
                .on_hover_text(".corktop/overrides/")
                .clicked()
            {
                match prefs.override_files().and_then(|files| {
                    self.materials.files = files;
                    self.materials.load_assets(prefs)
                }) {
                    Ok(()) => self.fonts_dirty = true,
                    Err(e) => self.error = e.to_string(),
                }
            }
            if style != self.materials.style {
                match prefs.store("appearance", &style) {
                    Ok(()) => {
                        let changed = self.materials.style.font != style.font
                            || self.materials.style.texture != style.texture;
                        self.materials.style = style;
                        if changed {
                            if let Err(e) = self.materials.load_assets(prefs) {
                                self.error = e.to_string();
                            }
                            self.fonts_dirty = true;
                        }
                    }
                    Err(e) => self.error = e.to_string(),
                }
            }
            ui.separator();
            let mut enabled = prefs.flag("auto_hide_controls", false);
            if ui.checkbox(&mut enabled, "Auto-hide top bar").changed()
                && let Err(e) = prefs.set("auto_hide_controls", enabled.into())
            {
                self.error = e.to_string();
            }
        }
    }
    fn settings_behavior(&mut self, ui: &mut egui::Ui) {
        ui.strong("Desktop");
        ui.checkbox(&mut self.machine.desktop, "Desktop layer");
        let mut start = self.machine.start_with_os;
        if ui.checkbox(&mut start, "Start with OS").changed() {
            match std::env::current_exe().and_then(|exe| crate::shell::login::set(start, &exe)) {
                Ok(()) => {
                    self.machine.start_with_os = start;
                    if let Err(e) = self.machine.save() {
                        self.error = e.to_string();
                    }
                }
                Err(e) => self.error = e.to_string(),
            }
        }
        ui.separator();
        ui.strong("Placards");
        if let Some(prefs) = &mut self.preferences {
            let mut enabled = prefs.flag("true_believer_mode", false);
            if ui
                .checkbox(&mut enabled, "True Believer Mode")
                .on_hover_text("Maintains signal alignment.")
                .changed()
            {
                if let Err(e) = prefs.set("true_believer_mode", enabled.into()) {
                    self.error = e.to_string();
                } else if !enabled {
                    self.smart_until = Some(std::time::Instant::now() + Duration::from_secs(2));
                }
            }
            if let Some(left) = self
                .smart_until
                .and_then(|end| end.checked_duration_since(std::time::Instant::now()))
            {
                ui.small("Smart.");
                ui.ctx().request_repaint_after(left);
            }
        }
    }
    fn settings_help(&mut self, ui: &mut egui::Ui) {
        ui.strong("Mouse and keyboard");
        for instruction in [
            "Click a card to copy. Double-click to open its file.",
            "Right-click a card for editing or recycling. Drag its body to move.",
            "Drag empty banner space to move the window; drag any edge or corner to resize.",
            "Double-click empty board space to write. Ctrl+Enter saves.",
            "Drag a pinhead or tack to connect. Shift-drag a bare tack to move it. Scroll to change yarn color.",
            "Connect three bare tacks in a loop to name a zone.",
            "Right-click a strand to cut it. Ctrl+Z undoes a board edit.",
            "Hold Ctrl to alternate copy and link on drop. Shift moves the file.",
        ] {
            ui.label(instruction);
        }
        ui.separator();
        if ui.button("Replay tutorial").clicked()
            && let Some(prefs) = &mut self.preferences
        {
            match prefs.set("tutorial_seen", false.into()) {
                Ok(()) => {
                    self.showing_settings = false;
                    self.show_bin = false;
                    self.flat_list = None;
                    if let Some(canvas) = &mut self.canvas {
                        canvas.options = None;
                        canvas.edit_title = None;
                        canvas.confirm_empty = false;
                        canvas.cut_menu = None;
                        canvas.spatial = Default::default();
                        canvas.pending_drop = Default::default();
                    }
                    egui::Popup::close_all(ui.ctx());
                    self.tutorial = true;
                    ui.ctx().request_repaint();
                }
                Err(e) => self.error = e.to_string(),
            }
        }
    }
}
