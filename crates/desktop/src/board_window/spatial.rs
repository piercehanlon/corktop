use super::*;

#[derive(Default)]
pub(super) struct Spatial {
    pub placing: bool,
    drag: Option<PinDrag>,
    naming: Option<(Vec<usize>, String)>,
}

struct PinDrag {
    id: usize,
    start: f64,
    origin: Pos2,
    moving: bool,
    color: usize,
}

impl Spatial {
    pub fn show(
        &mut self,
        board: &mut Board,
        ui: &mut egui::Ui,
        origin: Pos2,
        scale: f32,
        background: &egui::Response,
        style: &material::Style,
    ) -> std::io::Result<()> {
        let pointer = ui.input(|i| i.pointer.interact_pos());
        let now = ui.input(|i| i.time);
        if self.placing
            && background.clicked()
            && let Some(p) = pointer
        {
            board.add_pin(logical(
                ((p - origin) / scale).min(offset(board.canvas_size()) - vec2(6.0, 6.0)),
            ))?;
            self.placing = false;
        }
        let geometry = board.geometry();
        let points: Vec<_> = geometry
            .pins
            .iter()
            .map(|p| origin + offset(*p) * scale)
            .collect();
        let mut pack_zone = None;
        let mut moved_zone = None;
        let mut format_zone = None;
        for (i, z) in geometry.zones.iter().enumerate() {
            let center = origin + offset(geometry.center(i)) * scale;
            let label = ui.painter().text(
                center,
                egui::Align2::CENTER_CENTER,
                &z.title,
                egui::FontId::proportional(18.0),
                Color32::WHITE,
            );
            let response = ui.interact(
                label.expand(4.0),
                ui.id().with(("zone", i)),
                Sense::click_and_drag(),
            );
            response.context_menu(|ui| {
                if ui.button("Pack into folder").clicked() {
                    pack_zone = Some(i);
                    ui.close();
                }
                for mode in [
                    "Grid / Rows",
                    "Circle / Ring",
                    "Web / Untangle",
                    "Compact Stack",
                ] {
                    if ui.button(mode).clicked() {
                        format_zone = Some((i, mode));
                        ui.close();
                    }
                }
            });
            if response.drag_stopped()
                && let Some(delta) = ui.input(|i| {
                    i.pointer
                        .interact_pos()
                        .zip(i.pointer.press_origin())
                        .map(|(a, b)| a - b)
                })
            {
                moved_zone = Some((
                    i,
                    [
                        (delta.x / scale).round() as i32,
                        (delta.y / scale).round() as i32,
                    ],
                ));
            }
        }
        for (id, &p) in points.iter().enumerate() {
            ui.interact(
                Rect::from_center_size(p, vec2(22.0, 22.0)),
                ui.id().with(("bare-tack", id)),
                Sense::drag(),
            );
            style.pin(ui.painter(), p);
            if ui.ui_contains_pointer()
                && pointer.is_some_and(|at| at.distance(p) <= 11.0)
                && ui.input(|i| i.pointer.primary_pressed())
            {
                self.drag = Some(PinDrag {
                    id,
                    start: now,
                    origin: p,
                    moving: false,
                    color: 0,
                });
                ui.ctx().request_repaint_after(Duration::from_millis(200));
            }
        }
        if let Some(index) = pack_zone {
            board.pack_zone(index)?;
            ui.ctx().request_repaint();
            return Ok(());
        }
        if let Some((zone, mode)) = format_zone {
            board.format_zone(zone, mode)?;
        }
        if let Some((zone, delta)) = moved_zone {
            board.move_zone(zone, delta)?;
        }
        if let Some(drag) = &mut self.drag
            && let Some(p) = pointer
        {
            if now - drag.start < 0.2 && p.distance(drag.origin) > 5.0 {
                drag.moving = true;
            }
            let pulling = !drag.moving && now - drag.start >= 0.2;
            if pulling {
                let wheel = ui.input(|i| i.raw_scroll_delta.y);
                if wheel != 0.0 {
                    drag.color = (drag.color as i32 + wheel.signum() as i32).rem_euclid(6) as usize;
                }
                ui.painter().line_segment(
                    [drag.origin, p],
                    egui::Stroke::new(2.5, style.thread(drag.color)),
                );
            } else if drag.moving {
                ui.painter().circle_filled(p, 6.0, Color32::LIGHT_YELLOW);
            }
            if ui.input(|i| i.pointer.primary_released()) {
                let drag = self.drag.take().unwrap();
                let pos = logical(
                    ((p - origin) / scale).min(offset(board.canvas_size()) - vec2(6.0, 6.0)),
                );
                if drag.moving {
                    board.move_pin(drag.id, pos)?;
                } else if pulling && p.distance(drag.origin) > 10.0 {
                    let end = points.iter().position(|to| p.distance(*to) <= 12.0);
                    if end.is_none()
                        && let Some(card) = board.cards().iter().rposition(|c| {
                            Rect::from_min_size(
                                origin + offset(c.position) * scale,
                                CARD_SIZE * scale,
                            )
                            .contains(p)
                        })
                    {
                        board.anchor_card(card, Some(drag.id), pos, drag.color as u8)?;
                        ui.ctx().request_repaint();
                        return Ok(());
                    }
                    let loop_pins = end.and_then(|end| path_between(board, drag.id, end));
                    board.tie_divider(drag.id, end, pos, drag.color as u8)?;
                    if let Some(pins) = loop_pins {
                        self.naming = Some((pins, String::new()));
                    }
                }
                ui.ctx().request_repaint();
            }
        }
        if let Some((pins, title)) = &mut self.naming {
            let mut save = false;
            let mut cancel = false;
            egui::Window::new("Name zone")
                .collapsible(false)
                .show(ui.ctx(), |ui| {
                    ui.text_edit_singleline(title);
                    save = ui
                        .add_enabled(!title.trim().is_empty(), egui::Button::new("Save zone"))
                        .clicked();
                    cancel = ui.button("Cancel").clicked();
                });
            if save {
                board.name_zone(pins.clone(), title.trim().into())?;
            }
            if save || cancel {
                self.naming = None;
            }
        }
        Ok(())
    }
}

fn path_between(board: &Board, start: usize, end: usize) -> Option<Vec<usize>> {
    let g = board.geometry();
    let mut parent = vec![None; g.pins.len()];
    let mut queue = std::collections::VecDeque::from([start]);
    parent[start] = Some(start);
    while let Some(i) = queue.pop_front() {
        if i == end {
            let mut path = vec![i];
            while *path.last()? != start {
                path.push(parent[*path.last()?]?);
            }
            return (path.len() >= 3).then_some(path);
        }
        for d in &g.dividers {
            if let Some(other) = d.pins.iter().position(|&x| x == i).map(|n| d.pins[1 - n])
                && parent[other].is_none()
            {
                parent[other] = Some(i);
                queue.push_back(other);
            }
        }
    }
    None
}
