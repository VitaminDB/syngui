//! Состояние одного окна ОС и переключение активного окна.
//!
//! Всё, что принадлежит окну (окно winit, поверхность, рендерер, дерево
//! виджетов, фокус, IME, курсор, клики, касания, отладочные оверлеи), лежит в
//! [`AppWindow`]. Код событий и кадра один на все окна и работает с активным
//! окном `AppHandler::win`. Главное окно активно всегда; дополнительное
//! становится активным на время своих событий, сборки и кадра
//! ([`AppHandler::with_window`]): его состояние меняется местами с главным,
//! вместе с вьюпортом потока и «текущим окном» (`app::current_window`).
//! Главное от дополнительного отличается одним: его закрытие завершает
//! приложение (или прячет окно — `hide_on_close`, трей).

use super::AppHandler;
use crate::a11y::{A11yTree, FocusManager};
use crate::app::window_handle::{self, WindowBuild, WindowCommand, WindowHandle};
use crate::core::{Color, Point, Rect};
use crate::gpu::{Renderer, WindowSurface};
use crate::input::CursorIcon;
use crate::render::DisplayList;
use crate::widget::{ElementId, ElementTree};
use crate::window::Window;
use std::sync::Arc;
use web_time::Instant;

// Перо и перенос Wayland — только Linux с фичами pen / wayland-dnd, IME winit
// не нужен вебу и Android.
#[cfg_attr(
    not(all(target_os = "linux", feature = "pen", feature = "wayland-dnd")),
    allow(dead_code)
)]
pub(in crate::app) struct AppWindow {
    /// Ручка окна для приложения (`WindowHandle::MAIN` у главного).
    pub handle: WindowHandle,
    pub tree: ElementTree,
    pub root_id: Option<ElementId>,
    pub renderer: Option<Renderer>,
    pub surface: Option<WindowSurface>,
    pub window: Option<Arc<Window>>,
    pub display_list: DisplayList,
    /// Размер поверхности в физических пикселях.
    pub width: u32,
    pub height: u32,
    /// Итоговый множитель «логические единицы → физические пиксели»:
    /// системный DPI-масштаб окна, умноженный на пользовательский
    /// ([`crate::scale::ui_scale`]).
    pub scale_factor: f64,
    /// DPI-масштаб окна, каким его сообщает система, — без пользовательского
    /// множителя. Нужен, чтобы пересчитать `scale_factor` при смене любого
    /// из двух.
    pub system_scale_factor: f64,
    /// Clear-color кадра: фон темы или корня; у прозрачного окна (CSD) —
    /// прозрачный, фон рисует корень.
    pub background_color: Color,
    pub transparent: bool,
    pub surface_valid: bool,
    /// Последняя отданная IME область каретки — не дёргать winit каждый кадр.
    pub last_ime_area: Option<Rect>,
    /// IME окна включён (есть поле ввода в фокусе).
    pub ime_allowed: bool,
    /// Подпись последнего отрисованного кадра (`DisplayList::frame_signature`):
    /// совпала — GPU-кадр не рисуется.
    pub last_frame_sig: Option<u64>,
    /// Последний кадр совпал с показанным и не рисовался: present не было,
    /// frame callback не придёт — анимация, которая визуально почти ничего не
    /// меняет (затухание полосы прокрутки, хвост инерции), без паузы крутила
    /// бы цикл вхолостую (десятки тысяч итераций в секунду). Следующий кадр
    /// анимации — через период обновления экрана.
    pub last_frame_skipped: bool,
    pub last_paced_redraw: Option<Instant>,
    /// Момент последнего отрисованного кадра — для `keep_warm_interval`.
    pub last_render_at: Instant,
    pub cursor_position: Point,
    /// Перенос дерева идёт перетаскиванием Wayland (значок рисует композитор,
    /// события приходят от wl_data_device, а не от указателя).
    pub os_drag: bool,
    pub focus_manager: FocusManager,
    pub a11y_tree: A11yTree,
    pub a11y_dirty: bool,
    /// Номер последнего снимка для осмотра снаружи (`a11y::inspect`).
    pub inspect_seq: u64,
    #[cfg(feature = "accessibility")]
    pub accesskit_adapter: Option<accesskit_winit::Adapter>,
    pub current_cursor: CursorIcon,
    /// Свой указатель, поставленный окну: (номер, поколение картинки).
    pub custom_cursor_applied: Option<(u32, u64)>,
    pub last_click_time: Option<Instant>,
    pub last_click_pos: Option<Point>,
    pub debug_overlay: Option<crate::debug::DebugOverlay>,
    pub devtools: Option<crate::devtools::DevTools>,
    /// Отложенный tap-синтез для тача: клик синтезируется на отпускании и
    /// только если палец не сдвинулся — иначе скролл списка «проваливался» в
    /// строку под пальцем.
    pub touch: crate::input::TouchTracker,
    /// Автомат штриха пера (zwp_tablet_v2).
    pub pen: crate::input::PenTracker,
    /// Вьюпорт окна, пока оно не активно (см. `viewport::swap_context`).
    pub viewport: crate::viewport::ViewportContext,
    /// Окно попросили закрыть во время его же события — закрывается, когда
    /// перестаёт быть активным.
    pub close_requested: bool,
    pub on_close: Option<std::rc::Rc<dyn Fn()>>,
    pub close_guard: Option<std::rc::Rc<dyn Fn() -> bool>>,
}

impl AppWindow {
    pub(in crate::app) fn new(
        handle: WindowHandle,
        background_color: Color,
        transparent: bool,
        debug_overlay: bool,
        devtools: bool,
    ) -> Self {
        let mut dt = crate::devtools::DevTools::new();
        if devtools {
            dt.toggle();
        }
        Self {
            handle,
            tree: ElementTree::new(),
            root_id: None,
            renderer: None,
            surface: None,
            window: None,
            display_list: DisplayList::new(),
            width: 0,
            height: 0,
            scale_factor: crate::scale::ui_scale() as f64,
            system_scale_factor: 1.0,
            background_color,
            transparent,
            surface_valid: false,
            last_ime_area: None,
            ime_allowed: false,
            last_frame_sig: None,
            last_frame_skipped: false,
            last_paced_redraw: None,
            last_render_at: Instant::now(),
            cursor_position: Point::zero(),
            os_drag: false,
            focus_manager: FocusManager::new(),
            #[cfg(feature = "accessibility")]
            a11y_tree: A11yTree::new(Box::new(crate::a11y::AccessKitAdapter::new())),
            #[cfg(not(feature = "accessibility"))]
            a11y_tree: A11yTree::new(Box::new(crate::a11y::LoggingAdapter)),
            a11y_dirty: true,
            inspect_seq: 0,
            #[cfg(feature = "accessibility")]
            accesskit_adapter: None,
            current_cursor: CursorIcon::Default,
            custom_cursor_applied: None,
            last_click_time: None,
            last_click_pos: None,
            debug_overlay: debug_overlay.then(crate::debug::DebugOverlay::new),
            devtools: Some(dt),
            touch: crate::input::TouchTracker::new(),
            pen: crate::input::PenTracker::new(),
            viewport: crate::viewport::ViewportContext::default(),
            close_requested: false,
            on_close: None,
            close_guard: None,
        }
    }

    pub(in crate::app) fn is_main(&self) -> bool {
        self.handle.is_main()
    }

    pub(in crate::app) fn winit_id(&self) -> Option<winit::window::WindowId> {
        self.window.as_ref().map(|w| w.id())
    }
}

impl AppHandler {
    /// Перерисовать активное окно.
    pub(in crate::app) fn request_redraw(&self) {
        if let Some(window) = &self.win.window {
            crate::perf::redraw_from(file!(), line!());
            window.request_redraw();
        }
    }

    /// Выполнить `f` с окном `id` в активном слоте `self.win`. Уже активное
    /// окно — просто `f`; дополнительное меняется местами с активным на
    /// время вызова. Окно, попросившее закрыться во время `f`, закрывается
    /// после. `None` — такого окна нет.
    pub(in crate::app) fn with_window<R>(
        &mut self,
        id: winit::window::WindowId,
        f: impl FnOnce(&mut Self) -> R,
    ) -> Option<R> {
        if self.win.winit_id() == Some(id) {
            return Some(f(self));
        }
        let mut parked = self.secondary_windows.remove(&id)?;
        // Окно `id` — активное, прежнее ждёт в `parked`.
        std::mem::swap(&mut self.win, &mut parked);
        crate::viewport::swap_context(&mut self.win.viewport);
        let prev_current = window_handle::set_current(self.win.handle);
        self.win.tree.modifiers = self.modifiers;
        crate::input::set_current_modifiers(self.modifiers);

        let result = f(self);

        window_handle::set_current(prev_current);
        crate::viewport::swap_context(&mut self.win.viewport);
        std::mem::swap(&mut self.win, &mut parked);
        if parked.close_requested {
            self.release_secondary(parked);
        } else {
            self.secondary_windows.insert(id, parked);
        }
        Some(result)
    }

    /// `f` для каждого окна по очереди: главного, затем дополнительных.
    pub(in crate::app) fn for_each_window(&mut self, mut f: impl FnMut(&mut Self)) {
        f(self);
        for id in self.secondary_ids() {
            self.with_window(id, &mut f);
        }
    }

    pub(in crate::app) fn secondary_ids(&self) -> Vec<winit::window::WindowId> {
        self.secondary_windows.keys().copied().collect()
    }

    /// Закрыть активное дополнительное окно (✕, Alt+F4, `close_window` из
    /// его дерева). `by_user` — спросить `WindowConfig::on_close_request`.
    pub(in crate::app) fn request_close_current(&mut self, by_user: bool) {
        if by_user {
            if let Some(guard) = self.win.close_guard.clone() {
                if !guard() {
                    self.request_redraw();
                    return;
                }
            }
        }
        self.win.close_requested = true;
    }

    /// Освободить закрытое дополнительное окно: окно и GPU-ресурсы, затем
    /// дерево и эффекты сборщика; отметить ручку закрытой и позвать
    /// `on_close`.
    pub(in crate::app) fn release_secondary(&mut self, mut w: AppWindow) {
        if let Some(window) = &w.window {
            let id = window.id();
            window.set_visible(false);
            crate::signal::remove_window(window);
            self.window_positions.remove(&id);
            self.remove_from_sticky_group(id);
        }
        let tree = std::mem::replace(&mut w.tree, ElementTree::new());
        let on_close = w.on_close.take();
        let handle = w.handle;
        drop(w);
        window_handle::release_window(handle, tree, on_close);
    }

    fn secondary_id_of(&self, handle: WindowHandle) -> Option<winit::window::WindowId> {
        self.secondary_windows
            .iter()
            .find(|(_, w)| w.handle == handle)
            .map(|(id, _)| *id)
    }

    /// Окно winit по ручке (главное — пока оно существует).
    fn window_of(&self, handle: WindowHandle) -> Option<Arc<Window>> {
        if handle.is_main() {
            return self.win_main().and_then(|w| w.window.clone());
        }
        self.secondary_windows
            .values()
            .find(|w| w.handle == handle)
            .and_then(|w| w.window.clone())
    }

    /// Главное окно, когда оно активно (вне `with_window`).
    fn win_main(&self) -> Option<&AppWindow> {
        self.win.is_main().then_some(&self.win)
    }

    /// Заявки `open_window` / `close_window` / `WindowHandle::*`. Зовётся вне
    /// `with_window` (user-event, about_to_wait).
    pub(in crate::app) fn process_window_commands(
        &mut self,
        event_loop: &winit::event_loop::ActiveEventLoop,
    ) {
        for cmd in window_handle::take_commands() {
            match cmd {
                WindowCommand::Open {
                    handle,
                    config,
                    build,
                } => {
                    if self.gpu.is_none() {
                        // Главное окно ещё не готово (start_hidden, веб):
                        // окно создастся вместе с ним.
                        self.pending_windows.push((handle, config, build));
                    } else {
                        self.create_secondary_window(event_loop, handle, config, build);
                    }
                }
                WindowCommand::Close(handle) => {
                    if handle.is_main() {
                        self.close_main_window(event_loop);
                    } else if let Some(id) = self.secondary_id_of(handle) {
                        if let Some(w) = self.secondary_windows.remove(&id) {
                            self.release_secondary(w);
                        }
                    } else if let Some(pos) =
                        self.pending_windows.iter().position(|(h, ..)| *h == handle)
                    {
                        let (_, config, _) = self.pending_windows.remove(pos);
                        window_handle::release_window(handle, ElementTree::new(), config.on_close);
                    }
                }
                WindowCommand::SetTitle(handle, title) => {
                    if let Some(window) = self.window_of(handle) {
                        window.winit_window().set_title(&title);
                    }
                }
                WindowCommand::Focus(handle) => {
                    if handle.is_main() {
                        self.show_main_window();
                    } else if let Some(window) = self.window_of(handle) {
                        window.set_visible(true);
                        window.winit_window().set_minimized(false);
                        window.focus();
                        window.request_redraw();
                    }
                }
            }
        }
    }

    /// Окна, отложенные до готовности GPU (`add_window`, ранний `open_window`).
    #[cfg_attr(target_arch = "wasm32", allow(dead_code))]
    pub(in crate::app) fn create_pending_windows(
        &mut self,
        event_loop: &winit::event_loop::ActiveEventLoop,
    ) {
        for (handle, config, build) in std::mem::take(&mut self.pending_windows) {
            self.create_secondary_window(event_loop, handle, config, build);
        }
    }

    /// Закрытие главного окна: проверка приложения (`on_close_request`),
    /// затем спрятать (`hide_on_close`, трей) или завершить приложение.
    pub(in crate::app) fn close_main_window(
        &mut self,
        event_loop: &winit::event_loop::ActiveEventLoop,
    ) {
        if !self.close_allowed() {
            self.request_redraw();
            return;
        }
        if self.should_hide_on_close() {
            self.hide_main_window();
        } else {
            self.exit_app(event_loop);
        }
    }

    /// Завершить приложение: снять трей и замок единственного экземпляра,
    /// выйти из цикла событий.
    pub(in crate::app) fn exit_app(&mut self, event_loop: &winit::event_loop::ActiveEventLoop) {
        #[cfg(all(
            feature = "tray",
            not(target_arch = "wasm32"),
            not(target_os = "android")
        ))]
        {
            self.tray.take();
        }
        #[cfg(all(
            feature = "single-instance",
            not(target_arch = "wasm32"),
            not(target_os = "android")
        ))]
        {
            self.single_instance.take();
        }
        event_loop.exit();
    }

    /// Создать дополнительное окно: окно winit, поверхность на общем GPU,
    /// свой рендерер и дерево из `build`.
    pub(in crate::app) fn create_secondary_window(
        &mut self,
        event_loop: &winit::event_loop::ActiveEventLoop,
        handle: WindowHandle,
        wc: crate::app::WindowConfig,
        build: WindowBuild,
    ) {
        // Android, iOS и браузер — одно окно на приложение.
        #[cfg(any(target_os = "android", target_os = "ios", target_arch = "wasm32"))]
        {
            let _ = (event_loop, build);
            log::warn!("syngui: дополнительные окна на этой платформе не поддерживаются");
            window_handle::release_window(handle, ElementTree::new(), wc.on_close);
        }
        #[cfg(not(any(target_os = "android", target_os = "ios", target_arch = "wasm32")))]
        {
            use crate::window::WindowBuilder;
            let Some(gpu) = self.gpu.as_ref() else {
                self.pending_windows.push((handle, wc, build));
                return;
            };
            let title = if wc.title.is_empty() {
                &wc.name
            } else {
                &wc.title
            };
            let mut wb = WindowBuilder::new()
                .with_title(title)
                .with_size(wc.width, wc.height)
                .with_min_size(wc.min_width, wc.min_height)
                .with_decorations(wc.decorations)
                .with_transparent(wc.transparent)
                .with_maximized(wc.maximized)
                .with_resizable(wc.resizable);
            // Тот же app_id, что у главного: панель задач группирует окна.
            if let Some(id) = &self.config.app_id {
                wb = wb.with_app_id(id.clone());
            }
            let window = Arc::new(Window::new(event_loop, wb));

            if let Some((x, y)) = wc.position {
                window
                    .winit_window()
                    .set_outer_position(winit::dpi::PhysicalPosition::new(x, y));
            } else if let Some((dx, dy)) = wc.offset_from_main {
                let main_pos = self
                    .window_of(WindowHandle::MAIN)
                    .and_then(|w| w.winit_window().outer_position().ok());
                if let Some(p) = main_pos {
                    window
                        .winit_window()
                        .set_outer_position(winit::dpi::PhysicalPosition::new(p.x + dx, p.y + dy));
                }
            }
            if let Some(bytes) = self.config.window_icon_png.clone() {
                if let Err(e) = window.set_window_icon_from_png(&bytes) {
                    log::warn!("syngui: значок окна: {e}");
                }
            }

            let surface = super::lifecycle::create_window_surface(
                gpu,
                &window,
                self.config.vsync,
                wc.transparent,
            );
            let system_scale = window.scale_factor();
            let scale = system_scale * crate::scale::ui_scale() as f64;
            let (phys_w, phys_h) = window.size();
            let (phys_w, phys_h) = (phys_w.max(1), phys_h.max(1));
            let renderer =
                self.create_renderer(gpu, surface.surface_config.format, phys_w, phys_h, scale);

            // Clear-color — фон темы, как у главного; прозрачному (CSD) фон
            // рисует корень.
            let background = if wc.transparent {
                Color::TRANSPARENT
            } else if self.win.is_main() && !self.win.transparent {
                self.win.background_color
            } else {
                self.config.background_color
            };
            let mut w = AppWindow::new(handle, background, wc.transparent, false, false);
            Self::attach_renderer(&mut w.tree, &renderer);
            w.renderer = Some(renderer);
            w.surface = Some(surface);
            w.width = phys_w;
            w.height = phys_h;
            w.system_scale_factor = system_scale;
            w.scale_factor = scale;
            w.surface_valid = true;
            w.on_close = wc.on_close;
            w.close_guard = wc.close_guard;
            // Сигналы будят и это окно.
            crate::signal::add_window(window.clone());
            w.window = Some(window.clone());

            let id = window.id();
            self.secondary_windows.insert(id, w);
            self.with_window(id, |this| {
                this.mount_root(build);
                this.request_redraw();
            });
        }
    }
}
