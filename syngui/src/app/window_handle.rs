//! Окна ОС во время работы: [`open_window`], [`close_window`], [`WindowHandle`].
//!
//! Вызывать на UI-потоке — из обработчиков событий, эффектов,
//! `run_on_main_thread`. Заявка ложится в очередь потока, цикл событий winit
//! будится user-event'ом (`SynGuiUserEvent::WindowCommands`) и выполняет её у
//! себя: окно winit создаётся только там, где есть `ActiveEventLoop`.
//!
//! Дополнительное окно — полноправное: своё дерево виджетов, оверлеи и меню,
//! фокус, IME, курсоры, двойной щелчок, перетаскивание; общие с главным
//! сигналы, таблица стилей, тема и масштаб. Запросы окна из его дерева
//! (`WindowDragRegion`, `WindowResizeRegion`, `SystemWindowControls`,
//! `EventContext::close_window`) действуют на него же. Закрытие
//! дополнительного окна освобождает его дерево и эффекты, созданные при его
//! сборке; закрытие главного, как и раньше, завершает приложение.

use std::cell::{Cell, RefCell};
use std::collections::HashMap;

use super::builder::WindowConfig;
use crate::signal::{use_signal, RwSignal};
use crate::widget::{BuildContext, ElementId, ElementTree, Widget};
use crate::window::WindowState;

/// Ручка окна приложения. Дешёвая (`Copy`), живёт дольше окна: после
/// закрытия вызовы на ней ничего не делают, а [`closed`](Self::closed)
/// остаётся `true`.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub struct WindowHandle(u64);

/// Сборщик корня дополнительного окна.
pub(crate) type WindowBuild = Box<dyn FnOnce(&mut BuildContext) -> Box<dyn Widget>>;

/// Заявка циклу событий.
pub(crate) enum WindowCommand {
    Open {
        handle: WindowHandle,
        config: WindowConfig,
        build: WindowBuild,
    },
    Close(WindowHandle),
    SetTitle(WindowHandle, String),
    Focus(WindowHandle),
}

struct Entry {
    closed: RwSignal<bool>,
    state: Option<RwSignal<WindowState>>,
    title: String,
}

thread_local! {
    static COMMANDS: RefCell<Vec<WindowCommand>> = const { RefCell::new(Vec::new()) };
    static REGISTRY: RefCell<HashMap<u64, Entry>> = RefCell::new(HashMap::new());
    static NEXT_ID: Cell<u64> = const { Cell::new(1) };
    static CURRENT: Cell<WindowHandle> = const { Cell::new(WindowHandle::MAIN) };
}

impl WindowHandle {
    /// Главное окно приложения: его закрытие завершает приложение.
    pub const MAIN: WindowHandle = WindowHandle(0);

    /// Номер окна в процессе (у главного — 0).
    pub fn id(self) -> u64 {
        self.0
    }

    pub fn is_main(self) -> bool {
        self == Self::MAIN
    }

    /// Закрыть окно (то же, что [`close_window`]).
    pub fn close(self) {
        close_window(self);
    }

    /// Сменить заголовок окна.
    pub fn set_title(self, title: impl Into<String>) {
        let title = title.into();
        with_entry(self, |e| e.title = title.clone());
        push(WindowCommand::SetTitle(self, title));
    }

    /// Последний заданный заголовок.
    pub fn title(self) -> Option<String> {
        REGISTRY.with(|r| r.borrow().get(&self.0).map(|e| e.title.clone()))
    }

    /// Показать (если спрятано), поднять поверх и дать фокус ввода. На
    /// Wayland композитор вправе не поднимать окно по просьбе программы —
    /// тогда оно лишь просит внимания.
    pub fn focus(self) {
        push(WindowCommand::Focus(self));
    }

    /// То же, что [`focus`](Self::focus).
    pub fn raise(self) {
        self.focus();
    }

    /// Сигнал «окно закрыто»: `true` — окно закрылось (пользователем,
    /// [`close_window`], `EventContext::close_window`), его дерево и эффекты
    /// освобождены. Только для чтения — подписка в `Reactive`/эффекте даёт
    /// приложению убрать своё состояние. У главного окна всегда `false`.
    pub fn closed(self) -> RwSignal<bool> {
        REGISTRY.with(|r| {
            let mut r = r.borrow_mut();
            r.entry(self.0)
                .or_insert_with(|| Entry {
                    closed: use_signal(false),
                    state: None,
                    title: String::new(),
                })
                .closed
        })
    }

    /// Окно открыто (или ещё открывается).
    pub fn is_open(self) -> bool {
        REGISTRY.with(|r| {
            r.borrow()
                .get(&self.0)
                .is_some_and(|e| !e.closed.get_untracked())
        })
    }

    /// Сигнал состояния окна (развёрнуто / полноэкранно / в фокусе) — как
    /// [`AppBuilder::with_window_state`](crate::app::AppBuilder::with_window_state)
    /// у главного; те же флаги видны в MSS окна как `:window-maximized`,
    /// `:window-fullscreen`, `:window-focused`.
    pub fn state(self) -> RwSignal<WindowState> {
        REGISTRY.with(|r| {
            let mut r = r.borrow_mut();
            let e = r.entry(self.0).or_insert_with(|| Entry {
                closed: use_signal(false),
                state: None,
                title: String::new(),
            });
            *e.state
                .get_or_insert_with(|| use_signal(WindowState::default()))
        })
    }
}

/// Открыть окно ОС с деревом виджетов из `build`. Вызывать на UI-потоке;
/// окно появится на ближайшем проходе цикла событий, ручка годна сразу.
///
/// ```ignore
/// let panel = syngui::app::open_window(
///     WindowConfig::new("panel").title("Панель").size(900, 700).frameless().transparent(true)
///         .on_close(move || panel_open.set(false)),
///     move |_ctx| Box::new(panel_root()),
/// );
/// panel.focus();
/// panel.set_title("Панель — проект");
/// syngui::app::close_window(panel);
/// ```
pub fn open_window(
    config: WindowConfig,
    build: impl FnOnce(&mut BuildContext) -> Box<dyn Widget> + 'static,
) -> WindowHandle {
    let handle = register(&config);
    push(WindowCommand::Open {
        handle,
        config,
        build: Box::new(build),
    });
    handle
}

/// Закрыть окно. Дополнительное — освобождается его дерево и эффекты, затем
/// зовётся [`WindowConfig::on_close`]; главное — как закрытие пользователем
/// (с [`AppBuilder::on_close_request`](crate::app::AppBuilder::on_close_request)
/// и `hide_on_close`). Уже закрытое — ничего.
pub fn close_window(handle: WindowHandle) {
    if !handle.is_main() && !handle.is_open() {
        return;
    }
    push(WindowCommand::Close(handle));
}

/// Окно, чьё событие, сборка или кадр обрабатываются сейчас; вне их —
/// главное. Обработчику кнопки «закрыть панель» внутри окна этого хватает,
/// чтобы не таскать ручку: `current_window().close()`.
pub fn current_window() -> WindowHandle {
    CURRENT.with(|c| c.get())
}

/// Открытые сейчас дополнительные окна (без главного).
pub fn open_windows() -> Vec<WindowHandle> {
    let mut v: Vec<WindowHandle> = REGISTRY.with(|r| {
        r.borrow()
            .iter()
            .filter(|(&id, e)| id != 0 && !e.closed.get_untracked())
            .map(|(&id, _)| WindowHandle(id))
            .collect()
    });
    v.sort_by_key(|h| h.0);
    v
}

/// Завести ручку и запись реестра под новое окно.
pub(crate) fn register(config: &WindowConfig) -> WindowHandle {
    let id = NEXT_ID.with(|n| {
        let id = n.get();
        n.set(id + 1);
        id
    });
    let title = if config.title.is_empty() {
        config.name.clone()
    } else {
        config.title.clone()
    };
    REGISTRY.with(|r| {
        r.borrow_mut().insert(
            id,
            Entry {
                closed: use_signal(false),
                state: config.window_state,
                title,
            },
        )
    });
    WindowHandle(id)
}

/// Запись главного окна: его сигнал состояния —
/// [`AppBuilder::with_window_state`](crate::app::AppBuilder::with_window_state).
pub(crate) fn register_main(state: Option<RwSignal<WindowState>>) {
    REGISTRY.with(|r| {
        let mut r = r.borrow_mut();
        let e = r.entry(0).or_insert_with(|| Entry {
            closed: use_signal(false),
            state: None,
            title: String::new(),
        });
        if state.is_some() {
            e.state = state;
        }
    });
}

/// Сигнал состояния окна, если его завели (приложение или
/// [`WindowHandle::state`]).
pub(crate) fn state_signal(handle: WindowHandle) -> Option<RwSignal<WindowState>> {
    REGISTRY.with(|r| r.borrow().get(&handle.0).and_then(|e| e.state))
}

fn with_entry(handle: WindowHandle, f: impl FnOnce(&mut Entry)) {
    REGISTRY.with(|r| {
        if let Some(e) = r.borrow_mut().get_mut(&handle.0) {
            f(e);
        }
    });
}

fn push(cmd: WindowCommand) {
    // Очередь — у потока: с чужого потока её никто не заберёт.
    if !crate::signal::is_main_thread() {
        log::error!(
            "syngui: open_window/close_window/WindowHandle вызваны не с UI-потока — \
             оберните вызов в run_on_main_thread"
        );
    }
    COMMANDS.with(|q| q.borrow_mut().push(cmd));
    // Цикл событий ещё не запущен (или платформа без прокси) — очередь
    // заберёт ближайший about_to_wait.
    let _ = super::send_control(super::user_event::SynGuiUserEvent::WindowCommands);
}

/// Забрать накопленные заявки (цикл событий; тесты — `testing::WindowHost`).
pub(crate) fn take_commands() -> Vec<WindowCommand> {
    COMMANDS.with(|q| std::mem::take(&mut *q.borrow_mut()))
}

/// Отметить окно закрытым: сигнал `closed` → `true`.
pub(crate) fn mark_closed(handle: WindowHandle) {
    let closed = REGISTRY.with(|r| r.borrow().get(&handle.0).map(|e| e.closed));
    if let Some(sig) = closed {
        if !sig.get_untracked() {
            sig.set(true);
        }
    }
}

/// Текущее окно (см. [`current_window`]) — выставляет цикл событий на время
/// событий, сборки и кадра окна; возвращает прежнее.
pub(crate) fn set_current(handle: WindowHandle) -> WindowHandle {
    CURRENT.with(|c| c.replace(handle))
}

/// Владелец эффектов, созданных прямо в сборщике окна (вне `Reactive`):
/// идентификатор вне диапазона деревьев (у тех счётчик с единицы).
pub(crate) fn owner_id(handle: WindowHandle) -> ElementId {
    ElementId((1u64 << 63) | handle.0)
}

/// Собрать и смонтировать корень окна в `tree`. Эффекты, созданные в
/// `build` и при монтировании вне своих `Reactive`, записываются за
/// владельцем окна ([`owner_id`]) и освобождаются [`release_window`].
#[cfg_attr(target_arch = "wasm32", allow(dead_code))]
pub(crate) fn mount_window_root(
    handle: WindowHandle,
    tree: &mut ElementTree,
    build: impl FnOnce(&mut BuildContext) -> Box<dyn Widget>,
) -> ElementId {
    let prev = set_current(handle);
    let owner = (!handle.is_main()).then(|| owner_id(handle));
    if let Some(owner) = owner {
        crate::signal::begin_element_scope(owner);
    }
    let mut ctx = BuildContext::root();
    let widget = build(&mut ctx);
    let element = widget.create_element();
    let type_id = widget.as_any().type_id();
    let root_id = tree.insert_with_type_id(element, None, type_id);
    widget.mount(tree, root_id);
    if owner.is_some() {
        crate::signal::end_element_scope();
    }
    set_current(prev);
    root_id
}

/// Освободить закрытое окно: дерево (элементы снимают свои подписки и
/// эффекты), эффекты сборщика, затем отметить закрытым и позвать `on_close`.
pub(crate) fn release_window(
    handle: WindowHandle,
    tree: ElementTree,
    on_close: Option<std::rc::Rc<dyn Fn()>>,
) {
    drop(tree);
    if !handle.is_main() {
        crate::signal::cleanup_element(owner_id(handle));
    }
    mark_closed(handle);
    if let Some(f) = on_close {
        f();
    }
}
