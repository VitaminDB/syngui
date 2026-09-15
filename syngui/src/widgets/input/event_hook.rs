//! `EventHook` — прозрачная обёртка для перехвата клавиатуры/пульта на
//! уровне экрана, и `RemoteKey` — нормализованные кнопки ТВ-пульта.
//!
//! Обёртка не рисует ничего и отдаёт ребёнку всё место. События, которые ни
//! один элемент ниже не обработал, всплывают к ней (см. `dispatch_event` /
//! `dispatch_focus_bubble` в `widget/tree/event.rs`), поэтому её ставят у
//! корня экрана — глобальные хоткеи, D-pad-навигация на Android TV, «назад».
//!
//! ```ignore
//! EventHook::new()
//!     .on_remote(move |key| match key {
//!         RemoteKey::Left => { focus.update(|f| f.col = f.col.saturating_sub(1)); true }
//!         RemoteKey::Back => { router.back(); true }
//!         _ => false,
//!     })
//!     .child(screen)
//! ```

use std::any::Any;
use std::sync::Arc;
use std::time::Duration;

use crate::core::{Point, Rect, Size};
use crate::input::{Event, EventResult, Key, Modifiers};
use crate::layout::Constraints;
use crate::mss::{ComputedStyle, MssFields};
use crate::render::DisplayList;
use crate::widget::context::EventContext;
use crate::widget::{
    DirtyFlags, Element, ElementId, ElementTree, LayoutHint, StyledElement, UpdateContext, Widget,
};
use crate::widgets::containers::IntoWidget;

/// Кнопки ТВ-пульта / геймпада / клавиатуры, сведённые к одному набору.
///
/// Android TV: KEYCODE_DPAD_* приходят через winit как стрелки,
/// DPAD_CENTER — как Enter, KEYCODE_BACK — как `Event::BackPressed`.
/// На desktop то же даёт клавиатура: стрелки, Enter, Escape/Backspace,
/// пробел (play/pause).
#[derive(Clone, Copy, PartialEq, Eq, Debug, Hash)]
pub enum RemoteKey {
    Up,
    Down,
    Left,
    Right,
    /// OK / DPAD_CENTER / Enter.
    Select,
    /// KEYCODE_BACK / Escape / Backspace.
    Back,
    /// Play/Pause медиа-кнопка или пробел.
    PlayPause,
    /// Перемотка назад (media rewind / previous).
    Rewind,
    /// Перемотка вперёд (media fast-forward / next).
    FastForward,
    /// Меню / контекст (`M`, ContextMenu).
    Menu,
}

impl RemoteKey {
    /// Сопоставить событие фреймворка кнопке пульта. `None` — событие не
    /// про навигацию (символ, мышь и т.д.).
    pub fn from_event(event: &Event) -> Option<Self> {
        match event {
            Event::KeyDown(key) => Self::from_key(*key),
            Event::BackPressed => Some(Self::Back),
            _ => None,
        }
    }

    pub fn from_key(key: Key) -> Option<Self> {
        Some(match key {
            Key::Up => Self::Up,
            Key::Down => Self::Down,
            Key::Left => Self::Left,
            Key::Right => Self::Right,
            Key::Enter => Self::Select,
            Key::Escape | Key::Backspace => Self::Back,
            Key::Space | Key::MediaPlayPause => Self::PlayPause,
            Key::MediaRewind | Key::MediaPrevious => Self::Rewind,
            Key::MediaFastForward | Key::MediaNext => Self::FastForward,
            Key::M | Key::ContextMenu => Self::Menu,
            _ => return None,
        })
    }
}

#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum KeyReply {
    /// Не наше — пусть всплывает дальше.
    Ignore,
    /// Обработано.
    Handled,
    /// Обработано + прокрутить обёртку в видимую область.
    ScrollIntoView,
}

type KeyHandler = Arc<dyn Fn(Key, Modifiers) -> KeyReply + Send + Sync>;
type RemoteHandler = Arc<dyn Fn(RemoteKey) -> bool + Send + Sync>;
type BackHandler = Arc<dyn Fn() -> bool + Send + Sync>;
type CharHandler = Arc<dyn Fn(char) -> bool + Send + Sync>;

type LifecycleHandler = Arc<dyn Fn() + Send + Sync>;

pub struct EventHook {
    on_key_down: Option<KeyHandler>,
    on_key_up: Option<KeyHandler>,
    on_remote: Option<RemoteHandler>,
    on_back: Option<BackHandler>,
    on_char: Option<CharHandler>,
    on_suspend: Option<LifecycleHandler>,
    on_resume: Option<LifecycleHandler>,
    bounds_out: Option<Arc<crate::core::sync::Mutex<Rect>>>,
    child: Option<Box<dyn Widget>>,
}

impl EventHook {
    pub fn new() -> Self {
        Self {
            on_key_down: None,
            on_key_up: None,
            on_remote: None,
            on_back: None,
            on_char: None,
            on_suspend: None,
            on_resume: None,
            bounds_out: None,
            child: None,
        }
    }

    /// Приложение уходит в фон (`Event::AppSuspended`): на Android
    /// пропадают поверхность окна, видео-Surface и аудио-поток — самое
    /// время остановить плеер и запомнить позицию.
    pub fn on_suspend(mut self, handler: impl Fn() + Send + Sync + 'static) -> Self {
        self.on_suspend = Some(Arc::new(handler));
        self
    }

    /// Приложение вернулось на экран (`Event::AppResumed`).
    pub fn on_resume(mut self, handler: impl Fn() + Send + Sync + 'static) -> Self {
        self.on_resume = Some(Arc::new(handler));
        self
    }

    /// Ввод символа (`Event::CharInput`): физическая клавиатура на desktop.
    /// Backspace приходит как `'\u{8}'`, Enter — как `'\r'`/`'\n'`.
    pub fn on_char(mut self, handler: impl Fn(char) -> bool + Send + Sync + 'static) -> Self {
        self.on_char = Some(Arc::new(handler));
        self
    }

    /// Любая клавиша (с модификаторами) — для хоткеев вроде Ctrl+K.
    pub fn on_key_down(
        mut self,
        handler: impl Fn(Key, Modifiers) -> KeyReply + Send + Sync + 'static,
    ) -> Self {
        self.on_key_down = Some(Arc::new(handler));
        self
    }

    pub fn on_key_up(
        mut self,
        handler: impl Fn(Key, Modifiers) -> KeyReply + Send + Sync + 'static,
    ) -> Self {
        self.on_key_up = Some(Arc::new(handler));
        self
    }

    /// Кнопки пульта (см. [`RemoteKey`]). Вернуть `true` — событие съедено.
    /// Проверяется после `on_key_down`, если тот вернул `Ignore`.
    pub fn on_remote(mut self, handler: impl Fn(RemoteKey) -> bool + Send + Sync + 'static) -> Self {
        self.on_remote = Some(Arc::new(handler));
        self
    }

    /// Только «назад» (`Event::BackPressed`, Escape). Удобно для экранов
    /// без D-pad-навигации. Если задан `on_remote`, он получит
    /// `RemoteKey::Back` первым.
    pub fn on_back(mut self, handler: impl Fn() -> bool + Send + Sync + 'static) -> Self {
        self.on_back = Some(Arc::new(handler));
        self
    }

    /// Публиковать собственные границы (для позиционирования поповеров).
    pub fn report_bounds(mut self, out: Arc<crate::core::sync::Mutex<Rect>>) -> Self {
        self.bounds_out = Some(out);
        self
    }

    pub fn child<M>(mut self, child: impl IntoWidget<M>) -> Self {
        self.child = Some(child.into_widget());
        self
    }
}

impl Default for EventHook {
    fn default() -> Self {
        Self::new()
    }
}

impl Widget for EventHook {
    fn create_element(&self) -> Box<dyn Element> {
        Box::new(EventHookElement {
            id: ElementId::new(),
            on_key_down: self.on_key_down.clone(),
            on_key_up: self.on_key_up.clone(),
            on_remote: self.on_remote.clone(),
            on_back: self.on_back.clone(),
            on_char: self.on_char.clone(),
            on_suspend: self.on_suspend.clone(),
            on_resume: self.on_resume.clone(),
            bounds_out: self.bounds_out.clone(),
            has_child: self.child.is_some(),
            bounds: Rect::zero(),
            classes: Vec::new(),
            dirty_flags: DirtyFlags::LAYOUT | DirtyFlags::RENDER,
            mss: MssFields::new(),
        })
    }

    fn can_update(&self, other: &dyn Any) -> bool {
        other.is::<Self>()
    }

    fn as_any(&self) -> &dyn Any {
        self
    }

    fn as_any_mut(&mut self) -> &mut dyn Any {
        self
    }

    fn mount(&self, tree: &mut ElementTree, parent_id: ElementId) {
        if let Some(child) = &self.child {
            let element = child.create_element();
            let child_id =
                tree.insert_with_type_id(element, Some(parent_id), child.as_any().type_id());
            child.mount(tree, child_id);
        }
    }

    fn child_widgets(&self) -> Vec<&dyn Widget> {
        self.child
            .as_ref()
            .map(|c| vec![c.as_ref() as &dyn Widget])
            .unwrap_or_default()
    }
}

struct EventHookElement {
    id: ElementId,
    on_key_down: Option<KeyHandler>,
    on_key_up: Option<KeyHandler>,
    on_remote: Option<RemoteHandler>,
    on_back: Option<BackHandler>,
    on_char: Option<CharHandler>,
    on_suspend: Option<LifecycleHandler>,
    on_resume: Option<LifecycleHandler>,
    bounds_out: Option<Arc<crate::core::sync::Mutex<Rect>>>,
    has_child: bool,
    bounds: Rect,
    classes: Vec<String>,
    dirty_flags: DirtyFlags,
    mss: MssFields,
}

impl EventHookElement {
    fn publish_bounds(&self) {
        if let Some(out) = &self.bounds_out {
            if let Ok(mut rect) = out.lock() {
                *rect = self.bounds;
            }
        }
    }
}

impl Element for EventHookElement {
    fn update(&mut self, widget: &dyn Widget, _ctx: &mut UpdateContext) {
        if let Some(hook) = widget.as_any().downcast_ref::<EventHook>() {
            self.on_key_down = hook.on_key_down.clone();
            self.on_key_up = hook.on_key_up.clone();
            self.on_remote = hook.on_remote.clone();
            self.on_back = hook.on_back.clone();
            self.on_char = hook.on_char.clone();
            self.on_suspend = hook.on_suspend.clone();
            self.on_resume = hook.on_resume.clone();
            self.bounds_out = hook.bounds_out.clone();
            self.has_child = hook.child.is_some();
            self.publish_bounds();
        }
    }

    fn layout(&mut self, constraints: Constraints) -> Size {
        if !self.has_child {
            self.bounds = Rect::new(self.bounds.origin, Size::zero());
            return Size::zero();
        }
        let w = if constraints.max_width.is_finite() {
            constraints.max_width
        } else {
            0.0
        };
        let h = if constraints.max_height.is_finite() {
            constraints.max_height
        } else {
            0.0
        };
        self.bounds = Rect::new(self.bounds.origin, Size::new(w, h));
        Size::new(w, h)
    }

    fn build_display_list(&self, _list: &mut DisplayList, _clip: Rect) {}

    fn handle_event(&mut self, event: &Event, ctx: &mut EventContext) -> EventResult {
        match event {
            Event::AppSuspended => {
                if let Some(h) = self.on_suspend.as_ref() {
                    h();
                }
                // Не Handled: событие должны увидеть все хуки дерева.
                return EventResult::Ignored;
            }
            Event::AppResumed => {
                if let Some(h) = self.on_resume.as_ref() {
                    h();
                }
                return EventResult::Ignored;
            }
            _ => {}
        }
        if let Event::CharInput(c) = event {
            return match self.on_char.as_ref() {
                Some(h) if h(*c) => EventResult::Handled,
                _ => EventResult::Ignored,
            };
        }
        // Backspace/Enter с физической клавиатуры — тоже в on_char, чтобы
        // экранная клавиатура на desktop правила текст, а не закрывала экран.
        if let (Event::KeyDown(Key::Backspace), Some(h)) = (event, self.on_char.as_ref()) {
            if h('\u{8}') {
                return EventResult::Handled;
            }
        }
        let raw = match event {
            Event::KeyDown(key) => self.on_key_down.as_ref().map(|h| h(*key, ctx.modifiers)),
            Event::KeyUp(key) => self.on_key_up.as_ref().map(|h| h(*key, ctx.modifiers)),
            _ => None,
        };
        match raw {
            Some(KeyReply::Handled) => return EventResult::Handled,
            Some(KeyReply::ScrollIntoView) => {
                ctx.scroll_into_view(self.bounds);
                return EventResult::Handled;
            }
            Some(KeyReply::Ignore) | None => {}
        }

        let Some(remote) = RemoteKey::from_event(event) else {
            return EventResult::Ignored;
        };
        if let Some(h) = self.on_remote.as_ref() {
            if h(remote) {
                return EventResult::Handled;
            }
        }
        if remote == RemoteKey::Back {
            if let Some(h) = self.on_back.as_ref() {
                if h() {
                    return EventResult::Handled;
                }
            }
        }
        EventResult::Ignored
    }

    fn animate(&mut self, _dt: Duration) -> bool {
        false
    }

    fn needs_repaint(&self) -> bool {
        false
    }

    fn children(&self) -> &[ElementId] {
        &[]
    }

    fn bounds(&self) -> Rect {
        self.bounds
    }

    fn set_position(&mut self, pos: Point) {
        self.bounds.origin = pos;
        self.publish_bounds();
    }

    fn set_content_size(&mut self, size: Size) {
        self.bounds = Rect::new(self.bounds.origin, size);
        self.publish_bounds();
    }

    fn mark_dirty(&mut self, flags: DirtyFlags) {
        self.dirty_flags |= flags;
    }

    fn clear_dirty(&mut self, flags: DirtyFlags) {
        self.dirty_flags.remove(flags);
    }

    fn is_dirty(&self, flags: DirtyFlags) -> bool {
        self.dirty_flags.contains(flags)
    }

    fn id(&self) -> ElementId {
        self.id
    }

    fn set_id(&mut self, id: ElementId) {
        self.id = id;
    }

    fn mount(&mut self, _tree: &mut ElementTree) {}

    fn element_type_name(&self) -> &str {
        "EventHook"
    }

    fn layout_hint(&self) -> LayoutHint {
        LayoutHint::Padding {
            left: 0.0,
            top: 0.0,
            right: 0.0,
            bottom: 0.0,
        }
    }

    fn passthrough_hit_test(&self) -> bool {
        false
    }

    fn set_classes(&mut self, classes: Vec<String>) {
        self.classes = classes;
        self.mark_dirty(DirtyFlags::RENDER);
    }

    fn get_classes(&self) -> &[String] {
        &self.classes
    }

    fn reset_mss_styles(&mut self) {
        self.mss.reset();
    }

    fn apply_computed_style(&mut self, style: &ComputedStyle) {
        self.mss.apply(style);
        self.mark_dirty(DirtyFlags::LAYOUT | DirtyFlags::RENDER);
    }

    fn apply_transition_styles(
        &mut self,
        base: &ComputedStyle,
        hover: Option<&ComputedStyle>,
        active: Option<&ComputedStyle>,
        focus: Option<&ComputedStyle>,
        selected: Option<&ComputedStyle>,
        _checked: Option<&ComputedStyle>,
    ) {
        self.mss
            .apply_transitions(base, hover, active, focus, selected);
    }
}

impl StyledElement for EventHookElement {
    fn apply_style(&mut self, _style: &ComputedStyle) {
        self.mark_dirty(DirtyFlags::RENDER);
    }

    fn classes(&self) -> &[String] {
        &self.classes
    }

    fn set_classes(&mut self, classes: Vec<String>) {
        self.classes = classes;
        self.mark_dirty(DirtyFlags::RENDER);
    }
}
