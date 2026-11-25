use super::Host;
use eframe::egui::{self, Event, Key, PointerButton};
use smithay_client_toolkit::{
    delegate_keyboard, delegate_pointer, delegate_seat,
    seat::{
        Capability, SeatHandler, SeatState,
        keyboard::{KeyEvent, KeyboardHandler, Keysym, Modifiers},
        pointer::{PointerEvent, PointerEventKind, PointerHandler},
    },
};
use wayland_client::{
    Connection, QueueHandle,
    protocol::{
        wl_keyboard::WlKeyboard, wl_pointer::WlPointer, wl_seat::WlSeat, wl_surface::WlSurface,
    },
};

impl SeatHandler for Host {
    fn seat_state(&mut self) -> &mut SeatState {
        &mut self.seats
    }
    fn new_seat(&mut self, _: &Connection, qh: &QueueHandle<Self>, seat: WlSeat) {
        self.drops.add_seat(qh, &seat);
    }
    fn remove_seat(&mut self, _: &Connection, _: &QueueHandle<Self>, seat: WlSeat) {
        self.drops.remove_seat(&seat);
    }
    fn new_capability(
        &mut self,
        _: &Connection,
        qh: &QueueHandle<Self>,
        seat: WlSeat,
        cap: Capability,
    ) {
        match cap {
            Capability::Keyboard if self.keyboard.is_none() => {
                self.keyboard = self
                    .seats
                    .get_keyboard_with_repeat(
                        qh,
                        &seat,
                        None,
                        self.handle.clone(),
                        Box::new(|host, _, key| host.key(key, true, true)),
                    )
                    .ok()
            }
            Capability::Pointer if self.pointer.is_none() => {
                self.pointer = self
                    .seats
                    .get_pointer_with_theme(
                        qh,
                        &seat,
                        self.shm.wl_shm(),
                        self.compositor.create_surface(qh),
                        Default::default(),
                    )
                    .ok()
            }
            _ => {}
        }
    }
    fn remove_capability(
        &mut self,
        _: &Connection,
        _: &QueueHandle<Self>,
        _: WlSeat,
        cap: Capability,
    ) {
        match cap {
            Capability::Keyboard => {
                if let Some(k) = self.keyboard.take() {
                    k.release();
                }
            }
            Capability::Pointer => {
                self.pointer.take();
            }
            _ => {}
        }
    }
}
impl KeyboardHandler for Host {
    fn enter(
        &mut self,
        _: &Connection,
        _: &QueueHandle<Self>,
        _: &WlKeyboard,
        _: &WlSurface,
        _: u32,
        _: &[u32],
        _: &[Keysym],
    ) {
        self.input.focused = true;
        self.input.events.push(Event::WindowFocused(true));
        self.redraw();
    }
    fn leave(
        &mut self,
        _: &Connection,
        _: &QueueHandle<Self>,
        _: &WlKeyboard,
        _: &WlSurface,
        _: u32,
    ) {
        self.input.focused = false;
        self.input.events.push(Event::WindowFocused(false));
        self.redraw();
    }
    fn press_key(
        &mut self,
        _: &Connection,
        _: &QueueHandle<Self>,
        _: &WlKeyboard,
        _: u32,
        key: KeyEvent,
    ) {
        self.key(key, true, false);
    }
    fn release_key(
        &mut self,
        _: &Connection,
        _: &QueueHandle<Self>,
        _: &WlKeyboard,
        _: u32,
        key: KeyEvent,
    ) {
        self.key(key, false, false);
    }
    fn update_modifiers(
        &mut self,
        _: &Connection,
        _: &QueueHandle<Self>,
        _: &WlKeyboard,
        _: u32,
        m: Modifiers,
        _: u32,
    ) {
        self.input.modifiers = egui::Modifiers {
            ctrl: m.ctrl,
            command: m.ctrl,
            shift: m.shift,
            alt: m.alt,
            mac_cmd: false,
        };
        self.redraw();
    }
}
impl Host {
    fn key(&mut self, event: KeyEvent, pressed: bool, repeat: bool) {
        let key = match event.keysym {
            Keysym::Escape => Some(Key::Escape),
            Keysym::Return | Keysym::KP_Enter => Some(Key::Enter),
            Keysym::BackSpace => Some(Key::Backspace),
            Keysym::Delete => Some(Key::Delete),
            Keysym::Tab | Keysym::ISO_Left_Tab => Some(Key::Tab),
            Keysym::Up => Some(Key::ArrowUp),
            Keysym::Down => Some(Key::ArrowDown),
            Keysym::Left => Some(Key::ArrowLeft),
            Keysym::Right => Some(Key::ArrowRight),
            Keysym::Home => Some(Key::Home),
            Keysym::End => Some(Key::End),
            Keysym::Page_Up => Some(Key::PageUp),
            Keysym::Page_Down => Some(Key::PageDown),
            k => k
                .key_char()
                .and_then(|c| Key::from_name(&c.to_ascii_uppercase().to_string())),
        };
        if pressed && self.input.modifiers.command {
            match key {
                Some(Key::C) => self.input.events.push(Event::Copy),
                Some(Key::X) => self.input.events.push(Event::Cut),
                Some(Key::V) => {
                    if let Ok(text) = self.clipboard.load()
                        && !text.is_empty()
                    {
                        self.input.events.push(Event::Paste(text));
                    }
                    // The board checks for a bitmap on release when no text was offered.
                    self.redraw();
                    return;
                }
                _ => {}
            }
        }
        if let Some(key) = key {
            self.input.events.push(Event::Key {
                key,
                physical_key: None,
                pressed,
                repeat,
                modifiers: self.input.modifiers,
            });
        }
        if pressed
            && !self.input.modifiers.command
            && let Some(text) = event.utf8
            && !text.chars().any(char::is_control)
        {
            self.input.events.push(Event::Text(text));
        }
        self.redraw();
    }
}
impl PointerHandler for Host {
    fn pointer_frame(
        &mut self,
        conn: &Connection,
        _: &QueueHandle<Self>,
        _: &WlPointer,
        events: &[PointerEvent],
    ) {
        for e in events {
            if matches!(e.kind, PointerEventKind::Enter { .. })
                && let Some(pointer) = &self.pointer
            {
                let _ = pointer.set_cursor(
                    conn,
                    smithay_client_toolkit::seat::pointer::CursorIcon::Default,
                );
            }

            let pos = egui::pos2(e.position.0 as f32, e.position.1 as f32);
            let event = match e.kind {
                PointerEventKind::Enter { .. } | PointerEventKind::Motion { .. } => {
                    Event::PointerMoved(pos)
                }
                PointerEventKind::Leave { .. } => Event::PointerGone,
                PointerEventKind::Press { button, .. }
                | PointerEventKind::Release { button, .. } => {
                    let button = match button {
                        0x110 => PointerButton::Primary,
                        0x111 => PointerButton::Secondary,
                        0x112 => PointerButton::Middle,
                        _ => continue,
                    };
                    Event::PointerButton {
                        pos,
                        button,
                        pressed: matches!(e.kind, PointerEventKind::Press { .. }),
                        modifiers: self.input.modifiers,
                    }
                }
                PointerEventKind::Axis {
                    horizontal,
                    vertical,
                    ..
                } => Event::MouseWheel {
                    unit: egui::MouseWheelUnit::Point,
                    delta: egui::vec2(-horizontal.absolute as f32, -vertical.absolute as f32),
                    modifiers: self.input.modifiers,
                },
            };
            self.input.events.push(event);
            self.redraw();
        }
    }
}
delegate_seat!(Host);
delegate_keyboard!(Host);
delegate_pointer!(Host);
