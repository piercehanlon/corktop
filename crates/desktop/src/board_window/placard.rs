pub const TRUE_BELIEVER_PHRASES: &[&str] = &[
    "NO INDEPENDENT\nTHOUGHT",
    "DO NOT LOOK\nAT THE MOON",
    "HEARD",
    "DON'T LOOK UP",
    "OBEY",
    "DRINK OVALTINE",
    "CONSUME",
    "STAY ASLEEP",
    "IT'S ALL\nCONNECTED",
    "BIRDS AREN'T\nREAL",
    "CHECK THE BACK",
    "TRUST THE\nALGORITHM",
    "DO NOT OPEN\nTHE DRAWER",
    "THE SKY IS A\nBLUE CANVAS",
    "DO NOT FEED\nTHE TREES",
    "DO NOT ANSWER\nTHE DOOR",
    "WORK IS LIFE\nLIFE IS WORK",
    "SEE SOMETHING\nSAY SOMETHING",
    "LOOSE LIPS\nSINK SHIPS",
    "LOOK AWAY\nFROM THE GLASS",
    "REMAIN IN\nYOUR SEAT",
    "DO NOT LOOK\nAT THE SEAMS",
    "DO NOT REPEAT\nTHE FREQUENCY",
    "IGNORE THE\nSHADOWS",
    "NOT A CAMERA",
    "NOT A\nMICROPHONE",
    "DO NOT\nACKNOWLEDGE",
    "YOUR REFLECTION\nWAS LATE",
    "THERE IS NO\nRETURN ADDRESS",
    "DO NOT COUNT\nTHE STEPS",
    "WHAT IS\nTHAT HUM?",
    "DO NOT LEAVE\nTHE CIRCUIT OPEN",
    "DO NOT TURN OFF\nTHE MONITOR",
    "STANDBY",
    "THE BROADCAST\nIS COMPLETE",
    "SIGNAL LOST",
    "SEND SPIKE",
    "YOU ALREADY\nPINNED THAT",
    "THAT NOTE\nMOVED",
    "YOU BLINKED",
    "KEEP WORKING",
    "FIX YOUR\nPOSTURE",
];

use eframe::egui::{self, Color32, Context, FontFamily, FontId, Galley, Vec2};
use std::{
    sync::{Arc, OnceLock, mpsc},
    time::{Duration, Instant},
};

const FLASH: Duration = Duration::from_millis(42);

#[derive(Default)]
struct Bag {
    remaining: Vec<usize>,
    last: Option<usize>,
}

impl Bag {
    fn next(&mut self) -> usize {
        if self.remaining.is_empty() {
            self.remaining.extend(0..TRUE_BELIEVER_PHRASES.len());
            fastrand::shuffle(&mut self.remaining);
            let end = self.remaining.len() - 1;
            if Some(self.remaining[end]) == self.last {
                self.remaining.swap(0, end);
            }
        }
        let index = self.remaining.pop().unwrap();
        self.last = Some(index);
        index
    }
}

struct Layout {
    viewport: Vec2,
    pixels_per_point: f32,
    lines: Vec<(Vec2, Arc<Galley>)>,
}
impl Layout {
    fn prepare(ctx: &Context, phrase: &str, viewport: Vec2) -> Self {
        let words = if viewport.y > 1.5 * viewport.x {
            phrase.replace(' ', "\n")
        } else {
            phrase.to_owned()
        };
        let lines: Vec<_> = words.lines().collect();
        let measure = |text: &str, size: f32| {
            ctx.fonts(|fonts| {
                fonts.layout_no_wrap(
                    text.into(),
                    FontId::new(size, FontFamily::Name("Placard".into())),
                    Color32::BLACK,
                )
            })
        };
        let widest = lines
            .iter()
            .map(|s| measure(s, 100.).size().x)
            .fold(1., f32::max);
        let height = 105. * lines.len() as f32;
        let size = (100. * (viewport.x * 0.84 / widest).min(viewport.y * 0.72 / height)).max(18.);
        let mut bounds = egui::Rect::NOTHING;
        let mut block = Vec::new();
        for (row, text) in lines.iter().enumerate() {
            let galley = measure(text, size);
            let offset = egui::vec2(-galley.mesh_bounds.center().x, row as f32 * size * 1.05);
            bounds = bounds.union(galley.mesh_bounds.translate(offset));
            block.push((offset, galley));
        }
        let center = bounds.center().to_vec2();
        for (offset, _) in &mut block {
            *offset -= center;
        }
        Self {
            viewport,
            pixels_per_point: ctx.pixels_per_point(),
            lines: block,
        }
    }
}

fn next_flash(now: Instant) -> Instant {
    now + Duration::from_secs(fastrand::u64(9500..=10500))
}

type Wake = (Instant, Context);
struct WakeTimer(mpsc::Sender<Wake>);
impl WakeTimer {
    fn start() -> Self {
        let (tx, rx) = mpsc::channel::<Wake>();
        std::thread::spawn(move || {
            while let Ok((deadline, ctx)) = rx.recv() {
                std::thread::sleep(deadline.saturating_duration_since(Instant::now()));
                ctx.request_repaint();
            }
        });
        Self(tx)
    }
}

pub(super) struct Placard {
    bag: Bag,
    current: usize,
    due: Instant,
    layout: Option<Layout>,
    // The paint pass sets this once, after submitting the cached text meshes.
    submitted: Option<Arc<OnceLock<Instant>>>,
    timer: Option<WakeTimer>,
}
impl Default for Placard {
    fn default() -> Self {
        let mut bag = Bag::default();
        Self {
            current: bag.next(),
            bag,
            due: next_flash(Instant::now()),
            layout: None,
            submitted: None,
            timer: None,
        }
    }
}
impl Placard {
    pub(super) fn invalidate(&mut self) {
        self.layout = None;
        self.submitted = None;
    }
    pub(super) fn show(&mut self, ctx: &Context, visible: bool) -> bool {
        let now = Instant::now();
        if !visible {
            if self.submitted.take().is_some() || now >= self.due {
                self.due = next_flash(now);
            }
            return false;
        }
        if self
            .submitted
            .as_ref()
            .and_then(|s| s.get())
            .is_some_and(|&end| now >= end)
        {
            self.current = self.bag.next();
            self.invalidate();
            self.due = next_flash(now);
            ctx.request_repaint();
            return false;
        }
        let rect = ctx.screen_rect();
        if self.layout.as_ref().is_none_or(|l| {
            l.viewport != rect.size() || l.pixels_per_point != ctx.pixels_per_point()
        }) {
            self.layout = Some(Layout::prepare(
                ctx,
                TRUE_BELIEVER_PHRASES[self.current],
                rect.size(),
            ));
        }
        let timer = self.timer.get_or_insert_with(WakeTimer::start);
        if now < self.due {
            ctx.request_repaint_after(self.due - now);
            return false;
        }
        let submitted = self.submitted.get_or_insert_with(Default::default).clone();
        let painter = ctx.layer_painter(egui::LayerId::new(
            egui::Order::Debug,
            egui::Id::new("placard"),
        ));
        painter.rect_filled(rect, 0., Color32::WHITE);
        for (offset, galley) in &self.layout.as_ref().unwrap().lines {
            painter.galley(rect.center() + *offset, galley.clone(), Color32::BLACK);
        }
        let tx = timer.0.clone();
        let ctx = ctx.clone();
        painter.add(egui::PaintCallback {
            rect,
            callback: Arc::new(egui_glow::CallbackFn::new(move |_, _| {
                submitted.get_or_init(|| {
                    let deadline = Instant::now() + FLASH;
                    let _ = tx.send((deadline, ctx.clone()));
                    deadline
                });
            })),
        });
        true
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn prepared_ink_fits_landscape_and_portrait() {
        let ctx = egui::Context::default();
        super::super::fonts::install(&ctx);
        let _ = ctx.run(Default::default(), |ctx| {
            for viewport in [egui::vec2(1200., 700.), egui::vec2(460., 1000.)] {
                for phrase in TRUE_BELIEVER_PHRASES {
                    let layout = Layout::prepare(ctx, phrase, viewport);
                    let expected = if viewport.y > 1.5 * viewport.x {
                        phrase.split_whitespace().count()
                    } else {
                        phrase.lines().count()
                    };
                    assert_eq!(layout.lines.len(), expected);
                    let bounds = layout
                        .lines
                        .iter()
                        .fold(egui::Rect::NOTHING, |b, (offset, line)| {
                            b.union(line.mesh_bounds.translate(*offset))
                        });
                    assert!(
                        bounds.width() <= viewport.x * 0.84 + 2.
                            && bounds.height() <= viewport.y * 0.72 + 2.,
                        "{phrase}: {bounds:?}"
                    );
                    assert!(bounds.center().to_vec2().length() < 0.01);
                }
            }
        });
    }

    #[test]
    fn every_phrase_fires_once_per_bag_without_boundary_repeats() {
        assert_eq!(TRUE_BELIEVER_PHRASES.len(), 42);
        let mut bag = Bag::default();
        let mut last = None;
        for _ in 0..100 {
            let draws: Vec<_> = (0..42).map(|_| bag.next()).collect();
            assert_ne!(last, Some(draws[0]));
            last = draws.last().copied();
            let mut sorted = draws;
            sorted.sort_unstable();
            assert_eq!(sorted, (0..42).collect::<Vec<_>>());
        }
    }
}

#[cfg(all(test, target_os = "linux"))]
mod desktop_tests {
    use super::*;
    use std::sync::Mutex;
    use winit::platform::x11::EventLoopBuilderExtX11;

    #[test]
    #[ignore = "requires an X11 display and OpenGL"]
    fn idle_native_window_restores_after_submission() {
        // Each run injects one early redraw at the supplied display interval.
        // The deadline worker is the only normal wake after that redraw.
        struct Probe {
            placard: Placard,
            intervals: std::vec::IntoIter<u64>,
            active: Option<Arc<OnceLock<Instant>>>,
            observations: Arc<Mutex<Vec<Duration>>>,
        }
        impl eframe::App for Probe {
            fn update(&mut self, ctx: &Context, _: &mut eframe::Frame) {
                if self.placard.show(ctx, true) {
                    let submitted = self.placard.submitted.as_ref().unwrap();
                    if self.active.is_none() {
                        self.active = Some(submitted.clone());
                        if let Some(hz) = self.intervals.next() {
                            let ctx = ctx.clone();
                            let submitted = submitted.clone();
                            std::thread::spawn(move || {
                                while submitted.get().is_none() {
                                    std::thread::sleep(Duration::from_millis(1));
                                }
                                std::thread::sleep(Duration::from_secs_f64(1. / hz as f64));
                                ctx.request_repaint();
                            });
                        }
                    }
                } else {
                    egui::CentralPanel::default().show(ctx, |ui| {
                        ui.label("Board restored");
                    });
                    if let Some(deadline) = self.active.take().and_then(|v| v.get().copied()) {
                        let restored = Instant::now();
                        assert!(restored >= deadline);
                        let mut results = self.observations.lock().unwrap();
                        results.push(restored.duration_since(deadline - FLASH));
                        if results.len() == 3 {
                            ctx.send_viewport_cmd(egui::ViewportCommand::Close);
                        } else {
                            self.placard.due = restored + Duration::from_millis(150);
                            ctx.request_repaint();
                        }
                    }
                }
            }
        }
        let results = Arc::new(Mutex::new(Vec::new()));
        let observed = results.clone();
        let options = eframe::NativeOptions {
            vsync: false,
            viewport: egui::ViewportBuilder::default().with_inner_size([400., 240.]),
            event_loop_builder: Some(Box::new(|builder| {
                builder.with_any_thread(true);
            })),
            ..Default::default()
        };
        eframe::run_native(
            "corktop",
            options,
            Box::new(move |cc| {
                super::super::fonts::install(&cc.egui_ctx);
                Ok(Box::new(Probe {
                    placard: Placard {
                        due: Instant::now() + Duration::from_millis(300),
                        ..Default::default()
                    },
                    intervals: vec![60, 144, 240].into_iter(),
                    active: None,
                    observations: observed,
                }))
            }),
        )
        .unwrap();
        let times = results.lock().unwrap();
        eprintln!(
            "Idle board restoration at 60/144/240 Hz intervening-redraw intervals: {times:?}"
        );
        assert_eq!(times.len(), 3);
        assert!(
            times
                .iter()
                .all(|&t| t >= FLASH && t < FLASH + Duration::from_millis(35))
        );
    }
}
