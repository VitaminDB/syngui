use std::ffi::c_void;
use std::ptr::NonNull;
use std::sync::Arc;
use std::thread::JoinHandle;

use raw_window_handle::{HasDisplayHandle, RawDisplayHandle};
use smithay_client_toolkit::{
    data_device_manager::DataDeviceManagerState,
    output::OutputState,
    registry::RegistryState,
    seat::SeatState,
};
use wayland_backend::sys::client::Backend;
use wayland_client::{globals::registry_queue_init, Connection};
use winit::event_loop::EventLoopProxy;

use crate::app::user_event::MguiUserEvent;
use crate::window::Window;

mod state;
mod uri;

use state::DnDState;

/// Wayland гарантирует thread-safety для wl_display (он protected mutex'ом
struct WlDisplayPtr(NonNull<c_void>);
unsafe impl Send for WlDisplayPtr {}

pub fn try_start_wayland_dnd(
    window: Arc<Window>,
    proxy: EventLoopProxy<MguiUserEvent>,
) -> Option<JoinHandle<()>> {
    let display_ptr = match window.display_handle() {
        Ok(handle) => match handle.as_raw() {
            RawDisplayHandle::Wayland(w) => WlDisplayPtr(w.display),
            _ => return None,
        },
        Err(e) => {
            log::warn!("[wayland_dnd] window.display_handle() failed: {e}");
            return None;
        }
    };

    let window_keepalive = window;

    let handle = std::thread::Builder::new()
        .name("syngui-wayland-dnd".into())
        .spawn(move || {
            if let Err(e) = run_dispatch_loop(display_ptr, proxy) {
                log::warn!("[wayland_dnd] thread exited with error: {e:?}");
            }
            drop(window_keepalive);
        })
        .ok()?;
    Some(handle)
}

fn run_dispatch_loop(
    display_ptr: WlDisplayPtr,
    proxy: EventLoopProxy<MguiUserEvent>,
) -> Result<(), Box<dyn std::error::Error>> {
    // SAFETY: указатель wl_display получен из winit'овского окна и валиден
    let backend = unsafe { Backend::from_foreign_display(display_ptr.0.as_ptr().cast()) };
    let conn = Connection::from_backend(backend);

    let (globals, mut event_queue) = registry_queue_init::<DnDState>(&conn)?;
    let qh = event_queue.handle();

    let registry_state = RegistryState::new(&globals);
    let seat_state = SeatState::new(&globals, &qh);
    let output_state = OutputState::new(&globals, &qh);
    let data_device_manager = match DataDeviceManagerState::bind(&globals, &qh) {
        Ok(m) => m,
        Err(e) => {
            log::warn!("[wayland_dnd] wl_data_device_manager not available: {e}");
            return Ok(());
        }
    };

    let mut state = DnDState {
        registry_state,
        seat_state,
        output_state,
        data_device_manager,
        seats: Vec::new(),
        proxy,
        accept_counter: 0,
        exit: false,
    };

    event_queue.roundtrip(&mut state)?;

    while !state.exit {
        match event_queue.blocking_dispatch(&mut state) {
            Ok(_) => {}
            Err(e) => {
                log::debug!("[wayland_dnd] blocking_dispatch ended: {e}");
                break;
            }
        }
    }
    let _ = event_queue.roundtrip(&mut state);
    Ok(())
}
