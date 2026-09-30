//! `LiveView` — живой кадр извне: удалённый экран, камера, захват. Кадр —
//! общий буфер [`LiveFrame`] (RGBA, любой поток пишет, номер версии
//! растёт); виджет заливает в GPU новую версию, когда его перестраивают
//! (обычно `rx` по сигналу-счётчику кадров), — без роста `ImageStore` и
//! без постоянного тика анимаций.
//!
//! Ввод над кадром отдаётся колбэком [`LiveView::on_input`] в долях кадра
//! (0..1 по ширине и высоте, с учётом вписывания `fit`): мышь, колесо,
//! касания, клавиши и символы. Нажатие берёт захват — перетаскивание за
//! пределы кадра продолжает приходить (с долями за 0..1).

use std::any::Any;
use std::sync::Arc;

use crate::core::sync::Mutex;
use crate::core::{Color, Point, Rect, RectExt, Size};
use crate::gpu::image_store::{ImageHandle, ImageStore};
use crate::input::{Event, EventResult, Key, MouseButton};
use crate::layout::Constraints;
use crate::mss::{ComputedStyle, MssFields};
use crate::render::{DisplayList, TextureId};
use crate::widget::{DirtyFlags, Element, ElementId, ElementTree, StyledElement, UpdateContext, Widget};
use crate::widgets::ImageFit;

struct Inner {
    width: u32,
    height: u32,
    rgba: Arc<[u8]>,
    version: u64,
}

/// Общий кадр: пишет источник (любой поток), показывает [`LiveView`].
pub struct LiveFrame {
    inner: std::sync::Mutex<Inner>,
}

impl LiveFrame {
    pub fn new() -> Arc<Self> {
        Arc::new(Self { inner: std::sync::Mutex::new(Inner { width: 0, height: 0, rgba: Arc::from(Vec::new()), version: 0 }) })
    }

    /// Новый кадр целиком (RGBA, `width * height * 4` байт).
    pub fn set(&self, width: u32, height: u32, rgba: Arc<[u8]>) {
        let mut g = self.inner.lock().unwrap();
        g.width = width;
        g.height = height;
        g.rgba = rgba;
        g.version += 1;
    }

    /// Размер кадра (0×0 — кадра ещё нет).
    pub fn size(&self) -> (u32, u32) {
        let g = self.inner.lock().unwrap();
        (g.width, g.height)
    }

    pub fn version(&self) -> u64 {
        self.inner.lock().unwrap().version
    }

    fn snapshot(&self) -> (u32, u32, Arc<[u8]>, u64) {
        let g = self.inner.lock().unwrap();
        (g.width, g.height, g.rgba.clone(), g.version)
    }
}

/// Ввод над кадром; `x`, `y` — доли кадра.
#[derive(Debug, Clone, PartialEq)]
pub enum LiveInput {
    Move { x: f32, y: f32 },
    Down { button: MouseButton, x: f32, y: f32 },
    Up { button: MouseButton, x: f32, y: f32 },
    /// Колесо: `dy` > 0 — вниз, шаги (как у `Event::MouseWheel`, со знаком наоборот).
    Wheel { dx: f32, dy: f32, x: f32, y: f32 },
    TouchStart { id: u64, x: f32, y: f32 },
    TouchMove { id: u64, x: f32, y: f32 },
    TouchEnd { id: u64, x: f32, y: f32 },
    Key { key: Key, pressed: bool },
    Char(char),
}

type InputFn = Arc<dyn Fn(LiveInput) + Send + Sync>;

pub struct LiveView {
    frame: Arc<LiveFrame>,
    fit: ImageFit,
    classes: Vec<String>,
    on_input: Option<InputFn>,
}

impl LiveView {
    pub fn new(frame: Arc<LiveFrame>) -> Self {
        Self { frame, fit: ImageFit::Contain, classes: Vec::new(), on_input: None }
    }

    pub fn fit(mut self, fit: ImageFit) -> Self {
        self.fit = fit;
        self
    }

    pub fn class(mut self, c: impl Into<String>) -> Self {
        crate::widget::push_classes(&mut self.classes, c.into());
        self
    }

    pub fn on_input(mut self, f: impl Fn(LiveInput) + Send + Sync + 'static) -> Self {
        self.on_input = Some(Arc::new(f));
        self
    }
}

impl Widget for LiveView {
    fn create_element(&self) -> Box<dyn Element> {
        Box::new(LiveViewElement {
            id: ElementId::new(),
            frame: self.frame.clone(),
            fit: self.fit,
            classes: self.classes.clone(),
            on_input: self.on_input.clone(),
            bounds: Rect::zero(),
            dirty_flags: DirtyFlags::LAYOUT | DirtyFlags::RENDER,
            image_handle: None,
            image_store: None,
            natural: (0, 0),
            shown: 0,
            mss: MssFields::new(),
            pressed: false,
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

    fn mount(&self, _tree: &mut ElementTree, _parent_id: ElementId) {}

    fn widget_classes(&self) -> &[String] {
        &self.classes
    }
}

pub struct LiveViewElement {
    id: ElementId,
    frame: Arc<LiveFrame>,
    fit: ImageFit,
    classes: Vec<String>,
    on_input: Option<InputFn>,
    bounds: Rect,
    dirty_flags: DirtyFlags,
    image_handle: Option<ImageHandle>,
    image_store: Option<Arc<Mutex<ImageStore>>>,
    natural: (u32, u32),
    /// Показанная версия кадра.
    shown: u64,
    mss: MssFields,
    pressed: bool,
}

impl LiveViewElement {
    /// Залить новую версию кадра, если она есть.
    fn sync(&mut self) {
        let (w, h, rgba, v) = self.frame.snapshot();
        if v == self.shown || w == 0 || h == 0 || rgba.len() < (w as usize) * (h as usize) * 4 {
            return;
        }
        let Some(store) = self.image_store.as_ref() else { return };
        if let Ok(mut s) = store.lock() {
            match self.image_handle {
                Some(handle) => s.update_rgba(handle, w, h, rgba),
                None => {
                    let key = format!("live:{:p}:{}", Arc::as_ptr(&self.frame), self.id.0);
                    let (handle, _) = s.request_rgba(&key, w, h, rgba.to_vec());
                    self.image_handle = Some(handle);
                }
            }
        }
        self.shown = v;
        if (w, h) != self.natural {
            self.natural = (w, h);
            self.mark_dirty(DirtyFlags::LAYOUT | DirtyFlags::RENDER);
        } else {
            self.mark_dirty(DirtyFlags::RENDER);
        }
    }

    fn fit_rect(&self) -> Rect {
        let (nw, nh) = self.natural;
        if nw == 0 || nh == 0 {
            return self.bounds;
        }
        let (nw, nh) = (nw as f32, nh as f32);
        let (bw, bh) = (self.bounds.size.width, self.bounds.size.height);
        let (sw, sh) = match self.fit {
            ImageFit::Fill => (bw, bh),
            ImageFit::None => (nw, nh),
            ImageFit::Contain => {
                let k = (bw / nw).min(bh / nh);
                (nw * k, nh * k)
            }
            ImageFit::Cover => {
                let k = (bw / nw).max(bh / nh);
                (nw * k, nh * k)
            }
        };
        Rect::new(Point::new(self.bounds.x() + (bw - sw) / 2.0, self.bounds.y() + (bh - sh) / 2.0), Size::new(sw, sh))
    }

    fn frac(&self, p: Point) -> (f32, f32) {
        let r = self.fit_rect();
        let w = r.size.width.max(1.0);
        let h = r.size.height.max(1.0);
        ((p.x - r.x()) / w, (p.y - r.y()) / h)
    }

    fn inside(&self, p: Point) -> bool {
        self.fit_rect().contains(p)
    }

    fn emit(&self, e: LiveInput) {
        if let Some(f) = &self.on_input {
            f(e);
        }
    }
}

impl Element for LiveViewElement {
    fn update(&mut self, widget: &dyn Widget, _ctx: &mut UpdateContext) {
        if let Some(v) = widget.as_any().downcast_ref::<LiveView>() {
            if !Arc::ptr_eq(&self.frame, &v.frame) {
                self.frame = v.frame.clone();
                self.shown = 0;
            }
            self.fit = v.fit;
            self.on_input = v.on_input.clone();
            if self.classes != v.classes {
                self.classes = v.classes.clone();
            }
            self.sync();
            self.mark_dirty(DirtyFlags::RENDER);
        }
    }

    fn layout(&mut self, constraints: Constraints) -> Size {
        let (nw, nh) = self.natural;
        let (max_w, max_h) = (constraints.max_width, constraints.max_height);
        let width = match self.mss.width {
            Some(d) => d.resolve(max_w).min(max_w),
            None if max_w.is_finite() => max_w,
            None => (nw as f32).max(1.0),
        };
        let height = match self.mss.height {
            Some(d) => d.resolve(max_h).min(max_h),
            None if max_h.is_finite() => max_h,
            None if nw > 0 => width * nh as f32 / nw as f32,
            None => 1.0,
        };
        self.bounds = Rect::new(self.bounds.origin, Size::new(width, height));
        Size::new(width, height)
    }

    fn build_display_list(&self, list: &mut DisplayList, _clip: Rect) {
        let bg = self.mss.background_color.unwrap_or_else(|| Color::from_hex("#000000"));
        list.push_rect(self.bounds, bg, [0.0; 4]);
        if let Some(handle) = self.image_handle {
            let uv = Rect::new(Point::new(0.0, 0.0), Size::new(1.0, 1.0));
            list.push_image(self.fit_rect(), TextureId(handle.0), uv, Color::WHITE);
        }
    }

    fn handle_event(&mut self, event: &Event, ctx: &mut crate::widget::context::EventContext) -> EventResult {
        if self.on_input.is_none() {
            return EventResult::Ignored;
        }
        match event {
            Event::MouseMove(p) => {
                if self.pressed || self.inside(*p) {
                    let (x, y) = self.frac(*p);
                    self.emit(LiveInput::Move { x, y });
                    return EventResult::Handled;
                }
            }
            Event::MouseDown { button, position } if self.inside(*position) => {
                let (x, y) = self.frac(*position);
                self.pressed = true;
                ctx.capture();
                self.emit(LiveInput::Down { button: *button, x, y });
                return EventResult::Handled;
            }
            Event::MouseUp { button, position } if self.pressed => {
                let (x, y) = self.frac(*position);
                self.pressed = false;
                self.emit(LiveInput::Up { button: *button, x, y });
                return EventResult::Handled;
            }
            Event::MouseWheel { delta, delta_x, position } if self.inside(*position) => {
                let (x, y) = self.frac(*position);
                self.emit(LiveInput::Wheel { dx: -*delta_x, dy: -*delta, x, y });
                return EventResult::Handled;
            }
            Event::TouchStart { id, position } if self.inside(*position) => {
                let (x, y) = self.frac(*position);
                ctx.capture();
                self.emit(LiveInput::TouchStart { id: *id, x, y });
                return EventResult::Handled;
            }
            Event::TouchMove { id, position } => {
                let (x, y) = self.frac(*position);
                self.emit(LiveInput::TouchMove { id: *id, x, y });
                return EventResult::Handled;
            }
            Event::TouchEnd { id, position } => {
                let (x, y) = self.frac(*position);
                self.emit(LiveInput::TouchEnd { id: *id, x, y });
                return EventResult::Handled;
            }
            Event::KeyDown(k) => {
                self.emit(LiveInput::Key { key: *k, pressed: true });
                return EventResult::Handled;
            }
            Event::KeyUp(k) => {
                self.emit(LiveInput::Key { key: *k, pressed: false });
                return EventResult::Handled;
            }
            Event::CharInput(c) => {
                self.emit(LiveInput::Char(*c));
                return EventResult::Handled;
            }
            _ => {}
        }
        EventResult::Ignored
    }

    fn children(&self) -> &[ElementId] {
        &[]
    }

    fn bounds(&self) -> Rect {
        self.bounds
    }

    fn set_position(&mut self, pos: Point) {
        self.bounds.origin = pos;
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

    fn mount(&mut self, tree: &mut ElementTree) {
        self.image_store = tree.image_store.clone();
        self.sync();
    }

    fn set_classes(&mut self, classes: Vec<String>) {
        self.classes = classes;
        self.mark_dirty(DirtyFlags::RENDER);
    }

    fn get_classes(&self) -> &[String] {
        &self.classes
    }

    fn element_type_name(&self) -> &str {
        "LiveView"
    }

    fn mss(&self) -> Option<&crate::mss::MssFields> {
        Some(&self.mss)
    }

    fn reset_mss_styles(&mut self) {
        self.mss.reset();
    }

    fn apply_computed_style(&mut self, style: &ComputedStyle) {
        self.mss.apply(style);
        self.mark_dirty(DirtyFlags::RENDER | DirtyFlags::LAYOUT);
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
        self.mss.apply_transitions(base, hover, active, focus, selected);
    }
}

impl StyledElement for LiveViewElement {
    fn apply_style(&mut self, _style: &ComputedStyle) {
        self.mark_dirty(DirtyFlags::RENDER | DirtyFlags::LAYOUT);
    }

    fn classes(&self) -> &[String] {
        &self.classes
    }

    fn set_classes(&mut self, classes: Vec<String>) {
        self.classes = classes;
        self.mark_dirty(DirtyFlags::RENDER);
    }
}
