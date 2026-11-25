mod graphics;
mod input;

use crate::BoardWindow;
use eframe::egui;
use smithay_client_toolkit::{
    compositor::{CompositorHandler, CompositorState},
    delegate_compositor, delegate_layer, delegate_output, delegate_registry,
    output::{OutputHandler, OutputState},
    reexports::{
        calloop::{EventLoop, channel},
        calloop_wayland_source::WaylandSource,
    },
    registry::{ProvidesRegistryState, RegistryState},
    registry_handlers,
    seat::SeatState,
    shell::{
        WaylandSurface,
        wlr_layer::{
            Anchor, KeyboardInteractivity, Layer, LayerShell, LayerShellHandler, LayerSurface,
            LayerSurfaceConfigure,
        },
    },
};
use std::{io, time::Instant};
use wayland_client::{
    Connection, QueueHandle,
    globals::registry_queue_init,
    protocol::{wl_keyboard, wl_output, wl_surface},
};

pub enum Exit {
    Quit,
    Standard,
}
struct Host {
    clipboard: smithay_clipboard::Clipboard,
    registry: RegistryState,
    outputs: OutputState,
    seats: SeatState,
    layer: LayerSurface,
    keyboard: Option<wl_keyboard::WlKeyboard>,
    pointer: Option<smithay_client_toolkit::seat::pointer::ThemedPointer>,
    compositor: CompositorState,
    shm: smithay_client_toolkit::shm::Shm,
    handle: smithay_client_toolkit::reexports::calloop::LoopHandle<'static, Host>,
    input: egui::RawInput,
    size: [u32; 2],
    scale: i32,
    configured: bool,
    hidden: bool,
    closed: bool,
    next: Option<Instant>,
}
impl Host {
    fn redraw(&mut self) {
        self.next = Some(Instant::now());
    }
    fn schedule(&mut self, at: Instant) {
        self.next = Some(self.next.map_or(at, |old| old.min(at)));
    }
}

pub fn run(app: &mut BoardWindow) -> io::Result<Exit> {
    let conn = Connection::connect_to_env().map_err(io::Error::other)?;
    let (globals, queue) = registry_queue_init(&conn).map_err(io::Error::other)?;
    let qh = queue.handle();
    let compositor = CompositorState::bind(&globals, &qh).map_err(io::Error::other)?;
    let shell = LayerShell::bind(&globals, &qh).map_err(io::Error::other)?;
    let layer = shell.create_layer_surface(
        &qh,
        compositor.create_surface(&qh),
        Layer::Background,
        Some("corktop"),
        None,
    );
    layer.set_anchor(Anchor::all());
    layer.set_keyboard_interactivity(KeyboardInteractivity::OnDemand);
    layer.set_size(0, 0);
    layer.set_exclusive_zone(-1);
    layer.commit();
    // The connection outlives Host and its clipboard worker.
    let clipboard =
        unsafe { smithay_clipboard::Clipboard::new(conn.backend().display_ptr().cast()) };
    let mut events = EventLoop::try_new().map_err(io::Error::other)?;
    let mut host = Host {
        handle: events.handle(),
        compositor,
        shm: smithay_client_toolkit::shm::Shm::bind(&globals, &qh).map_err(io::Error::other)?,
        clipboard,
        registry: RegistryState::new(&globals),
        outputs: OutputState::new(&globals, &qh),
        seats: SeatState::new(&globals, &qh),
        layer,
        keyboard: None,
        pointer: None,
        input: Default::default(),
        size: [1, 1],
        scale: 1,
        configured: false,
        hidden: false,
        closed: false,
        next: None,
    };

    WaylandSource::new(conn.clone(), queue)
        .insert(events.handle())
        .map_err(|e| io::Error::other(e.to_string()))?;
    let (tx, rx) = channel::channel();
    events
        .handle()
        .insert_source(rx, |event, _, host: &mut Host| {
            if let channel::Event::Msg(at) = event {
                host.schedule(at);
            }
        })
        .map_err(|e| io::Error::other(e.to_string()))?;
    let ctx = egui::Context::default();
    ctx.set_request_repaint_callback(move |request| {
        if let Some(at) = Instant::now().checked_add(request.delay) {
            let _ = tx.send(at);
        }
    });
    app.attach(&ctx);
    let start = Instant::now();
    let mut graphics = None;
    while !host.closed {
        let timeout = host
            .next
            .map(|at| at.saturating_duration_since(Instant::now()));
        events
            .dispatch(timeout, &mut host)
            .map_err(io::Error::other)?;
        if host.next.is_none_or(|at| at > Instant::now()) {
            continue;
        }
        host.next = None;
        let logical = egui::vec2(host.size[0] as f32, host.size[1] as f32);
        let rect = egui::Rect::from_min_size(egui::Pos2::ZERO, logical);
        host.input.screen_rect = Some(rect);
        host.input.time = Some(start.elapsed().as_secs_f64());
        let view = host
            .input
            .viewports
            .entry(egui::ViewportId::ROOT)
            .or_default();
        view.native_pixels_per_point = Some(host.scale as f32);
        view.inner_rect = Some(rect);
        view.minimized = Some(host.hidden);
        let out = ctx.run(host.input.take(), |ctx| {
            app.process_controls(ctx);
            if host.configured && !host.hidden {
                app.draw(ctx);
            }
        });
        for cmd in &out.platform_output.commands {
            if let egui::OutputCommand::CopyText(text) = cmd {
                host.clipboard.store(text.clone());
            }
        }
        if !app.machine.desktop {
            return Ok(Exit::Standard);
        }
        for cmd in &out.viewport_output[&egui::ViewportId::ROOT].commands {
            match cmd {
                egui::ViewportCommand::Close => return Ok(Exit::Quit),
                egui::ViewportCommand::Visible(visible) if *visible == host.hidden => {
                    host.hidden = !*visible;
                    if host.hidden {
                        host.layer.wl_surface().attach(None, 0, 0);
                        host.layer.commit();
                        host.configured = false;
                    } else {
                        host.layer.set_anchor(Anchor::all());
                        host.layer.set_size(0, 0);
                        host.layer
                            .set_keyboard_interactivity(KeyboardInteractivity::OnDemand);
                        host.layer.set_exclusive_zone(-1);
                        host.layer.commit();
                    }
                }
                _ => {}
            }
        }
        if !host.configured || host.hidden {
            continue;
        }
        let size = host.size.map(|v| v.saturating_mul(host.scale as u32));
        if graphics.is_none() {
            graphics = Some(graphics::Graphics::new(
                &conn,
                host.layer.wl_surface(),
                size,
            )?);
        }
        graphics.as_mut().unwrap().present(size, &ctx, out)?;
    }
    Ok(Exit::Standard)
}

impl LayerShellHandler for Host {
    fn closed(&mut self, _: &Connection, _: &QueueHandle<Self>, _: &LayerSurface) {
        self.closed = true;
    }
    fn configure(
        &mut self,
        _: &Connection,
        _: &QueueHandle<Self>,
        _: &LayerSurface,
        conf: LayerSurfaceConfigure,
        _: u32,
    ) {
        self.size = [conf.new_size.0.max(1), conf.new_size.1.max(1)];
        self.configured = true;
        self.redraw();
    }
}
impl CompositorHandler for Host {
    fn scale_factor_changed(
        &mut self,
        _: &Connection,
        _: &QueueHandle<Self>,
        surface: &wl_surface::WlSurface,
        factor: i32,
    ) {
        if surface != self.layer.wl_surface() {
            return;
        }
        self.scale = factor.max(1);
        surface.set_buffer_scale(self.scale);
        self.redraw();
    }
    fn transform_changed(
        &mut self,
        _: &Connection,
        _: &QueueHandle<Self>,
        _: &wl_surface::WlSurface,
        _: wl_output::Transform,
    ) {
    }
    fn frame(&mut self, _: &Connection, _: &QueueHandle<Self>, _: &wl_surface::WlSurface, _: u32) {}
    fn surface_enter(
        &mut self,
        _: &Connection,
        _: &QueueHandle<Self>,
        _: &wl_surface::WlSurface,
        _: &wl_output::WlOutput,
    ) {
    }
    fn surface_leave(
        &mut self,
        _: &Connection,
        _: &QueueHandle<Self>,
        _: &wl_surface::WlSurface,
        _: &wl_output::WlOutput,
    ) {
    }
}
impl OutputHandler for Host {
    fn output_state(&mut self) -> &mut OutputState {
        &mut self.outputs
    }
    fn new_output(&mut self, _: &Connection, _: &QueueHandle<Self>, _: wl_output::WlOutput) {}
    fn update_output(&mut self, _: &Connection, _: &QueueHandle<Self>, _: wl_output::WlOutput) {}
    fn output_destroyed(&mut self, _: &Connection, _: &QueueHandle<Self>, _: wl_output::WlOutput) {}
}
impl ProvidesRegistryState for Host {
    fn registry(&mut self) -> &mut RegistryState {
        &mut self.registry
    }
    registry_handlers![OutputState, SeatState];
}
delegate_registry!(Host);
delegate_output!(Host);
delegate_compositor!(Host);
delegate_layer!(Host);

/// winit queues destruction of Wayland toplevels on its event loop. Dispatch
/// that queue before suspending the standard host for a background surface.
pub fn standard(
    events: &mut Option<winit::event_loop::EventLoop<eframe::UserEvent>>,
    options: eframe::NativeOptions,
    creator: eframe::AppCreator<'_>,
) -> eframe::Result {
    use winit::{event_loop::EventLoop, platform::run_on_demand::EventLoopExtRunOnDemand};
    if events.is_none() {
        *events = Some(EventLoop::with_user_event().build()?);
    }
    let events = events.as_mut().unwrap();
    let mut runner = eframe::create_native("corktop", options, creator, events);
    events.run_app_on_demand(&mut runner)?;
    drop(runner);
    struct Drain;
    impl winit::application::ApplicationHandler<eframe::UserEvent> for Drain {
        fn resumed(&mut self, _: &winit::event_loop::ActiveEventLoop) {}
        fn window_event(
            &mut self,
            _: &winit::event_loop::ActiveEventLoop,
            _: winit::window::WindowId,
            _: winit::event::WindowEvent,
        ) {
        }
        fn about_to_wait(&mut self, events: &winit::event_loop::ActiveEventLoop) {
            events.exit();
        }
    }
    events.run_app_on_demand(&mut Drain)?;
    Ok(())
}

impl smithay_client_toolkit::shm::ShmHandler for Host {
    fn shm_state(&mut self) -> &mut smithay_client_toolkit::shm::Shm {
        &mut self.shm
    }
}
smithay_client_toolkit::delegate_shm!(Host);
