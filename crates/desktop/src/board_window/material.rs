use super::*;
use serde::{Deserialize, Serialize};

#[derive(Clone, Copy, PartialEq, Eq, Default, Serialize, Deserialize)]
pub(super) enum Kit {
    #[default]
    Cork,
    DarkRoom,
    Chalkboard,
    Blueprint,
    Linen,
}
const KITS: [Kit; 5] = [
    Kit::Cork,
    Kit::DarkRoom,
    Kit::Chalkboard,
    Kit::Blueprint,
    Kit::Linen,
];
#[derive(Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
enum Fastener {
    Brass,
    MapPin,
    Tape,
    Steel,
}
const FASTENERS: [Fastener; 4] = [
    Fastener::Brass,
    Fastener::MapPin,
    Fastener::Tape,
    Fastener::Steel,
];
impl Fastener {
    fn name(self) -> &'static str {
        match self {
            Self::Brass => "Brass tack",
            Self::MapPin => "Map pin",
            Self::Tape => "Masking tape",
            Self::Steel => "Steel magnet",
        }
    }
}
impl Kit {
    fn name(self) -> &'static str {
        match self {
            Self::Cork => "Classic Cork",
            Self::DarkRoom => "Dark Room",
            Self::Chalkboard => "Chalkboard",
            Self::Blueprint => "Blueprint",
            Self::Linen => "Raw Linen",
        }
    }
    fn palette(self) -> ([u8; 3], [u32; 4], [u32; 6], Fastener) {
        match self {
            Self::Cork => (
                [174, 125, 80],
                [0xffe99e, 0xf3c4c3, 0xc9dfc2, 0xe2c18b],
                [0xb83029, 0x386b91, 0x4d744c, 0xd2912b, 0x855780, 0xeee5cb],
                Fastener::Brass,
            ),
            Self::DarkRoom => (
                [47, 49, 48],
                [0xd4c7a8, 0xa7b6bb, 0xd8af6a, 0xb2bd9f],
                [0xe25448, 0x81afcc, 0x9fba89, 0xd4aa56, 0xbf93bb, 0xdbd4c7],
                Fastener::MapPin,
            ),
            Self::Chalkboard => (
                [47, 58, 55],
                [0xf1e5c9, 0xc6dbe0, 0xdebbb7, 0xe7d7a8],
                [0xda7c76, 0xa6cbd8, 0xb3c9a2, 0xe8cf88, 0xc5adcb, 0xf2edda],
                Fastener::Tape,
            ),
            Self::Blueprint => (
                [47, 89, 125],
                [0xf5f5eb, 0xcbe9eb, 0xebc279, 0xdac799],
                [0xd75b3d, 0x96d4ea, 0x89b69f, 0xf0bf5f, 0xd1acc8, 0xf2ebd8],
                Fastener::Steel,
            ),
            Self::Linen => (
                [186, 171, 146],
                [0xdabb94, 0xf0ead8, 0xe4b198, 0xdbbd92],
                [0xa63e32, 0x486c87, 0x607859, 0xb48a3b, 0x865d7e, 0xfdf3da],
                Fastener::Brass,
            ),
        }
    }
}

#[derive(Clone, Default, PartialEq, Serialize, Deserialize)]
#[serde(default)]
pub(super) struct Style {
    kit: Kit,
    surface: Option<Kit>,
    fastener: Option<Fastener>,
    notes: Option<[[u8; 3]; 4]>,
    yarn: Option<[[u8; 3]; 6]>,
    pub(super) texture: Option<String>,
    pub(super) font: Option<String>,
    mono: bool,
    plain_titles: bool,
}
fn rgb(s: u32) -> [u8; 3] {
    [(s >> 16) as u8, (s >> 8) as u8, s as u8]
}
fn color(c: [u8; 3]) -> Color32 {
    Color32::from_rgb(c[0], c[1], c[2])
}
impl Style {
    fn notes(&self) -> [[u8; 3]; 4] {
        self.notes.unwrap_or_else(|| self.kit.palette().1.map(rgb))
    }
    fn threads(&self) -> [[u8; 3]; 6] {
        self.yarn.unwrap_or_else(|| self.kit.palette().2.map(rgb))
    }
    pub fn thread(&self, index: usize) -> Color32 {
        color(self.threads()[index % 6])
    }
    pub fn note(&self, file: &str, folder: bool) -> Color32 {
        let slot = if folder {
            3
        } else {
            file.bytes()
                .fold(0u32, |h, b| h.wrapping_mul(31).wrapping_add(b as u32)) as usize
                % 4
        };
        color(self.notes()[slot])
    }
    pub fn title(&self, size: f32) -> egui::FontId {
        egui::FontId::new(
            size,
            if self.font.is_some() {
                egui::FontFamily::Name("Custom title".into())
            } else if self.plain_titles {
                egui::FontFamily::Proportional
            } else {
                egui::FontFamily::Name("Marker".into())
            },
        )
    }
    pub fn body(&self, size: f32) -> egui::FontId {
        if self.mono {
            egui::FontId::monospace(size)
        } else {
            egui::FontId::proportional(size)
        }
    }
    pub fn pin(&self, painter: &egui::Painter, p: Pos2) {
        match self.fastener.unwrap_or(self.kit.palette().3) {
            Fastener::Tape => {
                let corners = [
                    vec2(-11., -4.),
                    vec2(12., -6.),
                    vec2(13., 2.),
                    vec2(-10., 5.),
                ];
                painter.add(egui::Shape::convex_polygon(
                    corners.map(|c| p + c).to_vec(),
                    Color32::from_rgba_unmultiplied(235, 218, 174, 220),
                    egui::Stroke::new(0.7, Color32::from_black_alpha(52)),
                ));
            }
            kind => {
                let (rim, cap, shine) = match kind {
                    Fastener::Brass => (0x68431f, 0xbf904f, 0xf1d79b),
                    Fastener::Steel => (0x575f68, 0xb6c3c8, 0xf5f8fa),
                    _ => (0x663e36, 0xa85d50, 0xd5937c),
                };
                painter.circle_filled(p + vec2(1.3, 2.), 6.2, Color32::from_black_alpha(64));
                painter.circle_filled(p, 6., color(rgb(rim)));
                painter.circle_filled(p - vec2(0.4, 0.65), 4.8, color(rgb(cap)));
                painter.circle_filled(p - vec2(1.7, 2.2), 1.5, color(rgb(shine)));
            }
        }
    }
    pub fn controls(&mut self, ui: &mut egui::Ui, files: &[String]) {
        let before = self.kit;
        egui::ComboBox::from_label("Material kit")
            .selected_text(before.name())
            .show_ui(ui, |ui| {
                for k in KITS {
                    ui.selectable_value(&mut self.kit, k, k.name());
                }
            });
        if self.kit != before {
            *self = Style {
                kit: self.kit,
                ..Default::default()
            };
        }
        ui.collapsing("Customize materials", |ui| {
            let surface = self.surface.unwrap_or(self.kit);
            egui::ComboBox::from_label("Surface")
                .selected_text(surface.name())
                .show_ui(ui, |ui| {
                    for k in KITS {
                        ui.selectable_value(&mut self.surface, Some(k), k.name());
                    }
                });
            let fastener = self.fastener.unwrap_or(self.kit.palette().3);
            egui::ComboBox::from_label("Fastener")
                .selected_text(fastener.name())
                .show_ui(ui, |ui| {
                    for k in FASTENERS {
                        ui.selectable_value(&mut self.fastener, Some(k), k.name());
                    }
                });
            let mut notes = self.notes();
            ui.horizontal(|ui| {
                ui.label("Paper");
                for c in &mut notes {
                    if ui.color_edit_button_srgb(c).changed() {
                        self.notes = Some(notes);
                        break;
                    }
                }
            });
            let mut yarn = self.threads();
            ui.horizontal(|ui| {
                ui.label("Yarn");
                for c in &mut yarn {
                    if ui.color_edit_button_srgb(c).changed() {
                        self.yarn = Some(yarn);
                        break;
                    }
                }
            });
            select_override(
                ui,
                "Surface file",
                &mut self.texture,
                files,
                &["png", "jpg", "jpeg"],
            );
            select_override(
                ui,
                "Title font file",
                &mut self.font,
                files,
                &["ttf", "otf"],
            );
            ui.checkbox(&mut self.plain_titles, "Sans-serif titles");
            ui.checkbox(&mut self.mono, "Monospace body");
        });
    }
}

#[derive(Default)]
pub(super) struct Materials {
    pub style: Style,
    pub files: Vec<String>,
    pub custom_font: Option<std::sync::Arc<egui::FontData>>,
    custom_image: Option<std::sync::Arc<egui::ColorImage>>,
    tile: Option<(Kit, egui::TextureHandle)>,
}
impl Materials {
    pub fn reset(&mut self) {
        self.tile = None;
    }
    pub fn paint(&mut self, ui: &egui::Ui, rect: Rect, origin: Pos2, scale: f32) {
        let kit = self.style.surface.unwrap_or(self.style.kit);
        if self.tile.as_ref().is_none_or(|(k, _)| *k != kit) {
            self.tile = Some((
                kit,
                ui.ctx().load_texture(
                    kit.name(),
                    self.custom_image
                        .clone()
                        .unwrap_or_else(|| tile(kit).into()),
                    egui::TextureOptions::LINEAR_REPEAT,
                ),
            ));
        }
        let dimensions = self.tile.as_ref().unwrap().1.size_vec2() * scale;
        let uv = Rect::from_min_max(
            ((rect.min - origin) / dimensions).to_pos2(),
            ((rect.max - origin) / dimensions).to_pos2(),
        );
        ui.painter()
            .image(self.tile.as_ref().unwrap().1.id(), rect, uv, Color32::WHITE);
    }
}

fn noise(x: u32, y: u32) -> u32 {
    let mut n = x.wrapping_mul(0x8da6b343) ^ y.wrapping_mul(0xd8163841);
    n ^= n >> 16;
    n = n.wrapping_mul(0x7feb352d);
    n ^ (n >> 15)
}
fn tile(kit: Kit) -> egui::ColorImage {
    let base = kit.palette().0;
    let mut pixels = Vec::with_capacity(192 * 192);
    for y in 0..192u32 {
        for x in 0..192u32 {
            let grain = (noise(x, y) % 11) as i32 - 5;
            let shade = match kit {
                Kit::Cork => {
                    // Periodic Voronoi chips keep tile edges continuous.
                    let (mut near, mut next, mut tone) = (u32::MAX, u32::MAX, 0);
                    for dy in -1..=1 {
                        for dx in -1..=1 {
                            let cx = x as i32 / 8 + dx;
                            let cy = y as i32 / 8 + dy;
                            let seed = noise(cx.rem_euclid(24) as u32, cy.rem_euclid(24) as u32);
                            let px = cx * 8 + (seed % 7) as i32 - x as i32;
                            let py = cy * 8 + ((seed >> 8) % 7) as i32 - y as i32;
                            let d = (px * px + py * py) as u32;
                            if d < near {
                                next = near;
                                near = d;
                                tone = ((seed >> 16) % 37) as i32 - 18;
                            } else {
                                next = next.min(d);
                            }
                        }
                    }
                    tone + grain * 2 - if next - near < 5 { 11 } else { 0 }
                }
                Kit::DarkRoom => grain / 2 + (noise(x / 3, y) % 5) as i32 - 2,
                Kit::Chalkboard => grain / 3 + (noise(x / 24, y / 2) % 7) as i32 - 3,
                Kit::Blueprint => {
                    grain / 3
                        + if x % 96 == 0 || y % 96 == 0 {
                            35
                        } else if x % 16 == 0 || y % 16 == 0 {
                            16
                        } else {
                            0
                        }
                }
                Kit::Linen => {
                    grain
                        + match (x % 4, y % 4) {
                            (0, _) => -13,
                            (_, 0) => -10,
                            (2, 2) => 9,
                            (_, 2) => 5,
                            _ => 0,
                        }
                }
            };
            pixels.push(color(
                base.map(|c| (i32::from(c) + shade).clamp(0, 255) as u8),
            ));
        }
    }
    egui::ColorImage::new([192, 192], pixels)
}

fn select_override(
    ui: &mut egui::Ui,
    label: &str,
    value: &mut Option<String>,
    files: &[String],
    extensions: &[&str],
) {
    egui::ComboBox::from_label(label)
        .selected_text(value.as_deref().unwrap_or("Built-in"))
        .show_ui(ui, |ui| {
            ui.selectable_value(value, None, "Built-in");
            for file in files.iter().filter(|f| {
                f.rsplit_once('.')
                    .is_some_and(|(_, ext)| extensions.iter().any(|e| e.eq_ignore_ascii_case(ext)))
            }) {
                ui.selectable_value(value, Some(file.clone()), file);
            }
        });
}

impl Materials {
    pub fn load_assets(&mut self, prefs: &corktop_board::Preferences) -> std::io::Result<()> {
        use std::io::{self, Cursor};
        self.reset();
        self.custom_image = None;
        self.custom_font = None;
        if let Some(name) = &self.style.texture {
            let data = prefs.read_override(name)?;
            let mut reader = image::ImageReader::new(Cursor::new(data)).with_guessed_format()?;
            let mut limits = image::Limits::default();
            limits.max_image_width = Some(2048);
            limits.max_image_height = Some(2048);
            limits.max_alloc = Some(32 << 20);
            reader.limits(limits);
            let img = reader.decode().map_err(io::Error::other)?.to_rgba8();
            self.custom_image = Some(
                egui::ColorImage::from_rgba_unmultiplied(
                    [img.width() as usize, img.height() as usize],
                    img.as_raw(),
                )
                .into(),
            );
        }
        if let Some(name) = &self.style.font {
            let data = prefs.read_override(name)?;
            ttf_parser::Face::parse(&data, 0)
                .map_err(|_| io::Error::other("invalid font override"))?;
            self.custom_font = Some(egui::FontData::from_owned(data).into());
        }
        Ok(())
    }
}
