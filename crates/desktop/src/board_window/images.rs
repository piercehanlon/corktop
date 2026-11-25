use super::*;
use image::{ImageEncoder, codecs::png::PngEncoder};
use std::{collections::HashMap, io, path::PathBuf, time::SystemTime};

type CachedPreview = (Option<(SystemTime, u64)>, Option<egui::TextureHandle>);

#[derive(Default)]
pub(super) struct Images {
    clipboard: Option<arboard::Clipboard>,
    #[cfg(windows)]
    writer: Option<std::sync::mpsc::Sender<(String, CopyPayload)>>,
    #[cfg(windows)]
    completed: Option<std::sync::mpsc::Receiver<io::Result<String>>>,
    previews: HashMap<String, CachedPreview>,
}

pub(super) fn supported(name: &str) -> bool {
    name.rsplit_once('.').is_some_and(|(_, ext)| {
        ["png", "jpg", "jpeg"]
            .iter()
            .any(|s| ext.eq_ignore_ascii_case(s))
    })
}

fn decode(path: &Path) -> io::Result<image::DynamicImage> {
    let mut reader = image::ImageReader::open(path)?.with_guessed_format()?;
    let mut limits = image::Limits::default();
    limits.max_alloc = Some(96 << 20);
    limits.max_image_width = Some(16384);
    limits.max_image_height = Some(16384);
    reader.limits(limits);
    reader.decode().map_err(io::Error::other)
}

impl Images {
    pub(super) fn reset_textures(&mut self) {
        self.previews.clear();
    }
    fn clipboard(&mut self) -> io::Result<&mut arboard::Clipboard> {
        if self.clipboard.is_none() {
            self.clipboard = Some(arboard::Clipboard::new().map_err(io::Error::other)?);
        }
        Ok(self.clipboard.as_mut().unwrap())
    }

    pub(super) fn clipboard_files(&mut self) -> io::Result<Vec<PathBuf>> {
        match self.clipboard()?.get().file_list() {
            Ok(paths) => Ok(paths),
            Err(arboard::Error::ContentNotAvailable) => Ok(Vec::new()),
            Err(e) => Err(io::Error::other(e)),
        }
    }

    pub(super) fn paste_menu(
        &mut self,
        board: &mut Board,
        pos: [i32; 2],
        link: bool,
        ctx: &egui::Context,
    ) -> io::Result<()> {
        if let Ok(paths) = self.clipboard_files()
            && !paths.is_empty()
        {
            for path in paths {
                board.import_file(&path, free_paste_position(board, pos), link, false)?;
            }
            return Ok(());
        }
        if let Ok(text) = self
            .clipboard()
            .and_then(|c| c.get_text().map_err(io::Error::other))
            && !text.trim().is_empty()
        {
            return board.paste_text(&text, pos);
        }
        if self.clipboard().is_err() {
            ctx.send_viewport_cmd(egui::ViewportCommand::RequestPaste);
            return Ok(());
        }
        self.paste(board, pos)
    }
    pub(super) fn paste(&mut self, board: &mut Board, pos: [i32; 2]) -> io::Result<()> {
        let clip = self.clipboard()?;
        let img = match clip.get_image() {
            Ok(img) => img,
            Err(arboard::Error::ContentNotAvailable) => return Ok(()),
            Err(e) => return Err(io::Error::other(e)),
        };
        if img.bytes.len() > 96 << 20 || img.width > 16384 || img.height > 16384 {
            return Err(io::Error::other("Clipboard image exceeds the image limit"));
        }
        let mut png = Vec::new();
        PngEncoder::new(&mut png)
            .write_image(
                &img.bytes,
                img.width as u32,
                img.height as u32,
                image::ExtendedColorType::Rgba8,
            )
            .map_err(io::Error::other)?;
        board.store_payload("clip", ".png", png.as_slice(), pos)
    }

    pub(super) fn copy(&mut self, path: &Path) -> io::Result<()> {
        let img = decode(path)?.into_rgba8();
        self.clipboard()?
            .set_image(arboard::ImageData {
                width: img.width() as usize,
                height: img.height() as usize,
                bytes: img.into_raw().into(),
            })
            .map_err(io::Error::other)
    }

    pub(super) fn preview(&mut self, board: &Board, idx: usize, ui: &egui::Ui, rect: Rect) {
        let card = &board.cards()[idx];
        if self
            .previews
            .get(&card.file)
            .is_none_or(|(revision, _)| *revision != card.revision())
        {
            let texture = board
                .note_path(idx)
                .and_then(|path| decode(&path))
                .ok()
                .map(|img| {
                    let small = img.thumbnail(198, 55).into_rgba8();
                    let pixels = egui::ColorImage::from_rgba_unmultiplied(
                        [small.width() as usize, small.height() as usize],
                        small.as_raw(),
                    );
                    ui.ctx()
                        .load_texture(&card.file, pixels, egui::TextureOptions::LINEAR)
                });
            self.previews
                .insert(card.file.clone(), (card.revision(), texture));
        }
        if let Some((_, Some(t))) = self.previews.get(&card.file) {
            let scale = rect.width() / CARD_SIZE.x;
            let target =
                Rect::from_min_size(rect.min + vec2(10.0, 48.0) * scale, t.size_vec2() * scale);
            ui.painter().image(
                t.id(),
                target,
                Rect::from_min_max(Pos2::ZERO, egui::pos2(1.0, 1.0)),
                Color32::WHITE,
            );
        }
    }
}

pub(super) fn thumbnail(ctx: &egui::Context, path: &Path) -> Option<egui::TextureHandle> {
    if !path.symlink_metadata().ok()?.is_file() {
        return None;
    }
    let small = decode(path).ok()?.thumbnail(64, 64).into_rgba8();
    let pixels = egui::ColorImage::from_rgba_unmultiplied(
        [small.width() as usize, small.height() as usize],
        small.as_raw(),
    );
    Some(ctx.load_texture("list preview", pixels, egui::TextureOptions::LINEAR))
}

#[cfg(windows)]
pub(super) enum CopyPayload {
    Text(String),
    Image(PathBuf),
}

#[cfg(windows)]
impl Images {
    pub(super) fn enqueue_copy(
        &mut self,
        file: String,
        payload: CopyPayload,
        ctx: &egui::Context,
    ) -> io::Result<()> {
        if self.writer.is_none() {
            let (send, requests) = std::sync::mpsc::channel::<(String, CopyPayload)>();
            let (done, completed) = std::sync::mpsc::channel();
            let wake = ctx.clone();
            std::thread::Builder::new()
                .name("clipboard-writer".into())
                .spawn(move || {
                    let mut clipboard = Images::default();
                    for (name, payload) in requests {
                        let result = match payload {
                            CopyPayload::Text(text) => clipboard
                                .clipboard()
                                .and_then(|c| c.set_text(text).map_err(io::Error::other)),
                            CopyPayload::Image(path) => clipboard.copy(&path),
                        };
                        if done.send(result.map(|()| name)).is_err() {
                            break;
                        }
                        wake.request_repaint();
                    }
                })?;
            self.writer = Some(send);
            self.completed = Some(completed);
        }
        self.writer
            .as_ref()
            .unwrap()
            .send((file, payload))
            .map_err(|_| io::Error::other("clipboard worker stopped"))
    }

    pub(super) fn copy_result(&self) -> Option<io::Result<String>> {
        self.completed.as_ref()?.try_recv().ok()
    }
}
