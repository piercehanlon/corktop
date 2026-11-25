use super::*;
use corktop_board::ListEntry;
use std::{
    collections::HashMap,
    path::PathBuf,
    sync::{
        Arc,
        atomic::{AtomicBool, Ordering},
        mpsc,
    },
};

pub(super) struct FlatList {
    root: PathBuf,
    query: String,
    recursive: bool,
    focus: bool,
    rows: Vec<ListEntry>,
    local_count: usize,
    results: Option<mpsc::Receiver<std::io::Result<Vec<ListEntry>>>>,
    cancelled: Arc<AtomicBool>,
    error: String,
    images: HashMap<PathBuf, Option<egui::TextureHandle>>,
}
impl Drop for FlatList {
    fn drop(&mut self) {
        self.cancelled.store(true, Ordering::Relaxed);
    }
}
impl FlatList {
    pub fn open(board: &Board) -> Self {
        let rows = board.list_entries();
        let local_count = rows.len();
        Self {
            root: board.root().into(),
            query: String::new(),
            recursive: false,
            focus: true,
            rows,
            local_count,
            results: None,
            cancelled: Default::default(),
            error: String::new(),
            images: Default::default(),
        }
    }
    fn scope(&mut self, ctx: &egui::Context) {
        self.cancelled.store(true, Ordering::Relaxed);
        self.rows.truncate(self.local_count);
        self.results = None;
        self.error.clear();
        if !self.recursive {
            return;
        }
        let (tx, rx) = mpsc::channel();
        let root = self.root.clone();
        let ctx = ctx.clone();
        self.cancelled = Default::default();
        let cancelled = self.cancelled.clone();
        std::thread::spawn(move || {
            Board::visit_child_entries(&root, |result| {
                if cancelled.load(Ordering::Relaxed) {
                    return false;
                }
                if result.as_ref().is_ok_and(|r| r.is_empty()) {
                    return true;
                }
                let sent = tx.send(result).is_ok();
                ctx.request_repaint();
                sent
            });
            drop(tx);
            ctx.request_repaint();
        });
        self.results = Some(rx);
    }
    pub fn show(
        &mut self,
        ctx: &egui::Context,
        style: &material::Style,
        open: &mut bool,
    ) -> Option<(PathBuf, String)> {
        if let Some(rx) = &self.results {
            loop {
                match rx.try_recv() {
                    Ok(Ok(rows)) => self.rows.extend(rows),
                    Ok(Err(e)) => self.error = e.to_string(),
                    Err(mpsc::TryRecvError::Empty) => break,
                    Err(mpsc::TryRecvError::Disconnected) => {
                        self.results = None;
                        break;
                    }
                }
            }
        }
        let mut choice = None;
        egui::Window::new("Flat list")
            .id("flat-list".into())
            .open(open)
            .default_width(430.)
            .default_height(350.)
            .show(ctx, |ui| {
                let search = ui.add(
                    egui::TextEdit::singleline(&mut self.query)
                        .hint_text("Search cards")
                        .desired_width(f32::INFINITY),
                );
                if std::mem::take(&mut self.focus) {
                    search.request_focus();
                }
                if ui
                    .checkbox(&mut self.recursive, "Include subfolders")
                    .changed()
                {
                    self.scope(ctx);
                }
                let query = self.query.to_lowercase();
                let matches: Vec<_> = self
                    .rows
                    .iter()
                    .filter(|r| {
                        [&r.title, &r.file, &r.kind, &r.zone, &r.snippet]
                            .iter()
                            .any(|s| s.to_lowercase().contains(&query))
                    })
                    .collect();
                if !self.error.is_empty() {
                    ui.colored_label(ui.visuals().error_fg_color, &self.error);
                }
                if matches.is_empty() {
                    ui.label(if self.results.is_some() {
                        "Searching…"
                    } else {
                        "No matching cards"
                    });
                }
                egui::ScrollArea::vertical().show_rows(ui, 57., matches.len(), |ui, range| {
                    for i in range {
                        let r = matches[i];
                        let (rect, hit) =
                            ui.allocate_exact_size(vec2(ui.available_width(), 57.), Sense::click());
                        if hit.hovered() {
                            ui.painter().rect_filled(
                                rect,
                                3.,
                                ui.visuals().widgets.hovered.weak_bg_fill,
                            );
                        }
                        let icon = Rect::from_min_size(rect.min + vec2(4., 7.), vec2(42., 40.));
                        ui.painter().rect_filled(
                            icon,
                            2.,
                            style.card_paper(&r.file, r.folder, r.tone),
                        );
                        if images::supported(&r.file) {
                            let path = r.board.join(&r.file);
                            let texture = self
                                .images
                                .entry(path.clone())
                                .or_insert_with(|| images::thumbnail(ctx, &path));
                            if let Some(tex) = texture {
                                let size = tex.size_vec2();
                                let size =
                                    size * (icon.width() / size.x).min(icon.height() / size.y);
                                ui.painter().image(
                                    tex.id(),
                                    Rect::from_center_size(icon.center(), size),
                                    Rect::from_min_max(Pos2::ZERO, egui::pos2(1., 1.)),
                                    Color32::WHITE,
                                );
                            }
                        }
                        let painter = ui.painter().with_clip_rect(rect.intersect(ui.clip_rect()));
                        let at = rect.min + vec2(53., 0.);
                        painter.text(
                            at,
                            egui::Align2::LEFT_TOP,
                            &r.title,
                            style.title(16.),
                            ui.visuals().strong_text_color(),
                        );
                        let folder = r
                            .board
                            .strip_prefix(&self.root)
                            .ok()
                            .and_then(|p| p.to_str())
                            .unwrap_or("");
                        let meta = [r.kind.as_str(), r.zone.as_str(), folder]
                            .into_iter()
                            .filter(|s| !s.is_empty())
                            .collect::<Vec<_>>()
                            .join(" · ");
                        painter.text(
                            at + vec2(0., 20.),
                            egui::Align2::LEFT_TOP,
                            meta,
                            egui::FontId::proportional(12.),
                            ui.visuals().text_color(),
                        );
                        painter.text(
                            at + vec2(0., 36.),
                            egui::Align2::LEFT_TOP,
                            r.snippet.lines().next().unwrap_or(""),
                            style.body(13.),
                            ui.visuals().text_color(),
                        );
                        if hit.clicked() {
                            choice = Some((r.board.clone(), r.file.clone()));
                        }
                    }
                });
            });
        choice
    }
}
