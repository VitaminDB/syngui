use crate::core::{Color, Point, Rect, RectExt, Size, Transform};
use crate::input::{CursorIcon, Event, EventResult, MouseButton};
use crate::layout::Constraints;
use crate::mss::ComputedStyle;
use crate::mss::MssFields;
use crate::render::DisplayList;
use crate::widget::context::{EventContext, EventContextExt};
use crate::widget::{DirtyFlags, Element, ElementId, ElementTree, LayoutHint, StyledElement, UpdateContext, Widget};
use super::IntoWidget;
use std::any::Any;
use std::sync::Arc;
use crate::core::sync::Mutex;
use std::time::Duration;

pub struct Carousel {
    children: Vec<Box<dyn Widget>>,
    current_page: usize,
    auto_play: bool,
    auto_play_interval_ms: u32,
    show_indicators: bool,
    on_page_change: Option<Arc<Mutex<dyn FnMut(usize) + Send>>>,
}

impl Carousel {
    pub fn new() -> Self {
        Self {
            children: Vec::new(),
            current_page: 0,
            auto_play: false,
            auto_play_interval_ms: 5000,
            show_indicators: true,
            on_page_change: None,
        }
    }

    pub fn child<M>(mut self, child: impl IntoWidget<M>) -> Self {
        self.children.push(child.into_widget());
        self
    }

    pub fn current_page(mut self, page: usize) -> Self {
        self.current_page = page;
        self
    }

    pub fn auto_play(mut self, enabled: bool) -> Self {
        self.auto_play = enabled;
        self
    }

    pub fn auto_play_interval_ms(mut self, ms: u32) -> Self {
        self.auto_play_interval_ms = ms;
        self
    }

    pub fn show_indicators(mut self, show: bool) -> Self {
        self.show_indicators = show;
        self
    }

    pub fn on_page_change(mut self, f: impl FnMut(usize) + Send + 'static) -> Self {
        self.on_page_change = Some(Arc::new(Mutex::new(f)));
        self
    }
}

impl Default for Carousel {
    fn default() -> Self { Self::new() }
}

impl Widget for Carousel {
    fn create_element(&self) -> Box<dyn Element> {
        Box::new(CarouselElement {
            id: ElementId::new(),
            page_count: self.children.len(),
            current_page: self.current_page,
            auto_play: self.auto_play,
            auto_play_interval_ms: self.auto_play_interval_ms,
            show_indicators: self.show_indicators,
            on_page_change: self.on_page_change.clone(),
            slide_offset: 0.0,
            target_offset: 0.0,
            anim_start_offset: 0.0,
            anim_progress: 1.0,
            animating: false,
            auto_play_elapsed: Duration::ZERO,
            drag_start_x: None,
            drag_offset: 0.0,
            prev_hover: false,
            next_hover: false,
            child_ids: Vec::new(),
            bounds: Rect::zero(),
            classes: Vec::new(),
            dirty_flags: DirtyFlags::LAYOUT | DirtyFlags::RENDER,
            mss: MssFields::new(),
        })
    }

    fn can_update(&self, other: &dyn Any) -> bool { other.is::<Self>() }
    fn as_any(&self) -> &dyn Any { self }
    fn as_any_mut(&mut self) -> &mut dyn Any { self }

    fn mount(&self, tree: &mut ElementTree, parent_id: ElementId) {
        for child in &self.children {
            let el = child.create_element();
            let id = tree.insert_with_type_id(el, Some(parent_id), child.as_any().type_id());
            child.mount(tree, id);
        }
    }

    fn child_widgets(&self) -> Vec<&dyn Widget> {
        self.children.iter().map(|c| c.as_ref() as &dyn Widget).collect()
    }
}

const INDICATOR_SIZE: f32 = 8.0;
const INDICATOR_GAP: f32 = 8.0;
const INDICATOR_AREA_HEIGHT: f32 = 28.0;
const ARROW_SIZE: f32 = 36.0;

pub struct CarouselElement {
    id: ElementId,
    page_count: usize,
    current_page: usize,
    auto_play: bool,
    auto_play_interval_ms: u32,
    show_indicators: bool,
    on_page_change: Option<Arc<Mutex<dyn FnMut(usize) + Send>>>,
    slide_offset: f32,
    target_offset: f32,
    anim_start_offset: f32,
    anim_progress: f32,
    animating: bool,
    auto_play_elapsed: Duration,
    drag_start_x: Option<f32>,
    drag_offset: f32,
    prev_hover: bool,
    next_hover: bool,
    child_ids: Vec<ElementId>,
    bounds: Rect,
    classes: Vec<String>,
    dirty_flags: DirtyFlags,
    mss: MssFields,
}

impl CarouselElement {
    fn fire_page_change(&self) {
        if let Some(ref cb) = self.on_page_change {
            if let Ok(mut f) = cb.lock() { f(self.current_page); }
        }
    }

    fn go_to_page(&mut self, page: usize) {
        if page < self.page_count && page != self.current_page {
            self.current_page = page;
            self.anim_start_offset = self.slide_offset;
            self.target_offset = page as f32 * self.bounds.size.width;
            self.anim_progress = 0.0;
            self.animating = true;
            self.auto_play_elapsed = Duration::ZERO;
            self.fire_page_change();
        }
    }

    fn content_height(&self) -> f32 {
        if self.show_indicators {
            self.bounds.size.height - INDICATOR_AREA_HEIGHT
        } else {
            self.bounds.size.height
        }
    }

    fn visible_offset(&self) -> f32 {
        if self.drag_start_x.is_some() {
            self.slide_offset + self.drag_offset
        } else {
            self.slide_offset
        }
    }

    fn prev_arrow_rect(&self) -> Rect {
        Rect::new(
            Point::new(self.bounds.x() + 8.0, self.bounds.y() + (self.content_height() - ARROW_SIZE) / 2.0),
            Size::new(ARROW_SIZE, ARROW_SIZE),
        )
    }

    fn next_arrow_rect(&self) -> Rect {
        Rect::new(
            Point::new(self.bounds.x() + self.bounds.size.width - ARROW_SIZE - 8.0, self.bounds.y() + (self.content_height() - ARROW_SIZE) / 2.0),
            Size::new(ARROW_SIZE, ARROW_SIZE),
        )
    }
}

impl Element for CarouselElement {
    fn update(&mut self, widget: &dyn Widget, _ctx: &mut UpdateContext) {
        if let Some(c) = widget.as_any().downcast_ref::<Carousel>() {
            self.page_count = c.children.len();
            self.current_page = c.current_page;
            self.auto_play = c.auto_play;
            self.auto_play_interval_ms = c.auto_play_interval_ms;
            self.show_indicators = c.show_indicators;
            self.on_page_change = c.on_page_change.clone();
            self.target_offset = c.current_page as f32 * self.bounds.size.width;
            self.mark_dirty(DirtyFlags::LAYOUT | DirtyFlags::RENDER);
        }
    }

    fn layout(&mut self, constraints: Constraints) -> Size {
        let w = constraints.max_width;
        let h = if constraints.max_height.is_finite() { constraints.max_height } else { 300.0 };
        let old_width = self.bounds.size.width;
        self.bounds = Rect::new(Point::zero(), Size::new(w, h));
        self.target_offset = self.current_page as f32 * w;
        if (old_width - w).abs() > 0.5 || old_width == 0.0 {
            self.slide_offset = self.target_offset;
        }
        Size::new(w, h)
    }

    fn layout_hint(&self) -> LayoutHint {
        LayoutHint::HorizontalPages
    }

    fn build_display_list(&self, list: &mut DisplayList, _clip: Rect) {
        let bg = self.mss.background_color.unwrap_or(Color::TRANSPARENT);
        list.push_rect(self.bounds, bg, [0.0; 4]);

        let content_rect = Rect::new(
            self.bounds.origin,
            Size::new(self.bounds.size.width, self.content_height()),
        );
        list.push_clip(content_rect);

        let offset = self.visible_offset();
        list.push_transform(Transform::translation(-offset, 0.0));

    }

    fn post_build_display_list(&self, list: &mut DisplayList, _clip: Rect) {
        list.pop_transform();
        list.pop_clip();

        if self.page_count > 1 {
            if self.current_page > 0 {
                let prev = self.prev_arrow_rect();
                let bg = if self.prev_hover { Color::BLACK.with_alpha(0.15) } else { Color::BLACK.with_alpha(0.06) };
                list.push_rect(prev, bg, [ARROW_SIZE / 2.0; 4]);
                list.push_text_centered("\u{25C0}", prev, Color::WHITE, 14.0);
            }
            if self.current_page < self.page_count - 1 {
                let next = self.next_arrow_rect();
                let bg = if self.next_hover { Color::BLACK.with_alpha(0.15) } else { Color::BLACK.with_alpha(0.06) };
                list.push_rect(next, bg, [ARROW_SIZE / 2.0; 4]);
                list.push_text_centered("\u{25B6}", next, Color::WHITE, 14.0);
            }
        }

        if self.show_indicators && self.page_count > 1 {
            let total_w = self.page_count as f32 * INDICATOR_SIZE + (self.page_count as f32 - 1.0) * INDICATOR_GAP;
            let start_x = self.bounds.x() + (self.bounds.size.width - total_w) / 2.0;
            let y = self.bounds.y() + self.bounds.size.height - INDICATOR_AREA_HEIGHT + (INDICATOR_AREA_HEIGHT - INDICATOR_SIZE) / 2.0;

            let active_color = self.mss.accent_color.unwrap_or(Color::from_hex("#3B82F6"));
            let inactive_color = self.mss.border_color.unwrap_or(Color::from_hex("#D1D5DB"));
            for i in 0..self.page_count {
                let x = start_x + i as f32 * (INDICATOR_SIZE + INDICATOR_GAP);
                let r = Rect::new(Point::new(x, y), Size::new(INDICATOR_SIZE, INDICATOR_SIZE));
                let color = if i == self.current_page {
                    active_color
                } else {
                    inactive_color
                };
                list.push_rect(r, color, [INDICATOR_SIZE / 2.0; 4]);
            }
        }
    }

    /// Кадры нужны на время переезда слайда и постоянно — при автопрокрутке.
    fn wants_animate_tick(&self) -> bool {
        self.animating || (self.auto_play && self.page_count > 1)
    }

    fn animate(&mut self, dt: Duration) -> bool {
        let mut needs_redraw = false;
        const SLIDE_DURATION: f32 = 0.35;

        if self.animating {
            self.anim_progress += dt.as_secs_f32() / SLIDE_DURATION;
            if self.anim_progress >= 1.0 {
                self.anim_progress = 1.0;
                self.animating = false;
                self.slide_offset = self.target_offset;
            } else {
                let t = self.anim_progress;
                let ease = 1.0 - (1.0 - t) * (1.0 - t) * (1.0 - t);
                self.slide_offset = self.anim_start_offset
                    + (self.target_offset - self.anim_start_offset) * ease;
            }
            needs_redraw = true;
        }

        if self.auto_play && self.page_count > 1 && self.drag_start_x.is_none() {
            self.auto_play_elapsed += dt;
            if self.auto_play_elapsed >= Duration::from_millis(self.auto_play_interval_ms as u64) {
                let next = (self.current_page + 1) % self.page_count;
                self.go_to_page(next);
                needs_redraw = true;
            }
        }

        needs_redraw || self.auto_play
    }

    fn handle_event(&mut self, event: &Event, ctx: &mut EventContext) -> EventResult {
        match event {
            Event::MouseMove(pos) => {
                if self.bounds.contains(*pos) {
                    if let Some(start) = self.drag_start_x {
                        self.drag_offset = start - pos.x;
                        ctx.request_paint();
                        return EventResult::Handled;
                    }

                    let prev_h = self.prev_arrow_rect().contains(*pos) && self.current_page > 0;
                    let next_h = self.next_arrow_rect().contains(*pos) && self.current_page < self.page_count - 1;
                    if prev_h != self.prev_hover || next_h != self.next_hover {
                        self.prev_hover = prev_h;
                        self.next_hover = next_h;
                        if prev_h || next_h { ctx.set_cursor(CursorIcon::Pointer); }
                        ctx.request_paint();
                    }
                    return EventResult::Handled;
                }
                EventResult::Ignored
            }
            Event::MouseDown { button, position } if *button == MouseButton::Left && self.bounds.contains(*position) => {
                if self.prev_arrow_rect().contains(*position) && self.current_page > 0 {
                    self.go_to_page(self.current_page - 1);
                    ctx.request_paint();
                    return EventResult::Handled;
                }
                if self.next_arrow_rect().contains(*position) && self.current_page < self.page_count - 1 {
                    self.go_to_page(self.current_page + 1);
                    ctx.request_paint();
                    return EventResult::Handled;
                }

                if self.show_indicators && self.page_count > 1 {
                    let total_w = self.page_count as f32 * INDICATOR_SIZE + (self.page_count as f32 - 1.0) * INDICATOR_GAP;
                    let start_x = self.bounds.x() + (self.bounds.size.width - total_w) / 2.0;
                    let ind_y = self.bounds.y() + self.bounds.size.height - INDICATOR_AREA_HEIGHT;
                    let ind_rect = Rect::new(Point::new(start_x, ind_y), Size::new(total_w, INDICATOR_AREA_HEIGHT));
                    if ind_rect.contains(*position) {
                        let idx = ((position.x - start_x) / (INDICATOR_SIZE + INDICATOR_GAP)) as usize;
                        if idx < self.page_count {
                            self.go_to_page(idx);
                            ctx.request_paint();
                            return EventResult::Handled;
                        }
                    }
                }

                self.drag_start_x = Some(position.x);
                self.drag_offset = 0.0;
                EventResult::Handled
            }
            Event::MouseUp { button, .. } if *button == MouseButton::Left && self.drag_start_x.is_some() => {
                let threshold = self.bounds.size.width * 0.2;
                self.slide_offset += self.drag_offset;
                self.drag_start_x = None;
                self.drag_offset = 0.0;

                if self.slide_offset - self.target_offset > threshold && self.current_page < self.page_count - 1 {
                    self.go_to_page(self.current_page + 1);
                } else if self.target_offset - self.slide_offset > threshold && self.current_page > 0 {
                    self.go_to_page(self.current_page - 1);
                } else {
                    self.anim_start_offset = self.slide_offset;
                    self.target_offset = self.current_page as f32 * self.bounds.size.width;
                    self.anim_progress = 0.0;
                    self.animating = true;
                }
                ctx.request_paint();
                EventResult::Handled
            }
            _ => EventResult::Ignored,
        }
    }

    fn children(&self) -> &[ElementId] { &self.child_ids }
    fn bounds(&self) -> Rect { self.bounds }
    fn set_position(&mut self, pos: Point) { self.bounds.origin = pos; }
    fn mark_dirty(&mut self, flags: DirtyFlags) { self.dirty_flags |= flags; }
    fn clear_dirty(&mut self, flags: DirtyFlags) { self.dirty_flags.remove(flags); }
    fn is_dirty(&self, flags: DirtyFlags) -> bool { self.dirty_flags.contains(flags) }
    fn id(&self) -> ElementId { self.id }
    fn set_id(&mut self, id: ElementId) { self.id = id; }
    fn mount(&mut self, _tree: &mut ElementTree) {}

    fn clip_content(&self) -> bool {
        false
    }

    fn scroll_offset(&self) -> Point {
        Point::new(self.visible_offset(), 0.0)
    }

    fn set_classes(&mut self, classes: Vec<String>) {
        self.classes = classes;
        self.mark_dirty(DirtyFlags::RENDER);
    }

    fn get_classes(&self) -> &[String] { &self.classes }

    fn element_type_name(&self) -> &str { "Carousel" }

    fn reset_mss_styles(&mut self) { self.mss.reset(); }
    fn mss(&self) -> Option<&crate::mss::MssFields> { Some(&self.mss) }
    fn apply_computed_style(&mut self, style: &ComputedStyle) {
        self.mss.apply(style);
        self.mark_dirty(DirtyFlags::RENDER);
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

impl StyledElement for CarouselElement {
    fn apply_style(&mut self, style: &ComputedStyle) {
        self.mss.apply(style);
        self.mark_dirty(DirtyFlags::RENDER);
    }

    fn classes(&self) -> &[String] { &self.classes }

    fn set_classes(&mut self, classes: Vec<String>) {
        self.classes = classes;
        self.mark_dirty(DirtyFlags::RENDER);
    }
}
