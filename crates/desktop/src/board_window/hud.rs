use super::*;

#[derive(Clone, Copy, PartialEq, Debug)]
enum Control {
    Pin,
    List,
    Bin,
    Settings,
}
impl Control {
    fn label(self) -> &'static str {
        match self {
            Self::Pin => "Place a tack",
            Self::List => "Flat list",
            Self::Bin => "Recycle bin",
            Self::Settings => "Settings and help",
        }
    }
    fn icon(self, p: &egui::Painter, c: Pos2) {
        let stroke = egui::Stroke::new(1.7, Color32::from_gray(239));
        let line = |a: Vec2, b: Vec2| {
            p.line_segment([c + a, c + b], stroke);
        };
        match self {
            Self::Pin => {
                p.circle_stroke(c + vec2(2., -3.), 4.2, stroke);
                line(vec2(-0.5, 0.5), vec2(-6., 7.));
            }
            Self::List => {
                for y in [-5., 0., 5.] {
                    line(vec2(-3., y), vec2(7., y));
                    p.circle_filled(c + vec2(-7., y), 1.2, stroke.color);
                }
            }
            Self::Bin => {
                p.rect_stroke(
                    Rect::from_min_max(c + vec2(-5., -4.), c + vec2(5., 7.)),
                    1.,
                    stroke,
                    egui::StrokeKind::Middle,
                );
                line(vec2(-7., -5.), vec2(7., -5.));
                line(vec2(-2., -8.), vec2(2., -8.));
                line(vec2(0., -1.), vec2(0., 4.));
            }
            Self::Settings => {
                p.circle_stroke(c, 5.4, stroke);
                p.circle_stroke(c, 1.9, stroke);
                for n in 0..8 {
                    let dir = Vec2::angled(n as f32 * std::f32::consts::TAU / 8.);
                    line(dir * 5.2, dir * 8.);
                }
            }
        }
    }
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
            canvas.link_drops = p.flag("link_file_drops", false);
        }
        for (control, corner, offset) in [
            (Control::Pin, rect.left_top(), vec2(22., 22.)),
            (Control::List, rect.right_top(), vec2(-22., 22.)),
            (Control::Bin, rect.left_bottom(), vec2(22., -22.)),
            (Control::Settings, rect.right_bottom(), vec2(-22., -22.)),
        ] {
            let center = corner + offset;
            let active = pointer.is_some_and(|p| p.distance(center) < 68.)
                || (control == Control::Pin && canvas.spatial.placing);
            let id = egui::Id::new(("corner", control.label()));
            let opacity = ctx.animate_bool_with_time(id.with("fade"), active, 0.17);
            if opacity <= 0. {
                continue;
            }
            egui::Area::new(id)
                .order(egui::Order::Middle)
                .fixed_pos(center - vec2(16., 16.))
                .movable(false)
                .show(ctx, |ui| {
                    ui.set_opacity(opacity);
                    let (r, button) = ui.allocate_exact_size(vec2(32., 32.), Sense::click());
                    ui.painter().rect_filled(
                        r,
                        6.,
                        Color32::from_black_alpha(if button.hovered() { 230 } else { 195 }),
                    );
                    control.icon(ui.painter(), r.center());
                    if button.on_hover_text(control.label()).clicked() {
                        match control {
                            Control::Pin => canvas.spatial.placing = !canvas.spatial.placing,
                            Control::List => {
                                self.flat_list = Some(list::FlatList::open(&canvas.board))
                            }
                            Control::Bin => self.show_bin = true,
                            Control::Settings => self.showing_settings = true,
                        }
                    }
                });
        }
        let bar = rect.width() > 120.
            && pointer.is_some_and(|p| {
                p.y < rect.top() + 45. && p.x > rect.left() + 42. && p.x < rect.right() - 42.
            });
        let fade = ctx.animate_bool_with_time("top-edge-fade".into(), bar, 0.2);
        if fade > 0. {
            let mut parent = None;
            egui::Area::new("top-edge".into())
                .fixed_pos(rect.left_top() + vec2(45., 5.))
                .movable(false)
                .show(ctx, |ui| {
                    ui.set_opacity(fade);
                    ui.set_max_width((rect.width() - 90.).max(1.));
                    egui::Frame::popup(ui.style()).show(ui, |ui| {
                        ui.horizontal_wrapped(|ui| {
                            for (n, ancestor) in self.parents.iter().enumerate() {
                                if ui.small_button(folder(&ancestor.board)).clicked() {
                                    parent = Some(n);
                                }
                                ui.label("›");
                            }
                            let name = ui.label(folder(&canvas.board)).interact(Sense::drag());
                            if !self.machine.desktop {
                                if name.drag_started() {
                                    ctx.send_viewport_cmd(egui::ViewportCommand::StartDrag);
                                }
                                if ui.small_button("Maximize / restore").clicked() {
                                    ctx.send_viewport_cmd(egui::ViewportCommand::Maximized(
                                        !ctx.input(|i| i.viewport().maximized.unwrap_or(false)),
                                    ));
                                }
                            }
                            if ui.small_button("Close").clicked() {
                                ctx.send_viewport_cmd(egui::ViewportCommand::Close);
                            }
                        });
                    });
                });
            if let Some(n) = parent {
                self.parents.truncate(n + 1);
                self.canvas = self.parents.pop();
                self.flat_list = None;
                return;
            }
        }
        let count = canvas.board.inbox().len();
        if count > 0 {
            egui::Area::new("inbox-corner".into())
                .anchor(egui::Align2::RIGHT_TOP, vec2(-8., 44.))
                .movable(false)
                .show(ctx, |ui| {
                    egui::Frame::popup(ui.style()).show(ui, |ui| {
                        ui.horizontal(|ui| {
                            let phase = (ui.input(|i| i.time) as f32 * 2.4).sin() * 0.5 + 0.5;
                            let (r, _) = ui.allocate_exact_size(vec2(7., 8.), Sense::hover());
                            ui.painter().circle_filled(
                                r.center(),
                                3.,
                                Color32::from_rgb((146. + 90. * phase) as u8, 98, 44),
                            );
                            ui.menu_button(format!("Inbox +{count}"), |ui| {
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
