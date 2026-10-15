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

mod icon;
mod state;
pub(crate) mod uri;

use smithay_client_toolkit::data_device_manager::data_source::DataSourceData;
use state::DnDState;
use wayland_client::protocol::{
    wl_buffer::WlBuffer,
    wl_compositor::WlCompositor,
    wl_data_device::WlDataDevice,
    wl_data_device_manager::{DndAction, WlDataDeviceManager},
    wl_data_source::WlDataSource,
    wl_shm::{self, WlShm},
    wl_surface::WlSurface,
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
    /// Последнее касание: serial, сиденье и пальцы, что сейчас прижаты.
    pub touch: Option<TouchPress>,
    pub source: Option<(WlDataSource, String)>,
    pub compositor: Option<WlCompositor>,
    pub shm: Option<WlShm>,
    /// Значок текущего перетаскивания: поверхность, вид, масштаб буфера и
    /// последние буферы (живут, пока композитор может их читать).
    pub icon: Option<DragIcon>,
    /// Своё перетаскивание сейчас над своим окном: принятие решает дерево.
    pub over_own: bool,
}

pub(crate) struct DragIcon {
    surface: WlSurface,
    look: icon::Look,
    scale: i32,
    buffers: Vec<WlBuffer>,
}

impl OsDrag {
    /// Нарисовать значок в новый буфер и показать его.
    fn paint_icon(&mut self, look: icon::Look, first: bool) {
        let (Some(shm), Some(ic)) = (self.shm.clone(), self.icon.as_mut()) else {
            return;
        };
        let (w, h, px) = icon::render(look, ic.scale);
        let Some(buffer) = shm_buffer(&shm, &self.qh, w, h, &px) else {
            return;
        };
        // Смещение задаёт положение значка относительно курсора (копится).
        let (dx, dy) = if first { icon::OFFSET } else { (0, 0) };
        // С wl_surface v5 смещение — отдельным запросом, attach — только (0, 0).
        if wayland_client::Proxy::version(&ic.surface) >= 5 {
            ic.surface.attach(Some(&buffer), 0, 0);
            if (dx, dy) != (0, 0) {
                ic.surface.offset(dx, dy);
            }
        } else {
            ic.surface.attach(Some(&buffer), dx, dy);
        }
        ic.surface.set_buffer_scale(ic.scale);
        ic.surface.damage_buffer(0, 0, w, h);
        ic.surface.commit();
        ic.look = look;
        ic.buffers.push(buffer);
        if ic.buffers.len() > 3 {
            ic.buffers.remove(0).destroy();
        }
        let _ = self.conn.flush();
    }
}

/// Буфер wl_shm из памяти memfd.
fn shm_buffer(
    shm: &WlShm,
    qh: &QueueHandle<DnDState>,
    w: i32,
    h: i32,
    px: &[u8],
) -> Option<WlBuffer> {
    use std::io::Write;
    use std::os::fd::{AsFd, FromRawFd};
    let fd = unsafe { libc::memfd_create(c"syngui-dnd-icon".as_ptr(), libc::MFD_CLOEXEC) };
    if fd < 0 {
        return None;
    }
    let mut file = unsafe { std::fs::File::from_raw_fd(fd) };
    file.write_all(px).ok()?;
    let pool = shm.create_pool(file.as_fd(), px.len() as i32, qh, ());
    let buffer = pool.create_buffer(0, w, h, w * 4, wl_shm::Format::Argb8888, qh, ());
    pool.destroy();
    Some(buffer)
}

/// Над целью, которая примет брошенное (`true`), значок крупнее, с рамкой и «+».
pub fn set_os_drag_accepted(accepted: bool) {
    let mut g = OS_DRAG.lock().unwrap_or_else(|e| e.into_inner());
    let Some(d) = g.as_mut() else { return };
    let Some(look) = d.icon.as_ref().map(|i| i.look) else {
        return;
    };
    if look.accepted != accepted {
        d.paint_icon(icon::Look { accepted, ..look }, false);
    }
}

/// Идёт ли сейчас своё перетаскивание наружу.
pub fn os_drag_active() -> bool {
    OS_DRAG
        .lock()
        .unwrap_or_else(|e| e.into_inner())
        .as_ref()
        .is_some_and(|d| d.source.is_some())
}

pub(crate) static OS_DRAG: Mutex<Option<OsDrag>> = Mutex::new(None);

pub(crate) struct TouchPress {
    pub serial: u32,
    pub seat: wayland_client::protocol::wl_seat::WlSeat,
    pub fingers: Vec<i32>,
}

/// Окно тянут пальцем (свой заголовок или край): `xdg_toplevel.move`/`resize`
/// с serial прижатого пальца. winit (`drag_window`, `drag_resize_window`)
/// шлёт их только от указателя, вошедшего в окно, — с касанием запрос не
/// уходил вовсе. `false` — палец не прижат или нет Wayland: тогда путь winit.
pub fn start_touch_window_op(
    window: &winit::window::Window,
    resize: Option<winit::window::ResizeDirection>,
) -> bool {
    use smithay_client_toolkit::reexports::protocols::xdg::shell::client::xdg_toplevel::{
        ResizeEdge, XdgToplevel,
    };
    use wayland_backend::client::ObjectId;
    use wayland_client::Proxy;
    use winit::platform::wayland::WindowExtWayland;
    use winit::window::ResizeDirection as D;

    let g = OS_DRAG.lock().unwrap_or_else(|e| e.into_inner());
    let Some(d) = g.as_ref() else { return false };
    let Some(t) = d.touch.as_ref().filter(|t| !t.fingers.is_empty()) else {
        return false;
    };
    let Some(ptr) = window.xdg_toplevel() else {
        return false;
    };
    // SAFETY: xdg_toplevel живого winit-окна на том же wl_display, поверх
    // которого построено соединение DnD.
    let Ok(id) = (unsafe { ObjectId::from_ptr(XdgToplevel::interface(), ptr.as_ptr().cast()) })
    else {
        return false;
    };
    let Ok(toplevel) = XdgToplevel::from_id(&d.conn, id) else {
        return false;
    };
    match resize {
        None => toplevel._move(&t.seat, t.serial),
        Some(dir) => toplevel.resize(
            &t.seat,
            t.serial,
            match dir {
                D::North => ResizeEdge::Top,
                D::South => ResizeEdge::Bottom,
                D::East => ResizeEdge::Right,
                D::West => ResizeEdge::Left,
                D::NorthEast => ResizeEdge::TopRight,
                D::NorthWest => ResizeEdge::TopLeft,
                D::SouthEast => ResizeEdge::BottomRight,
                D::SouthWest => ResizeEdge::BottomLeft,
            },
        ),
    }
    let _ = d.conn.flush();
    true
}

/// Начать перетаскивание наружу (`text/uri-list`): курсор ушёл из окна во время внутреннего
/// переноса файлов. `false` — нет Wayland, нет нажатия или устройства.
pub fn start_os_drag(uri_list: &str, multi: bool, scale: f64) -> bool {
    let mut g = OS_DRAG.lock().unwrap_or_else(|e| e.into_inner());
    let Some(d) = g.as_mut() else { return false };
    let (Some((serial, surface)), Some(dev)) = (d.press.clone(), d.devices.first().cloned()) else {
        return false;
    };
    if let Some((old, _)) = d.source.take() {
        old.destroy();
    }
    let src = d
        .manager
        .create_data_source(&d.qh, DataSourceData::default());
    for m in ["text/uri-list", "text/plain;charset=utf-8", "text/plain"] {
        src.offer(m.to_string());
    }
    if wayland_client::Proxy::version(&d.manager) >= 3 {
        src.set_actions(DndAction::Copy | DndAction::Move);
    }
    if let Some(old) = d.icon.take() {
        for b in old.buffers {
            b.destroy();
        }
        old.surface.destroy();
    }
    if let Some(comp) = d.compositor.clone() {
        let surface = comp.create_surface(&d.qh, ());
        d.icon = Some(DragIcon {
            surface,
            look: icon::Look {
                accepted: false,
                multi,
            },
            scale: scale.ceil().max(1.0) as i32,
            buffers: Vec::new(),
        });
        d.paint_icon(
            icon::Look {
                accepted: false,
                multi,
            },
            true,
        );
    }
    d.over_own = false;
    let icon_surface = d.icon.as_ref().map(|i| i.surface.clone());
    dev.start_drag(Some(&src), &surface, icon_surface.as_ref(), serial);
    d.source = Some((src, uri_list.to_string()));
    let _ = d.conn.flush();
    log::info!(
        "[wayland_dnd] перетаскивание наружу: {} байт",
        uri_list.len()
    );
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
        touch: None,
        source: None,
        compositor: globals.bind::<WlCompositor, _, _>(&qh, 1..=5, ()).ok(),
        shm: globals.bind::<WlShm, _, _>(&qh, 1..=1, ()).ok(),
        icon: None,
        over_own: false,
    });
    let mut state = DnDState {
        registry_state,
        seat_state,
        output_state,
        data_device_manager,
        seats: Vec::new(),
        touches: Vec::new(),
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
