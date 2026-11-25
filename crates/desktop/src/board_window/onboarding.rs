use super::*;

#[derive(Default)]
pub(super) struct Setup {
    pub active: bool,
    pub style: material::Style,
    pub styled: bool,
    pub drop_action: Option<file_drop::Action>,
}
impl BoardWindow {
    pub(super) fn setup_card(&mut self, ctx: &egui::Context) {
        if !self.machine.introduction_seen && self.machine.folder.is_none() {
            if tutorial(ctx, &self.setup.style, "Continue to folder setup") {
                self.machine.introduction_seen = true;
                if let Err(e) = self.machine.save() {
                    self.error = e.to_string();
                }
                ctx.request_repaint();
            }
            return;
        }
        self.setup.active = true;
        let width = (ctx.screen_rect().width() - 32.).clamp(120., 420.);
        egui::Window::new("Open a corktop")
            .anchor(egui::Align2::CENTER_CENTER, Vec2::ZERO)
            .resizable(false)
            .collapsible(false)
            .default_width(width)
            .show(ctx, |ui| {
                egui::ScrollArea::vertical()
                    .max_height((ctx.screen_rect().height() - 84.).max(64.))
                    .show(ui, |ui| {
                        ui.label("Board folder");
                        let enter = ui
                            .horizontal(|ui| {
                                let field = ui.add(
                                    egui::TextEdit::singleline(&mut self.folder)
                                        .hint_text("Paste a folder path")
                                        .desired_width((ui.available_width() - 78.).max(60.)),
                                );
                                if ui.button("Browse…").clicked() {
                                    self.choose();
                                }
                                field.lost_focus() && ui.input(|i| i.key_pressed(Key::Enter))
                            })
                            .inner;
                        let before = self.setup.style.clone();
                        self.setup.style.controls(ui, &[]);
                        self.setup.styled |= before != self.setup.style;
                        let mut action = self.setup.drop_action.unwrap_or_default();
                        if action.setting(ui) {
                            self.setup.drop_action = Some(action);
                        }
                        ui.horizontal_wrapped(|ui| {
                            if ui
                                .add_enabled(
                                    !self.folder.trim().is_empty(),
                                    egui::Button::new("Open folder"),
                                )
                                .clicked()
                                || enter
                            {
                                let folder = self.folder.trim().to_owned();
                                self.open(Path::new(&folder));
                            }
                            if ui.button("Close").clicked() {
                                ctx.send_viewport_cmd(egui::ViewportCommand::Close);
                            }
                        });
                    });
            });
    }
}

const LESSONS: [(&str, &str); 4] = [
    (
        "Read the evidence",
        "Click a card to copy it. Double-click to open its file.",
    ),
    (
        "Leave a note",
        "Double-click empty cork, type a note, then press Ctrl+Enter.",
    ),
    (
        "Follow the thread",
        "Drag a pinhead to another card and release. Scroll to change color.",
    ),
    (
        "Mark a zone",
        "Use Pin to place three tacks. Join them in a closed loop.",
    ),
];

pub(super) fn tutorial(ctx: &egui::Context, style: &material::Style, continue_label: &str) -> bool {
    let screen = ctx.screen_rect();
    let columns = if screen.width() > 460. { 2 } else { 1 };
    // The lesson cards live only in the UI. Notes made on the surrounding board use normal storage.
    let width = ((screen.width() - 40.) / columns as f32 - 12.).clamp(128., 208.);
    let mut done = false;
    egui::Area::new("tutorial-controls".into())
        .anchor(egui::Align2::CENTER_BOTTOM, vec2(0., -8.))
        .movable(false)
        .show(ctx, |ui| {
            egui::Frame::popup(ui.style()).show(ui, |ui| {
                ui.horizontal_wrapped(|ui| {
                    ui.strong("Board tutorial");
                    if ui.button(continue_label).clicked() {
                        done = true;
                    }
                })
            });
        });
    let t = ctx.input(|i| i.time) as f32;
    for (i, (title, description)) in LESSONS.iter().enumerate() {
        let origin = screen.min
            + vec2(
                18. + (i % columns) as f32 * (width + 8.),
                45. + (i / columns) as f32 * 109.,
            );
        egui::Area::new(egui::Id::new(("lesson", i)))
            .default_pos(origin)
            .default_size(vec2(width, 100.))
            .movable(true)
            .show(ctx, |ui| {
                egui::Frame::new()
                    .fill(style.note(title, false))
                    .corner_radius(3.)
                    .inner_margin(8.)
                    .show(ui, |ui| {
                        ui.set_width(width - 16.);
                        let (r, _) = ui.allocate_exact_size(vec2(width - 16., 22.), Sense::hover());
                        style.pin(ui.painter(), r.center_top() + vec2(0., 2.));
                        let ink = if style.note(title, false).intensity() < 0.45 {
                            Color32::WHITE
                        } else {
                            Color32::from_gray(38)
                        };
                        hint(
                            ui.painter(),
                            r.left_center() + vec2(5., 0.),
                            i,
                            (t * 0.6 + i as f32 * 0.2).fract(),
                            ink,
                        );
                        ui.label(
                            egui::RichText::new(*title)
                                .font(style.title(16.))
                                .color(ink),
                        );
                        ui.label(
                            egui::RichText::new(*description)
                                .font(style.body(13.))
                                .color(ink),
                        );
                    });
            });
    }
    ctx.request_repaint_after(Duration::from_millis(100));
    done
}
fn hint(p: &egui::Painter, at: Pos2, lesson: usize, t: f32, ink: Color32) {
    let stroke = egui::Stroke::new(1.4, ink.gamma_multiply(0.65));
    let points = [at, at + vec2(25., 0.), at + vec2(12., -8.)];
    let pointer = match lesson {
        0 | 1 => {
            let pulse = if lesson == 0 { t } else { (t * 2.).fract() };
            p.circle_stroke(at + vec2(8., 0.), 2. + pulse * 5., stroke);
            at + vec2(9., 1. + pulse * 2.)
        }
        2 => {
            let pos = points[0].lerp(points[1], t);
            p.line_segment([points[0], pos], stroke);
            for q in &points[..2] {
                p.circle_filled(*q, 2., ink);
            }
            pos
        }
        _ => {
            let segment = (t * 3.) as usize;
            let pos = points[segment].lerp(points[(segment + 1) % 3], (t * 3.).fract());
            for n in 0..segment {
                p.line_segment([points[n], points[n + 1]], stroke);
            }
            p.line_segment([points[segment], pos], stroke);
            for q in points {
                p.circle_filled(q, 1.8, ink);
            }
            pos
        }
    };
    let hand = [
        vec2(0., 0.),
        vec2(1., 7.),
        vec2(5., 10.),
        vec2(9., 8.),
        vec2(8., 4.),
        vec2(4., 4.),
        vec2(2., 0.),
    ];
    p.add(egui::Shape::convex_polygon(
        hand.map(|v| pointer + v).to_vec(),
        Color32::from_white_alpha(180),
        stroke,
    ));
}
