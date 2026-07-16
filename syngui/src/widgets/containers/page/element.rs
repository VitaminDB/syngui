use super::{Page, ScrollbarPolicy, ScrollPhysics, ScrollTarget};
use crate::animation::{Animation, Easing, Spring};
use crate::core::{Color, EdgeInsets, Point, Rect, Size, Transform};
use crate::input::{Event, EventResult, Key, MouseButton};
use crate::layout::Constraints;
use crate::mss::ComputedStyle;
use crate::mss::MssFields;
use crate::render::DisplayList;
use crate::widget::context::{EventContext, EventContextExt};
use crate::widget::{
    DirtyFlags, Element, ElementId, ElementTree, LayoutHint, StyledElement, UpdateContext, Widget,
};
use crate::widgets::scroll::ScrollDirection;
use std::any::Any;
use std::time::Duration;

const VELOCITY_SCALE: f32 = 25.0;

impl Widget for Page {
    fn create_element(&self) -> Box<dyn Element> {
        Box::new(PageElement {
            id: ElementId::new(),
            bounds: Rect::zero(),
            content_size: Size::zero(),
            direction: self.direction,
            scrollbar_policy: self.scrollbar_policy,
            scrollbar_width: self.scrollbar_width,
            padding: EdgeInsets::default(),
            physics: self.physics,
            background: None,

            scroll_offset: Point::zero(),
            velocity: Point::zero(),
            is_coasting: false,

            overscroll_x: 0.0,
            overscroll_y: 0.0,
            bounce_velocity_x: 0.0,
            bounce_velocity_y: 0.0,
            is_bouncing: false,
            bounce_target_y: 0.0,
            bounce_target_x: 0.0,

            scroll_animation: None,
            scroll_anim_start_y: 0.0,
            scroll_anim_target_y: 0.0,

            dragging_vertical: false,
            dragging_horizontal: false,
            hover_vertical: false,
            hover_horizontal: false,
            hover_scrollbar_area: false,
            scrollbar_opacity: 0.0,
            scrollbar_idle_time: 0.0,

            child_id: None,
            classes: Vec::new(),
            dirty_flags: DirtyFlags::LAYOUT | DirtyFlags::RENDER,

            mss: MssFields::new(),

            touch_drag_start: None,
            touch_id: None,
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
            let child_id = tree.insert_with_type_id(child_element, Some(parent_id), child.as_any().type_id());
            child.mount(tree, child_id);
        }
    }

    fn child_widgets(&self) -> Vec<&dyn Widget> {
        self.child.as_ref().map(|c| vec![c.as_ref() as &dyn Widget]).unwrap_or_default()
    }
}

pub struct PageElement {
    id: ElementId,
    bounds: Rect,
    content_size: Size,
    direction: ScrollDirection,
    scrollbar_policy: ScrollbarPolicy,
    scrollbar_width: f32,
    padding: EdgeInsets,
    physics: ScrollPhysics,
    background: Option<Color>,

    scroll_offset: Point,
    velocity: Point,
    is_coasting: bool,

    overscroll_x: f32,
    overscroll_y: f32,
    bounce_velocity_x: f32,
    bounce_velocity_y: f32,
    is_bouncing: bool,
    bounce_target_y: f32,
    bounce_target_x: f32,

    scroll_animation: Option<Animation>,
    scroll_anim_start_y: f32,
    scroll_anim_target_y: f32,

    dragging_vertical: bool,
    dragging_horizontal: bool,
    hover_vertical: bool,
    hover_horizontal: bool,
    hover_scrollbar_area: bool,
    scrollbar_opacity: f32,
    scrollbar_idle_time: f32,

    child_id: Option<ElementId>,
    classes: Vec<String>,
    dirty_flags: DirtyFlags,

    mss: MssFields,

    touch_drag_start: Option<Point>,
    touch_id: Option<u64>,
}

impl PageElement {
    fn viewport(&self) -> Rect {
        Rect::new(
            Point::new(
                self.bounds.origin.x + self.padding.left,
                self.bounds.origin.y + self.padding.top,
            ),
            Size::new(
                (self.bounds.size.width - self.padding.left - self.padding.right).max(0.0),
                (self.bounds.size.height - self.padding.top - self.padding.bottom).max(0.0),
            ),
        )
    }

    fn can_scroll_x(&self) -> bool {
        matches!(self.direction, ScrollDirection::Horizontal | ScrollDirection::Both)
    }

    fn can_scroll_y(&self) -> bool {
        matches!(self.direction, ScrollDirection::Vertical | ScrollDirection::Both)
    }

    fn max_scroll_x(&self) -> f32 {
        let vp = self.viewport();
        (self.content_size.width - vp.size.width).max(0.0)
    }

    fn max_scroll_y(&self) -> f32 {
        let vp = self.viewport();
        (self.content_size.height - vp.size.height).max(0.0)
    }

    fn clamp_offset(&self, offset: Point) -> Point {
        Point::new(
            if self.can_scroll_x() {
                offset.x.clamp(0.0, self.max_scroll_x())
            } else {
                0.0
            },
            if self.can_scroll_y() {
                offset.y.clamp(0.0, self.max_scroll_y())
            } else {
                0.0
            },
        )
    }

    fn overscroll_amount_y(&self) -> f32 {
        if self.scroll_offset.y < 0.0 {
            self.scroll_offset.y
        } else if self.scroll_offset.y > self.max_scroll_y() {
            self.scroll_offset.y - self.max_scroll_y()
        } else {
            0.0
        }
    }

    fn overscroll_amount_x(&self) -> f32 {
        if self.scroll_offset.x < 0.0 {
            self.scroll_offset.x
        } else if self.scroll_offset.x > self.max_scroll_x() {
            self.scroll_offset.x - self.max_scroll_x()
        } else {
            0.0
        }
    }

    fn effective_opacity(&self) -> f32 {
        match self.scrollbar_policy {
            ScrollbarPolicy::Always => 1.0,
            ScrollbarPolicy::Never => 0.0,
            ScrollbarPolicy::Auto => self.scrollbar_opacity,
        }
    }

    fn show_vertical_scrollbar_track(&self) -> bool {
        if !self.can_scroll_y() { return false; }
        let has_overflow = self.content_size.height > self.viewport().size.height;
        match self.scrollbar_policy {
            ScrollbarPolicy::Always => true,
            ScrollbarPolicy::Never => false,
            ScrollbarPolicy::Auto => has_overflow,
        }
    }

    fn show_vertical_scrollbar_thumb(&self) -> bool {
        if !self.can_scroll_y() { return false; }
        let has_overflow = self.content_size.height > self.viewport().size.height;
        match self.scrollbar_policy {
            ScrollbarPolicy::Always => has_overflow,
            ScrollbarPolicy::Never => false,
            ScrollbarPolicy::Auto => has_overflow,
        }
    }

    fn show_horizontal_scrollbar_track(&self) -> bool {
        if !self.can_scroll_x() { return false; }
        let has_overflow = self.content_size.width > self.viewport().size.width;
        match self.scrollbar_policy {
            ScrollbarPolicy::Always => true,
            ScrollbarPolicy::Never => false,
            ScrollbarPolicy::Auto => has_overflow,
        }
    }

    fn show_horizontal_scrollbar_thumb(&self) -> bool {
        if !self.can_scroll_x() { return false; }
        let has_overflow = self.content_size.width > self.viewport().size.width;
        match self.scrollbar_policy {
            ScrollbarPolicy::Always => has_overflow,
            ScrollbarPolicy::Never => false,
            ScrollbarPolicy::Auto => has_overflow,
        }
    }

    fn vertical_scrollbar_thumb(&self) -> Rect {
        let vp = self.viewport();
        let track_height = vp.size.height;
        let content_h = self.content_size.height;
        if content_h <= 0.0 || track_height <= 0.0 {
            return Rect::zero();
        }
        let thumb_h = (vp.size.height / content_h * track_height).clamp(20.0, track_height);
        let scroll_ratio = if self.max_scroll_y() > 0.0 {
            self.scroll_offset.y.clamp(0.0, self.max_scroll_y()) / self.max_scroll_y()
        } else {
            0.0
        };
        let thumb_y = scroll_ratio * (track_height - thumb_h);
        Rect::new(
            Point::new(
                vp.origin.x + vp.size.width - self.scrollbar_width,
                vp.origin.y + thumb_y,
            ),
            Size::new(self.scrollbar_width, thumb_h),
        )
    }

    fn horizontal_scrollbar_thumb(&self) -> Rect {
        let vp = self.viewport();
        let track_width = vp.size.width;
        let content_w = self.content_size.width;
        if content_w <= 0.0 || track_width <= 0.0 {
            return Rect::zero();
        }
        let thumb_w = (vp.size.width / content_w * track_width).clamp(20.0, track_width);
        let scroll_ratio = if self.max_scroll_x() > 0.0 {
            self.scroll_offset.x.clamp(0.0, self.max_scroll_x()) / self.max_scroll_x()
        } else {
            0.0
        };
        let thumb_x = scroll_ratio * (track_width - thumb_w);
        Rect::new(
            Point::new(
                vp.origin.x + thumb_x,
                vp.origin.y + vp.size.height - self.scrollbar_width,
            ),
            Size::new(thumb_w, self.scrollbar_width),
        )
    }

    fn compose_scrollbar_style(&self) -> crate::widgets::scroll::ScrollbarStyle {
        let fg = self.mss.color.unwrap_or(Color::from_hex("#9CA3AF"));
        let mut style = self.mss.scrollbar_style(fg);
        if self.mss.scrollbar_width.is_none() {
            style.width = self.scrollbar_width;
            style.corner_radius = self.scrollbar_width / 2.0;
        }
        if self.mss.scrollbar_policy.is_none() {
            style.policy = self.scrollbar_policy;
        }
        style
    }

    fn flash_scrollbar(&mut self) {
        self.scrollbar_opacity = 1.0;
        self.scrollbar_idle_time = 0.0;
    }

    fn scroll_to_animated(&mut self, target_y: f32) {
        let target_y = target_y.clamp(0.0, self.max_scroll_y());
        if (self.scroll_offset.y - target_y).abs() < 0.5 {
            return;
        }
        self.velocity = Point::zero();
        self.is_coasting = false;
        self.overscroll_y = 0.0;
        self.is_bouncing = false;

        self.scroll_anim_start_y = self.scroll_offset.y;
        self.scroll_anim_target_y = target_y;
        self.scroll_animation = Some(
            Animation::tween(Easing::EaseOutCubic)
                .from(0.0)
                .to(1.0)
                .duration_ms(300)
                .build(),
        );
        self.flash_scrollbar();
        self.mark_dirty(DirtyFlags::ANIMATION | DirtyFlags::RENDER);
    }

    fn is_animating(&self) -> bool {
        let scrollbar_fading = self.scrollbar_policy == ScrollbarPolicy::Auto
            && self.scrollbar_opacity > 0.0
            && !self.hover_scrollbar_area;
        self.is_coasting || self.is_bouncing || self.scroll_animation.is_some()
            || scrollbar_fading
    }
}

impl Element for PageElement {
    fn update(&mut self, widget: &dyn Widget, _ctx: &mut UpdateContext) {
        if let Some(page) = widget.as_any().downcast_ref::<Page>() {
            self.direction = page.direction;
            self.scrollbar_policy = page.scrollbar_policy;
            self.scrollbar_width = page.scrollbar_width;
            self.physics = page.physics;

            if let Some(target) = &page.scroll_to {
                match target {
                    ScrollTarget::Top => self.scroll_to_animated(0.0),
                    ScrollTarget::Bottom => self.scroll_to_animated(self.max_scroll_y()),
                    ScrollTarget::Offset(y) => self.scroll_to_animated(*y),
                }
            }

            self.mark_dirty(DirtyFlags::LAYOUT | DirtyFlags::RENDER);
        }
    }

    fn layout(&mut self, constraints: Constraints) -> Size {
        let width = constraints.max_width;
        let height = constraints.max_height;
        self.bounds = Rect::new(self.bounds.origin, Size::new(width, height));
        Size::new(width, height)
    }

    fn layout_hint(&self) -> LayoutHint {
        LayoutHint::Scroll {
            left: self.padding.left,
            top: self.padding.top,
            right: self.padding.right,
            bottom: self.padding.bottom,
            unbounded_width: self.can_scroll_x(),
            unbounded_height: self.can_scroll_y(),
        }
    }

    fn build_display_list(&self, list: &mut DisplayList, _clip: Rect) {
        if let Some(bg) = self.background {
            list.push_rect(self.bounds, bg, [0.0; 4]);
        }

        let clip_bounds = self.bounds;
        list.push_clip(clip_bounds);

        let sf = list.scale_factor().max(1.0);
        let snap = |v: f32| (v * sf).trunc() / sf;
        let tx = snap(-self.scroll_offset.x + self.overscroll_x);
        let ty = snap(-self.scroll_offset.y + self.overscroll_y);
        let transform = Transform::translation(tx, ty);
        list.push_transform(transform);

    }

    fn post_build_display_list(&self, list: &mut DisplayList, _clip: Rect) {
        list.pop_transform();
        list.pop_clip();

        let opacity = self.effective_opacity();
        if opacity <= 0.0 {
            return;
        }
        let style = self.compose_scrollbar_style();

        if self.show_vertical_scrollbar_track() && self.show_vertical_scrollbar_thumb() {
            let fader = crate::widgets::scroll::ScrollbarFader {
                opacity,
                idle_time: self.scrollbar_idle_time,
                hovered: self.hover_scrollbar_area || self.hover_vertical,
                dragging: self.dragging_vertical,
            };
            crate::widgets::scroll::render_vertical(
                list,
                self.bounds,
                self.content_size.height,
                self.scroll_offset.y,
                &style,
                &fader,
                opacity,
            );
        }
        if self.show_horizontal_scrollbar_track() && self.show_horizontal_scrollbar_thumb() {
            let fader = crate::widgets::scroll::ScrollbarFader {
                opacity,
                idle_time: self.scrollbar_idle_time,
                hovered: self.hover_scrollbar_area || self.hover_horizontal,
                dragging: self.dragging_horizontal,
            };
            crate::widgets::scroll::render_horizontal(
                list,
                self.bounds,
                self.content_size.width,
                self.scroll_offset.x,
                &style,
                &fader,
                opacity,
            );
        }
    }

    fn handle_event(&mut self, event: &Event, ctx: &mut EventContext) -> EventResult {
        match event {
            Event::MouseWheel { delta, delta_x: ev_dx, position } => {
                if !self.bounds.contains(*position) {
                    return EventResult::Ignored;
                }

                let can_y = self.can_scroll_y() && self.max_scroll_y() > 0.0;
                let can_x = self.can_scroll_x() && self.max_scroll_x() > 0.0;
                if !can_y && !can_x {
                    return EventResult::Ignored;
                }

                let mut delta_y = if can_y { -*delta } else { 0.0 };
                let mut delta_x = if can_x { -*ev_dx } else { 0.0 };

                let old_y = self.scroll_offset.y;
                let old_x = self.scroll_offset.x;

                if delta_y < 0.0 && old_y <= 0.0 {
                    delta_y = 0.0;
                }
                if delta_y > 0.0 && old_y >= self.max_scroll_y() {
                    delta_y = 0.0;
                }
                if delta_x < 0.0 && old_x <= 0.0 {
                    delta_x = 0.0;
                }
                if delta_x > 0.0 && old_x >= self.max_scroll_x() {
                    delta_x = 0.0;
                }

                self.scroll_offset.y = (old_y + delta_y).clamp(0.0, self.max_scroll_y());
                self.scroll_offset.x = (old_x + delta_x).clamp(0.0, self.max_scroll_x());

                if (self.scroll_offset.y - old_y).abs() < 0.001
                    && (self.scroll_offset.x - old_x).abs() < 0.001
                {
                    return EventResult::Handled;
                }

                let alpha = 0.3;
                self.velocity.y = self.velocity.y * (1.0 - alpha) + delta_y * VELOCITY_SCALE * alpha;
                self.velocity.x = self.velocity.x * (1.0 - alpha) + delta_x * VELOCITY_SCALE * alpha;
                self.is_coasting = true;

                self.scroll_animation = None;

                self.flash_scrollbar();
                ctx.request_paint();
                EventResult::Handled
            }

            Event::KeyDown(key) => {
                let vp_height = self.viewport().size.height;
                match key {
                    Key::Home => {
                        self.scroll_to_animated(0.0);
                        ctx.request_paint();
                        EventResult::Handled
                    }
                    Key::End => {
                        self.scroll_to_animated(self.max_scroll_y());
                        ctx.request_paint();
                        EventResult::Handled
                    }
                    Key::PageUp => {
                        let target = (self.scroll_offset.y - vp_height).max(0.0);
                        self.scroll_to_animated(target);
                        ctx.request_paint();
                        EventResult::Handled
                    }
                    Key::PageDown => {
                        let target = (self.scroll_offset.y + vp_height).min(self.max_scroll_y());
                        self.scroll_to_animated(target);
                        ctx.request_paint();
                        EventResult::Handled
                    }
                    Key::Up if self.can_scroll_y() => {
                        let target = (self.scroll_offset.y - 40.0).max(0.0);
                        self.scroll_to_animated(target);
                        ctx.request_paint();
                        EventResult::Handled
                    }
                    Key::Down if self.can_scroll_y() => {
                        let target = (self.scroll_offset.y + 40.0).min(self.max_scroll_y());
                        self.scroll_to_animated(target);
                        ctx.request_paint();
                        EventResult::Handled
                    }
                    _ => EventResult::Ignored,
                }
            }

            Event::MouseDown { button, position } if *button == MouseButton::Left => {
                if self.show_vertical_scrollbar_thumb() {
                    let thumb = self.vertical_scrollbar_thumb();
                    if thumb.contains(*position) {
                        self.dragging_vertical = true;
                        ctx.request_paint();
                        return EventResult::Captured;
                    }
                }
                if self.show_horizontal_scrollbar_thumb() {
                    let thumb = self.horizontal_scrollbar_thumb();
                    if thumb.contains(*position) {
                        self.dragging_horizontal = true;
                        ctx.request_paint();
                        return EventResult::Captured;
                    }
                }
                EventResult::Ignored
            }

            Event::MouseMove(pos) => {
                let mut result = EventResult::Ignored;

                if self.dragging_vertical {
                    let vp = self.viewport();
                    let thumb_h = self.vertical_scrollbar_thumb().size.height;
                    let track_h = vp.size.height;
                    if track_h > thumb_h {
                        let relative = (pos.y - vp.origin.y - thumb_h / 2.0) / (track_h - thumb_h);
                        self.scroll_offset.y = (relative.clamp(0.0, 1.0) * self.max_scroll_y())
                            .clamp(0.0, self.max_scroll_y());
                    }
                    self.flash_scrollbar();
                    ctx.request_paint();
                    return EventResult::Captured;
                }
                if self.dragging_horizontal {
                    let vp = self.viewport();
                    let thumb_w = self.horizontal_scrollbar_thumb().size.width;
                    let track_w = vp.size.width;
                    if track_w > thumb_w {
                        let relative = (pos.x - vp.origin.x - thumb_w / 2.0) / (track_w - thumb_w);
                        self.scroll_offset.x = (relative.clamp(0.0, 1.0) * self.max_scroll_x())
                            .clamp(0.0, self.max_scroll_x());
                    }
                    self.flash_scrollbar();
                    ctx.request_paint();
                    return EventResult::Captured;
                }

                let was_area = self.hover_scrollbar_area;
                let mut in_area = false;
                let hit_margin = 20.0_f32;

                if self.show_vertical_scrollbar_track() && self.bounds.contains(*pos) {
                    let vp = self.viewport();
                    let area_x = vp.origin.x + vp.size.width - self.scrollbar_width - hit_margin;
                    if pos.x >= area_x && pos.x <= vp.origin.x + vp.size.width
                        && pos.y >= vp.origin.y && pos.y <= vp.origin.y + vp.size.height
                    {
                        in_area = true;
                    }
                }
                if self.show_horizontal_scrollbar_track() && self.bounds.contains(*pos) {
                    let vp = self.viewport();
                    let area_y = vp.origin.y + vp.size.height - self.scrollbar_width - hit_margin;
                    if pos.y >= area_y && pos.y <= vp.origin.y + vp.size.height
                        && pos.x >= vp.origin.x && pos.x <= vp.origin.x + vp.size.width
                    {
                        in_area = true;
                    }
                }
                self.hover_scrollbar_area = in_area;

                if in_area && self.scrollbar_opacity < 1.0 {
                    self.flash_scrollbar();
                    ctx.request_paint();
                    result = EventResult::Handled;
                } else if was_area && !in_area {
                    ctx.request_paint();
                    result = EventResult::Handled;
                }

                if self.show_vertical_scrollbar_thumb() {
                    let was = self.hover_vertical;
                    self.hover_vertical = self.vertical_scrollbar_thumb().contains(*pos);
                    if self.hover_vertical != was {
                        ctx.request_paint();
                        result = EventResult::Handled;
                    }
                }
                if self.show_horizontal_scrollbar_thumb() {
                    let was = self.hover_horizontal;
                    self.hover_horizontal = self.horizontal_scrollbar_thumb().contains(*pos);
                    if self.hover_horizontal != was {
                        ctx.request_paint();
                        result = EventResult::Handled;
                    }
                }

                result
            }

            Event::MouseUp { button, .. } if *button == MouseButton::Left => {
                if self.dragging_vertical || self.dragging_horizontal {
                    self.dragging_vertical = false;
                    self.dragging_horizontal = false;
                    ctx.request_paint();
                    return EventResult::Handled;
                }
                EventResult::Ignored
            }

            Event::TouchStart { id, position } => {
                if !self.bounds.contains(*position) {
                    return EventResult::Ignored;
                }
                let can_y = self.can_scroll_y() && self.max_scroll_y() > 0.0;
                let can_x = self.can_scroll_x() && self.max_scroll_x() > 0.0;
                if !can_y && !can_x {
                    return EventResult::Ignored;
                }
                self.touch_drag_start = Some(*position);
                self.touch_id = Some(*id);
                self.velocity = Point::zero();
                self.is_coasting = false;
                self.scroll_animation = None;
                EventResult::Handled
            }
            Event::TouchMove { id, position } => {
                if self.touch_id != Some(*id) {
                    return EventResult::Ignored;
                }
                if let Some(start) = self.touch_drag_start {
                    let dy = start.y - position.y;
                    let dx = start.x - position.x;

                    if self.can_scroll_y() {
                        self.scroll_offset.y = self.scroll_offset.y + dy;
                        let max_y = self.max_scroll_y();
                        self.scroll_offset.y = self.scroll_offset.y.clamp(-50.0, max_y + 50.0);
                    }
                    if self.can_scroll_x() {
                        self.scroll_offset.x = self.scroll_offset.x + dx;
                        let max_x = self.max_scroll_x();
                        self.scroll_offset.x = self.scroll_offset.x.clamp(-50.0, max_x + 50.0);
                    }

                    let alpha = 0.3;
                    self.velocity.y = self.velocity.y * (1.0 - alpha) + dy * VELOCITY_SCALE * alpha;
                    self.velocity.x = self.velocity.x * (1.0 - alpha) + dx * VELOCITY_SCALE * alpha;
                    self.touch_drag_start = Some(*position);
                    self.flash_scrollbar();
                    ctx.request_paint();
                    EventResult::Handled
                } else {
                    EventResult::Ignored
                }
            }
            Event::TouchEnd { id, .. } => {
                if self.touch_id != Some(*id) {
                    return EventResult::Ignored;
                }
                self.touch_drag_start = None;
                self.touch_id = None;

                if self.scroll_offset.y < 0.0 || self.scroll_offset.y > self.max_scroll_y()
                    || self.scroll_offset.x < 0.0 || self.scroll_offset.x > self.max_scroll_x()
                {
                    self.is_bouncing = true;
                    self.bounce_target_y = self.scroll_offset.y.clamp(0.0, self.max_scroll_y());
                    self.bounce_target_x = self.scroll_offset.x.clamp(0.0, self.max_scroll_x());
                    self.velocity = Point::zero();
                } else if self.velocity.y.abs() > 1.0 || self.velocity.x.abs() > 1.0 {
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
        if dt_secs <= 0.0 {
            return self.is_animating();
        }

        let physics = self.physics;
        let mut needs_repaint = false;

        if self.is_coasting {
            let decay = physics.friction.powf(dt_secs * 60.0);

            if self.can_scroll_y() {
                self.velocity.y *= decay;
                self.scroll_offset.y += self.velocity.y * dt_secs;
            }
            if self.can_scroll_x() {
                self.velocity.x *= decay;
                self.scroll_offset.x += self.velocity.x * dt_secs;
            }

            let os_y = self.overscroll_amount_y();
            let os_x = self.overscroll_amount_x();

            if os_y.abs() > 0.0 || os_x.abs() > 0.0 {
                if self.can_scroll_y() && os_y.abs() > 0.0 {
                    let max = physics.max_overscroll;
                    self.scroll_offset.y = self.scroll_offset.y.clamp(
                        -max,
                        self.max_scroll_y() + max,
                    );
                    self.overscroll_y = self.overscroll_amount_y();
                    self.bounce_velocity_y = self.velocity.y;
                    self.velocity.y = 0.0;
                    self.bounce_target_y = if self.scroll_offset.y < 0.0 { 0.0 } else { self.max_scroll_y() };
                    self.is_bouncing = true;
                }
                if self.can_scroll_x() && os_x.abs() > 0.0 {
                    let max = physics.max_overscroll;
                    self.scroll_offset.x = self.scroll_offset.x.clamp(
                        -max,
                        self.max_scroll_x() + max,
                    );
                    self.overscroll_x = self.overscroll_amount_x();
                    self.bounce_velocity_x = self.velocity.x;
                    self.velocity.x = 0.0;
                    self.bounce_target_x = if self.scroll_offset.x < 0.0 { 0.0 } else { self.max_scroll_x() };
                    self.is_bouncing = true;
                }
                self.is_coasting = false;
            }

            if self.velocity.x.abs() < physics.min_velocity
                && self.velocity.y.abs() < physics.min_velocity
            {
                self.is_coasting = false;
                self.velocity = Point::zero();
                self.scroll_offset = self.clamp_offset(self.scroll_offset);
            }

            needs_repaint = true;
        }

        if self.is_bouncing {
            let spring = Spring {
                stiffness: physics.bounce_stiffness,
                damping: physics.bounce_damping,
                mass: 1.0,
            };

            let mut at_rest = true;

            if self.can_scroll_y() && self.overscroll_y.abs() > 0.0 {
                let (new_os, new_vel) =
                    spring.update(self.overscroll_y, 0.0, self.bounce_velocity_y, dt_secs);
                self.overscroll_y = new_os;
                self.bounce_velocity_y = new_vel;

                self.scroll_offset.y = self.bounce_target_y + self.overscroll_y;

                if !spring.is_at_rest(self.overscroll_y, self.bounce_velocity_y) {
                    at_rest = false;
                }
            }

            if self.can_scroll_x() && self.overscroll_x.abs() > 0.0 {
                let (new_os, new_vel) =
                    spring.update(self.overscroll_x, 0.0, self.bounce_velocity_x, dt_secs);
                self.overscroll_x = new_os;
                self.bounce_velocity_x = new_vel;

                self.scroll_offset.x = self.bounce_target_x + self.overscroll_x;

                if !spring.is_at_rest(self.overscroll_x, self.bounce_velocity_x) {
                    at_rest = false;
                }
            }

            if at_rest {
                self.is_bouncing = false;
                self.overscroll_x = 0.0;
                self.overscroll_y = 0.0;
                self.bounce_velocity_x = 0.0;
                self.bounce_velocity_y = 0.0;
                self.scroll_offset = self.clamp_offset(self.scroll_offset);
            }

            needs_repaint = true;
        }

        if let Some(ref mut anim) = self.scroll_animation {
            let still_running = anim.tick(dt);
            let t = anim.current_value();
            self.scroll_offset.y =
                self.scroll_anim_start_y + (self.scroll_anim_target_y - self.scroll_anim_start_y) * t;

            if !still_running || anim.is_complete() {
                self.scroll_offset.y = self.scroll_anim_target_y;
                self.scroll_animation = None;
            }
            needs_repaint = true;
        }

        if self.scrollbar_policy == ScrollbarPolicy::Auto && self.scrollbar_opacity > 0.0 {
            if self.hover_scrollbar_area || self.dragging_vertical || self.dragging_horizontal {
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

    fn set_content_size(&mut self, size: Size) {
        self.content_size = size;
        let max_y = self.max_scroll_y();
        if self.scroll_offset.y > max_y {
            self.scroll_offset.y = max_y;
        }
        let max_x = self.max_scroll_x();
        if self.scroll_offset.x > max_x {
            self.scroll_offset.x = max_x;
        }
    }

    fn children(&self) -> &[ElementId] {
        static EMPTY: &[ElementId] = &[];
        match self.child_id {
            Some(ref id) => std::slice::from_ref(id),
            None => EMPTY,
        }
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

    fn element_type_name(&self) -> &str { "Page" }

    fn clip_content(&self) -> bool {
        false
    }

    fn scroll_offset(&self) -> Point {
        self.scroll_offset
    }

    fn is_scroll_container(&self) -> bool {
        true
    }

    fn ensure_visible(&mut self, child_rect: Rect) -> bool {
        if !self.can_scroll_y() { return false; }

        let vp = self.viewport();
        let margin = 20.0;
        let visible_top = self.scroll_offset.y;
        let visible_bottom = visible_top + vp.size.height;
        let child_top = child_rect.origin.y;
        let child_bottom = child_top + child_rect.size.height;
        let max_y = self.max_scroll_y();

        let target_y = if child_bottom + margin > visible_bottom {
            child_bottom + margin - vp.size.height
        } else if child_top - margin < visible_top {
            (child_top - margin).max(0.0)
        } else {
            return false;
        };

        self.scroll_offset.y = target_y.max(0.0).min(max_y);
        self.velocity = Point::zero();
        self.is_coasting = false;
        true
    }

    fn accessibility_info(&self) -> Option<crate::a11y::AccessibilityInfo> {
        use crate::a11y::{AccessibilityInfo, NodeProperties, NodeState, Role};
        Some(AccessibilityInfo {
            role: Role::ScrollBar,
            state: NodeState::default(),
            properties: NodeProperties {
                label: Some("Scrollable area".to_string()),
                value: Some(format!("{:.0}", self.scroll_offset.y)),
                ..Default::default()
            },
        })
    }

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

impl StyledElement for PageElement {
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
