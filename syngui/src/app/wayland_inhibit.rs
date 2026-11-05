//! Вся клавиатура — приложению: `zwp_keyboard_shortcuts_inhibit_manager_v1` на поверхности
//! окна (Alt+Tab, Super, Alt+F4 не уходят композитору — игры, виртуальные машины).
//! Своя очередь на том же `wl_display`, что у winit (как `wayland_pen`); запросы
//! отправляются сразу, события ингибитора (active/inactive) приложению не нужны.

use std::cell::RefCell;

use raw_window_handle::{HasDisplayHandle, HasWindowHandle, RawDisplayHandle, RawWindowHandle};
use wayland_backend::client::ObjectId;
use wayland_backend::sys::client::Backend;
use wayland_client::globals::{registry_queue_init, GlobalListContents};
use wayland_client::protocol::{wl_registry::WlRegistry, wl_seat::WlSeat, wl_surface::WlSurface};
use wayland_client::{Connection, Dispatch, EventQueue, Proxy, QueueHandle};
use wayland_protocols::wp::keyboard_shortcuts_inhibit::zv1::client::{
    zwp_keyboard_shortcuts_inhibit_manager_v1::ZwpKeyboardShortcutsInhibitManagerV1,
    zwp_keyboard_shortcuts_inhibitor_v1::ZwpKeyboardShortcutsInhibitorV1,
};

struct State;

struct Inhibit {
    conn: Connection,
    queue: EventQueue<State>,
    manager: ZwpKeyboardShortcutsInhibitManagerV1,
    seat: WlSeat,
    /// Поверхность (`wl_proxy*`) и ингибитор на ней.
    active: Option<(usize, ZwpKeyboardShortcutsInhibitorV1)>,
}

thread_local! {
    static INHIBIT: RefCell<Option<Inhibit>> = const { RefCell::new(None) };
}

fn connect(display: *mut std::ffi::c_void) -> Result<Inhibit, String> {
    // SAFETY: wl_display взят у окна winit и живёт весь процесс.
    let backend = unsafe { Backend::from_foreign_display(display.cast()) };
    let conn = Connection::from_backend(backend);
    let (globals, queue) = registry_queue_init::<State>(&conn).map_err(|e| format!("реестр Wayland: {e}"))?;
    let qh = queue.handle();
    let manager = globals
        .bind::<ZwpKeyboardShortcutsInhibitManagerV1, _, _>(&qh, 1..=1, ())
        .map_err(|_| "композитор без zwp_keyboard_shortcuts_inhibit_manager_v1".to_string())?;
    let seat = globals.bind::<WlSeat, _, _>(&qh, 1..=1, ()).map_err(|_| "нет wl_seat".to_string())?;
    Ok(Inhibit { conn, queue, manager, seat, active: None })
}

/// Включить/выключить перехват сочетаний для окна. Не Wayland — ошибка.
pub fn set(window: &winit::window::Window, on: bool) -> Result<(), String> {
    let display = match window.display_handle().map_err(|e| e.to_string())?.as_raw() {
        RawDisplayHandle::Wayland(d) => d.display.as_ptr(),
        _ => return Err("перехват сочетаний — только на Wayland".into()),
    };
    let surface_ptr = match window.window_handle().map_err(|e| e.to_string())?.as_raw() {
        RawWindowHandle::Wayland(w) => w.surface.as_ptr(),
        _ => return Err("нет wl_surface окна".into()),
    };
    INHIBIT.with(|cell| {
        let mut slot = cell.borrow_mut();
        if slot.is_none() {
            if !on {
                return Ok(());
            }
            *slot = Some(connect(display)?);
        }
        let st = slot.as_mut().unwrap();
        let _ = st.queue.dispatch_pending(&mut State);
        if let Some((_, inh)) = st.active.take() {
            inh.destroy();
        }
        if on {
            // SAFETY: wl_surface окна winit живёт, пока живо окно (ингибитор снимается раньше).
            let id = unsafe { ObjectId::from_ptr(WlSurface::interface(), surface_ptr.cast()) }
                .map_err(|e| format!("wl_surface: {e:?}"))?;
            let surface = WlSurface::from_id(&st.conn, id).map_err(|e| format!("wl_surface: {e:?}"))?;
            let qh = st.queue.handle();
            let inh = st.manager.inhibit_shortcuts(&surface, &st.seat, &qh, ());
            st.active = Some((surface_ptr as usize, inh));
        }
        st.conn.flush().map_err(|e| format!("wayland flush: {e}"))
    })
}

/// Перехват включён.
pub fn active() -> bool {
    INHIBIT.with(|c| c.borrow().as_ref().is_some_and(|s| s.active.is_some()))
}

impl Dispatch<WlRegistry, GlobalListContents> for State {
    fn event(_: &mut Self, _: &WlRegistry, _: <WlRegistry as Proxy>::Event, _: &GlobalListContents, _: &Connection, _: &QueueHandle<Self>) {}
}
impl Dispatch<WlSeat, ()> for State {
    fn event(_: &mut Self, _: &WlSeat, _: <WlSeat as Proxy>::Event, _: &(), _: &Connection, _: &QueueHandle<Self>) {}
}
impl Dispatch<ZwpKeyboardShortcutsInhibitManagerV1, ()> for State {
    fn event(_: &mut Self, _: &ZwpKeyboardShortcutsInhibitManagerV1, _: <ZwpKeyboardShortcutsInhibitManagerV1 as Proxy>::Event, _: &(), _: &Connection, _: &QueueHandle<Self>) {}
}
impl Dispatch<ZwpKeyboardShortcutsInhibitorV1, ()> for State {
    fn event(_: &mut Self, _: &ZwpKeyboardShortcutsInhibitorV1, _: <ZwpKeyboardShortcutsInhibitorV1 as Proxy>::Event, _: &(), _: &Connection, _: &QueueHandle<Self>) {}
}
