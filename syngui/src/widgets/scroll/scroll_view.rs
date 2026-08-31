use crate::core::{Color, Point, Rect, Size, Transform};
use crate::input::{Event, EventResult, Key, MouseButton};
use crate::layout::Constraints;
use crate::mss::{ComputedStyle, MssFields};
use crate::render::DisplayList;
use crate::widget::context::{EventContext, EventContextExt};
use crate::widget::{
    DirtyFlags, Element, ElementId, ElementTree, LayoutHint, StyledElement, UpdateContext, Widget,
};
use crate::widgets::containers::page::ScrollbarPolicy;
use crate::widgets::containers::IntoWidget;
use std::any::Any;
use std::time::Duration;

const FRICTION: f32 = 0.98;
const MIN_VELOCITY: f32 = 0.3;
const VELOCITY_SCALE: f32 = 25.0;
const SCROLLBAR_FADE_DELAY: f32 = 1.5;
const SCROLLBAR_FADE_RATE: f32 = 3.0;

#[derive(Clone, Copy, Debug, Default)]
pub enum ScrollDirection {
    #[default]
    Vertical,
    Horizontal,
    Both,
}

pub struct ScrollView {
    child: Option<Box<dyn Widget>>,
    direction: ScrollDirection,
    scrollbar_policy: ScrollbarPolicy,
    scrollbar_width: f32,
    center_content: bool,
    classes: Vec<String>,
}

impl ScrollView {
    pub fn new() -> Self {
        Self {
            child: None,
            direction: ScrollDirection::default(),
            scrollbar_policy: ScrollbarPolicy::Auto,
            scrollbar_width: 8.0,
            center_content: false,
            classes: Vec::new(),
        }
    }

    pub fn child<M>(mut self, child: impl IntoWidget<M>) -> Self {
        self.child = Some(child.into_widget());
        self
    }

    pub fn direction(mut self, direction: ScrollDirection) -> Self {
        self.direction = direction;
        self
    }

    /// Содержимое, которое уже (или ниже) области просмотра, стоит по её
    /// центру, а не прижимается к левому верхнему углу. По оси с
    /// переполнением ничего не меняется — там идёт обычная прокрутка.
    pub fn center_content(mut self, center: bool) -> Self {
        self.center_content = center;
        self
    }

    pub fn vertical(self) -> Self {
        self.direction(ScrollDirection::Vertical)
    }

    pub fn horizontal(self) -> Self {
        self.direction(ScrollDirection::Horizontal)
    }

    pub fn both(self) -> Self {
        self.direction(ScrollDirection::Both)
    }

    pub fn scrollbar_policy(mut self, policy: ScrollbarPolicy) -> Self {
        self.scrollbar_policy = policy;
        self
    }

    pub fn scrollbar_width(mut self, width: f32) -> Self {
        self.scrollbar_width = width;
        self
    }

    pub fn always_show_scrollbar(mut self, always: bool) -> Self {
        self.scrollbar_policy = if always {
            ScrollbarPolicy::Always
        } else {
            ScrollbarPolicy::Auto
        };
        self
    }

    pub fn class(mut self, name: &str) -> Self {
        self.classes.push(name.to_string());
        self
    }
}

impl Default for ScrollView {
    fn default() -> Self {
        Self::new()
    }
}

impl Widget for ScrollView {
    fn create_element(&self) -> Box<dyn Element> {
        Box::new(ScrollViewElement {
            id: ElementId::new(),
            bounds: Rect::zero(),
            content_size: Size::zero(),
            direction: self.direction,
            scrollbar_policy: self.scrollbar_policy,
            scrollbar_width: self.scrollbar_width,
            center_content: self.center_content,

            scroll_offset: Point::zero(),
            velocity: Point::zero(),
            is_coasting: false,

            dragging_vertical: false,
            dragging_horizontal: false,
            hover_vertical: false,
            hover_horizontal: false,
            hover_scrollbar_area: false,
            scrollbar_opacity: 0.0,
            scrollbar_idle_time: 0.0,

            child_id: None,
            classes: self.classes.clone(),
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

    fn widget_classes(&self) -> &[String] { &self.classes }
}

pub struct ScrollViewElement {
    id: ElementId,
    bounds: Rect,
    content_size: Size,
    direction: ScrollDirection,
    scrollbar_policy: ScrollbarPolicy,
    scrollbar_width: f32,
    center_content: bool,

    scroll_offset: Point,
    velocity: Point,
    is_coasting: bool,

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

impl ScrollViewElement {
    fn can_scroll_x(&self) -> bool {
        matches!(
            self.direction,
            ScrollDirection::Horizontal | ScrollDirection::Both
        )
    }

    fn can_scroll_y(&self) -> bool {
        matches!(
            self.direction,
            ScrollDirection::Vertical | ScrollDirection::Both
        )
    }

    /// Сдвиг содержимого к центру области по осям, где оно свободно
    /// помещается. По оси с переполнением сдвиг нулевой.
    fn center_offset(&self) -> Point {
        if !self.center_content {
            return Point::zero();
        }
        Point::new(
            ((self.bounds.size.width - self.content_size.width) * 0.5).max(0.0),
            ((self.bounds.size.height - self.content_size.height) * 0.5).max(0.0),
        )
    }

    fn max_scroll_x(&self) -> f32 {
        (self.content_size.width - self.bounds.size.width).max(0.0)
    }

    fn max_scroll_y(&self) -> f32 {
        (self.content_size.height - self.bounds.size.height).max(0.0)
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

    fn effective_opacity(&self) -> f32 {
        match self.scrollbar_policy {
            ScrollbarPolicy::Always => 1.0,
            ScrollbarPolicy::Never => 0.0,
            ScrollbarPolicy::Auto => self.scrollbar_opacity,
        }
    }

    fn show_vertical_track(&self) -> bool {
        if !self.can_scroll_y() {
            return false;
        }
        let overflow = self.content_size.height > self.bounds.size.height;
        match self.scrollbar_policy {
            ScrollbarPolicy::Always => true,
            ScrollbarPolicy::Never => false,
            ScrollbarPolicy::Auto => overflow,
        }
    }

    fn show_vertical_thumb(&self) -> bool {
        if !self.can_scroll_y() {
            return false;
        }
        let overflow = self.content_size.height > self.bounds.size.height;
        match self.scrollbar_policy {
            ScrollbarPolicy::Always => overflow,
            ScrollbarPolicy::Never => false,
            ScrollbarPolicy::Auto => overflow,
        }
    }

    fn show_horizontal_track(&self) -> bool {
        if !self.can_scroll_x() {
            return false;
        }
        let overflow = self.content_size.width > self.bounds.size.width;
        match self.scrollbar_policy {
            ScrollbarPolicy::Always => true,
            ScrollbarPolicy::Never => false,
            ScrollbarPolicy::Auto => overflow,
        }
    }

    fn show_horizontal_thumb(&self) -> bool {
        if !self.can_scroll_x() {
            return false;
        }
        let overflow = self.content_size.width > self.bounds.size.width;
        match self.scrollbar_policy {
            ScrollbarPolicy::Always => overflow,
            ScrollbarPolicy::Never => false,
            ScrollbarPolicy::Auto => overflow,
        }
    }

    fn vertical_thumb_rect(&self) -> Rect {
        let track_h = self.bounds.size.height;
        let content_h = self.content_size.height;
        if content_h <= 0.0 || track_h <= 0.0 {
            return Rect::zero();
        }
        let thumb_h = (self.bounds.size.height / content_h * track_h).clamp(20.0_f32.min(track_h), track_h);
        let ratio = if self.max_scroll_y() > 0.0 {
            self.scroll_offset.y.clamp(0.0, self.max_scroll_y()) / self.max_scroll_y()
        } else {
            0.0
        };
        let thumb_y = ratio * (track_h - thumb_h);
        Rect::new(
            Point::new(
                self.bounds.origin.x + self.bounds.size.width - self.scrollbar_width,
                self.bounds.origin.y + thumb_y,
            ),
            Size::new(self.scrollbar_width, thumb_h),
        )
    }

    fn horizontal_thumb_rect(&self) -> Rect {
        let track_w = self.bounds.size.width;
        let content_w = self.content_size.width;
        if content_w <= 0.0 || track_w <= 0.0 {
            return Rect::zero();
        }
        let thumb_w = (self.bounds.size.width / content_w * track_w).clamp(20.0_f32.min(track_w), track_w);
        let ratio = if self.max_scroll_x() > 0.0 {
            self.scroll_offset.x.clamp(0.0, self.max_scroll_x()) / self.max_scroll_x()
        } else {
            0.0
        };
        let thumb_x = ratio * (track_w - thumb_w);
        Rect::new(
            Point::new(
                self.bounds.origin.x + thumb_x,
                self.bounds.origin.y + self.bounds.size.height - self.scrollbar_width,
            ),
            Size::new(thumb_w, self.scrollbar_width),
        )
    }

    fn flash_scrollbar(&mut self) {
        self.scrollbar_opacity = 1.0;
        self.scrollbar_idle_time = 0.0;
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

    fn is_animating(&self) -> bool {
        let fading = self.scrollbar_policy == ScrollbarPolicy::Auto
            && self.scrollbar_opacity > 0.0
            && !self.hover_scrollbar_area;
        self.is_coasting || fading
    }
}

impl Element for ScrollViewElement {
    fn update(&mut self, widget: &dyn Widget, _ctx: &mut UpdateContext) {
        if let Some(sv) = widget.as_any().downcast_ref::<ScrollView>() {
            self.direction = sv.direction;
            self.scrollbar_policy = sv.scrollbar_policy;
            self.scrollbar_width = sv.scrollbar_width;
            self.center_content = sv.center_content;
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
            left: 0.0,
            top: 0.0,
            right: 0.0,
            bottom: 0.0,
            unbounded_width: self.can_scroll_x(),
            unbounded_height: self.can_scroll_y(),
        }
    }

    fn build_display_list(&self, list: &mut DisplayList, _clip: Rect) {
        if let Some(bg) = self.mss.background_color {
            list.push_rect(self.bounds, bg, [0.0; 4]);
        }

        let clip_bounds = Rect::new(
            Point::new(self.bounds.origin.x - 1.0, self.bounds.origin.y - 1.0),
            Size::new(self.bounds.size.width + 2.0, self.bounds.size.height + 2.0),
        );
        list.push_clip(clip_bounds);
        let sf = list.scale_factor().max(1.0);
        let snap = |v: f32| (v * sf).trunc() / sf;
        let center = self.center_offset();
        let transform = Transform::translation(
            snap(center.x - self.scroll_offset.x),
            snap(center.y - self.scroll_offset.y),
        );
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

        if self.show_vertical_track() && self.show_vertical_thumb() {
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
        if self.show_horizontal_track() && self.show_horizontal_thumb() {
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

                let mut dy = if can_y { -*delta } else { 0.0 };
                let mut dx = if can_x { -*ev_dx } else { 0.0 };

                let old_y = self.scroll_offset.y;
                let old_x = self.scroll_offset.x;

                if dy < 0.0 && old_y <= 0.0 {
                    dy = 0.0;
                }
                if dy > 0.0 && old_y >= self.max_scroll_y() {
                    dy = 0.0;
                }
                if dx < 0.0 && old_x <= 0.0 {
                    dx = 0.0;
                }
                if dx > 0.0 && old_x >= self.max_scroll_x() {
                    dx = 0.0;
                }

                self.scroll_offset.y = (old_y + dy).clamp(0.0, self.max_scroll_y());
                self.scroll_offset.x = (old_x + dx).clamp(0.0, self.max_scroll_x());

                if (self.scroll_offset.y - old_y).abs() < 0.001
                    && (self.scroll_offset.x - old_x).abs() < 0.001
                {
                    return EventResult::Handled;
                }

                // Разворот колеса обнуляет прежнюю инерцию, иначе усреднение
                // сохраняет знак старого движения и прокрутка ещё пару
                // оборотов едет в обратную сторону.
                if dy * self.velocity.y < 0.0 {
                    self.velocity.y = 0.0;
                }
                if dx * self.velocity.x < 0.0 {
                    self.velocity.x = 0.0;
                }

                let alpha = 0.3;
                self.velocity.y = self.velocity.y * (1.0 - alpha) + dy * VELOCITY_SCALE * alpha;
                self.velocity.x = self.velocity.x * (1.0 - alpha) + dx * VELOCITY_SCALE * alpha;
                self.is_coasting = true;

                self.flash_scrollbar();
                ctx.request_paint();
                EventResult::Handled
            }

            Event::KeyDown(key) => {
                let vp_h = self.bounds.size.height;
                match key {
                    Key::Home if self.can_scroll_y() => {
                        self.scroll_offset.y = 0.0;
                        self.velocity = Point::zero();
                        self.is_coasting = false;
                        self.flash_scrollbar();
                        ctx.request_paint();
                        EventResult::Handled
                    }
                    Key::End if self.can_scroll_y() => {
                        self.scroll_offset.y = self.max_scroll_y();
                        self.velocity = Point::zero();
                        self.is_coasting = false;
                        self.flash_scrollbar();
                        ctx.request_paint();
                        EventResult::Handled
                    }
                    Key::PageUp if self.can_scroll_y() => {
                        self.scroll_offset.y =
                            (self.scroll_offset.y - vp_h).max(0.0);
                        self.flash_scrollbar();
                        ctx.request_paint();
                        EventResult::Handled
                    }
                    Key::PageDown if self.can_scroll_y() => {
                        self.scroll_offset.y =
                            (self.scroll_offset.y + vp_h).min(self.max_scroll_y());
                        self.flash_scrollbar();
                        ctx.request_paint();
                        EventResult::Handled
                    }
                    Key::Up if self.can_scroll_y() => {
                        self.scroll_offset.y =
                            (self.scroll_offset.y - 40.0).max(0.0);
                        self.flash_scrollbar();
                        ctx.request_paint();
                        EventResult::Handled
                    }
                    Key::Down if self.can_scroll_y() => {
                        self.scroll_offset.y =
                            (self.scroll_offset.y + 40.0).min(self.max_scroll_y());
                        self.flash_scrollbar();
                        ctx.request_paint();
                        EventResult::Handled
                    }
                    _ => EventResult::Ignored,
                }
            }

            Event::MouseDown { button, position } if *button == MouseButton::Left => {
                if self.show_vertical_thumb() {
                    let thumb = self.vertical_thumb_rect();
                    if thumb.contains(*position) {
                        self.dragging_vertical = true;
                        ctx.request_paint();
                        return EventResult::Captured;
                    }
                }
                if self.show_horizontal_thumb() {
                    let thumb = self.horizontal_thumb_rect();
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
                    let thumb_h = self.vertical_thumb_rect().size.height;
                    let track_h = self.bounds.size.height;
                    if track_h > thumb_h {
                        let rel = (pos.y - self.bounds.origin.y - thumb_h / 2.0)
                            / (track_h - thumb_h);
                        self.scroll_offset.y = (rel.clamp(0.0, 1.0)
                            * self.max_scroll_y())
                        .clamp(0.0, self.max_scroll_y());
                    }
                    self.flash_scrollbar();
                    ctx.request_paint();
                    return EventResult::Captured;
                }
                if self.dragging_horizontal {
                    let thumb_w = self.horizontal_thumb_rect().size.width;
                    let track_w = self.bounds.size.width;
                    if track_w > thumb_w {
                        let rel = (pos.x - self.bounds.origin.x - thumb_w / 2.0)
                            / (track_w - thumb_w);
                        self.scroll_offset.x = (rel.clamp(0.0, 1.0)
                            * self.max_scroll_x())
                        .clamp(0.0, self.max_scroll_x());
                    }
                    self.flash_scrollbar();
                    ctx.request_paint();
                    return EventResult::Captured;
                }

                let was_area = self.hover_scrollbar_area;
                let mut in_area = false;
                let hit_margin = 20.0_f32;

                if self.show_vertical_track() && self.bounds.contains(*pos) {
                    let area_x = self.bounds.origin.x + self.bounds.size.width
                        - self.scrollbar_width
                        - hit_margin;
                    if pos.x >= area_x
                        && pos.x <= self.bounds.origin.x + self.bounds.size.width
                        && pos.y >= self.bounds.origin.y
                        && pos.y <= self.bounds.origin.y + self.bounds.size.height
                    {
                        in_area = true;
                    }
                }
                if self.show_horizontal_track() && self.bounds.contains(*pos) {
                    let area_y = self.bounds.origin.y + self.bounds.size.height
                        - self.scrollbar_width
                        - hit_margin;
                    if pos.y >= area_y
                        && pos.y <= self.bounds.origin.y + self.bounds.size.height
                        && pos.x >= self.bounds.origin.x
                        && pos.x <= self.bounds.origin.x + self.bounds.size.width
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

                if self.show_vertical_thumb() {
                    let was = self.hover_vertical;
                    self.hover_vertical = self.vertical_thumb_rect().contains(*pos);
                    if self.hover_vertical != was {
                        ctx.request_paint();
                        result = EventResult::Handled;
                    }
                }
                if self.show_horizontal_thumb() {
                    let was = self.hover_horizontal;
                    self.hover_horizontal = self.horizontal_thumb_rect().contains(*pos);
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
                        self.scroll_offset.y =
                            (self.scroll_offset.y + dy).clamp(0.0, self.max_scroll_y());
                    }
                    if self.can_scroll_x() {
                        self.scroll_offset.x =
                            (self.scroll_offset.x + dx).clamp(0.0, self.max_scroll_x());
                    }

                    let alpha = 0.3;
                    self.velocity.y =
                        self.velocity.y * (1.0 - alpha) + dy * VELOCITY_SCALE * alpha;
                    self.velocity.x =
                        self.velocity.x * (1.0 - alpha) + dx * VELOCITY_SCALE * alpha;

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

                if self.velocity.y.abs() > MIN_VELOCITY
                    || self.velocity.x.abs() > MIN_VELOCITY
                {
                    self.is_coasting = true;
                }
                EventResult::Handled
            }

            _ => EventResult::Ignored,
        }
    }

    fn animate(&mut self, dt: Duration) -> bool {
        let dt_secs = dt.as_secs_f32();
        let mut needs_repaint = false;

        if self.is_coasting {
            let friction = FRICTION.powf(dt_secs * 60.0);
            self.velocity.x *= friction;
            self.velocity.y *= friction;

            self.scroll_offset.x += self.velocity.x * dt_secs;
            self.scroll_offset.y += self.velocity.y * dt_secs;

            self.scroll_offset = self.clamp_offset(self.scroll_offset);

            let at_edge_y = (self.scroll_offset.y <= 0.0 && self.velocity.y < 0.0)
                || (self.scroll_offset.y >= self.max_scroll_y() && self.velocity.y > 0.0);
            let at_edge_x = (self.scroll_offset.x <= 0.0 && self.velocity.x < 0.0)
                || (self.scroll_offset.x >= self.max_scroll_x() && self.velocity.x > 0.0);

            if at_edge_y {
                self.velocity.y = 0.0;
            }
            if at_edge_x {
                self.velocity.x = 0.0;
            }

            if self.velocity.y.abs() < MIN_VELOCITY
                && self.velocity.x.abs() < MIN_VELOCITY
            {
                self.velocity = Point::zero();
                self.is_coasting = false;
            }

            needs_repaint = true;
        }

        if self.scrollbar_policy == ScrollbarPolicy::Auto && self.scrollbar_opacity > 0.0 {
            if self.hover_scrollbar_area || self.dragging_vertical || self.dragging_horizontal {
                self.scrollbar_idle_time = 0.0;
            } else {
                self.scrollbar_idle_time += dt_secs;
                if self.scrollbar_idle_time > SCROLLBAR_FADE_DELAY {
                    self.scrollbar_opacity =
                        (self.scrollbar_opacity - SCROLLBAR_FADE_RATE * dt_secs).max(0.0);
                    needs_repaint = true;
                }
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

    /// Смещение содержимого для попадания курсора. Центрирующий сдвиг —
    /// часть этого смещения: иначе указатель уезжал бы относительно того,
    /// что нарисовано.
    fn scroll_offset(&self) -> Point {
        let center = self.center_offset();
        Point::new(
            self.scroll_offset.x - center.x,
            self.scroll_offset.y - center.y,
        )
    }

    fn is_scroll_container(&self) -> bool {
        true
    }

    fn ensure_visible(&mut self, child_rect: Rect) -> bool {
        if self.can_scroll_y() {
            let margin = 20.0;
            let visible_top = self.scroll_offset.y;
            let visible_bottom = visible_top + self.bounds.size.height;
            let child_top = child_rect.origin.y;
            let child_bottom = child_rect.origin.y + child_rect.size.height;

            if child_bottom + margin > visible_bottom {
                let target_y = (child_bottom + margin - self.bounds.size.height)
                    .clamp(0.0, self.max_scroll_y());
                self.scroll_offset.y = target_y;
                return true;
            } else if child_top - margin < visible_top {
                let target_y = (child_top - margin).max(0.0);
                self.scroll_offset.y = target_y;
                return true;
            }
        }
        false
    }

    fn clip_content(&self) -> bool {
        false
    }

    fn is_relayout_boundary(&self) -> bool {
        true
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

    fn set_classes(&mut self, classes: Vec<String>) {
        self.classes = classes;
        self.mark_dirty(DirtyFlags::RENDER);
    }

    fn get_classes(&self) -> &[String] {
        &self.classes
    }

    fn element_type_name(&self) -> &str {
        "ScrollView"
    }

    fn reset_mss_styles(&mut self) { self.mss.reset(); }
    fn mss(&self) -> Option<&crate::mss::MssFields> { Some(&self.mss) }
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
        self.mss.apply_transitions(base, hover, active, focus, selected);
    }

    fn accessibility_info(&self) -> Option<crate::a11y::AccessibilityInfo> {
        Some(crate::a11y::AccessibilityInfo {
            role: crate::a11y::Role::ScrollBar,
            state: crate::a11y::NodeState::default(),
            properties: crate::a11y::NodeProperties::default(),
        })
    }
}

impl StyledElement for ScrollViewElement {
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
    use super::*;

    fn element(center: bool, viewport: Size, content: Size) -> Box<dyn Element> {
        let sv = ScrollView::new().both().center_content(center);
        let mut el = sv.create_element();
        el.layout(Constraints::tight(viewport));
        el.set_content_size(content);
        el
    }

    fn center_of(el: &dyn Element) -> Point {
        // `scroll_offset` для событий содержит центрирующий сдвиг со знаком
        // минус — по нему и проверяем, куда уехало содержимое.
        Point::new(-el.scroll_offset().x, -el.scroll_offset().y)
    }

    /// Документ уже области — стоит по центру по горизонтали.
    #[test]
    fn narrow_content_is_centered() {
        let el = element(true, Size::new(1000.0, 600.0), Size::new(600.0, 2000.0));
        let c = center_of(el.as_ref());
        assert_eq!(c.x, 200.0);
        // По вертикали содержимое переполняет область — сдвига нет.
        assert_eq!(c.y, 0.0);
    }

    /// Документ шире области — центрировать нечего, идёт обычная прокрутка.
    #[test]
    fn wide_content_is_not_centered() {
        let el = element(true, Size::new(1000.0, 600.0), Size::new(1400.0, 2000.0));
        assert_eq!(center_of(el.as_ref()), Point::zero());
    }

    /// Без явного включения поведение прежнее — прижатие к левому верхнему углу.
    #[test]
    fn centering_is_opt_in() {
        let el = element(false, Size::new(1000.0, 600.0), Size::new(600.0, 200.0));
        assert_eq!(center_of(el.as_ref()), Point::zero());
    }
}
