#[cfg(windows)]
pub mod caption;
pub mod control;
pub mod login;
pub mod picker;
pub mod tray;
use eframe::egui;
use raw_window_handle::{HasWindowHandle, RawWindowHandle};
use serde::{Deserialize, Serialize};
use std::{
    fs,
    io::{self, Read, Write},
    path::{Path, PathBuf},
};

#[cfg(windows)]
mod windows;
#[cfg(target_os = "linux")]
mod x11;

#[derive(Default, Debug, PartialEq, Serialize, Deserialize)]
#[serde(default)]
pub struct MachineState {
    pub folder: Option<PathBuf>,
    pub size: Option<[f32; 2]>,
    pub position: Option<[f32; 2]>,
    pub desktop: bool,
    pub start_with_os: bool,
    pub introduction_seen: bool,
}

pub fn app_icon() -> egui::IconData {
    eframe::icon_data::from_png_bytes(include_bytes!("../assets/corktop.png"))
        .expect("embedded corktop icon")
}

pub fn display_path(path: &Path) -> String {
    let text = path.to_string_lossy();
    #[cfg(windows)]
    if let Some(std::path::Component::Prefix(p)) = path.components().next() {
        match p.kind() {
            std::path::Prefix::VerbatimDisk(_) => return text[4..].to_owned(),
            std::path::Prefix::VerbatimUNC(_, _) => return format!(r"\\{}", &text[8..]),
            _ => {}
        }
    }
    text.into_owned()
}

pub fn state_directory() -> io::Result<PathBuf> {
    #[cfg(windows)]
    let dir = std::env::var_os("LOCALAPPDATA").map(PathBuf::from);
    #[cfg(not(windows))]
    let dir = std::env::var_os("XDG_STATE_HOME")
        .map(PathBuf::from)
        .filter(|p| p.is_absolute())
        .or_else(|| std::env::var_os("HOME").map(|h| PathBuf::from(h).join(".local/state")));
    Ok(dir
        .ok_or_else(|| io::Error::other("no local application state directory"))?
        .join("corktop"))
}

impl MachineState {
    pub fn load() -> io::Result<Self> {
        let file = match fs::File::open(state_directory()?.join("window.json")) {
            Ok(f) => f,
            Err(e) if e.kind() == io::ErrorKind::NotFound => return Ok(Self::default()),
            Err(e) => return Err(e),
        };
        if file.metadata()?.len() > 65536 {
            return Err(io::Error::other("invalid window state"));
        }
        serde_json::from_reader(file.take(65536)).map_err(io::Error::other)
    }
    pub fn save(&self) -> io::Result<()> {
        let dir = state_directory()?;
        fs::create_dir_all(&dir)?;
        let mut tmp = tempfile::NamedTempFile::new_in(&dir)?;
        serde_json::to_writer(&mut tmp, self)?;
        tmp.flush()?;
        tmp.as_file().sync_all()?;
        tmp.persist(dir.join("window.json"))?;
        Ok(())
    }
    pub fn viewport(&self) -> egui::ViewportBuilder {
        let [w, h] = self
            .size
            .filter(|s| s.iter().all(|v| v.is_finite()))
            .unwrap_or([780., 540.]);
        let mut builder = egui::ViewportBuilder::default()
            .with_app_id("corktop")
            .with_icon(app_icon())
            .with_decorations(false)
            .with_resizable(true)
            .with_inner_size([w.clamp(160., 16384.), h.clamp(100., 16384.)]);
        if let Some(p) = self
            .position
            .filter(|p| p.iter().all(|v| v.is_finite() && v.abs() < 100000.))
        {
            builder = builder.with_position(p);
        }
        builder
    }

    pub fn observe(&mut self, ctx: &egui::Context, root: Option<&Path>) {
        if let Some(root) = root {
            self.folder = Some(root.into());
        }
        if !self.desktop {
            ctx.input(|i| {
                let viewport = i.viewport();
                if viewport.minimized != Some(true) {
                    if let Some(r) = viewport.inner_rect.filter(|r| r.is_positive()) {
                        self.size = Some(r.size().into());
                    }
                    if let Some(r) = viewport.outer_rect.filter(|r| r.is_positive()) {
                        self.position = Some(r.min.into());
                    }
                }
            });
        }
    }
}

#[derive(Default)]
pub struct Layer {
    pub active: bool,
    #[cfg(windows)]
    original: Option<windows::Placement>,
}

impl Layer {
    pub fn set(&mut self, frame: &eframe::Frame, desktop: bool) -> io::Result<()> {
        if desktop == self.active {
            return Ok(());
        }
        let handle = frame.window_handle().map_err(io::Error::other)?.as_raw();
        match handle {
            #[cfg(target_os = "linux")]
            RawWindowHandle::Xlib(h) => x11::set(h.window as u32, desktop)?,
            #[cfg(target_os = "linux")]
            RawWindowHandle::Xcb(h) => x11::set(h.window.get(), desktop)?,
            #[cfg(windows)]
            RawWindowHandle::Win32(h) => windows::set(h.hwnd.get(), desktop, &mut self.original)?,
            _ => {
                return Err(io::Error::other(
                    "Desktop layer unavailable on this compositor. Standard window is active.",
                ));
            }
        }
        self.active = desktop;
        Ok(())
    }
}

#[cfg(target_os = "linux")]
pub(crate) mod wayland;
