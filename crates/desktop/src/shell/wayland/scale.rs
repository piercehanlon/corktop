use super::Host;
use smithay_client_toolkit::reexports::protocols::wp::{
    fractional_scale::v1::client::{
        wp_fractional_scale_manager_v1::WpFractionalScaleManagerV1,
        wp_fractional_scale_v1::{Event, WpFractionalScaleV1},
    },
    viewporter::client::{wp_viewport::WpViewport, wp_viewporter::WpViewporter},
};
use wayland_client::{
    Connection, Dispatch, QueueHandle, delegate_noop, globals::GlobalList,
    protocol::wl_surface::WlSurface,
};

pub(super) struct Fractional {
    viewport: WpViewport,
    preferred: WpFractionalScaleV1,
}
impl Fractional {
    pub fn bind(globals: &GlobalList, qh: &QueueHandle<Host>, surface: &WlSurface) -> Option<Self> {
        let scaler: WpFractionalScaleManagerV1 = globals.bind(qh, 1..=1, ()).ok()?;
        let viewporter: WpViewporter = globals.bind(qh, 1..=1, ()).ok()?;
        let preferred = scaler.get_fractional_scale(surface, qh, ());
        let viewport = viewporter.get_viewport(surface, qh, ());
        scaler.destroy();
        viewporter.destroy();
        Some(Self {
            viewport,
            preferred,
        })
    }
    pub fn resize(&self, size: [u32; 2]) {
        self.viewport
            .set_destination(size[0] as i32, size[1] as i32);
    }
}
impl Drop for Fractional {
    fn drop(&mut self) {
        self.viewport.destroy();
        self.preferred.destroy();
    }
}
impl Dispatch<WpFractionalScaleV1, ()> for Host {
    fn event(
        state: &mut Self,
        _: &WpFractionalScaleV1,
        event: Event,
        _: &(),
        _: &Connection,
        _: &QueueHandle<Self>,
    ) {
        if let Event::PreferredScale { scale } = event
            && scale > 0
        {
            state.scale = scale as f32 / 120.;
            state.redraw();
        }
    }
}
delegate_noop!(Host: ignore WpFractionalScaleManagerV1);
delegate_noop!(Host: ignore WpViewporter);
delegate_noop!(Host: ignore WpViewport);
