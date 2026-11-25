use super::*;

#[derive(Default)]
pub(super) struct Spatial {
    pub placing: bool,
    drag: Option<PinDrag>,
    relocate: Option<usize>,
    rename: Option<(usize, String)>,
    zone_origin: Option<(usize, Pos2)>,
    naming: Option<(Vec<usize>, String)>,
}

struct PinDrag {
    id: usize,
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

        if self.placing
            && ui.input(|i| i.key_pressed(Key::Escape) || i.modifiers.command || i.modifiers.ctrl)
        {
            self.placing = false;
        }
        if self.placing && background.hovered() {
            ui.ctx().set_cursor_icon(egui::CursorIcon::Crosshair);
        }
        if self.placing
            && background.clicked()
            && let Some(p) = pointer
        {
            board.add_pin(logical(
                ((p - origin) / scale).min(offset(board.canvas_size()) - vec2(6.0, 6.0)),
            ))?;
            self.placing = false;
        }
        if let Some(id) = self.relocate {
            ui.ctx().set_cursor_icon(egui::CursorIcon::Move);
            if ui.input(|i| i.key_pressed(Key::Escape)) {
                self.relocate = None;
            } else if background.clicked()
                && let Some(p) = pointer
            {
                board.move_pin(
                    id,
                    logical(((p - origin) / scale).min(offset(board.canvas_size()) - vec2(6., 6.))),
                )?;
                self.relocate = None;
            }
        }
        let geometry = board.geometry();
        let points: Vec<_> = geometry
            .pins
            .iter()
            .map(|p| origin + offset(*p) * scale)
            .collect();
        let mut dissolve = None;
        let mut pack_zone = None;
        let mut moved_zone = None;
        let mut format_zone = None;
        for (i, z) in geometry.zones.iter().enumerate() {
            let center = origin + offset(geometry.center(i)) * scale;
            let text = ui.painter().layout_no_wrap(
                z.title.clone(),
                egui::FontId::proportional(17.),
                Color32::from_gray(42),
            );
            let label = Rect::from_center_size(center, text.size() + vec2(20., 10.));
            let response = ui
                .interact(label, ui.id().with(("zone", i)), Sense::click_and_drag())
                .on_hover_cursor(egui::CursorIcon::Grab)
                .on_hover_text("Drag to move the zone · Right-click for options");
            if response.drag_started_by(egui::PointerButton::Primary) {
                self.zone_origin = ui
                    .input(|input| input.pointer.press_origin())
                    .map(|p| (i, p));
            }
            let delta = self
                .zone_origin
                .filter(|(idx, _)| *idx == i)
                .and_then(|(_, start)| pointer.map(|p| p - start))
                .unwrap_or_default();
            if delta != Vec2::ZERO {
                let mut ring: Vec<_> = z.pins.iter().map(|&n| points[n] + delta).collect();
                ring.push(ring[0]);
                ui.painter().add(egui::Shape::line(
                    ring,
                    egui::Stroke::new(2., Color32::from_rgb(255, 229, 177)),
                ));
                ui.ctx().set_cursor_icon(egui::CursorIcon::Grabbing);
            }
            ui.painter()
                .rect_filled(label.translate(delta), 3., Color32::from_rgb(241, 225, 192));
            ui.painter().galley(
                center + delta - text.size() * 0.5,
                text,
                Color32::from_gray(42),
            );
            if response.double_clicked() {
                self.rename = Some((i, z.title.clone()));
            }
            response.context_menu(|ui| {
                if ui.button("Rename zone").clicked() {
                    self.rename = Some((i, z.title.clone()));
                    ui.close();
                }
                if ui.button("Dissolve zone").clicked() {
                    dissolve = Some(i);
                    ui.close();
                }

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
            if response.drag_stopped_by(egui::PointerButton::Primary)
                && self.zone_origin.take().is_some()
                && delta != Vec2::ZERO
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
        let mut remove_pin = None;
        for (id, &p) in points.iter().enumerate() {
            let pin = ui.interact(
                Rect::from_center_size(p, vec2(22.0, 22.0)),
                ui.id().with(("bare-tack", id)),
                Sense::click_and_drag(),
            );
            pin.context_menu(|ui| {
                if ui.button("Move tack").clicked() {
                    self.relocate = Some(id);
                    ui.close();
                }
                if ui.button("Delete tack").clicked() {
                    remove_pin = Some(id);
                    ui.close();
                }
            });
            style.pin(ui.painter(), p);
            if pin.is_pointer_button_down_on() && ui.input(|i| i.pointer.primary_pressed()) {
                self.drag = Some(PinDrag {
                    id,
                    origin: p,
                    moving: ui.input(|i| i.modifiers.shift),
                    color: 0,
                });
            }
        }
        if let Some(idx) = dissolve {
            board.dissolve_zone(idx)?;
            self.zone_origin = None;
            return Ok(());
        }
        if let Some((idx, title)) = &mut self.rename {
            let mut save = false;
            let mut cancel = false;
            egui::Window::new("Rename zone")
                .collapsible(false)
                .show(ui.ctx(), |ui| {
                    ui.text_edit_singleline(title);
                    save = ui
                        .add_enabled(!title.trim().is_empty(), egui::Button::new("Save"))
                        .clicked();
                    cancel = ui.button("Cancel").clicked();
                });
            if save {
                board.rename_zone(*idx, title.trim().into())?;
            }
            if save || cancel {
                self.rename = None;
            }
        }
        if let Some(id) = remove_pin {
            self.drag = None;
            board.remove_pin(id)?;
            ui.ctx().request_repaint();
            return Ok(());
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
            let pulling = !drag.moving;
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
