use corktop_board::Board;
use eframe::egui::{self, Color32, Key, Pos2, Rect, Sense, Vec2, vec2};
use std::path::Path;

const CARD_SIZE: Vec2 = vec2(220.0, 112.0);

struct Draft {
    position: [i32; 2],
    text: String,
    focus: bool,
}

struct TackDrag {
    source: usize,
    started: f64,
    armed: bool,
}

struct Canvas {
    board: Board,
    draft: Option<Draft>,
    moving: Option<(usize, [i32; 2])>,
    tack_drag: Option<TackDrag>,
    cut_menu: Option<([usize; 2], Pos2)>,
}

#[derive(Default)]
pub struct BoardWindow {
    folder: String,
    canvas: Option<Canvas>,
    error: String,
}

impl BoardWindow {
    pub fn open(&mut self, path: &Path) {
        match Board::open(path) {
            Ok(board) => {
                self.canvas = Some(Canvas {
                    board,
                    draft: None,
                    moving: None,
                    tack_drag: None,
                    cut_menu: None,
                });
                self.error.clear();
            }
            Err(e) => self.error = format!("Could not open folder: {e}"),
        }
    }
}

impl eframe::App for BoardWindow {
    fn update(&mut self, ctx: &egui::Context, _: &mut eframe::Frame) {
        egui::TopBottomPanel::top("header").show(ctx, |ui| {
            ui.horizontal(|ui| {
                let title = ui.heading("corktop").interact(Sense::drag());
                if title.drag_started() {
                    ctx.send_viewport_cmd(egui::ViewportCommand::StartDrag);
                }

                if self.canvas.is_some() {
                    ui.separator();
                    ui.label("Double-click to write a note. Drag a card to move it.");
                }
                ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                    if ui.button("Close").clicked() {
                        ctx.send_viewport_cmd(egui::ViewportCommand::Close);
                    }
                });
            });
            if !self.error.is_empty() {
                ui.colored_label(ui.visuals().error_fg_color, &self.error);
            }
        });
        egui::CentralPanel::default().show(ctx, |ui| {
            if let Some(canvas) = &mut self.canvas {
                let scroll = egui::scroll_area::ScrollSource {
                    drag: false,
                    ..Default::default()
                };
                egui::ScrollArea::both()
                    .scroll_source(scroll)
                    .show_viewport(ui, |ui, viewport| {
                        if let Err(e) = canvas.show(ui, viewport.size()) {
                            self.error = format!("Could not save board: {e}");
                        }
                    });
            } else {
                ui.label("Board folder");
                let field = ui.add(
                    egui::TextEdit::singleline(&mut self.folder)
                        .hint_text("Path to an empty folder or an existing board")
                        .desired_width(440.0),
                );
                let enter = field.lost_focus() && ui.input(|i| i.key_pressed(Key::Enter));
                if ui.button("Open folder").clicked() || enter {
                    let folder = self.folder.clone();
                    self.open(Path::new(&folder));
                }
            }
        });
    }
}

fn logical(p: Vec2) -> [i32; 2] {
    [
        p.x.round().clamp(0.0, 1_000_000.0) as i32,
        p.y.round().clamp(0.0, 1_000_000.0) as i32,
    ]
}

fn offset(p: [i32; 2]) -> Vec2 {
    vec2(p[0] as f32, p[1] as f32)
}

fn tack(rect: Rect) -> Pos2 {
    rect.right_top() + vec2(-14.0, 14.0)
}

fn strand(a: Pos2, b: Pos2) -> [Pos2; 17] {
    let sag = (a.distance(b) * 0.06).min(18.0);
    std::array::from_fn(|i| {
        let t = i as f32 / 16.0;
        a.lerp(b, t) + vec2(0.0, 4.0 * sag * t * (1.0 - t))
    })
}

impl Canvas {
    fn show(&mut self, ui: &mut egui::Ui, viewport: Vec2) -> std::io::Result<()> {
        let mut size = viewport;
        for card in self.board.cards() {
            size = size.max(offset(card.position) + CARD_SIZE);
        }
        if let Some(draft) = &self.draft {
            size = size.max(offset(draft.position) + CARD_SIZE);
        }
        let (canvas, background) = ui.allocate_exact_size(size, Sense::click());
        let origin = canvas.min;
        let mut dropped = None;
        let mut rects = Vec::with_capacity(self.board.cards().len());
        for (idx, card) in self.board.cards().iter().enumerate() {
            let position = self
                .moving
                .filter(|(i, _)| *i == idx)
                .map_or(card.position, |(_, pos)| pos);
            let rect = Rect::from_min_size(origin + offset(position), CARD_SIZE);
            let response = ui.interact(rect, ui.id().with(("card", idx)), Sense::drag());
            if self.draft.is_none() && (response.dragged() || response.drag_stopped()) {
                let delta = ui.input(|i| {
                    i.pointer
                        .interact_pos()
                        .zip(i.pointer.press_origin())
                        .map(|(p, start)| p - start)
                });
                if let Some(delta) = delta {
                    self.moving = Some((idx, logical(offset(card.position) + delta)));
                }
                if response.drag_stopped() {
                    dropped = self.moving.take();
                }
                ui.ctx().request_repaint();
            }
            rects.push(rect);
            if !ui.is_rect_visible(rect) {
                continue;
            }
            let painter = ui.painter().with_clip_rect(rect.intersect(ui.clip_rect()));
            painter.rect_filled(rect, 4.0, Color32::from_rgb(243, 228, 178));
            let pin = ui
                .interact(
                    Rect::from_center_size(tack(rect), vec2(22.0, 22.0)),
                    ui.id().with(("tack", idx)),
                    Sense::drag(),
                )
                .on_hover_text("Hold, then drag to another card");
            if self.draft.is_none()
                && pin.is_pointer_button_down_on()
                && ui.input(|i| i.pointer.primary_pressed())
            {
                self.tack_drag = Some(TackDrag {
                    source: idx,
                    started: ui.input(|i| i.time),
                    armed: false,
                });
                ui.ctx()
                    .request_repaint_after(std::time::Duration::from_millis(200));
            }
            let mut lines = card.text.lines();
            let title = lines
                .next()
                .unwrap_or("")
                .chars()
                .take(96)
                .collect::<String>();
            let body: String = lines
                .take(4)
                .flat_map(|line| line.chars().chain(['\n']))
                .take(480)
                .collect();
            painter.text(
                rect.min + vec2(10.0, 8.0),
                egui::Align2::LEFT_TOP,
                title,
                egui::FontId::proportional(18.0),
                Color32::from_gray(32),
            );
            let text = painter.layout(
                body,
                egui::FontId::proportional(14.0),
                Color32::from_gray(45),
                CARD_SIZE.x - 20.0,
            );
            painter.galley(rect.min + vec2(10.0, 34.0), text, Color32::from_gray(45));
        }
        if let Some((idx, pos)) = dropped {
            self.board.move_card(idx, pos)?;
        }
        self.show_strings(ui, &rects)?;
        if self.draft.is_none()
            && background.double_clicked()
            && let Some(p) = background.interact_pointer_pos()
        {
            self.draft = Some(Draft {
                position: logical(p - origin),
                text: String::new(),
                focus: true,
            });
        }
        if let Some(draft) = &mut self.draft {
            let rect = Rect::from_min_size(origin + offset(draft.position), CARD_SIZE);
            let commit = ui.input_mut(|i| i.consume_key(egui::Modifiers::CTRL, Key::Enter));
            let outside = ui.input(|i| {
                i.pointer.any_pressed()
                    && i.pointer.interact_pos().is_some_and(|p| !rect.contains(p))
            });
            let cancel = draft.text.trim().is_empty() && ui.input(|i| i.key_pressed(Key::Escape));
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
                if !draft.text.trim().is_empty() {
                    self.board.create(&draft.text, draft.position)?;
                }
                self.draft = None;
                ui.ctx().request_repaint();
            }
        }
        Ok(())
    }
    fn show_strings(&mut self, ui: &mut egui::Ui, cards: &[Rect]) -> std::io::Result<()> {
        let pointer = ui.input(|i| i.pointer.interact_pos());
        let stroke = egui::Stroke::new(2.5, Color32::from_rgb(218, 83, 73));
        let right_click = ui.input(|i| i.pointer.button_clicked(egui::PointerButton::Secondary))
            && ui.ui_contains_pointer()
            && self.draft.is_none();
        let mut closest = None;
        let mut distance = 7.0;
        for &[a, b] in self.board.strings() {
            let path = strand(tack(cards[a]), tack(cards[b]));
            if right_click && let Some(p) = pointer {
                for segment in path.windows(2) {
                    let v = segment[1] - segment[0];
                    let t =
                        ((p - segment[0]).dot(v) / v.length_sq().max(f32::EPSILON)).clamp(0.0, 1.0);
                    let d = (segment[0] + t * v).distance(p);
                    if d <= distance {
                        distance = d;
                        closest = Some(([a, b], p));
                    }
                }
            }
            ui.painter().add(egui::Shape::line(path.to_vec(), stroke));
        }
        if let Some(drag) = &mut self.tack_drag {
            let elapsed = ui.input(|i| i.time) - drag.started;
            let source = drag.source;
            let start = tack(cards[source]);
            if elapsed >= 0.2 {
                drag.armed = true;
            }
            let released = ui.input(|i| i.pointer.primary_released());
            if let Some(p) = pointer
                && drag.armed
            {
                ui.painter()
                    .add(egui::Shape::line(strand(start, p).to_vec(), stroke));
                if released
                    && let Some(target) = cards.iter().rposition(|rect| rect.contains(p))
                    && source != target
                {
                    self.tack_drag = None;
                    self.board.connect(source, target)?;
                }
            } else if pointer.is_some_and(|p| p.distance(start) > 16.0) {
                self.tack_drag = None;
            }
            if !ui.input(|i| i.pointer.primary_down()) {
                self.tack_drag = None;
            }
            if released {
                ui.ctx().request_repaint();
            }
        }
        for rect in cards {
            if ui.is_rect_visible(*rect) {
                ui.painter()
                    .circle_filled(tack(*rect), 6.0, Color32::from_rgb(135, 61, 43));
                ui.painter().circle_filled(
                    tack(*rect) - vec2(1.0, 1.0),
                    2.5,
                    Color32::from_rgb(236, 166, 108),
                );
            }
        }
        let id = ui.id().with("cut-string");
        if right_click {
            self.cut_menu = closest;
            if closest.is_some() {
                egui::Popup::open_id(ui.ctx(), id);
            }
        }
        if let Some(([a, b], pos)) = self.cut_menu {
            let menu = egui::Popup::new(id, ui.ctx().clone(), pos, ui.layer_id())
                .open_memory(None)
                .show(|ui| {
                    let cut = ui.button("Cut String").clicked();
                    if cut {
                        ui.close();
                    }
                    cut
                });
            if menu.is_some_and(|response| response.inner) {
                self.cut_menu = None;
                self.board.cut(a, b)?;
                ui.ctx().request_repaint();
            }
        }
        Ok(())
    }
}
