use super::*;

pub(super) const THREADS: [Color32; 6] = [
    Color32::from_rgb(218, 83, 73),
    Color32::from_rgb(69, 137, 188),
    Color32::from_rgb(82, 148, 96),
    Color32::from_rgb(202, 155, 65),
    Color32::from_rgb(159, 104, 171),
    Color32::from_rgb(199, 198, 184),
];

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
        background: &egui::Response,
    ) -> std::io::Result<()> {
        let pointer = ui.input(|i| i.pointer.interact_pos());
        let now = ui.input(|i| i.time);
        if self.placing
            && background.clicked()
            && let Some(p) = pointer
        {
            board.add_pin(logical(p - origin))?;
            self.placing = false;
        }
        let geometry = board.geometry();
        let points: Vec<_> = geometry.pins.iter().map(|p| origin + offset(*p)).collect();
        for d in &geometry.dividers {
            let [a, b] = d.pins.map(|i| points[i]);
            ui.painter().line_segment(
                [a + vec2(0.0, 3.0), b + vec2(0.0, 3.0)],
                egui::Stroke::new(3.0, Color32::from_black_alpha(75)),
            );
            ui.painter().line_segment(
                [a, b],
                egui::Stroke::new(2.5, THREADS[usize::from(d.color)]),
            );
        }
        let mut moved_zone = None;
        for (i, z) in geometry.zones.iter().enumerate() {
            let center = origin + offset(geometry.center(i));
            let label = ui.painter().text(
                center,
                egui::Align2::CENTER_CENTER,
                &z.title,
                egui::FontId::proportional(18.0),
                Color32::WHITE,
            );
            let response = ui.interact(label.expand(4.0), ui.id().with(("zone", i)), Sense::drag());
            if response.drag_stopped()
                && let Some(delta) = ui.input(|i| {
                    i.pointer
                        .interact_pos()
                        .zip(i.pointer.press_origin())
                        .map(|(a, b)| a - b)
                })
            {
                moved_zone = Some((i, [delta.x.round() as i32, delta.y.round() as i32]));
            }
        }
        for (id, &p) in points.iter().enumerate() {
            let r = ui.interact(
                Rect::from_center_size(p, vec2(22.0, 22.0)),
                ui.id().with(("bare-tack", id)),
                Sense::drag(),
            );
            ui.painter()
                .circle_filled(p, 6.0, Color32::from_rgb(151, 106, 52));
            ui.painter()
                .circle_filled(p - vec2(1.2, 1.2), 2.3, Color32::from_rgb(239, 197, 119));
            if r.is_pointer_button_down_on() && ui.input(|i| i.pointer.primary_pressed()) {
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
                    egui::Stroke::new(2.5, THREADS[drag.color]),
                );
            } else if drag.moving {
                ui.painter().circle_filled(p, 6.0, Color32::LIGHT_YELLOW);
            }
            if ui.input(|i| i.pointer.primary_released()) {
                let drag = self.drag.take().unwrap();
                let pos = logical(p - origin);
                if drag.moving {
                    board.move_pin(drag.id, pos)?;
                } else if pulling && p.distance(drag.origin) > 10.0 {
                    let end = points.iter().position(|to| p.distance(*to) <= 12.0);
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
