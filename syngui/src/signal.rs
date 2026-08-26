use crate::widget::ElementId;
use std::any::Any;
use std::cell::{Cell, RefCell};
use std::collections::{HashMap, HashSet};
use std::marker::PhantomData;
use std::sync::{Arc, OnceLock};
use std::thread::ThreadId;

pub trait RedrawNotifier: Send + Sync {
    fn request_redraw(&self);
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub struct EffectId(u64);

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub struct SignalId(u64);

struct SignalSlot {
    value: Box<dyn Any>,
    subscribers: HashSet<ElementId>,
    effect_subscribers: HashSet<EffectId>,
}

struct EffectSlot {
    closure: Box<dyn Fn() -> Option<Box<dyn Fn()>>>,
    dependencies: HashSet<SignalId>,
    cleanup: Option<Box<dyn Fn()>>,
    active: bool,
}

struct SignalRuntime {
    slots: Vec<SignalSlot>,
    notifiers: Vec<Arc<dyn RedrawNotifier>>,
    dirty_elements: HashSet<ElementId>,
    tracking_stack: Vec<ElementId>,
    effects: Vec<EffectSlot>,
    pending_effect_ids: HashSet<EffectId>,
    effect_tracking: Option<EffectId>,
    element_scope_stack: Vec<ElementId>,
    element_effects: HashMap<ElementId, Vec<EffectId>>,
}

impl SignalRuntime {
    fn new() -> Self {
        Self {
            slots: Vec::new(),
            notifiers: Vec::new(),
            dirty_elements: HashSet::new(),
            tracking_stack: Vec::new(),
            effects: Vec::new(),
            pending_effect_ids: HashSet::new(),
            effect_tracking: None,
            element_scope_stack: Vec::new(),
            element_effects: HashMap::new(),
        }
    }
}

thread_local! {
    static RUNTIME: RefCell<SignalRuntime> = RefCell::new(SignalRuntime::new());
}

static MAIN_THREAD_ID: OnceLock<ThreadId> = OnceLock::new();

thread_local! {
    /// Поток объявлен владельцем собственного runtime сигналов
    /// (см. [`allow_signal_reads_on_this_thread`]).
    static THREAD_OWNS_SIGNALS: Cell<bool> = const { Cell::new(false) };
}

/// Разрешить чтение сигналов в текущем потоке независимо от того, какой
/// поток стал «главным». Runtime сигналов — thread-local, поэтому поток,
/// создавший сигнал, читает его корректно; но `MAIN_THREAD_ID` достаётся
/// первому вызвавшему [`init_main_thread`], и в параллельных тестах остальные
/// потоки получали ложный панический «чтение вне главного потока».
/// Используется `TestHarness`; в приложении вызывать не нужно.
pub fn allow_signal_reads_on_this_thread() {
    THREAD_OWNS_SIGNALS.with(|c| c.set(true));
}

pub(crate) fn is_main_thread() -> bool {
    #[cfg(target_arch = "wasm32")]
    { true }
    #[cfg(not(target_arch = "wasm32"))]
    {
        if THREAD_OWNS_SIGNALS.with(|c| c.get()) {
            return true;
        }
        MAIN_THREAD_ID
            .get()
            .map_or(true, |id| *id == std::thread::current().id())
    }
}

#[cfg(not(target_arch = "wasm32"))]
#[inline]
fn assert_main_thread_for_read<T: 'static>(method: &'static str) {
    if !is_main_thread() {
        panic!(
            "RwSignal::<{ty}>::{method}() called from non-main thread {tid:?}. \
             Signal reads must happen on the main GUI thread. \
             Either capture the value BEFORE `spawn(async move {{ … }})`, \
             or wrap the read in `run_on_main_thread(|| {{ … }})` inside the task.",
            ty = std::any::type_name::<T>(),
            method = method,
            tid = std::thread::current().id(),
        );
    }
}

#[cfg(target_arch = "wasm32")]
#[inline]
fn assert_main_thread_for_read<T: 'static>(_method: &'static str) {}

pub fn init_main_thread() {
    #[cfg(not(target_arch = "wasm32"))]
    {
        let _ = MAIN_THREAD_ID.set(std::thread::current().id());
    }
}

pub struct RwSignal<T> {
    id: SignalId,
    _marker: PhantomData<T>,
}

impl<T> Clone for RwSignal<T> {
    fn clone(&self) -> Self { *self }
}
impl<T> Copy for RwSignal<T> {}

// SAFETY: RwSignal is just a SignalId (u64) + PhantomData<T>.
unsafe impl<T> Send for RwSignal<T> {}
unsafe impl<T> Sync for RwSignal<T> {}

impl<T: 'static + Clone> RwSignal<T> {
    pub fn get(&self) -> T {
        assert_main_thread_for_read::<T>("get");
        RUNTIME.with(|rt| {
            let mut rt = rt.borrow_mut();
            let idx = self.id.0 as usize;
            if let Some(&element_id) = rt.tracking_stack.last() {
                rt.slots[idx].subscribers.insert(element_id);
            }
            if let Some(effect_id) = rt.effect_tracking {
                rt.slots[idx].effect_subscribers.insert(effect_id);
                rt.effects[effect_id.0 as usize].dependencies.insert(self.id);
            }
            rt.slots[idx].value.downcast_ref::<T>().expect("Signal type mismatch").clone()
        })
    }

    pub fn get_untracked(&self) -> T {
        assert_main_thread_for_read::<T>("get_untracked");
        RUNTIME.with(|rt| {
            let rt = rt.borrow();
            rt.slots[self.id.0 as usize].value.downcast_ref::<T>().expect("Signal type mismatch").clone()
        })
    }

    pub fn subscribe_element(&self, element_id: ElementId) {
        RUNTIME.with(|rt| {
            let mut rt = rt.borrow_mut();
            let idx = self.id.0 as usize;
            if idx < rt.slots.len() {
                rt.slots[idx].subscribers.insert(element_id);
            }
        });
    }

    pub fn set(&self, new_value: T)
    where
        T: PartialEq + Send,
    {
        if is_main_thread() {
            RUNTIME.with(|rt| {
                let mut rt = rt.borrow_mut();
                let idx = self.id.0 as usize;
                {
                    let old = rt.slots[idx].value.downcast_ref::<T>().unwrap();
                    if *old == new_value { return; }
                }
                rt.slots[idx].value = Box::new(new_value);
                notify_subscribers(&mut rt, idx);
            });
        } else {
            let signal = *self;
            crate::async_runtime::run_on_main_thread(move || {
                signal.set(new_value);
            });
        }
    }

    pub fn set_always(&self, new_value: T)
    where
        T: Send,
    {
        if is_main_thread() {
            RUNTIME.with(|rt| {
                let mut rt = rt.borrow_mut();
                let idx = self.id.0 as usize;
                rt.slots[idx].value = Box::new(new_value);
                notify_subscribers(&mut rt, idx);
            });
        } else {
            let signal = *self;
            crate::async_runtime::run_on_main_thread(move || {
                signal.set_always(new_value);
            });
        }
    }

    pub fn update(&self, f: impl FnOnce(&mut T)) {
        debug_assert!(
            is_main_thread(),
            "RwSignal::update() called from a non-main thread. \
             Use set() or set_always() for cross-thread updates, \
             or wrap in run_on_main_thread()."
        );
        RUNTIME.with(|rt| {
            let mut rt = rt.borrow_mut();
            let idx = self.id.0 as usize;
            {
                let value = rt.slots[idx].value.downcast_mut::<T>().unwrap();
                f(value);
            }
            notify_subscribers(&mut rt, idx);
        });
    }
}

fn notify_subscribers(rt: &mut SignalRuntime, idx: usize) {
    let subscribers: Vec<ElementId> = rt.slots[idx].subscribers.iter().copied().collect();
    for elem_id in subscribers {
        rt.dirty_elements.insert(elem_id);
    }
    let effect_subs: Vec<EffectId> = rt.slots[idx].effect_subscribers.iter().copied().collect();
    for eid in effect_subs {
        rt.pending_effect_ids.insert(eid);
    }
    for notifier in &rt.notifiers {
        notifier.request_redraw();
    }
}

pub fn use_signal<T: 'static + Clone>(initial: T) -> RwSignal<T> {
    RUNTIME.with(|rt| {
        let mut rt = rt.borrow_mut();
        let id = SignalId(rt.slots.len() as u64);
        rt.slots.push(SignalSlot {
            value: Box::new(initial),
            subscribers: HashSet::new(),
            effect_subscribers: HashSet::new(),
        });
        RwSignal { id, _marker: PhantomData }
    })
}

pub struct Memo<T> {
    compute: Box<dyn Fn() -> T>,
}

impl<T: 'static + Clone> Memo<T> {
    pub fn get(&self) -> T {
        (self.compute)()
    }
}

pub fn create_memo<T: 'static + Clone>(compute: impl Fn() -> T + 'static) -> Memo<T> {
    Memo { compute: Box::new(compute) }
}

pub fn set_notifier(notifier: Arc<dyn RedrawNotifier>) {
    RUNTIME.with(|rt| {
        let mut rt = rt.borrow_mut();
        rt.notifiers.clear();
        rt.notifiers.push(notifier);
    });
}

pub fn add_notifier(notifier: Arc<dyn RedrawNotifier>) {
    RUNTIME.with(|rt| {
        rt.borrow_mut().notifiers.push(notifier);
    });
}

#[cfg(feature = "winit")]
pub fn set_window(window: Arc<crate::window::Window>) {
    PRIMARY_WINDOW.with(|cell| *cell.borrow_mut() = Some(window.clone()));
    set_notifier(window as Arc<dyn RedrawNotifier>);
}

#[cfg(feature = "winit")]
pub fn clear_window() {
    PRIMARY_WINDOW.with(|cell| *cell.borrow_mut() = None);
    RUNTIME.with(|rt| rt.borrow_mut().notifiers.clear());
}

#[cfg(feature = "winit")]
thread_local! {
    static PRIMARY_WINDOW: std::cell::RefCell<Option<Arc<crate::window::Window>>> =
        const { std::cell::RefCell::new(None) };
}

#[cfg(feature = "winit")]
pub fn primary_window() -> Option<Arc<crate::window::Window>> {
    PRIMARY_WINDOW.with(|cell| cell.borrow().clone())
}

/// Переключить полноэкранный режим главного окна. В браузере — Fullscreen
/// API для canvas: вызывать из обработчика действия пользователя (клик,
/// клавиша), иначе браузер отклонит запрос.
#[cfg(feature = "winit")]
pub fn toggle_fullscreen() {
    #[cfg(not(target_os = "android"))]
    if let Some(window) = primary_window() {
        let win = window.winit_window();
        let is_fs = win.fullscreen().is_some();
        win.set_fullscreen(if is_fs {
            None
        } else {
            Some(winit::window::Fullscreen::Borderless(None))
        });
        window.request_redraw();
    }
}

#[cfg(feature = "winit")]
pub fn add_window(window: Arc<crate::window::Window>) {
    add_notifier(window as Arc<dyn RedrawNotifier>);
}

pub fn begin_tracking(element_id: ElementId) {
    RUNTIME.with(|rt| {
        let mut rt = rt.borrow_mut();
        for slot in &mut rt.slots {
            slot.subscribers.remove(&element_id);
        }
        rt.tracking_stack.push(element_id);
    });
}

pub fn end_tracking() {
    RUNTIME.with(|rt| {
        rt.borrow_mut().tracking_stack.pop();
    });
}

pub fn has_dirty_elements() -> bool {
    RUNTIME.with(|rt| {
        !rt.borrow().dirty_elements.is_empty()
    })
}

pub fn dirty_element_ids() -> Vec<ElementId> {
    RUNTIME.with(|rt| rt.borrow().dirty_elements.iter().copied().collect())
}

pub fn is_element_dirty(element_id: ElementId) -> bool {
    RUNTIME.with(|rt| {
        rt.borrow().dirty_elements.contains(&element_id)
    })
}

pub fn clear_element_dirty(element_id: ElementId) {
    RUNTIME.with(|rt| {
        rt.borrow_mut().dirty_elements.remove(&element_id);
    });
}

pub fn mark_all_reactive_dirty() {
    RUNTIME.with(|rt| {
        let mut rt = rt.borrow_mut();
        let all_subscribers: Vec<ElementId> = rt.slots.iter()
            .flat_map(|slot| slot.subscribers.iter().copied())
            .collect();
        for elem_id in all_subscribers {
            rt.dirty_elements.insert(elem_id);
        }
        for notifier in &rt.notifiers {
            notifier.request_redraw();
        }
    });
}

pub fn cleanup_element(element_id: ElementId) {
    let cleanups: Vec<Box<dyn Fn()>> = RUNTIME.with(|rt| {
        let mut rt = rt.borrow_mut();
        rt.dirty_elements.remove(&element_id);
        for slot in &mut rt.slots {
            slot.subscribers.remove(&element_id);
        }
        let mut taken: Vec<Box<dyn Fn()>> = Vec::new();
        if let Some(effect_ids) = rt.element_effects.remove(&element_id) {
            for eid in effect_ids {
                let idx = eid.0 as usize;
                if idx < rt.effects.len() && rt.effects[idx].active {
                    if let Some(cleanup) = rt.effects[idx].cleanup.take() {
                        taken.push(cleanup);
                    }
                    rt.effects[idx].active = false;
                    let deps: Vec<SignalId> = rt.effects[idx].dependencies.drain().collect();
                    for sig_id in deps {
                        let sig_idx = sig_id.0 as usize;
                        if sig_idx < rt.slots.len() {
                            rt.slots[sig_idx].effect_subscribers.remove(&eid);
                        }
                    }
                    rt.pending_effect_ids.remove(&eid);
                }
            }
        }
        taken
    });
    for cleanup in cleanups {
        cleanup();
    }
}

pub fn begin_element_scope(element_id: ElementId) {
    RUNTIME.with(|rt| {
        rt.borrow_mut().element_scope_stack.push(element_id);
    });
}

pub fn end_element_scope() {
    RUNTIME.with(|rt| {
        rt.borrow_mut().element_scope_stack.pop();
    });
}

pub fn create_effect(f: impl Fn() + 'static) -> EffectId {
    create_effect_with_cleanup(move || {
        f();
        None
    })
}

pub fn create_effect_with_cleanup(
    f: impl Fn() -> Option<Box<dyn Fn()>> + 'static,
) -> EffectId {
    let effect_id = RUNTIME.with(|rt| {
        let mut rt = rt.borrow_mut();
        let id = EffectId(rt.effects.len() as u64);
        rt.effects.push(EffectSlot {
            closure: Box::new(f),
            dependencies: HashSet::new(),
            cleanup: None,
            active: true,
        });
        if let Some(&owner) = rt.element_scope_stack.last() {
            rt.element_effects.entry(owner).or_default().push(id);
        }
        id
    });
    run_effect(effect_id);
    effect_id
}

fn run_effect(effect_id: EffectId) {
    let prev_cleanup: Option<Box<dyn Fn()>> = RUNTIME.with(|rt| {
        let mut rt = rt.borrow_mut();
        let idx = effect_id.0 as usize;
        if !rt.effects[idx].active {
            return None;
        }
        let cleanup = rt.effects[idx].cleanup.take();
        let old_deps: Vec<SignalId> = rt.effects[idx].dependencies.drain().collect();
        for sig_id in old_deps {
            let sig_idx = sig_id.0 as usize;
            if sig_idx < rt.slots.len() {
                rt.slots[sig_idx].effect_subscribers.remove(&effect_id);
            }
        }
        cleanup
    });
    if let Some(cleanup) = prev_cleanup {
        cleanup();
    }
    let still_active = RUNTIME.with(|rt| {
        let rt = rt.borrow();
        let idx = effect_id.0 as usize;
        idx < rt.effects.len() && rt.effects[idx].active
    });
    if !still_active {
        return;
    }

    let closure_result = RUNTIME.with(|rt| {
        let closure_ptr: *const dyn Fn() -> Option<Box<dyn Fn()>>;
        {
            let rt = rt.borrow();
            let idx = effect_id.0 as usize;
            if !rt.effects[idx].active {
                return None;
            }
            closure_ptr = &*rt.effects[idx].closure as *const _;
        }
        let saved_stack = std::mem::take(&mut rt.borrow_mut().tracking_stack);
        rt.borrow_mut().effect_tracking = Some(effect_id);
        // SAFETY: The closure lives in the RUNTIME vec which we don't
        let result = unsafe { (*closure_ptr)() };
        rt.borrow_mut().effect_tracking = None;
        rt.borrow_mut().tracking_stack = saved_stack;
        Some(result)
    });

    if let Some(cleanup) = closure_result.flatten() {
        RUNTIME.with(|rt| {
            let mut rt = rt.borrow_mut();
            let idx = effect_id.0 as usize;
            rt.effects[idx].cleanup = Some(cleanup);
        });
    }
}

pub fn drain_and_run_effects() {
    loop {
        let pending: Vec<EffectId> = RUNTIME.with(|rt| {
            let mut rt = rt.borrow_mut();
            rt.pending_effect_ids.drain().collect()
        });
        if pending.is_empty() {
            break;
        }
        for eid in pending {
            run_effect(eid);
        }
    }
}

pub fn use_effect(f: impl Fn() + 'static) -> EffectId {
    create_effect(f)
}

pub fn use_effect_with_cleanup(
    f: impl Fn() -> Option<Box<dyn Fn()>> + 'static,
) -> EffectId {
    create_effect_with_cleanup(f)
}

pub fn dispose_effect(effect_id: EffectId) {
    let cleanup: Option<Box<dyn Fn()>> = RUNTIME.with(|rt| {
        let mut rt = rt.borrow_mut();
        let idx = effect_id.0 as usize;
        if idx >= rt.effects.len() || !rt.effects[idx].active {
            return None;
        }
        let cleanup = rt.effects[idx].cleanup.take();
        rt.effects[idx].active = false;
        let deps: Vec<SignalId> = rt.effects[idx].dependencies.drain().collect();
        for sig_id in deps {
            let sig_idx = sig_id.0 as usize;
            if sig_idx < rt.slots.len() {
                rt.slots[sig_idx].effect_subscribers.remove(&effect_id);
            }
        }
        rt.pending_effect_ids.remove(&effect_id);
        cleanup
    });
    if let Some(cleanup) = cleanup {
        cleanup();
    }
}
