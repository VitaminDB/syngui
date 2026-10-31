//! Перо на Wayland: `zwp_tablet_v2` на своей очереди того же `wl_display`
//! (как `wayland_dnd`). Композитор, видя у клиента инструмент tablet-v2,
//! шлёт перо родными событиями (synwm — маршрут `Native`), а не эмуляцией
//! указателя; здесь они собираются по `frame` в отсчёты и уходят в цикл
//! событий (`SynGuiUserEvent::Pen`), где окно находится по `wl_surface`.

use std::ffi::c_void;
use std::ptr::NonNull;

use raw_window_handle::{HasDisplayHandle, RawDisplayHandle};
use wayland_backend::sys::client::Backend;
use wayland_client::globals::{registry_queue_init, GlobalListContents};
use wayland_client::protocol::{wl_registry::WlRegistry, wl_seat::WlSeat, wl_surface::WlSurface};
use wayland_client::{event_created_child, Connection, Dispatch, Proxy, QueueHandle, WEnum};
use wayland_protocols::wp::tablet::zv2::client::{
    zwp_tablet_manager_v2::ZwpTabletManagerV2,
    zwp_tablet_pad_group_v2::{self, ZwpTabletPadGroupV2},
    zwp_tablet_pad_ring_v2::ZwpTabletPadRingV2,
    zwp_tablet_pad_strip_v2::ZwpTabletPadStripV2,
    zwp_tablet_pad_v2::{self, ZwpTabletPadV2},
    zwp_tablet_seat_v2::{self, ZwpTabletSeatV2},
    zwp_tablet_tool_v2::{self, ZwpTabletToolV2},
    zwp_tablet_v2::ZwpTabletV2,
};
use winit::event_loop::EventLoopProxy;

use crate::app::user_event::SynGuiUserEvent;
use crate::core::Point;
use crate::input::{PenPhase, PenSample, PenTool};
use crate::window::Window;

/// Кнопка на корпусе пера (evdev BTN_STYLUS).
const BTN_STYLUS: u32 = 0x14b;

/// Отсчёт пера для цикла событий: поверхность (`wl_proxy*`) и координаты в
/// логических пикселях поверхности.
#[derive(Debug, Clone, Copy)]
pub struct PenFrame {
    pub surface: usize,
    pub phase: PenPhase,
    pub sample: PenSample,
}

struct WlDisplayPtr(NonNull<c_void>);
// SAFETY: wl_display потокобезопасен (libwayland защищает его мьютексом).
unsafe impl Send for WlDisplayPtr {}

/// Состояние одного инструмента между `frame`.
#[derive(Default)]
struct Tool {
    eraser: bool,
    surface: Option<usize>,
    pos: (f64, f64),
    pressure: f32,
    tilt: (f32, f32),
    barrel: bool,
    /// Кончик прижат (по down/up).
    down: bool,
    /// События текущего кадра.
    entered: bool,
    went_down: bool,
    went_up: bool,
    left: bool,
}

struct PenState {
    proxy: EventLoopProxy<SynGuiUserEvent>,
    manager: Option<ZwpTabletManagerV2>,
    seats: Vec<(WlSeat, Option<ZwpTabletSeatV2>)>,
    tools: Vec<(ZwpTabletToolV2, Tool)>,
    exit: bool,
}

impl PenState {
    fn bind_tablet_seats(&mut self, qh: &QueueHandle<Self>) {
        let Some(manager) = self.manager.clone() else { return };
        for (seat, ts) in self.seats.iter_mut() {
            if ts.is_none() {
                *ts = Some(manager.get_tablet_seat(seat, qh, ()));
            }
        }
    }

    fn tool(&mut self, t: &ZwpTabletToolV2) -> Option<&mut Tool> {
        self.tools.iter_mut().find(|(p, _)| p == t).map(|(_, s)| s)
    }

    fn send(&mut self, frame: PenFrame) {
        if self.proxy.send_event(SynGuiUserEvent::Pen(frame)).is_err() {
            self.exit = true;
        }
    }
}

pub fn try_start(window: &Window, proxy: EventLoopProxy<SynGuiUserEvent>) -> Option<std::thread::JoinHandle<()>> {
    let display = match window.display_handle().ok()?.as_raw() {
        RawDisplayHandle::Wayland(w) => WlDisplayPtr(w.display),
        _ => return None,
    };
    std::thread::Builder::new()
        .name("syngui-wayland-pen".into())
        .spawn(move || {
            if let Err(e) = run(display, proxy) {
                log::warn!("[wayland_pen] поток пера завершился: {e:?}");
            }
        })
        .ok()
}

fn run(display: WlDisplayPtr, proxy: EventLoopProxy<SynGuiUserEvent>) -> Result<(), Box<dyn std::error::Error>> {
    // SAFETY: указатель wl_display взят у окна winit и живёт весь процесс.
    let backend = unsafe { Backend::from_foreign_display(display.0.as_ptr().cast()) };
    let conn = Connection::from_backend(backend);
    let (globals, mut queue) = registry_queue_init::<PenState>(&conn)?;
    let qh = queue.handle();
    let manager = match globals.bind::<ZwpTabletManagerV2, _, _>(&qh, 1..=1, ()) {
        Ok(m) => m,
        Err(_) => {
            log::debug!("[wayland_pen] композитор без zwp_tablet_manager_v2 — перо как указатель");
            return Ok(());
        }
    };
    let mut state = PenState { proxy, manager: Some(manager), seats: Vec::new(), tools: Vec::new(), exit: false };
    for g in globals.contents().clone_list() {
        if g.interface == WlSeat::interface().name {
            let seat: WlSeat = globals.registry().bind(g.name, g.version.min(5), &qh, ());
            state.seats.push((seat, None));
        }
    }
    state.bind_tablet_seats(&qh);
    log::info!("[wayland_pen] tablet-v2: сидений {}", state.seats.len());
    while !state.exit {
        if let Err(e) = queue.blocking_dispatch(&mut state) {
            log::debug!("[wayland_pen] blocking_dispatch: {e}");
            break;
        }
    }
    Ok(())
}

impl Dispatch<WlRegistry, GlobalListContents> for PenState {
    fn event(
        state: &mut Self,
        registry: &WlRegistry,
        event: wayland_client::protocol::wl_registry::Event,
        _: &GlobalListContents,
        _: &Connection,
        qh: &QueueHandle<Self>,
    ) {
        if let wayland_client::protocol::wl_registry::Event::Global { name, interface, version } = event {
            if interface == WlSeat::interface().name {
                let seat: WlSeat = registry.bind(name, version.min(5), qh, ());
                state.seats.push((seat, None));
                state.bind_tablet_seats(qh);
            }
        }
    }
}

impl Dispatch<WlSeat, ()> for PenState {
    fn event(_: &mut Self, _: &WlSeat, _: wayland_client::protocol::wl_seat::Event, _: &(), _: &Connection, _: &QueueHandle<Self>) {}
}

impl Dispatch<ZwpTabletManagerV2, ()> for PenState {
    fn event(
        _: &mut Self,
        _: &ZwpTabletManagerV2,
        _: <ZwpTabletManagerV2 as Proxy>::Event,
        _: &(),
        _: &Connection,
        _: &QueueHandle<Self>,
    ) {
    }
}

impl Dispatch<ZwpTabletSeatV2, ()> for PenState {
    fn event(
        state: &mut Self,
        _: &ZwpTabletSeatV2,
        event: zwp_tablet_seat_v2::Event,
        _: &(),
        _: &Connection,
        _: &QueueHandle<Self>,
    ) {
        if let zwp_tablet_seat_v2::Event::ToolAdded { id } = event {
            state.tools.push((id, Tool::default()));
        }
    }

    event_created_child!(PenState, ZwpTabletSeatV2, [
        zwp_tablet_seat_v2::EVT_TABLET_ADDED_OPCODE => (ZwpTabletV2, ()),
        zwp_tablet_seat_v2::EVT_TOOL_ADDED_OPCODE => (ZwpTabletToolV2, ()),
        zwp_tablet_seat_v2::EVT_PAD_ADDED_OPCODE => (ZwpTabletPadV2, ()),
    ]);
}

impl Dispatch<ZwpTabletV2, ()> for PenState {
    fn event(_: &mut Self, t: &ZwpTabletV2, e: <ZwpTabletV2 as Proxy>::Event, _: &(), _: &Connection, _: &QueueHandle<Self>) {
        if matches!(e, wayland_protocols::wp::tablet::zv2::client::zwp_tablet_v2::Event::Removed) {
            t.destroy();
        }
    }
}

impl Dispatch<ZwpTabletPadV2, ()> for PenState {
    fn event(_: &mut Self, p: &ZwpTabletPadV2, e: zwp_tablet_pad_v2::Event, _: &(), _: &Connection, _: &QueueHandle<Self>) {
        if matches!(e, zwp_tablet_pad_v2::Event::Removed) {
            p.destroy();
        }
    }

    event_created_child!(PenState, ZwpTabletPadV2, [
        zwp_tablet_pad_v2::EVT_GROUP_OPCODE => (ZwpTabletPadGroupV2, ()),
    ]);
}

impl Dispatch<ZwpTabletPadGroupV2, ()> for PenState {
    fn event(_: &mut Self, _: &ZwpTabletPadGroupV2, _: zwp_tablet_pad_group_v2::Event, _: &(), _: &Connection, _: &QueueHandle<Self>) {}

    event_created_child!(PenState, ZwpTabletPadGroupV2, [
        zwp_tablet_pad_group_v2::EVT_RING_OPCODE => (ZwpTabletPadRingV2, ()),
        zwp_tablet_pad_group_v2::EVT_STRIP_OPCODE => (ZwpTabletPadStripV2, ()),
    ]);
}

impl Dispatch<ZwpTabletPadRingV2, ()> for PenState {
    fn event(_: &mut Self, _: &ZwpTabletPadRingV2, _: <ZwpTabletPadRingV2 as Proxy>::Event, _: &(), _: &Connection, _: &QueueHandle<Self>) {}
}

impl Dispatch<ZwpTabletPadStripV2, ()> for PenState {
    fn event(_: &mut Self, _: &ZwpTabletPadStripV2, _: <ZwpTabletPadStripV2 as Proxy>::Event, _: &(), _: &Connection, _: &QueueHandle<Self>) {}
}

fn surface_key(s: &WlSurface) -> Option<usize> {
    let p = s.id().as_ptr();
    (!p.is_null()).then_some(p as usize)
}

impl Dispatch<ZwpTabletToolV2, ()> for PenState {
    fn event(
        state: &mut Self,
        proxy: &ZwpTabletToolV2,
        event: zwp_tablet_tool_v2::Event,
        _: &(),
        _: &Connection,
        _: &QueueHandle<Self>,
    ) {
        use zwp_tablet_tool_v2::Event as E;
        if let E::Removed = event {
            state.tools.retain(|(p, _)| p != proxy);
            proxy.destroy();
            return;
        }
        let Some(t) = state.tool(proxy) else { return };
        match event {
            E::Type { tool_type } => {
                t.eraser = matches!(tool_type, WEnum::Value(zwp_tablet_tool_v2::Type::Eraser));
            }
            E::ProximityIn { surface, .. } => {
                t.surface = surface_key(&surface);
                t.entered = true;
                t.left = false;
            }
            E::ProximityOut => {
                t.left = true;
            }
            E::Down { .. } => {
                t.down = true;
                t.went_down = true;
            }
            E::Up => {
                t.down = false;
                t.went_up = true;
            }
            E::Motion { x, y } => t.pos = (x, y),
            E::Pressure { pressure } => t.pressure = pressure as f32 / 65535.0,
            E::Tilt { tilt_x, tilt_y } => t.tilt = (tilt_x as f32, tilt_y as f32),
            E::Button { button, state: bs, .. } => {
                if button == BTN_STYLUS {
                    t.barrel = matches!(bs, WEnum::Value(zwp_tablet_tool_v2::ButtonState::Pressed));
                }
            }
            E::Frame { time } => {
                let Some(surface) = t.surface else {
                    t.entered = false;
                    t.went_down = false;
                    t.went_up = false;
                    t.left = false;
                    return;
                };
                let mut sample = PenSample {
                    position: Point::new(t.pos.0 as f32, t.pos.1 as f32),
                    pressure: t.pressure,
                    tilt_x: t.tilt.0,
                    tilt_y: t.tilt.1,
                    tool: if t.eraser { PenTool::Eraser } else { PenTool::Pen },
                    barrel: t.barrel,
                    time_ms: time,
                };
                // Фазы кадра по порядку: касание, движение/наведение, отрыв, уход.
                let mut phases: Vec<PenPhase> = Vec::with_capacity(2);
                if t.went_down {
                    phases.push(PenPhase::Down);
                } else if t.down {
                    phases.push(PenPhase::Move);
                }
                if t.went_up {
                    phases.push(PenPhase::Up);
                }
                if !t.went_down && !t.down && !t.went_up && !t.left {
                    phases.push(PenPhase::Hover);
                }
                if t.left {
                    phases.push(PenPhase::Leave);
                    t.surface = None;
                    t.down = false;
                }
                t.entered = false;
                t.went_down = false;
                t.went_up = false;
                t.left = false;
                for phase in phases {
                    if matches!(phase, PenPhase::Hover | PenPhase::Up | PenPhase::Leave) {
                        sample.pressure = if phase == PenPhase::Hover { 0.0 } else { sample.pressure };
                    }
                    state.send(PenFrame { surface, phase, sample });
                }
            }
            _ => {}
        }
    }
}
