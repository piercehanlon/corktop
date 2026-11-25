use super::Host;
use eframe::egui;
use smithay_client_toolkit::{
    data_device_manager::{
        DataDeviceManagerState, WritePipe,
        data_device::{DataDevice, DataDeviceData, DataDeviceHandler},
        data_offer::{DataOfferHandler, DragOffer},
        data_source::DataSourceHandler,
    },
    delegate_data_device,
    reexports::calloop::PostAction,
    shell::WaylandSurface,
};
use std::{
    collections::VecDeque,
    io::{self, Read},
};
use wayland_client::{
    Connection, Proxy, QueueHandle,
    globals::GlobalList,
    protocol::{
        wl_data_device::WlDataDevice, wl_data_device_manager::DndAction,
        wl_data_source::WlDataSource, wl_seat::WlSeat, wl_surface::WlSurface,
    },
};

const URI_LIST: &str = "text/uri-list";
struct Drop {
    files: Vec<egui::DroppedFile>,
    pos: egui::Pos2,
    modifiers: egui::Modifiers,
}
pub(super) struct DropTarget {
    manager: Option<DataDeviceManagerState>,
    devices: Vec<DataDevice>,
    pending: VecDeque<Drop>,
}
impl DropTarget {
    pub fn bind(globals: &GlobalList, qh: &QueueHandle<Host>) -> Self {
        Self {
            manager: DataDeviceManagerState::bind(globals, qh).ok(),
            devices: vec![],
            pending: VecDeque::new(),
        }
    }
    pub fn add_seat(&mut self, qh: &QueueHandle<Host>, seat: &WlSeat) {
        if let Some(manager) = &self.manager {
            self.devices.push(manager.get_data_device(qh, seat));
        }
    }
    pub fn remove_seat(&mut self, seat: &WlSeat) {
        self.devices.retain(|d| d.data().seat() != seat);
    }
    pub fn deliver(&mut self, input: &mut egui::RawInput) -> bool {
        if let Some(drop) = self.pending.pop_front() {
            input.events.push(egui::Event::PointerMoved(drop.pos));
            input.modifiers = drop.modifiers;
            input.dropped_files = drop.files;
        }
        !self.pending.is_empty()
    }
}

fn offer(device: &WlDataDevice) -> Option<DragOffer> {
    device.data::<DataDeviceData>()?.drag_offer()
}
fn files(bytes: &[u8]) -> Vec<egui::DroppedFile> {
    std::str::from_utf8(bytes)
        .unwrap_or_default()
        .lines()
        .filter(|l| !l.starts_with('#'))
        .filter_map(|line| url::Url::parse(line).ok()?.to_file_path().ok())
        .map(|path| egui::DroppedFile {
            path: Some(path),
            ..Default::default()
        })
        .collect()
}

impl DataDeviceHandler for Host {
    fn enter(
        &mut self,
        _: &Connection,
        _: &QueueHandle<Self>,
        device: &WlDataDevice,
        x: f64,
        y: f64,
        surface: &WlSurface,
    ) {
        let Some(offer) = offer(device) else { return };
        if surface != self.layer.wl_surface()
            || !offer.with_mime_types(|m| m.iter().any(|s| s == URI_LIST))
        {
            offer.accept_mime_type(offer.serial, None);
            return;
        }
        offer.accept_mime_type(offer.serial, Some(URI_LIST.into()));
        // corktop owns filesystem mutations. A source-side move could delete a
        // file a second time or remove it after an unsuccessful import.
        offer.set_actions(DndAction::Copy, DndAction::Copy);
        self.input.hovered_files = vec![egui::HoveredFile {
            mime: URI_LIST.into(),
            ..Default::default()
        }];
        self.input
            .events
            .push(egui::Event::PointerMoved(egui::pos2(x as f32, y as f32)));
        self.redraw();
    }
    fn motion(&mut self, _: &Connection, _: &QueueHandle<Self>, _: &WlDataDevice, x: f64, y: f64) {
        if !self.input.hovered_files.is_empty() {
            self.input
                .events
                .push(egui::Event::PointerMoved(egui::pos2(x as f32, y as f32)));
            self.redraw();
        }
    }
    fn leave(&mut self, _: &Connection, _: &QueueHandle<Self>, _: &WlDataDevice) {
        self.input.hovered_files.clear();
        self.redraw();
    }
    fn selection(&mut self, _: &Connection, _: &QueueHandle<Self>, _: &WlDataDevice) {}
    fn drop_performed(&mut self, _: &Connection, _: &QueueHandle<Self>, device: &WlDataDevice) {
        let Some(offer) = offer(device) else { return };
        self.input.hovered_files.clear();
        self.redraw();
        let Ok(pipe) = offer.receive(URI_LIST.into()) else {
            offer.destroy();
            return;
        };
        let pos = egui::pos2(offer.x as f32, offer.y as f32);
        let modifiers = self.input.modifiers;
        let mut bytes = Vec::new();
        let transfer = offer.clone();
        let registered = self.handle.insert_source(pipe, move |_, file, state| {
            let mut chunk = [0; 8192];
            let mut reader = &**file;
            match reader.read(&mut chunk) {
                Ok(0) => {
                    let files = files(&bytes);
                    if !files.is_empty() {
                        state.drops.pending.push_back(Drop {
                            files,
                            pos,
                            modifiers,
                        });
                        state.redraw();
                        transfer.finish();
                    }
                }
                Ok(n) if bytes.len() + n <= 1024 * 1024 => {
                    bytes.extend_from_slice(&chunk[..n]);
                    return PostAction::Continue;
                }
                Err(e)
                    if matches!(
                        e.kind(),
                        io::ErrorKind::Interrupted | io::ErrorKind::WouldBlock
                    ) =>
                {
                    return PostAction::Continue;
                }
                _ => {}
            }
            transfer.destroy();
            PostAction::Remove
        });
        if registered.is_err() {
            offer.destroy();
        }
    }
}
impl DataOfferHandler for Host {
    fn source_actions(
        &mut self,
        _: &Connection,
        _: &QueueHandle<Self>,
        _: &mut DragOffer,
        _: DndAction,
    ) {
    }
    fn selected_action(
        &mut self,
        _: &Connection,
        _: &QueueHandle<Self>,
        _: &mut DragOffer,
        _: DndAction,
    ) {
    }
}
// This data device receives drops. Clipboard ownership uses smithay-clipboard.
impl DataSourceHandler for Host {
    fn accept_mime(
        &mut self,
        _: &Connection,
        _: &QueueHandle<Self>,
        _: &WlDataSource,
        _: Option<String>,
    ) {
    }
    fn send_request(
        &mut self,
        _: &Connection,
        _: &QueueHandle<Self>,
        _: &WlDataSource,
        _: String,
        _: WritePipe,
    ) {
    }
    fn cancelled(&mut self, _: &Connection, _: &QueueHandle<Self>, _: &WlDataSource) {}
    fn dnd_dropped(&mut self, _: &Connection, _: &QueueHandle<Self>, _: &WlDataSource) {}
    fn dnd_finished(&mut self, _: &Connection, _: &QueueHandle<Self>, _: &WlDataSource) {}
    fn action(&mut self, _: &Connection, _: &QueueHandle<Self>, _: &WlDataSource, _: DndAction) {}
}
delegate_data_device!(Host);
