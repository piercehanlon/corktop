use super::*;
use std::path::PathBuf;

#[derive(Clone, Copy, Default, PartialEq, serde::Serialize, serde::Deserialize)]
#[serde(rename_all = "snake_case")]
pub(super) enum Action {
    #[default]
    Copy,
    Link,
    Move,
    Ask,
}

impl Action {
    pub fn saved(p: &corktop_board::Preferences) -> Self {
        p.get("file_drop_action").ok().flatten().unwrap_or_else(|| {
            if p.flag("link_file_drops", false) {
                Self::Link
            } else {
                Self::Copy
            }
        })
    }
    fn label(self) -> &'static str {
        match self {
            Self::Copy => "Copy",
            Self::Link => "Link",
            Self::Move => "Move",
            Self::Ask => "Ask each time",
        }
    }
    pub fn setting(&mut self, ui: &mut egui::Ui) -> bool {
        let before = *self;
        egui::ComboBox::from_label("When dropping files")
            .selected_text(self.label())
            .show_ui(ui, |ui| {
                for value in [Self::Copy, Self::Link, Self::Move, Self::Ask] {
                    ui.selectable_value(self, value, value.label());
                }
            });
        before != *self
    }
    pub fn with_keys(self, mods: egui::Modifiers) -> Self {
        if self == Self::Ask {
            Self::Ask
        } else if mods.shift {
            Self::Move
        } else if mods.ctrl {
            if self == Self::Copy {
                Self::Link
            } else {
                Self::Copy
            }
        } else {
            self
        }
    }
    pub fn badge(self) -> &'static str {
        match self {
            Self::Copy => "+ COPY",
            Self::Link => "LINK",
            Self::Move => "MOVE",
            Self::Ask => "CHOOSE ON DROP",
        }
    }
}

#[derive(Default)]
pub(super) struct Pending(Vec<(PathBuf, [i32; 2])>);

impl Pending {
    pub fn add(&mut self, path: PathBuf, pos: [i32; 2]) {
        self.0.push((path, pos));
    }
    pub fn apply(
        &mut self,
        ui: &egui::Ui,
        board: &mut Board,
        action: Action,
    ) -> std::io::Result<()> {
        if self.0.is_empty() {
            return Ok(());
        }
        let action = if action == Action::Ask {
            let mut choice = None;
            let dialog = egui::Modal::new("file-drop-choice".into()).show(ui.ctx(), |ui| {
                ui.heading("Add files to this board");
                if self.0.len() == 1 {
                    ui.label(self.0[0].0.file_name().unwrap_or_default().to_string_lossy());
                } else { ui.label(format!("{} files", self.0.len())); }
                ui.label("Copy keeps the originals. Link references them. Move transfers them into the board folder.");
                ui.horizontal_wrapped(|ui| {
                    for a in [Action::Copy, Action::Link, Action::Move] {
                        if ui.button(a.label()).clicked() { choice = Some(a); }
                    }
                    if ui.button("Cancel").clicked() { ui.close(); }
                });
            });
            if dialog.should_close() {
                self.0.clear();
                return Ok(());
            }
            let Some(a) = choice else { return Ok(()) };
            a
        } else {
            action
        };
        for (path, pos) in std::mem::take(&mut self.0) {
            board.import_file(&path, pos, action == Action::Link, action == Action::Move)?;
        }
        ui.ctx().request_repaint();
        Ok(())
    }
}
