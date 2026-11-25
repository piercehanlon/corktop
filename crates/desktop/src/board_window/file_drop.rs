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
            Self::Ask => "Ask",
        }
    }
    pub fn setting(&mut self, ui: &mut egui::Ui) -> bool {
        let before = *self;
        ui.horizontal_wrapped(|ui| {
            ui.label("Dropped files:");
            for value in [Self::Ask, Self::Copy, Self::Link, Self::Move] {
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
pub(super) struct Pending {
    files: Vec<(PathBuf, [i32; 2])>,
    remember: bool,
}

impl Pending {
    pub fn add(&mut self, path: PathBuf, pos: [i32; 2]) {
        self.files.push((path, pos));
    }
    pub fn apply(
        &mut self,
        ui: &egui::Ui,
        board: &mut Board,
        action: Action,
    ) -> std::io::Result<Option<Action>> {
        if self.files.is_empty() {
            return Ok(None);
        }
        let action = if action == Action::Ask {
            let mut choice = None;
            let dialog = egui::Modal::new("file-drop-choice".into()).show(ui.ctx(), |ui| {
                ui.heading("Add files to this board");
                egui::ScrollArea::vertical()
                    .max_height(110.)
                    .show(ui, |ui| {
                        for (path, _) in &self.files {
                            ui.label(path.file_name().unwrap_or_default().to_string_lossy())
                                .on_hover_text(path.display().to_string());
                        }
                    });
                ui.checkbox(&mut self.remember, "Remember this choice");
                ui.horizontal_wrapped(|ui| {
                    for a in [Action::Copy, Action::Link, Action::Move] {
                        if ui.button(a.label()).clicked() {
                            choice = Some(a);
                        }
                    }
                    if ui.button("Cancel (Esc)").clicked() {
                        ui.close();
                    }
                });
            });
            if dialog.should_close() {
                self.files.clear();
                self.remember = false;
                return Ok(None);
            }
            let Some(a) = choice else { return Ok(None) };
            a
        } else {
            action
        };
        let remember = std::mem::take(&mut self.remember);
        for (path, pos) in std::mem::take(&mut self.files) {
            board.import_file(&path, pos, action == Action::Link, action == Action::Move)?;
        }
        ui.ctx().request_repaint();
        Ok(remember.then_some(action))
    }
}
