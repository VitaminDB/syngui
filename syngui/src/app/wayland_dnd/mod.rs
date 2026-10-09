use std::ffi::c_void;
use std::ptr::NonNull;
use std::sync::{Arc, Mutex};
use std::thread::JoinHandle;

use raw_window_handle::{HasDisplayHandle, RawDisplayHandle};
use smithay_client_toolkit::{
    data_device_manager::DataDeviceManagerState, output::OutputState, registry::RegistryState,
    seat::SeatState,
};
use wayland_backend::sys::client::Backend;
use wayland_client::{globals::registry_queue_init, Connection};
use winit::event_loop::EventLoopProxy;

use crate::app::user_event::SynGuiUserEvent;
use crate::window::Window;

mod state;
pub(crate) mod uri;

use state::DnDState;
use smithay_client_toolkit::data_device_manager::data_source::DataSourceData;
use wayland_client::protocol::{
    wl_data_device::WlDataDevice, wl_data_device_manager::{DndAction, WlDataDeviceManager},
    wl_data_source::WlDataSource, wl_surface::WlSurface,
};
use wayland_client::QueueHandle;

/// Вытаскивание из окна наружу (источник DnD Wayland): объекты потока DnD, последнее нажатие
/// кнопки (serial и поверхность — `start_drag` требует serial неявного захвата) и текущий источник.
pub(crate) struct OsDrag {
    pub conn: Connection,
    pub qh: QueueHandle<DnDState>,
    pub manager: WlDataDeviceManager,
    pub devices: Vec<WlDataDevice>,
    pub press: Option<(u32, WlSurface)>,
    pub source: Option<(WlDataSource, String)>,
}

pub(crate) static OS_DRAG: Mutex<Option<OsDrag>> = Mutex::new(None);

/// Начать перетаскивание наружу (`text/uri-list`): курсор ушёл из окна во время внутреннего
/// переноса файлов. `false` — нет Wayland, нет нажатия или устройства.
pub fn start_os_drag(uri_list: &str) -> bool {
    let mut g = OS_DRAG.lock().unwrap_or_else(|e| e.into_inner());
    let Some(d) = g.as_mut() else { return false };
    let (Some((serial, surface)), Some(dev)) = (d.press.clone(), d.devices.first().cloned()) else {
        return false;
    };
    if let Some((old, _)) = d.source.take() {
        old.destroy();
    }
    let src = d.manager.create_data_source(&d.qh, DataSourceData::default());
    for m in ["text/uri-list", "text/plain;charset=utf-8", "text/plain"] {
        src.offer(m.to_string());
    }
    if wayland_client::Proxy::version(&d.manager) >= 3 {
        src.set_actions(DndAction::Copy | DndAction::Move);
    }
    dev.start_drag(Some(&src), &surface, None, serial);
    d.source = Some((src, uri_list.to_string()));
    let _ = d.conn.flush();
    log::info!("[wayland_dnd] перетаскивание наружу: {} байт", uri_list.len());
    true
}

/// Wayland гарантирует thread-safety для wl_display (он protected mutex'ом
struct WlDisplayPtr(NonNull<c_void>);
unsafe impl Send for WlDisplayPtr {}

pub fn try_start_wayland_dnd(
    window: Arc<Window>,
    proxy: EventLoopProxy<SynGuiUserEvent>,
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
    proxy: EventLoopProxy<SynGuiUserEvent>,
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

    *OS_DRAG.lock().unwrap_or_else(|e| e.into_inner()) = Some(OsDrag {
        conn: conn.clone(),
        qh: qh.clone(),
        manager: data_device_manager.data_device_manager().clone(),
        devices: Vec::new(),
        press: None,
        source: None,
    });
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
