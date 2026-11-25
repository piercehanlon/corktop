use eframe::{egui, glow};
use glutin::{
    config::ConfigTemplateBuilder,
    context::{ContextApi, ContextAttributesBuilder, PossiblyCurrentContext},
    display::{Display, DisplayApiPreference},
    prelude::*,
    surface::{Surface, SurfaceAttributesBuilder, WindowSurface},
};
use raw_window_handle::{
    RawDisplayHandle, RawWindowHandle, WaylandDisplayHandle, WaylandWindowHandle,
};
use std::{ffi::CString, io, num::NonZeroU32, ptr::NonNull, sync::Arc};
use wayland_client::{Connection, Proxy, protocol::wl_surface::WlSurface};

pub struct Graphics {
    painter: egui_glow::Painter,
    surface: Surface<WindowSurface>,
    context: PossiblyCurrentContext,
    size: [u32; 2],
    // Keep both native objects alive until the EGL surface and context are gone.
    _display: OwnedDisplay,
    _window: WlSurface,
    _connection: Connection,
}

impl Graphics {
    pub fn new(connection: &Connection, window: &WlSurface, size: [u32; 2]) -> io::Result<Self> {
        let display = NonNull::new(connection.backend().display_ptr().cast())
            .ok_or_else(|| io::Error::other("missing Wayland display"))?;
        let raw = NonNull::new(window.id().as_ptr().cast())
            .ok_or_else(|| io::Error::other("missing Wayland surface"))?;
        let raw = RawWindowHandle::Wayland(WaylandWindowHandle::new(raw));
        // glutin borrows native handles. The owning proxies are retained in Graphics.
        unsafe {
            let display = Display::new(
                RawDisplayHandle::Wayland(WaylandDisplayHandle::new(display)),
                DisplayApiPreference::Egl,
            )
            .map_err(io::Error::other)?;
            let config = display
                .find_configs(ConfigTemplateBuilder::new().build())
                .map_err(io::Error::other)?
                .next()
                .ok_or_else(|| io::Error::other("no EGL config"))?;
            let attrs = ContextAttributesBuilder::new()
                .with_context_api(ContextApi::Gles(None))
                .build(Some(raw));
            let context = display
                .create_context(&config, &attrs)
                .map_err(io::Error::other)?;
            let attrs = SurfaceAttributesBuilder::<WindowSurface>::new().build(
                raw,
                nz(size[0]),
                nz(size[1]),
            );
            let surface = display
                .create_window_surface(&config, &attrs)
                .map_err(io::Error::other)?;
            let context = context.make_current(&surface).map_err(io::Error::other)?;
            let gl = glow::Context::from_loader_function(|name| {
                display.get_proc_address(&CString::new(name).unwrap())
            });
            let painter =
                egui_glow::Painter::new(Arc::new(gl), "", None, true).map_err(io::Error::other)?;
            Ok(Self {
                painter,
                surface,
                context,
                size,
                _display: OwnedDisplay(Some(display)),
                _window: window.clone(),
                _connection: connection.clone(),
            })
        }
    }
    pub fn present(
        &mut self,
        size: [u32; 2],
        ctx: &egui::Context,
        out: egui::FullOutput,
    ) -> io::Result<()> {
        if size != self.size {
            self.surface.resize(&self.context, nz(size[0]), nz(size[1]));
            self.size = size;
        }
        let meshes = ctx.tessellate(out.shapes, out.pixels_per_point);
        self.painter.clear(size, [0., 0., 0., 1.]);
        self.painter.paint_and_update_textures(
            size,
            out.pixels_per_point,
            &meshes,
            &out.textures_delta,
        );
        self.surface
            .swap_buffers(&self.context)
            .map_err(io::Error::other)
    }
}

fn nz(n: u32) -> NonZeroU32 {
    NonZeroU32::new(n).unwrap_or(NonZeroU32::MIN)
}

impl Drop for Graphics {
    fn drop(&mut self) {
        self.painter.destroy();
        let _ = self.context.make_not_current_in_place();
    }
}

struct OwnedDisplay(Option<Display>);
impl Drop for OwnedDisplay {
    fn drop(&mut self) {
        if let Some(Display::Egl(display)) = self.0.take() {
            // This EGL display uses our private Wayland connection. No other
            // EGL contexts share it and the surface/context have already dropped.
            unsafe {
                display.terminate();
            }
        }
    }
}
