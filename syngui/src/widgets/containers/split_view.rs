use crate::core::{Color, Point, Rect, RectExt, Size};
use crate::input::{CursorIcon, Event, EventResult, MouseButton};
use crate::layout::Constraints;
use crate::mss::ComputedStyle;
use crate::mss::MssFields;
use crate::render::DisplayList;
use crate::signal::RwSignal;
use crate::widget::context::{EventContext, EventContextExt};
use crate::widget::{DirtyFlags, Element, ElementId, ElementTree, LayoutHint, StyledElement, UpdateContext, Widget};
use std::any::Any;

#[derive(Clone, Copy, Debug, PartialEq)]
pub enum SplitDirection {
    Horizontal,
    Vertical,
}

pub struct SplitView {
    direction: SplitDirection,
    initial_ratio: f32,
    min_size: f32,
    divider_width: f32,
    first: Box<dyn Widget>,
    second: Box<dyn Widget>,
    classes: Vec<String>,
    ratio_signal: Option<RwSignal<f32>>,
}

impl SplitView {
    pub fn new(first: impl Widget + 'static, second: impl Widget + 'static) -> Self {
        Self {
            direction: SplitDirection::Horizontal,
            initial_ratio: 0.5,
            min_size: 50.0,
            divider_width: 6.0,
            first: Box::new(first),
            second: Box::new(second),
            classes: Vec::new(),
            ratio_signal: None,
        }
    }

    pub fn ratio_signal(mut self, signal: RwSignal<f32>) -> Self {
        self.ratio_signal = Some(signal);
        self
    }

    pub fn class(mut self, class: impl Into<String>) -> Self {
        self.classes.push(class.into());
        self
    }

    pub fn direction(mut self, d: SplitDirection) -> Self {
        self.direction = d;
        self
    }

    pub fn initial_ratio(mut self, r: f32) -> Self {
        self.initial_ratio = r.clamp(0.05, 0.95);
        self
    }

    pub fn min_size(mut self, s: f32) -> Self {
        self.min_size = s;
        self
    }

    pub fn divider_width(mut self, w: f32) -> Self {
        self.divider_width = w;
        self
    }

    pub fn first(mut self, w: impl Widget + 'static) -> Self {
        self.first = Box::new(w);
        self
    }

    pub fn second(mut self, w: impl Widget + 'static) -> Self {
        self.second = Box::new(w);
        self
    }
}

impl Widget for SplitView {
    fn create_element(&self) -> Box<dyn Element> {
        let initial = self
            .ratio_signal
            .map(|s| s.get_untracked())
            .unwrap_or(self.initial_ratio)
            .clamp(0.05, 0.95);
        Box::new(SplitViewElement {
            id: ElementId::new(),
            direction: self.direction,
            ratio: initial,
            min_size: self.min_size,
            divider_width: self.divider_width,
            dragging: false,
            hover_divider: false,
            bounds: Rect::zero(),
            classes: self.classes.clone(),
            dirty_flags: DirtyFlags::LAYOUT | DirtyFlags::RENDER,
            mss: MssFields::new(),
            ratio_signal: self.ratio_signal,
        })
    }

    fn can_update(&self, other: &dyn Any) -> bool { other.is::<Self>() }
    fn as_any(&self) -> &dyn Any { self }
    fn as_any_mut(&mut self) -> &mut dyn Any { self }

    fn mount(&self, tree: &mut ElementTree, parent_id: ElementId) {
        let first_el = self.first.create_element();
        let first_id = tree.insert_with_type_id(first_el, Some(parent_id), self.first.as_any().type_id());
        self.first.mount(tree, first_id);

        let second_el = self.second.create_element();
        let second_id = tree.insert_with_type_id(second_el, Some(parent_id), self.second.as_any().type_id());
        self.second.mount(tree, second_id);
    }

    fn child_widgets(&self) -> Vec<&dyn Widget> {
        vec![self.first.as_ref() as &dyn Widget, self.second.as_ref() as &dyn Widget]
    }

    fn widget_classes(&self) -> &[String] { &self.classes }
}

pub struct SplitViewElement {
    id: ElementId,
    direction: SplitDirection,
    ratio: f32,
    min_size: f32,
    divider_width: f32,
    dragging: bool,
    hover_divider: bool,
    bounds: Rect,
    classes: Vec<String>,
    dirty_flags: DirtyFlags,
    mss: MssFields,
    ratio_signal: Option<RwSignal<f32>>,
}

impl SplitViewElement {
    fn divider_rect(&self) -> Rect {
        let is_h = self.direction == SplitDirection::Horizontal;
        if is_h {
            let avail = (self.bounds.size.width - self.divider_width).max(0.0);
            let x = self.bounds.x() + avail * self.ratio;
            Rect::new(
                Point::new(x, self.bounds.y()),
                Size::new(self.divider_width, self.bounds.size.height),
            )
        } else {
            let avail = (self.bounds.size.height - self.divider_width).max(0.0);
            let y = self.bounds.y() + avail * self.ratio;
            Rect::new(
                Point::new(self.bounds.x(), y),
                Size::new(self.bounds.size.width, self.divider_width),
            )
        }
    }

    fn clamp_ratio(&mut self) {
        let is_h = self.direction == SplitDirection::Horizontal;
        let total = if is_h { self.bounds.size.width } else { self.bounds.size.height } - self.divider_width;
        if total <= 0.0 { return; }
        let min_ratio = self.min_size / total;
        let max_ratio = 1.0 - min_ratio;
        self.ratio = self.ratio.clamp(min_ratio.min(0.5), max_ratio.max(0.5));
    }
}

impl Element for SplitViewElement {
    fn update(&mut self, widget: &dyn Widget, _ctx: &mut UpdateContext) {
        if let Some(sv) = widget.as_any().downcast_ref::<SplitView>() {
            self.direction = sv.direction;
            self.min_size = sv.min_size;
            self.divider_width = sv.divider_width;
            self.ratio_signal = sv.ratio_signal;
            if let Some(sig) = self.ratio_signal {
                if !self.dragging {
                    let new = sig.get_untracked().clamp(0.05, 0.95);
                    if (new - self.ratio).abs() > f32::EPSILON {
                        self.ratio = new;
                    }
                }
            }
            self.mark_dirty(DirtyFlags::LAYOUT | DirtyFlags::RENDER);
        }
    }

    fn layout(&mut self, constraints: Constraints) -> Size {
        let w = if constraints.max_width.is_finite() { constraints.max_width } else { 400.0 };
        let h = if constraints.max_height.is_finite() { constraints.max_height } else { 300.0 };
        self.bounds = Rect::new(Point::zero(), Size::new(w, h));
        self.clamp_ratio();
        Size::new(w, h)
    }

    fn layout_hint(&self) -> LayoutHint {
        LayoutHint::Split {
            horizontal: self.direction == SplitDirection::Horizontal,
            ratio: self.ratio,
            divider: self.divider_width,
        }
    }

    fn build_display_list(&self, _list: &mut DisplayList, _clip: Rect) {
    }

    fn post_build_display_list(&self, list: &mut DisplayList, _clip: Rect) {
        let hit = self.divider_rect();
        let is_h = self.direction == SplitDirection::Horizontal;
        let visual_t = self.mss.divider_thickness.unwrap_or(self.divider_width).max(0.0);
        let visual = if is_h {
            let cx = hit.x() + (hit.size.width - visual_t) / 2.0;
            Rect::new(Point::new(cx, hit.y()), Size::new(visual_t, hit.size.height))
        } else {
            let cy = hit.y() + (hit.size.height - visual_t) / 2.0;
            Rect::new(Point::new(hit.x(), cy), Size::new(hit.size.width, visual_t))
        };

        let accent = self.mss.accent_color.unwrap_or(Color::from_hex("#3B82F6"));
        let border = self.mss.border_color.unwrap_or(Color::from_hex("#E5E7EB"));
        let fg = self.mss.color.unwrap_or(Color::from_hex("#9CA3AF"));

        let bg = if self.dragging {
            accent
        } else if self.hover_divider {
            accent.with_alpha(0.5)
        } else {
            border
        };
        list.push_rect(visual, bg, [0.0; 4]);

        let dot_size = 3.0;
        if visual_t >= dot_size + 1.0 {
            let dot_color = if self.dragging || self.hover_divider {
                Color::WHITE
            } else {
                fg
            };
            let dot_gap = 5.0;
            for i in -1i32..=1 {
                let (dx, dy) = if is_h {
                    (
                        visual.x() + (visual.size.width - dot_size) / 2.0,
                        visual.y() + visual.size.height / 2.0 + i as f32 * dot_gap - dot_size / 2.0,
                    )
                } else {
                    (
                        visual.x() + visual.size.width / 2.0 + i as f32 * dot_gap - dot_size / 2.0,
                        visual.y() + (visual.size.height - dot_size) / 2.0,
                    )
                };
                list.push_rect(
                    Rect::new(Point::new(dx, dy), Size::new(dot_size, dot_size)),
                    dot_color,
                    [dot_size / 2.0; 4],
                );
            }
        }
    }

    fn handle_event(&mut self, event: &Event, ctx: &mut EventContext) -> EventResult {
        match event {
            Event::MouseMove(pos) => {
                if self.dragging {
                    let is_h = self.direction == SplitDirection::Horizontal;
                    let total = if is_h {
                        self.bounds.size.width
                    } else {
                        self.bounds.size.height
                    } - self.divider_width;

                    if total > 0.0 {
                        let local = if is_h {
                            pos.x - self.bounds.x()
                        } else {
                            pos.y - self.bounds.y()
                        };
                        self.ratio = (local / (total + self.divider_width)).clamp(0.0, 1.0);
                        self.clamp_ratio();
                        if let Some(sig) = self.ratio_signal {
                            if (sig.get_untracked() - self.ratio).abs() > f32::EPSILON {
                                sig.set(self.ratio);
                            }
                        }
                        ctx.request_layout();
                        ctx.request_paint();
                    }
                    let cursor = if is_h { CursorIcon::ColResize } else { CursorIcon::RowResize };
                    ctx.set_cursor(cursor);
                    return EventResult::Handled;
                }

                let divider = self.divider_rect();
                let was_hover = self.hover_divider;
                self.hover_divider = divider.contains(*pos);

                if self.hover_divider {
                    let is_h = self.direction == SplitDirection::Horizontal;
                    let cursor = if is_h { CursorIcon::ColResize } else { CursorIcon::RowResize };
                    ctx.set_cursor(cursor);
                }

                if was_hover != self.hover_divider {
                    ctx.request_paint();
                    return EventResult::Handled;
                }

                EventResult::Ignored
            }
            Event::MouseDown { button, position } if *button == MouseButton::Left => {
                let divider = self.divider_rect();
                if divider.contains(*position) {
                    self.dragging = true;
                    ctx.request_paint();
                    return EventResult::Handled;
                }
                EventResult::Ignored
            }
            Event::MouseUp { button, .. } if *button == MouseButton::Left => {
                if self.dragging {
                    self.dragging = false;
                    ctx.request_paint();
                    return EventResult::Handled;
                }
                EventResult::Ignored
            }
            _ => EventResult::Ignored,
        }
    }

    fn children(&self) -> &[ElementId] { &[] }
    fn bounds(&self) -> Rect { self.bounds }
    fn set_position(&mut self, pos: Point) { self.bounds.origin = pos; }
    fn mark_dirty(&mut self, flags: DirtyFlags) { self.dirty_flags |= flags; }
    fn clear_dirty(&mut self, flags: DirtyFlags) { self.dirty_flags.remove(flags); }
    fn is_dirty(&self, flags: DirtyFlags) -> bool { self.dirty_flags.contains(flags) }
    fn id(&self) -> ElementId { self.id }
    fn set_id(&mut self, id: ElementId) { self.id = id; }
    fn mount(&mut self, _tree: &mut ElementTree) {}
    fn clip_content(&self) -> bool { true }

    fn intercepts_child_events(&self) -> bool { self.dragging }

    fn set_classes(&mut self, classes: Vec<String>) {
        self.classes = classes;
        self.mark_dirty(DirtyFlags::RENDER);
    }

    fn get_classes(&self) -> &[String] { &self.classes }

    fn element_type_name(&self) -> &str { "SplitView" }

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

impl StyledElement for SplitViewElement {
    fn apply_style(&mut self, _style: &ComputedStyle) {
        self.mark_dirty(DirtyFlags::LAYOUT | DirtyFlags::RENDER);
    }

    fn classes(&self) -> &[String] { &self.classes }

    fn set_classes(&mut self, classes: Vec<String>) {
        self.classes = classes;
        self.mark_dirty(DirtyFlags::RENDER);
    }
}
