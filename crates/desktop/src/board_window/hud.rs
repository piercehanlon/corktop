use super::*;

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
        egui::TopBottomPanel::top("board-banner")
            .frame(
                egui::Frame::NONE
                    .fill(Color32::from_rgb(233, 221, 195))
                    .inner_margin(egui::Margin::symmetric(14, 4)),
            )
            .show(ctx, |ui| {
                ui.set_style({
                    let mut style = (**ui.style()).clone();
                    style.visuals = egui::Visuals::light();
                    style
                });
                ui.visuals_mut().override_text_color = Some(Color32::from_rgb(48, 42, 32));
                ui.visuals_mut().widgets.hovered.weak_bg_fill = Color32::from_rgb(204, 188, 158);
                ui.horizontal_wrapped(|ui| {
                    let drag = ui.interact(
                        ui.max_rect(),
                        ui.id().with("banner-drag"),
                        Sense::click_and_drag(),
                    );
                    ui.label(egui::RichText::new(folder(&canvas.board)).strong());
                    if !self.machine.desktop {
                        if drag.drag_started() {
                            ctx.send_viewport_cmd(egui::ViewportCommand::StartDrag);
                        }
                        if drag.double_clicked() {
                            ctx.send_viewport_cmd(egui::ViewportCommand::Maximized(
                                !ctx.input(|i| i.viewport().maximized.unwrap_or(false)),
                            ));
                        }
                    }
                    if reveal {
                        ui.separator();
                        let compact = rect.width() < 560.;
                        let mut tools = |ui: &mut egui::Ui| {
                            if ui
                                .selectable_label(canvas.spatial.placing, "Place tack")
                                .clicked()
                            {
                                canvas.spatial.placing = !canvas.spatial.placing;
                                if compact {
                                    ui.close();
                                }
                            }
                            if ui.selectable_label(false, "Search").clicked() {
                                self.flat_list = Some(list::FlatList::open(&canvas.board));
                                if compact {
                                    ui.close();
                                }
                            }
                            if count > 0 {
                                let inbox = ui.selectable_label(false, format!("Inbox +{count}"));
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
                            }
                            if ui.selectable_label(false, "Recycle bin").clicked() {
                                self.show_bin = true;
                                if compact {
                                    ui.close();
                                }
                            }
                            if ui.selectable_label(false, "Settings").clicked() {
                                self.showing_settings = true;
                                if compact {
                                    ui.close();
                                }
                            }
                        };
                        if compact {
                            ui.menu_button("Tools", tools);
                        } else {
                            tools(ui);
                        }
                        ui.menu_button("Board", |ui| {
                            for (i, ancestor) in self.parents.iter().enumerate() {
                                if ui
                                    .button(format!("Return to {}", folder(&ancestor.board)))
                                    .clicked()
                                {
                                    parent = Some(i);
                                    ui.close();
                                }
                            }
                            if !self.machine.desktop && ui.button("Maximize / restore").clicked() {
                                ctx.send_viewport_cmd(egui::ViewportCommand::Maximized(
                                    !ctx.input(|i| i.viewport().maximized.unwrap_or(false)),
                                ));
                                ui.close();
                            }
                            if ui.button("Close").clicked() {
                                ctx.send_viewport_cmd(egui::ViewportCommand::Close);
                            }
                        });
                    }
                });
            });
        if let Some(n) = parent {
            self.parents.truncate(n + 1);
            self.canvas = self.parents.pop();
            self.flat_list = None;
            return;
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
