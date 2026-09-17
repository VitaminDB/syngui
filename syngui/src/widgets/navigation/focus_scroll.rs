//! `FocusScroll` — горизонтальная лента для пульта: содержимое (обычно `Row`)
//! шире своей области сдвигается так, чтобы элемент с индексом `focus` был
//! виден целиком, с запасом `peek` до края — край соседа подсказывает, что
//! лента продолжается. Сдвиг «липкий», как у списка: пока фокус в видимой
//! части, лента стоит, а выйдя за край, доезжает ровно настолько, насколько
//! нужно. Ширины элементов любые — берутся из раскладки, а не из констант.
//!
//! ```ignore
//! FocusScroll::new()
//!     .focus(focused_index)          // индекс ребёнка содержимого
//!     .peek(64.0)
//!     .child(Row::new().gap(8.0).children(chips))
//!     .class("opt-strip")            // в колонке высота по содержимому, иначе — height в MSS
//! ```
//!
//! Мышь и колесо не прокручивают: это лента для D-pad (фокус хранит
//! приложение, например в [`super::GridFocus`]).

use crate::core::Transform;
use crate::core::{Point, Rect, Size};
use crate::input::{Event, EventResult};
use crate::layout::Constraints;
use crate::mss::{ComputedStyle, MssFields};
use crate::render::DisplayList;
use crate::widget::{
    DirtyFlags, Element, ElementId, ElementTree, LayoutHint, StyledElement, UpdateContext, Widget,
};
use crate::widgets::containers::IntoWidget;
use std::any::Any;
use std::time::Duration;

pub struct FocusScroll {
    child: Option<Box<dyn Widget>>,
    focus: Option<usize>,
    peek: f32,
    smooth: bool,
}

impl Default for FocusScroll {
    fn default() -> Self {
        Self::new()
    }
}

impl FocusScroll {
    pub fn new() -> Self {
        Self {
            child: None,
            focus: None,
            peek: 48.0,
            smooth: true,
        }
    }

    /// Какой элемент содержимого держать в виду; `None` — не двигать ленту.
    pub fn focus(mut self, index: impl Into<Option<usize>>) -> Self {
        self.focus = index.into();
        self
    }

    /// Запас до края области, px (по умолчанию 48).
    pub fn peek(mut self, px: f32) -> Self {
        self.peek = px.max(0.0);
        self
    }

    /// Плавная прокрутка (по умолчанию да); `false` — прыжком.
    pub fn smooth(mut self, smooth: bool) -> Self {
        self.smooth = smooth;
        self
    }

    pub fn child<M>(mut self, child: impl IntoWidget<M>) -> Self {
        self.child = Some(child.into_widget());
        self
    }
}

impl Widget for FocusScroll {
    fn create_element(&self) -> Box<dyn Element> {
        Box::new(FocusScrollElement {
            id: ElementId::new(),
            bounds: Rect::zero(),
            focus: self.focus,
            peek: self.peek,
            smooth: self.smooth,
            content_width: 0.0,
            offset: 0.0,
            target: 0.0,
            placed: false,
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
            let child_element = child.create_element();
            let child_id =
                tree.insert_with_type_id(child_element, Some(parent_id), child.as_any().type_id());
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

/// Новый сдвиг ленты, чтобы `[start, end]` (координаты в содержимом) был
/// виден с запасом `peek`; сдвиг меняется, только если элемент за краем.
fn sticky_offset(offset: f32, viewport: f32, content: f32, start: f32, end: f32, peek: f32) -> f32 {
    let max = (content - viewport).max(0.0);
    // Запас не больше, чем помещается рядом с самим элементом.
    let peek = peek.min(((viewport - (end - start)) / 2.0).max(0.0));
    let mut off = offset;
    if end + peek > off + viewport {
        off = end + peek - viewport;
    }
    if start - peek < off {
        off = start - peek;
    }
    off.clamp(0.0, max)
}

pub struct FocusScrollElement {
    id: ElementId,
    bounds: Rect,
    focus: Option<usize>,
    peek: f32,
    smooth: bool,
    content_width: f32,
    /// Текущий сдвиг содержимого влево и цель плавной прокрутки.
    offset: f32,
    target: f32,
    /// Сдвиг уже выставлялся: первый показ — сразу на месте, без анимации.
    placed: bool,
    classes: Vec<String>,
    dirty_flags: DirtyFlags,
    mss: MssFields,
}

impl FocusScrollElement {
    fn animating(&self) -> bool {
        (self.offset - self.target).abs() > 0.5
    }
}

impl Element for FocusScrollElement {
    fn update(&mut self, widget: &dyn Widget, _ctx: &mut UpdateContext) {
        if let Some(fs) = widget.as_any().downcast_ref::<FocusScroll>() {
            if self.focus != fs.focus || self.peek != fs.peek {
                self.focus = fs.focus;
                self.peek = fs.peek;
                self.mark_dirty(DirtyFlags::LAYOUT | DirtyFlags::RENDER);
            }
            self.smooth = fs.smooth;
        }
    }

    fn layout(&mut self, constraints: Constraints) -> Size {
        let size = Size::new(constraints.max_width, constraints.max_height);
        self.bounds = Rect::new(self.bounds.origin, size);
        size
    }

    fn layout_hint(&self) -> LayoutHint {
        LayoutHint::Scroll {
            left: 0.0,
            top: 0.0,
            right: 0.0,
            bottom: 0.0,
            unbounded_width: true,
            unbounded_height: false,
        }
    }

    fn set_content_size(&mut self, size: Size) {
        self.content_width = size.width;
    }

    fn wants_content_child_rects(&self) -> bool {
        true
    }

    fn set_content_child_rects(&mut self, rects: &[Rect]) {
        let viewport = self.bounds.size.width;
        let max = (self.content_width - viewport).max(0.0);
        let target = match self.focus.and_then(|i| rects.get(i)) {
            Some(r) => {
                let start = r.origin.x - self.bounds.origin.x;
                sticky_offset(
                    self.target,
                    viewport,
                    self.content_width,
                    start,
                    start + r.size.width,
                    self.peek,
                )
            }
            None => self.target.clamp(0.0, max),
        };
        if (target - self.target).abs() > 0.01 || !self.placed {
            self.target = target;
            if !self.smooth || !self.placed {
                self.offset = target;
            }
            self.placed = true;
            self.mark_dirty(DirtyFlags::RENDER | DirtyFlags::PAINT);
        }
    }

    fn animate(&mut self, dt: Duration) -> bool {
        if !self.animating() {
            self.offset = self.target;
            return false;
        }
        // Экспоненциальное приближение: ~150 мс до цели при любом шаге кадра.
        let k = 1.0 - (-dt.as_secs_f32() / 0.05).exp();
        self.offset += (self.target - self.offset) * k;
        if !self.animating() {
            self.offset = self.target;
        }
        self.mark_dirty(DirtyFlags::RENDER | DirtyFlags::PAINT);
        true
    }

    fn needs_repaint(&self) -> bool {
        self.animating()
    }

    fn wants_animate_tick(&self) -> bool {
        self.animating()
    }

    // Клип — до сдвига и свой, а не через `clip_content`: клип дерева
    // ставится после `build_display_list` и уехал бы вместе с содержимым.
    fn build_display_list(&self, list: &mut DisplayList, _clip: Rect) {
        list.push_clip(self.bounds);
        let sf = list.scale_factor().max(1.0);
        list.push_transform(Transform::translation(
            (-self.offset * sf).round() / sf,
            0.0,
        ));
    }

    fn post_build_display_list(&self, list: &mut DisplayList, _clip: Rect) {
        list.pop_transform();
        list.pop_clip();
    }

    fn scroll_offset(&self) -> Point {
        Point::new(self.offset, 0.0)
    }

    fn handle_event(
        &mut self,
        _event: &Event,
        _ctx: &mut crate::widget::context::EventContext,
    ) -> EventResult {
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

    fn mount(&mut self, _tree: &mut ElementTree) {}

    fn element_type_name(&self) -> &str {
        "FocusScroll"
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

    fn mss(&self) -> Option<&MssFields> {
        Some(&self.mss)
    }

    fn apply_computed_style(&mut self, style: &ComputedStyle) {
        self.mss.apply(style);
        self.mark_dirty(DirtyFlags::RENDER | DirtyFlags::LAYOUT);
    }
}

impl StyledElement for FocusScrollElement {
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

#[cfg(test)]
mod tests {
    use super::sticky_offset;

    // Лента 1000 px, содержимое 3000 px, запас 50.
    #[test]
    fn stays_while_focus_visible() {
        assert_eq!(sticky_offset(0.0, 1000.0, 3000.0, 400.0, 500.0, 50.0), 0.0);
        assert_eq!(
            sticky_offset(700.0, 1000.0, 3000.0, 900.0, 1000.0, 50.0),
            700.0
        );
    }

    #[test]
    fn scrolls_just_enough_past_edges() {
        // Вправо: конец элемента + запас встаёт к правому краю.
        assert_eq!(
            sticky_offset(0.0, 1000.0, 3000.0, 1000.0, 1100.0, 50.0),
            150.0
        );
        // Влево: начало элемента − запас встаёт к левому краю.
        assert_eq!(
            sticky_offset(800.0, 1000.0, 3000.0, 600.0, 700.0, 50.0),
            550.0
        );
    }

    #[test]
    fn clamped_to_content() {
        assert_eq!(
            sticky_offset(0.0, 1000.0, 3000.0, 2900.0, 3000.0, 50.0),
            2000.0
        );
        assert_eq!(sticky_offset(500.0, 1000.0, 3000.0, 0.0, 100.0, 50.0), 0.0);
        // Содержимое уже области — не двигается.
        assert_eq!(sticky_offset(0.0, 1000.0, 600.0, 500.0, 600.0, 50.0), 0.0);
    }

    #[test]
    fn wide_item_limits_peek() {
        // Элемент 980 px в ленте 1000: запас ужимается до 10, иначе дёргалось бы.
        assert_eq!(
            sticky_offset(0.0, 1000.0, 3000.0, 1000.0, 1980.0, 50.0),
            990.0
        );
    }
}
