use std::io::Read;

use smithay_client_toolkit::{
    data_device_manager::{
        data_device::{DataDevice, DataDeviceHandler},
        data_offer::{DataOfferHandler, DragOffer},
        data_source::DataSourceHandler,
        DataDeviceManagerState, WritePipe,
    },
    delegate_data_device, delegate_registry, delegate_seat,
    output::{OutputHandler, OutputState},
    registry::{ProvidesRegistryState, RegistryState},
    registry_handlers,
    seat::{Capability, SeatHandler, SeatState},
};
use wayland_client::{
    protocol::{
        wl_data_device::WlDataDevice, wl_data_device_manager::DndAction,
        wl_data_source::WlDataSource, wl_output::WlOutput, wl_seat::WlSeat,
        wl_surface::WlSurface,
    },
    Connection, QueueHandle,
};
use winit::event_loop::EventLoopProxy;

use crate::app::user_event::{MguiUserEvent, WaylandDndEvent};

use super::uri::parse_uri_list;

const PREFERRED_MIMES: &[&str] = &[
    "text/uri-list",
    "application/vnd.portal.filetransfer",
    "text/x-moz-url",
];

fn pick_mime(mimes: &[String]) -> Option<String> {
    for preferred in PREFERRED_MIMES {
        for got in mimes {
            if got == preferred {
                return Some(got.clone());
            }
        }
    }
    None
}

pub(super) struct DnDState {
    pub registry_state: RegistryState,
    pub seat_state: SeatState,
    pub output_state: OutputState,
    pub data_device_manager: DataDeviceManagerState,
    pub seats: Vec<SeatEntry>,
    pub proxy: EventLoopProxy<MguiUserEvent>,
    pub accept_counter: u32,
    pub exit: bool,
}

pub(super) struct SeatEntry {
    pub seat: WlSeat,
    pub data_device: DataDevice,
}

impl DnDState {
    fn negotiate_offer(offer: &DragOffer, accept_counter: u32) -> Option<String> {
        let mime = offer.with_mime_types(|mimes| pick_mime(mimes))?;
        offer.accept_mime_type(accept_counter, Some(mime.clone()));
        offer.set_actions(DndAction::Copy, DndAction::Copy);
        Some(mime)
    }
}

impl DataDeviceHandler for DnDState {
    fn enter(
        &mut self,
        _conn: &Connection,
        _qh: &QueueHandle<Self>,
        wl_data_device: &WlDataDevice,
        x: f64,
        y: f64,
        _surface: &WlSurface,
    ) {
        let Some(seat_entry) = self.seats.iter().find(|s| s.data_device.inner() == wl_data_device)
        else {
            return;
        };
        let Some(offer) = seat_entry.data_device.data().drag_offer() else {
            return;
        };
        self.accept_counter = self.accept_counter.wrapping_add(1);
        if Self::negotiate_offer(&offer, self.accept_counter).is_none() {
            offer.accept_mime_type(self.accept_counter, None);
            return;
        }
        let _ = self.proxy.send_event(MguiUserEvent::WaylandDnd(
            WaylandDndEvent::Enter { x: x as f32, y: y as f32 },
        ));
    }

    fn leave(&mut self, _conn: &Connection, _qh: &QueueHandle<Self>, _wl_dd: &WlDataDevice) {
        if self.proxy.send_event(MguiUserEvent::WaylandDnd(WaylandDndEvent::Leave)).is_err() {
            self.exit = true;
        }
    }

    fn motion(
        &mut self,
        _conn: &Connection,
        _qh: &QueueHandle<Self>,
        _wl_dd: &WlDataDevice,
        x: f64,
        y: f64,
    ) {
        if self
            .proxy
            .send_event(MguiUserEvent::WaylandDnd(WaylandDndEvent::Motion {
                x: x as f32,
                y: y as f32,
            }))
            .is_err()
        {
            self.exit = true;
        }
    }

    fn selection(&mut self, _conn: &Connection, _qh: &QueueHandle<Self>, _wl_dd: &WlDataDevice) {
    }

    fn drop_performed(
        &mut self,
        _conn: &Connection,
        _qh: &QueueHandle<Self>,
        wl_data_device: &WlDataDevice,
    ) {
        let Some(seat_entry) = self.seats.iter().find(|s| s.data_device.inner() == wl_data_device)
        else {
            return;
        };
        let Some(offer) = seat_entry.data_device.data().drag_offer() else {
            return;
        };
        let Some(mime) = offer.with_mime_types(|m| pick_mime(m)) else {
            offer.destroy();
            return;
        };
        let pipe = match offer.receive(mime) {
            Ok(p) => p,
            Err(e) => {
                log::warn!("[wayland_dnd] DataOffer::receive failed: {e}");
                offer.destroy();
                return;
            }
        };
        let (x, y) = (offer.x as f32, offer.y as f32);
        let proxy = self.proxy.clone();
        std::thread::spawn(move || {
            let mut pipe = pipe;
            let mut buf = Vec::with_capacity(4096);
            if let Err(e) = pipe.read_to_end(&mut buf) {
                log::warn!("[wayland_dnd] read uri-list failed: {e}");
                offer.finish();
                offer.destroy();
                return;
            }
            offer.finish();
            offer.destroy();
            let text = String::from_utf8_lossy(&buf);
            let paths = parse_uri_list(&text);
            if paths.is_empty() {
                return;
            }
            let _ = proxy.send_event(MguiUserEvent::WaylandDnd(WaylandDndEvent::Drop {
                x,
                y,
                paths,
            }));
        });
    }
}

impl DataSourceHandler for DnDState {
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
    fn action(
        &mut self,
        _: &Connection,
        _: &QueueHandle<Self>,
        _: &WlDataSource,
        _: DndAction,
    ) {
    }
}

impl DataOfferHandler for DnDState {
    fn source_actions(
        &mut self,
        _conn: &Connection,
        _qh: &QueueHandle<Self>,
        offer: &mut DragOffer,
        _actions: DndAction,
    ) {
        offer.set_actions(DndAction::Copy, DndAction::Copy);
    }

    fn selected_action(
        &mut self,
        _conn: &Connection,
        _qh: &QueueHandle<Self>,
        _offer: &mut DragOffer,
        _actions: DndAction,
    ) {
    }
}

impl SeatHandler for DnDState {
    fn seat_state(&mut self) -> &mut SeatState {
        &mut self.seat_state
    }

    fn new_seat(&mut self, _: &Connection, _qh: &QueueHandle<Self>, _seat: WlSeat) {}

    fn new_capability(
        &mut self,
        _conn: &Connection,
        qh: &QueueHandle<Self>,
        seat: WlSeat,
        capability: Capability,
    ) {
        if matches!(capability, Capability::Pointer | Capability::Touch)
            && !self.seats.iter().any(|s| s.seat == seat)
        {
            let data_device = self.data_device_manager.get_data_device(qh, &seat);
            self.seats.push(SeatEntry { seat, data_device });
        }
    }

    fn remove_capability(
        &mut self,
        _conn: &Connection,
        _qh: &QueueHandle<Self>,
        _seat: WlSeat,
        _capability: Capability,
    ) {
    }

    fn remove_seat(&mut self, _: &Connection, _: &QueueHandle<Self>, seat: WlSeat) {
        self.seats.retain(|s| s.seat != seat);
    }
}

impl OutputHandler for DnDState {
    fn output_state(&mut self) -> &mut OutputState {
        &mut self.output_state
    }
    fn new_output(&mut self, _: &Connection, _: &QueueHandle<Self>, _: WlOutput) {}
    fn update_output(&mut self, _: &Connection, _: &QueueHandle<Self>, _: WlOutput) {}
    fn output_destroyed(&mut self, _: &Connection, _: &QueueHandle<Self>, _: WlOutput) {}
}

impl ProvidesRegistryState for DnDState {
    fn registry(&mut self) -> &mut RegistryState {
        &mut self.registry_state
    }
    registry_handlers![SeatState, OutputState];
}

delegate_seat!(DnDState);
delegate_data_device!(DnDState);
delegate_registry!(DnDState);
smithay_client_toolkit::delegate_output!(DnDState);
