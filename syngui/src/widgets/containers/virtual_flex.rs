use std::any::Any;
use std::sync::Arc;
use std::time::Duration;

use crate::core::{Color, Point, Rect, Size, Transform};
use crate::input::{Event, EventResult, Key, MouseButton};
use crate::layout::Constraints;
use crate::mss::{ComputedStyle, MssFields};
use crate::render::{DisplayList, display_list::Border};
use crate::widget::context::{EventContext, EventContextExt};
use crate::widget::{
    DirtyFlags, Element, ElementId, ElementTree, LayoutHint, StyledElement, UpdateContext, Widget,
};
use crate::widgets::containers::page::ScrollbarPolicy;

type ItemBuilderFn = Arc<dyn Fn(usize) -> Box<dyn Widget> + Send + Sync>;

struct VirtualSpacer(f32);

impl Widget for VirtualSpacer {
    fn create_element(&self) -> Box<dyn Element> {
        Box::new(VirtualSpacerElement {
            height: self.0,
            id: ElementId::new(),
            bounds: Rect::zero(),
            dirty_flags: DirtyFlags::LAYOUT,
        })
    }
    fn can_update(&self, other: &dyn Any) -> bool {
        other.is::<Self>()
    }
    fn as_any(&self) -> &dyn Any { self }
    fn as_any_mut(&mut self) -> &mut dyn Any { self }
    fn mount(&self, _tree: &mut ElementTree, _parent_id: ElementId) {}
}

struct VirtualSpacerElement {
    height: f32,
    id: ElementId,
    bounds: Rect,
    dirty_flags: DirtyFlags,
}

impl Element for VirtualSpacerElement {
    fn update(&mut self, widget: &dyn Widget, _ctx: &mut UpdateContext) {
        if let Some(s) = widget.as_any().downcast_ref::<VirtualSpacer>() {
            self.height = s.0;
            self.mark_dirty(DirtyFlags::LAYOUT);
        }
    }
    fn layout(&mut self, c: Constraints) -> Size {
        let w = if c.max_width.is_finite() { c.max_width } else { 0.0 };
        self.bounds.size = Size::new(w, self.height);
        Size::new(w, self.height)
    }
    fn build_display_list(&self, _list: &mut DisplayList, _clip: Rect) {}
    fn handle_event(&mut self, _event: &Event, _ctx: &mut EventContext) -> EventResult {
        EventResult::Ignored
    }
    fn id(&self) -> ElementId { self.id }
    fn set_id(&mut self, id: ElementId) { self.id = id; }
    fn bounds(&self) -> Rect { self.bounds }
    fn set_position(&mut self, pos: Point) { self.bounds.origin = pos; }
    fn mark_dirty(&mut self, flags: DirtyFlags) { self.dirty_flags |= flags; }
    fn clear_dirty(&mut self, flags: DirtyFlags) { self.dirty_flags.remove(flags); }
    fn is_dirty(&self, flags: DirtyFlags) -> bool { self.dirty_flags.contains(flags) }
    fn mount(&mut self, _tree: &mut ElementTree) {}
    fn element_type_name(&self) -> &str { "VirtualSpacer" }
    fn children(&self) -> &[ElementId] { &[] }
}

pub struct VirtualFlex {
    cols: usize,
    min_item_width: Option<f32>,
    item_count: usize,
    item_builder: ItemBuilderFn,
    gap: f32,
    estimated_item_height: f32,
    scrollbar_policy: ScrollbarPolicy,
}

impl VirtualFlex {
    pub fn grid(
        cols: usize,
        item_count: usize,
        item_builder: impl Fn(usize) -> Box<dyn Widget> + Send + Sync + 'static,
    ) -> Self {
        Self {
            cols: cols.max(1),
            min_item_width: None,
            item_count,
            item_builder: Arc::new(item_builder),
            gap: 0.0,
            estimated_item_height: 200.0,
            scrollbar_policy: ScrollbarPolicy::Auto,
        }
    }

    pub fn flex(
        min_item_width: f32,
        item_count: usize,
        item_builder: impl Fn(usize) -> Box<dyn Widget> + Send + Sync + 'static,
    ) -> Self {
        Self {
            cols: 1,
            min_item_width: Some(min_item_width.max(50.0)),
            item_count,
            item_builder: Arc::new(item_builder),
            gap: 0.0,
            estimated_item_height: 200.0,
            scrollbar_policy: ScrollbarPolicy::Auto,
        }
    }

    pub fn grid_with_builder_arc(
        cols: usize,
        item_count: usize,
        item_builder: ItemBuilderFn,
    ) -> Self {
        Self {
            cols: cols.max(1),
            min_item_width: None,
            item_count,
            item_builder,
            gap: 0.0,
            estimated_item_height: 200.0,
            scrollbar_policy: ScrollbarPolicy::Auto,
        }
    }

    pub fn flex_with_builder_arc(
        min_item_width: f32,
        item_count: usize,
        item_builder: ItemBuilderFn,
    ) -> Self {
        Self {
            cols: 1,
            min_item_width: Some(min_item_width.max(50.0)),
            item_count,
            item_builder,
            gap: 0.0,
            estimated_item_height: 200.0,
            scrollbar_policy: ScrollbarPolicy::Auto,
        }
    }

    pub fn gap(mut self, gap: f32) -> Self {
        self.gap = gap;
        self
    }

    pub fn estimated_item_height(mut self, h: f32) -> Self {
        self.estimated_item_height = h;
        self
    }

    pub fn scrollbar_policy(mut self, policy: ScrollbarPolicy) -> Self {
        self.scrollbar_policy = policy;
        self
    }
}

impl Widget for VirtualFlex {
    fn create_element(&self) -> Box<dyn Element> {
        Box::new(VirtualFlexElement {
            id: ElementId::new(),
            bounds: Rect::zero(),
            cols: self.cols,
            min_item_width: self.min_item_width,
            item_count: self.item_count,
            item_builder: self.item_builder.clone(),
            gap: self.gap,
            estimated_row_height: self.estimated_item_height,
            scrollbar_policy: self.scrollbar_policy,
            scroll_offset: 0.0,
            velocity: 0.0,
            is_coasting: false,
            scrollbar_opacity: 0.0,
            scrollbar_idle_time: 0.0,
            scrollbar_width: 8.0,
            dragging_scrollbar: false,
            hover_scrollbar: false,
            hover_scrollbar_area: false,
            visible_start_row: 0,
            visible_end_row: 0,
            needs_child_rebuild: true,
            built_top_spacer_h: 0.0,
            built_bottom_spacer_h: 0.0,
            built_cols: 0,
            window_viewport_h: 0.0,
            classes: Vec::new(),
            dirty_flags: DirtyFlags::LAYOUT | DirtyFlags::RENDER,
            mss: MssFields::new(),
            background: None,
            padding: crate::core::EdgeInsets::default(),
            touch_drag_start: None,
            touch_id: None,
        })
    }

    fn can_update(&self, other: &dyn Any) -> bool {
        other.is::<Self>()
    }

    fn as_any(&self) -> &dyn Any { self }
    fn as_any_mut(&mut self) -> &mut dyn Any { self }

    fn mount(&self, _tree: &mut ElementTree, _parent_id: ElementId) {
    }

    fn child_widgets(&self) -> Vec<&dyn Widget> {
        vec![]
    }
}

const VELOCITY_SCALE: f32 = 25.0;
const FRICTION: f32 = 0.95;
const MIN_VELOCITY: f32 = 0.5;
const BUFFER_ROWS: usize = 3;

struct VirtualFlexElement {
    id: ElementId,
    bounds: Rect,

    cols: usize,
    min_item_width: Option<f32>,
    item_count: usize,
    item_builder: ItemBuilderFn,
    gap: f32,
    estimated_row_height: f32,
    scrollbar_policy: ScrollbarPolicy,

    scroll_offset: f32,
    velocity: f32,
    is_coasting: bool,

    scrollbar_opacity: f32,
    scrollbar_idle_time: f32,
    scrollbar_width: f32,
    dragging_scrollbar: bool,
    hover_scrollbar: bool,
    hover_scrollbar_area: bool,

    visible_start_row: usize,
    visible_end_row: usize,
    needs_child_rebuild: bool,
    built_top_spacer_h: f32,
    built_bottom_spacer_h: f32,
    built_cols: usize,
    window_viewport_h: f32,

    classes: Vec<String>,
    dirty_flags: DirtyFlags,
    mss: MssFields,
    background: Option<Color>,
    padding: crate::core::EdgeInsets,

    touch_drag_start: Option<Point>,
    touch_id: Option<u64>,
}

impl VirtualFlexElement {
    fn scrollbar_inset(&self) -> f32 {
        if self.item_count > 0 && self.scroll_offset > 0.0 {
            return self.scrollbar_width;
        }
        let cols = if let Some(min_w) = self.min_item_width {
            let available = (self.bounds.size.width - self.padding.left - self.padding.right).max(0.0);
            if available <= 0.0 || min_w <= 0.0 { 1 } else {
                ((available + self.gap) / (min_w + self.gap)).floor().max(1.0) as usize
            }
        } else {
            self.cols
        };
        let rows = if cols > 0 { (self.item_count + cols - 1) / cols } else { 0 };
        let est_h = if rows > 0 { rows as f32 * (self.estimated_row_height + self.gap) - self.gap } else { 0.0 };
        if est_h > self.viewport_height() {
            self.scrollbar_width
        } else {
            0.0
        }
    }

    fn effective_cols(&self) -> usize {
        if let Some(min_w) = self.min_item_width {
            let available = (self.bounds.size.width - self.padding.left - self.padding.right - self.scrollbar_inset()).max(0.0);
            if available <= 0.0 || min_w <= 0.0 { return 1; }
            let cols = ((available + self.gap) / (min_w + self.gap)).floor() as usize;
            cols.max(1)
        } else {
            self.cols
        }
    }

    fn total_rows(&self) -> usize {
        if self.item_count == 0 { return 0; }
        let cols = self.effective_cols();
        (self.item_count + cols - 1) / cols
    }

    fn content_height(&self) -> f32 {
        let rows = self.total_rows();
        if rows == 0 { return 0.0; }
        rows as f32 * (self.estimated_row_height + self.gap) - self.gap
    }

    fn viewport_height(&self) -> f32 {
        let raw = (self.bounds.size.height - self.padding.top - self.padding.bottom).max(0.0);
        if self.window_viewport_h > 0.0 {
            raw.min(self.window_viewport_h)
        } else {
            raw
        }
    }

    fn max_scroll(&self) -> f32 {
        (self.content_height() - self.viewport_height()).max(0.0)
    }

    fn compute_visible_range(&self) -> (usize, usize) {
        let total = self.total_rows();
        if total == 0 { return (0, 0); }

        let row_step = self.estimated_row_height + self.gap;
        if row_step <= 0.0 { return (0, total); }

        let first_visible = (self.scroll_offset / row_step).floor() as usize;
        let visible_count = (self.viewport_height() / row_step).ceil() as usize + 1;
        let start = first_visible.saturating_sub(BUFFER_ROWS);
        let end = (first_visible + visible_count + BUFFER_ROWS).min(total);
        (start, end)
    }

    fn flash_scrollbar(&mut self) {
        self.scrollbar_opacity = 1.0;
        self.scrollbar_idle_time = 0.0;
    }

    fn is_animating(&self) -> bool {
        let scrollbar_fading = self.scrollbar_policy == ScrollbarPolicy::Auto
            && self.scrollbar_opacity > 0.0
            && !self.hover_scrollbar_area;
        self.is_coasting || scrollbar_fading
    }

    fn scrollbar_track(&self) -> Rect {
        let x = self.bounds.origin.x + self.bounds.size.width - self.scrollbar_width;
        let y = self.bounds.origin.y + self.padding.top;
        let h = self.viewport_height();
        Rect::new(Point::new(x, y), Size::new(self.scrollbar_width, h))
    }

    fn scrollbar_thumb(&self) -> Rect {
        let track = self.scrollbar_track();
        let ch = self.content_height();
        let vh = self.viewport_height();
        if ch <= 0.0 || vh <= 0.0 { return Rect::zero(); }

        let thumb_h = (vh / ch * track.size.height).clamp(20.0, track.size.height);
        let max = self.max_scroll();
        let ratio = if max > 0.0 { self.scroll_offset.clamp(0.0, max) / max } else { 0.0 };
        let thumb_y = ratio * (track.size.height - thumb_h);

        Rect::new(
            Point::new(track.origin.x, track.origin.y + thumb_y),
            Size::new(self.scrollbar_width, thumb_h),
        )
    }

    fn effective_opacity(&self) -> f32 {
        match self.scrollbar_policy {
            ScrollbarPolicy::Always => 1.0,
            ScrollbarPolicy::Never => 0.0,
            ScrollbarPolicy::Auto => self.scrollbar_opacity,
        }
    }
}

impl Element for VirtualFlexElement {
    fn update(&mut self, widget: &dyn Widget, _ctx: &mut UpdateContext) {
        if let Some(vf) = widget.as_any().downcast_ref::<VirtualFlex>() {
            let count_changed = self.item_count != vf.item_count;
            let cols_changed = self.cols != vf.cols;
            let width_changed = self.min_item_width != vf.min_item_width;
            let gap_changed = (self.gap - vf.gap).abs() > f32::EPSILON;
            let builder_changed = !std::sync::Arc::ptr_eq(&self.item_builder, &vf.item_builder);

            self.cols = vf.cols;
            self.min_item_width = vf.min_item_width;
            self.item_count = vf.item_count;
            self.item_builder = vf.item_builder.clone();
            self.gap = vf.gap;
            self.scrollbar_policy = vf.scrollbar_policy;

            if count_changed {
                let max = self.max_scroll();
                if self.scroll_offset > max {
                    self.scroll_offset = max;
                }
            }

            if count_changed || cols_changed || width_changed || gap_changed || builder_changed {
                self.needs_child_rebuild = true;
                self.mark_dirty(DirtyFlags::LAYOUT | DirtyFlags::RENDER);
            }
        }
    }

    fn layout(&mut self, constraints: Constraints) -> Size {
        let w = if constraints.max_width.is_finite() { constraints.max_width } else { constraints.min_width.max(0.0) };
        let h = if constraints.max_height.is_finite() { constraints.max_height } else { constraints.min_height.max(0.0) };
        self.bounds = Rect::new(self.bounds.origin, Size::new(w, h));
        Size::new(w, h)
    }

    fn layout_hint(&self) -> LayoutHint {
        LayoutHint::Scroll {
            left: self.padding.left,
            top: self.padding.top,
            right: self.padding.right + self.scrollbar_inset(),
            bottom: self.padding.bottom,
            unbounded_width: false,
            unbounded_height: true,
        }
    }

    fn set_content_size(&mut self, size: Size) {
        let visible_rows = self.visible_end_row.saturating_sub(self.visible_start_row);
        if visible_rows > 0 && size.height > 0.0 {
            let grid_h = size.height - self.built_top_spacer_h - self.built_bottom_spacer_h;
            if grid_h > 0.0 {
                let actual = (grid_h + self.gap) / visible_rows as f32 - self.gap;
                if actual > 10.0 && actual < 5000.0 {
                    self.estimated_row_height = self.estimated_row_height * 0.3 + actual * 0.7;
                }
            }
        }

        let max = self.max_scroll();
        if self.scroll_offset > max {
            self.scroll_offset = max;
        }
    }

    fn manages_own_children(&self) -> bool {
        true
    }

    fn needs_rebuild(&self) -> bool {
        if self.needs_child_rebuild { return true; }
        if self.effective_cols() != self.built_cols { return true; }
        let (start, end) = self.compute_visible_range();
        start != self.visible_start_row || end != self.visible_end_row
    }

    fn build_children(&self) -> Vec<Box<dyn Widget>> {
        if self.item_count == 0 {
            return vec![];
        }

        let (start_row, end_row) = self.compute_visible_range();
        let row_step = self.estimated_row_height + self.gap;
        let total = self.total_rows();

        let top_h = start_row as f32 * row_step;
        let bottom_h = total.saturating_sub(end_row) as f32 * row_step;

        let cols = self.effective_cols();
        let mut grid_children: Vec<Box<dyn Widget>> = Vec::new();
        for row in start_row..end_row {
            for col in 0..cols {
                let idx = row * cols + col;
                if idx < self.item_count {
                    grid_children.push((self.item_builder)(idx));
                }
            }
        }

        let grid = crate::widgets::containers::Grid::new(cols)
            .gap(self.gap)
            .children(grid_children);

        let mut col_children: Vec<Box<dyn Widget>> = Vec::with_capacity(3);
        if top_h > 0.0 {
            col_children.push(Box::new(VirtualSpacer(top_h)));
        }
        col_children.push(Box::new(grid));
        if bottom_h > 0.0 {
            col_children.push(Box::new(VirtualSpacer(bottom_h)));
        }

        use crate::layout::CrossAxisAlignment;
        vec![Box::new(
            crate::widgets::containers::Column::new()
                .cross_axis_alignment(CrossAxisAlignment::Stretch)
                .children(col_children),
        )]
    }

    fn clear_rebuild(&mut self) {
        let (start, end) = self.compute_visible_range();
        self.visible_start_row = start;
        self.visible_end_row = end;
        self.built_cols = self.effective_cols();

        let row_step = self.estimated_row_height + self.gap;
        self.built_top_spacer_h = start as f32 * row_step;
        self.built_bottom_spacer_h = self.total_rows().saturating_sub(end) as f32 * row_step;

        self.needs_child_rebuild = false;
    }

    fn build_display_list(&self, list: &mut DisplayList, _clip: Rect) {
        let br = self.mss.border_radius_resolved(
            self.bounds.size.width.min(self.bounds.size.height), 0.0,
        );
        let bw = self.mss.border_width_or(0.0);
        let bc = self.mss.border_color;

        if let Some(bg) = self.background {
            if bw > 0.0 {
                if let Some(bc) = bc {
                    list.push_rect_bordered(self.bounds, bg, br, Border::new(bw, bc));
                } else {
                    list.push_rect(self.bounds, bg, br);
                }
            } else {
                list.push_rect(self.bounds, bg, br);
            }
        } else if bw > 0.0 {
            if let Some(bc) = bc {
                list.push_rect_bordered(self.bounds, Color::TRANSPARENT, br, Border::new(bw, bc));
            }
        }

        list.push_clip(self.bounds);

        let ty = -self.scroll_offset;
        list.push_transform(Transform::translation(0.0, ty));
    }

    fn post_build_display_list(&self, list: &mut DisplayList, _clip: Rect) {
        list.pop_transform();
        list.pop_clip();

        let opacity = self.effective_opacity();
        if opacity <= 0.0 || self.content_height() <= self.viewport_height() {
            return;
        }

        let thumb_base = self.mss.color.unwrap_or(Color::from_hex("#9CA3AF"));
        let radius = [self.scrollbar_width / 2.0; 4];

        if self.hover_scrollbar_area {
            let track = self.scrollbar_track();
            let track_color = thumb_base.with_alpha(opacity * 0.15);
            list.push_rect(track, track_color, radius);
        }

        let thumb = self.scrollbar_thumb();
        let color = if self.dragging_scrollbar {
            thumb_base.darken(0.5).with_alpha(opacity)
        } else if self.hover_scrollbar {
            thumb_base.darken(0.3).with_alpha(opacity)
        } else {
            thumb_base.with_alpha(opacity * 0.7)
        };
        list.push_rect(thumb, color, radius);
    }

    fn handle_event(&mut self, event: &Event, ctx: &mut EventContext) -> EventResult {
        match event {
            Event::MouseWheel { delta, position, .. } => {
                if !self.bounds.contains(*position) {
                    return EventResult::Ignored;
                }
                if self.max_scroll() <= 0.0 {
                    return EventResult::Ignored;
                }

                let delta_y = -*delta;
                let old = self.scroll_offset;

                if delta_y < 0.0 && old <= 0.0 { return EventResult::Handled; }
                if delta_y > 0.0 && old >= self.max_scroll() { return EventResult::Handled; }

                self.scroll_offset = (old + delta_y).clamp(0.0, self.max_scroll());

                if (self.scroll_offset - old).abs() < 0.001 {
                    return EventResult::Handled;
                }

                let alpha = 0.3;
                self.velocity = self.velocity * (1.0 - alpha) + delta_y * VELOCITY_SCALE * alpha;
                self.is_coasting = true;

                self.flash_scrollbar();
                ctx.request_layout();
                ctx.request_paint();
                EventResult::Handled
            }

            Event::KeyDown(key) => {
                let vp = self.viewport_height();
                let max = self.max_scroll();
                let target = match key {
                    Key::Home => Some(0.0),
                    Key::End => Some(max),
                    Key::PageUp => Some((self.scroll_offset - vp).max(0.0)),
                    Key::PageDown => Some((self.scroll_offset + vp).min(max)),
                    Key::Up => Some((self.scroll_offset - 40.0).max(0.0)),
                    Key::Down => Some((self.scroll_offset + 40.0).min(max)),
                    _ => None,
                };
                if let Some(t) = target {
                    self.scroll_offset = t;
                    self.velocity = 0.0;
                    self.is_coasting = false;
                    self.flash_scrollbar();
                    ctx.request_layout();
                    ctx.request_paint();
                    EventResult::Handled
                } else {
                    EventResult::Ignored
                }
            }

            Event::MouseDown { button, position } if *button == MouseButton::Left => {
                let thumb = self.scrollbar_thumb();
                if thumb.size.height > 0.0 && thumb.contains(*position) {
                    self.dragging_scrollbar = true;
                    ctx.request_paint();
                    return EventResult::Captured;
                }
                EventResult::Ignored
            }

            Event::MouseMove(pos) => {
                let mut result = EventResult::Ignored;

                if self.dragging_scrollbar {
                    let track = self.scrollbar_track();
                    let thumb_h = self.scrollbar_thumb().size.height;
                    let track_h = track.size.height;
                    if track_h > thumb_h {
                        let relative = (pos.y - track.origin.y - thumb_h / 2.0) / (track_h - thumb_h);
                        self.scroll_offset = (relative.clamp(0.0, 1.0) * self.max_scroll())
                            .clamp(0.0, self.max_scroll());
                    }
                    self.flash_scrollbar();
                    ctx.request_layout();
                    ctx.request_paint();
                    return EventResult::Captured;
                }

                let was_area = self.hover_scrollbar_area;
                let hit_margin = 20.0;
                let in_area = self.bounds.contains(*pos) && {
                    let right = self.bounds.origin.x + self.bounds.size.width;
                    pos.x >= right - self.scrollbar_width - hit_margin
                };
                self.hover_scrollbar_area = in_area;

                if in_area && self.scrollbar_opacity < 1.0 {
                    self.flash_scrollbar();
                    ctx.request_paint();
                    result = EventResult::Handled;
                } else if was_area && !in_area {
                    ctx.request_paint();
                    result = EventResult::Handled;
                }

                let was_hover = self.hover_scrollbar;
                self.hover_scrollbar = self.scrollbar_thumb().contains(*pos);
                if self.hover_scrollbar != was_hover {
                    ctx.request_paint();
                    result = EventResult::Handled;
                }

                result
            }

            Event::MouseUp { button, .. } if *button == MouseButton::Left => {
                if self.dragging_scrollbar {
                    self.dragging_scrollbar = false;
                    ctx.request_paint();
                    return EventResult::Handled;
                }
                EventResult::Ignored
            }

            Event::TouchStart { id, position } => {
                if !self.bounds.contains(*position) || self.max_scroll() <= 0.0 {
                    return EventResult::Ignored;
                }
                self.touch_drag_start = Some(*position);
                self.touch_id = Some(*id);
                self.velocity = 0.0;
                self.is_coasting = false;
                EventResult::Handled
            }
            Event::TouchMove { id, position } => {
                if self.touch_id != Some(*id) { return EventResult::Ignored; }
                if let Some(start) = self.touch_drag_start {
                    let dy = start.y - position.y;
                    self.scroll_offset = (self.scroll_offset + dy).clamp(0.0, self.max_scroll());
                    let alpha = 0.3;
                    self.velocity = self.velocity * (1.0 - alpha) + dy * VELOCITY_SCALE * alpha;
                    self.touch_drag_start = Some(*position);
                    self.flash_scrollbar();
                    ctx.request_layout();
                    ctx.request_paint();
                    EventResult::Handled
                } else {
                    EventResult::Ignored
                }
            }
            Event::TouchEnd { id, .. } => {
                if self.touch_id != Some(*id) { return EventResult::Ignored; }
                self.touch_drag_start = None;
                self.touch_id = None;
                if self.velocity.abs() > 1.0 {
                    self.is_coasting = true;
                }
                ctx.request_paint();
                EventResult::Handled
            }

            _ => EventResult::Ignored,
        }
    }

    fn animate(&mut self, dt: Duration) -> bool {
        let dt_secs = dt.as_secs_f32();
        if dt_secs <= 0.0 { return self.is_animating(); }

        let mut needs_repaint = false;

        if self.is_coasting {
            let decay = FRICTION.powf(dt_secs * 60.0);
            self.velocity *= decay;
            self.scroll_offset += self.velocity * dt_secs;

            self.scroll_offset = self.scroll_offset.clamp(0.0, self.max_scroll());

            if self.velocity.abs() < MIN_VELOCITY {
                self.is_coasting = false;
                self.velocity = 0.0;
            }
            self.mark_dirty(DirtyFlags::LAYOUT | DirtyFlags::RENDER);
            needs_repaint = true;
        }

        if self.scrollbar_policy == ScrollbarPolicy::Auto && self.scrollbar_opacity > 0.0 {
            if self.hover_scrollbar_area || self.dragging_scrollbar {
                self.scrollbar_idle_time = 0.0;
            } else {
                self.scrollbar_idle_time += dt_secs;
            }
            if self.scrollbar_idle_time > 1.0 {
                self.scrollbar_opacity = (self.scrollbar_opacity - dt_secs * 3.0).max(0.0);
                needs_repaint = self.scrollbar_opacity > 0.0;
            }
        }

        needs_repaint
    }

    fn needs_repaint(&self) -> bool {
        self.is_animating()
    }

    fn scroll_offset(&self) -> Point {
        Point::new(0.0, self.scroll_offset)
    }

    fn is_scroll_container(&self) -> bool {
        true
    }

    fn clip_content(&self) -> bool {
        false
    }

    fn intercepts_child_events(&self) -> bool {
        self.dragging_scrollbar || self.hover_scrollbar
    }

    fn ensure_visible(&mut self, child_rect: Rect) -> bool {
        let vp_h = self.viewport_height();
        let visible_top = self.scroll_offset;
        let visible_bottom = visible_top + vp_h;
        let margin = 20.0;

        let target = if child_rect.origin.y + child_rect.size.height + margin > visible_bottom {
            child_rect.origin.y + child_rect.size.height + margin - vp_h
        } else if child_rect.origin.y - margin < visible_top {
            (child_rect.origin.y - margin).max(0.0)
        } else {
            return false;
        };

        self.scroll_offset = target.clamp(0.0, self.max_scroll());
        self.velocity = 0.0;
        self.is_coasting = false;
        true
    }

    fn id(&self) -> ElementId { self.id }
    fn set_id(&mut self, id: ElementId) { self.id = id; }

    fn children(&self) -> &[ElementId] {
        &[]
    }

    fn bounds(&self) -> Rect { self.bounds }

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

    fn mount(&mut self, _tree: &mut ElementTree) {}

    fn set_viewport_size(&mut self, size: crate::core::Size) {
        self.window_viewport_h = size.height;
    }

    fn element_type_name(&self) -> &str { "VirtualFlex" }

    fn set_classes(&mut self, classes: Vec<String>) {
        self.classes = classes;
        self.mark_dirty(DirtyFlags::RENDER);
    }

    fn get_classes(&self) -> &[String] {
        &self.classes
    }

    fn reset_mss_styles(&mut self) { self.mss.reset(); }
    fn mss(&self) -> Option<&crate::mss::MssFields> { Some(&self.mss) }

    fn apply_computed_style(&mut self, style: &ComputedStyle) {
        self.mss.apply(style);
        if let Some(bg) = self.mss.background_color {
            self.background = Some(bg);
        }
        if let Some(pl) = self.mss.padding_left { self.padding.left = pl; }
        if let Some(pr) = self.mss.padding_right { self.padding.right = pr; }
        if let Some(pt) = self.mss.padding_top { self.padding.top = pt; }
        if let Some(pb) = self.mss.padding_bottom { self.padding.bottom = pb; }
        if let Some(v) = style.get("scrollbar-width").and_then(|v| v.as_px()) {
            self.scrollbar_width = v;
        }
        self.mark_dirty(DirtyFlags::LAYOUT | DirtyFlags::RENDER);
    }
}

impl StyledElement for VirtualFlexElement {
    fn apply_style(&mut self, style: &ComputedStyle) {
        self.apply_computed_style(style);
    }

    fn classes(&self) -> &[String] {
        &self.classes
    }

    fn set_classes(&mut self, classes: Vec<String>) {
        self.classes = classes;
        self.mark_dirty(DirtyFlags::RENDER);
    }
}
