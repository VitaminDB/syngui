use crate::core::{Color, Point, Rect, RectExt, Size, Transform};
use crate::input::{CursorIcon, Event, EventResult, MouseButton};
use crate::layout::Constraints;
use crate::mss::{ComputedStyle, Dimension, IconState, MssFields};
use crate::render::{Border, DisplayList};
use crate::widget::context::{EventContext, EventContextExt};
use crate::widget::{DirtyFlags, Element, ElementId, ElementTree, LayoutHint, StyledElement, UpdateContext, Widget};
use std::any::Any;
use std::collections::HashMap;
use std::ops::Range;
use std::sync::Arc;
use crate::core::sync::Mutex;
use crate::signal::RwSignal;
use std::time::Duration;

#[derive(Clone, Debug)]
pub struct ListItem {
    pub text: String,
    pub secondary_text: Option<String>,
    pub icon: Option<String>,
    pub trailing: Option<String>,
    pub disabled: bool,
}

impl ListItem {
    pub fn new(text: impl Into<String>) -> Self {
        Self {
            text: text.into(),
            secondary_text: None,
            icon: None,
            trailing: None,
            disabled: false,
        }
    }

    pub fn secondary(mut self, text: impl Into<String>) -> Self {
        self.secondary_text = Some(text.into());
        self
    }

    pub fn icon(mut self, icon: impl Into<String>) -> Self {
        self.icon = Some(icon.into());
        self
    }

    pub fn trailing(mut self, text: impl Into<String>) -> Self {
        self.trailing = Some(text.into());
        self
    }

    pub fn disabled(mut self, disabled: bool) -> Self {
        self.disabled = disabled;
        self
    }
}

#[derive(Clone, Copy, Debug, Default, PartialEq)]
pub enum SelectionMode {
    #[default]
    None,
    Single,
    Multiple,
}

type ItemBuilderFn = Arc<dyn Fn(usize) -> ListItem + Send + Sync>;

type ItemWidgetBuilderFn = Arc<dyn Fn(usize, &ListItem, bool, bool) -> Box<dyn Widget> + Send + Sync>;

enum ListDataSource {
    Eager(Vec<ListItem>),
    Virtual {
        item_count: usize,
        item_builder: ItemBuilderFn,
    },
}

impl ListDataSource {
    fn item_count(&self) -> usize {
        match self {
            ListDataSource::Eager(items) => items.len(),
            ListDataSource::Virtual { item_count, .. } => *item_count,
        }
    }
}

pub struct ListView {
    data: ListDataSource,
    item_height: f32,
    buffer_size: usize,
    selection_mode: SelectionMode,
    selected: Vec<usize>,
    on_select: Option<Arc<Mutex<dyn FnMut(usize) + Send>>>,
    width: Option<Dimension>,
    height: Option<Dimension>,
    item_widget: Option<ItemWidgetBuilderFn>,
    classes: Vec<String>,
    selected_signal: Option<RwSignal<usize>>,
    selected_signal_offset: usize,
    on_reach_top: Option<Arc<Mutex<dyn FnMut() + Send>>>,
    reach_top_threshold: f32,
}

impl ListView {
    pub fn new(items: Vec<ListItem>) -> Self {
        Self {
            data: ListDataSource::Eager(items),
            item_height: 48.0,
            buffer_size: 5,
            selection_mode: SelectionMode::None,
            selected: Vec::new(),
            on_select: None,
            width: None,
            height: None,
            item_widget: None,
            classes: Vec::new(),
            selected_signal: None,
            selected_signal_offset: 0,
            on_reach_top: None,
            reach_top_threshold: 0.0,
        }
    }

    pub fn virtual_new(
        item_count: usize,
        item_builder: impl Fn(usize) -> ListItem + Send + Sync + 'static,
    ) -> Self {
        Self {
            data: ListDataSource::Virtual {
                item_count,
                item_builder: Arc::new(item_builder),
            },
            item_height: 48.0,
            buffer_size: 5,
            selection_mode: SelectionMode::None,
            selected: Vec::new(),
            on_select: None,
            width: None,
            height: None,
            item_widget: None,
            classes: Vec::new(),
            selected_signal: None,
            selected_signal_offset: 0,
            on_reach_top: None,
            reach_top_threshold: 0.0,
        }
    }

    pub fn item_height(mut self, h: f32) -> Self {
        self.item_height = h;
        self
    }

    pub fn buffer_size(mut self, n: usize) -> Self {
        self.buffer_size = n;
        self
    }

    pub fn selection_mode(mut self, mode: SelectionMode) -> Self {
        self.selection_mode = mode;
        self
    }

    pub fn selected(mut self, indices: Vec<usize>) -> Self {
        self.selected = indices;
        self
    }

    pub fn selected_signal(mut self, signal: RwSignal<usize>, offset: usize) -> Self {
        self.selected_signal = Some(signal);
        self.selected_signal_offset = offset;
        self
    }

    pub fn on_select(mut self, f: impl FnMut(usize) + Send + 'static) -> Self {
        self.on_select = Some(Arc::new(Mutex::new(f)));
        self
    }

    pub fn width(mut self, w: f32) -> Self {
        self.width = Some(Dimension::Px(w));
        self
    }

    pub fn height(mut self, h: f32) -> Self {
        self.height = Some(Dimension::Px(h));
        self
    }

    pub fn item_widget(
        mut self,
        builder: impl Fn(usize, &ListItem, bool, bool) -> Box<dyn Widget> + Send + Sync + 'static,
    ) -> Self {
        self.item_widget = Some(Arc::new(builder));
        self
    }

    pub fn class(mut self, class: impl Into<String>) -> Self {
        self.classes.push(class.into());
        self
    }

    pub fn on_reach_top(
        mut self,
        threshold_px: f32,
        f: impl FnMut() + Send + 'static,
    ) -> Self {
        self.reach_top_threshold = threshold_px.max(0.0);
        self.on_reach_top = Some(Arc::new(Mutex::new(f)));
        self
    }
}

impl Widget for ListView {
    fn create_element(&self) -> Box<dyn Element> {
        let data = match &self.data {
            ListDataSource::Eager(items) => ListDataSource::Eager(items.clone()),
            ListDataSource::Virtual { item_count, item_builder } => ListDataSource::Virtual {
                item_count: *item_count,
                item_builder: item_builder.clone(),
            },
        };
        let compositional = self.item_widget.is_some();
        Box::new(ListViewElement {
            id: ElementId::new(),
            data,
            item_height: self.item_height,
            buffer_size: self.buffer_size,
            selection_mode: self.selection_mode,
            selected: self.selected.clone(),
            on_select: self.on_select.clone(),
            width: self.width,
            height: self.height,
            scroll_offset: 0.0,
            velocity: 0.0,
            hovered_index: None,
            bounds: Rect::zero(),
            scrollbar_dragging: false,
            scrollbar_drag_offset: 0.0,
            scrollbar_hovered: false,
            scrollbar_fader: crate::widgets::scroll::ScrollbarFader::default(),
            item_cache: HashMap::new(),
            cache_range: 0..0,
            classes: self.classes.clone(),
            dirty_flags: DirtyFlags::LAYOUT | DirtyFlags::RENDER,
            mss: MssFields::new(),
            item_widget_builder: self.item_widget.clone(),
            compositional,
            needs_child_rebuild: compositional,
            actual_content_height: 0.0,
            selected_signal: self.selected_signal,
            selected_signal_offset: self.selected_signal_offset,
            on_reach_top: self.on_reach_top.clone(),
            reach_top_threshold: self.reach_top_threshold,
            was_above_reach_top: true,
        })
    }

    fn can_update(&self, other: &dyn Any) -> bool { other.is::<Self>() }
    fn as_any(&self) -> &dyn Any { self }
    fn as_any_mut(&mut self) -> &mut dyn Any { self }
    fn mount(&self, _tree: &mut ElementTree, _parent_id: ElementId) {}

    fn widget_classes(&self) -> &[String] {
        &self.classes
    }
}

pub struct ListViewElement {
    id: ElementId,
    data: ListDataSource,
    item_height: f32,
    buffer_size: usize,
    selection_mode: SelectionMode,
    selected: Vec<usize>,
    on_select: Option<Arc<Mutex<dyn FnMut(usize) + Send>>>,
    width: Option<Dimension>,
    height: Option<Dimension>,
    scroll_offset: f32,
    velocity: f32,
    hovered_index: Option<usize>,
    bounds: Rect,
    scrollbar_dragging: bool,
    scrollbar_drag_offset: f32,
    scrollbar_hovered: bool,
    scrollbar_fader: crate::widgets::scroll::ScrollbarFader,
    item_cache: HashMap<usize, ListItem>,
    cache_range: Range<usize>,
    classes: Vec<String>,
    dirty_flags: DirtyFlags,
    mss: MssFields,
    item_widget_builder: Option<ItemWidgetBuilderFn>,
    compositional: bool,
    needs_child_rebuild: bool,
    actual_content_height: f32,
    selected_signal: Option<RwSignal<usize>>,
    selected_signal_offset: usize,
    on_reach_top: Option<Arc<Mutex<dyn FnMut() + Send>>>,
    reach_top_threshold: f32,
    was_above_reach_top: bool,
}

impl ListViewElement {
    fn item_count(&self) -> usize {
        self.data.item_count()
    }

    fn content_height(&self) -> f32 {
        if self.compositional && self.actual_content_height > 0.0 {
            self.actual_content_height
        } else {
            self.item_count() as f32 * self.item_height
        }
    }

    fn max_scroll(&self) -> f32 {
        (self.content_height() - self.bounds.size.height).max(0.0)
    }

    fn item_at_y(&self, y: f32) -> Option<usize> {
        if y < self.bounds.y() || y > self.bounds.y() + self.bounds.size.height {
            return None;
        }
        let local_y = y - self.bounds.y() + self.scroll_offset;
        let idx = (local_y / self.item_height) as usize;
        if idx < self.item_count() { Some(idx) } else { None }
    }

    fn get_item(&self, index: usize) -> Option<&ListItem> {
        match &self.data {
            ListDataSource::Eager(items) => items.get(index),
            ListDataSource::Virtual { .. } => self.item_cache.get(&index),
        }
    }

    fn is_item_disabled(&self, index: usize) -> bool {
        self.get_item(index).map_or(true, |i| i.disabled)
    }

    fn check_reach_top(&mut self) {
        if self.on_reach_top.is_none() {
            return;
        }
        let above = self.scroll_offset > self.reach_top_threshold;
        if self.was_above_reach_top && !above {
            if let Some(cb) = self.on_reach_top.as_ref() {
                if let Ok(mut f) = cb.lock() {
                    f();
                }
            }
        }
        self.was_above_reach_top = above;
    }

    fn effective_selected(&self) -> Vec<usize> {
        if let Some(sig) = self.selected_signal {
            let global = sig.get_untracked();
            if global >= self.selected_signal_offset {
                let local = global - self.selected_signal_offset;
                if local < self.item_count() {
                    return vec![local];
                }
            }
            return vec![];
        }
        self.selected.clone()
    }

    fn scrollbar_rects(&self) -> Option<(Rect, Rect)> {
        if self.content_height() <= self.bounds.size.height {
            return None;
        }
        let style = self.compose_scrollbar_style();
        let track = crate::widgets::scroll::vertical_track_rect(self.bounds, &style);
        let thumb = crate::widgets::scroll::vertical_thumb_rect(
            self.bounds,
            self.content_height(),
            self.scroll_offset,
            &style,
        );
        Some((track, thumb))
    }

    fn compose_scrollbar_style(&self) -> crate::widgets::scroll::ScrollbarStyle {
        let fg = self.mss.color
            .or(self.mss.border_color)
            .unwrap_or(Color::from_hex("#9CA3AF"));
        self.mss.scrollbar_style(fg)
    }

    fn draw_scrollbar(&self, list: &mut DisplayList) {
        if self.content_height() <= self.bounds.size.height {
            return;
        }
        let style = self.compose_scrollbar_style();
        let opacity = crate::widgets::scroll::effective_opacity(&self.scrollbar_fader, &style);
        if opacity <= 0.0 { return; }
        let mut fader = self.scrollbar_fader;
        fader.dragging = self.scrollbar_dragging;
        fader.hovered = self.scrollbar_hovered || fader.hovered;
        crate::widgets::scroll::render_vertical(
            list,
            self.bounds,
            self.content_height(),
            self.scroll_offset,
            &style,
            &fader,
            opacity,
        );
    }

    fn ensure_cached_for_viewport(&mut self) {
        if let ListDataSource::Virtual { item_count, ref item_builder } = self.data {
            let viewport_top = self.scroll_offset;
            let viewport_bottom = viewport_top + self.bounds.size.height;
            let vis_first = (viewport_top / self.item_height) as usize;
            let vis_last = ((viewport_bottom / self.item_height) as usize + 1).min(item_count);

            let fetch_start = vis_first.saturating_sub(self.buffer_size);
            let fetch_end = (vis_last + self.buffer_size).min(item_count);
            let fetch_range = fetch_start..fetch_end;

            if fetch_range.start >= self.cache_range.start && fetch_range.end <= self.cache_range.end {
                return;
            }

            let retain_start = vis_first.saturating_sub(self.buffer_size * 2);
            let retain_end = (vis_last + self.buffer_size * 2).min(item_count);

            self.item_cache.retain(|k, _| *k >= retain_start && *k < retain_end);

            let builder = item_builder.clone();
            for i in fetch_range.clone() {
                if !self.item_cache.contains_key(&i) {
                    self.item_cache.insert(i, builder(i));
                }
            }

            self.cache_range = retain_start..retain_end;
        }
    }
}

impl Element for ListViewElement {
    fn update(&mut self, widget: &dyn Widget, _ctx: &mut UpdateContext) {
        if let Some(lv) = widget.as_any().downcast_ref::<ListView>() {
            match &lv.data {
                ListDataSource::Eager(items) => {
                    self.data = ListDataSource::Eager(items.clone());
                    self.item_cache.clear();
                    self.cache_range = 0..0;
                }
                ListDataSource::Virtual { item_count, item_builder } => {
                    let old_count = self.item_count();
                    self.data = ListDataSource::Virtual {
                        item_count: *item_count,
                        item_builder: item_builder.clone(),
                    };
                    if *item_count != old_count {
                        self.item_cache.clear();
                        self.cache_range = 0..0;
                    }
                }
            }
            self.item_height = lv.item_height;
            self.buffer_size = lv.buffer_size;
            self.selection_mode = lv.selection_mode;
            self.selected = lv.selected.clone();
            self.on_select = lv.on_select.clone();
            self.width = lv.width;
            self.height = lv.height;
            self.selected_signal = lv.selected_signal;
            self.selected_signal_offset = lv.selected_signal_offset;
            self.item_widget_builder = lv.item_widget.clone();
            self.compositional = lv.item_widget.is_some();
            if self.compositional {
                self.needs_child_rebuild = true;
            }
            self.ensure_cached_for_viewport();
            self.mark_dirty(DirtyFlags::LAYOUT | DirtyFlags::RENDER);
        }
    }

    fn layout(&mut self, constraints: Constraints) -> Size {
        let w = self.width.map(|d| d.resolve(constraints.max_width)).unwrap_or(constraints.max_width).min(constraints.max_width);
        let h = self.height.map(|d| d.resolve(constraints.max_height)).unwrap_or(constraints.max_height).min(constraints.max_height);
        let natural_h = self.content_height();
        let h = if h.is_infinite() { natural_h } else { h.min(constraints.max_height) };
        self.bounds = Rect::new(Point::zero(), Size::new(w, h));
        if !self.compositional {
            self.ensure_cached_for_viewport();
        }
        Size::new(w, h)
    }

    fn build_display_list(&self, list: &mut DisplayList, _clip: Rect) {
        let bg_color = self.mss.background_color.unwrap_or(Color::WHITE);
        let border_color = self.mss.border_color.unwrap_or(Color::from_hex("#E5E7EB"));
        let resolve_base = self.bounds.size.width.min(self.bounds.size.height);
        let border_radius = self.mss.border_radius_resolved(resolve_base, 8.0);
        let border_width = self.mss.border_width_or(1.0);

        let h_bg = self
            .content_height()
            .min(self.bounds.size.height)
            .max(0.0);
        let bg_rect = Rect::new(
            self.bounds.origin,
            Size::new(self.bounds.size.width, h_bg),
        );

        if bg_color.a > 0.001 || border_color.a > 0.001 {
            if border_width > 0.0 && border_color.a > 0.001 {
                list.push_rect_bordered(
                    bg_rect, bg_color, border_radius,
                    Border::new(border_width, border_color),
                );
            } else {
                list.push_rect_bordered(
                    bg_rect, bg_color, border_radius,
                    Border::new(0.0, Color::TRANSPARENT),
                );
            }
        }

        if self.compositional {
            list.push_clip(self.bounds);
            let ty = -self.scroll_offset;
            list.push_transform(Transform::translation(0.0, ty));
            return;
        }

        let fg = self.mss.color.unwrap_or(Color::from_hex("#1F2937"));
        let primary = self.mss.accent_color.unwrap_or(Color::from_hex("#3B82F6"));

        list.push_clip(self.bounds);

        let viewport_top = self.scroll_offset;
        let viewport_bottom = viewport_top + self.bounds.size.height;
        let count = self.item_count();

        let vis_first = (viewport_top / self.item_height) as usize;
        let vis_last = ((viewport_bottom / self.item_height) as usize + 1).min(count);

        let render_first = vis_first.saturating_sub(self.buffer_size);
        let render_last = (vis_last + self.buffer_size).min(count);

        let effective_sel = self.effective_selected();
        for i in render_first..render_last {
            let item = match self.get_item(i) {
                Some(item) => item,
                None => continue,
            };
            let y = self.bounds.y() + (i as f32 * self.item_height) - self.scroll_offset;
            let row_rect = Rect::new(
                Point::new(self.bounds.x(), y),
                Size::new(self.bounds.size.width, self.item_height),
            );

            let is_selected = effective_sel.contains(&i);
            let is_hovered = self.hovered_index == Some(i);
            let row_bg = if is_selected {
                primary.with_alpha(0.15)
            } else if is_hovered && !item.disabled {
                bg_color.darken(0.03)
            } else if i % 2 == 1 {
                bg_color.darken(0.015)
            } else {
                Color::TRANSPARENT
            };
            if row_bg != Color::TRANSPARENT {
                list.push_rect(row_rect, row_bg, [0.0; 4]);
            }

            if i > render_first {
                let div_y = y.round();
                let div_rect = Rect::new(
                    Point::new(self.bounds.x() + 12.0, div_y),
                    Size::new(self.bounds.size.width - 24.0, 1.0),
                );
                list.push_rect(div_rect, border_color.with_alpha(0.15), [0.0; 4]);
            }

            let row_state = if item.disabled {
                IconState::Disabled
            } else if is_selected {
                IconState::Selected
            } else if is_hovered {
                IconState::Hover
            } else {
                IconState::Normal
            };
            let text_color = if item.disabled {
                fg.with_alpha(0.3)
            } else {
                fg
            };

            let mut text_x = self.bounds.x() + 16.0;

            if let Some(ref icon) = item.icon {
                let icon_rect = Rect::new(
                    Point::new(text_x, y + (self.item_height - 18.0) / 2.0),
                    Size::new(24.0, 18.0),
                );
                list.push_text(icon, icon_rect, self.mss.icon_color(row_state, fg), 18.0);
                text_x += 32.0;
            }

            let has_secondary = item.secondary_text.is_some();
            let primary_y = if has_secondary {
                y + (self.item_height / 2.0 - 16.0)
            } else {
                y + (self.item_height - 14.0) / 2.0
            };
            let primary_rect = Rect::new(
                Point::new(text_x, primary_y),
                Size::new(self.bounds.size.width - (text_x - self.bounds.x()) - 16.0, 16.0),
            );
            list.push_text(&item.text, primary_rect, text_color, 14.0);

            if let Some(ref secondary) = item.secondary_text {
                let sec_y = y + self.item_height / 2.0 + 2.0;
                let sec_rect = Rect::new(
                    Point::new(text_x, sec_y),
                    Size::new(self.bounds.size.width - (text_x - self.bounds.x()) - 16.0, 14.0),
                );
                let base = self.mss.icon_color(row_state, fg);
                let sec_color = base.with_alpha(base.a * 0.75);
                list.push_text(secondary, sec_rect, sec_color, 12.0);
            }

            if let Some(ref trailing) = item.trailing {
                let trail_rect = Rect::new(
                    Point::new(self.bounds.x() + self.bounds.size.width - 60.0, y + (self.item_height - 12.0) / 2.0),
                    Size::new(44.0, 14.0),
                );
                let base = self.mss.icon_color(row_state, fg);
                let trail_color = base.with_alpha(base.a * 0.6);
                list.push_text(trailing, trail_rect, trail_color, 12.0);
            }
        }

        self.draw_scrollbar(list);

        list.pop_clip();

        if border_width > 0.0 && border_color.a > 0.001 {
            list.push_rect_bordered(
                self.bounds, Color::TRANSPARENT, border_radius,
                Border::new(border_width, border_color),
            );
        }
    }

    fn post_build_display_list(&self, list: &mut DisplayList, _clip: Rect) {
        if !self.compositional { return; }
        let border_color = self.mss.border_color.unwrap_or(Color::from_hex("#E5E7EB"));
        let resolve_base = self.bounds.size.width.min(self.bounds.size.height);
        let border_radius = self.mss.border_radius_resolved(resolve_base, 8.0);
        let border_width = self.mss.border_width_or(1.0);
        list.pop_transform();
        list.pop_clip();
        list.push_clip(self.bounds);
        self.draw_scrollbar(list);
        list.pop_clip();
        if border_width > 0.0 && border_color.a > 0.001 {
            list.push_rect_bordered(
                self.bounds, Color::TRANSPARENT, border_radius,
                Border::new(border_width, border_color),
            );
        }
    }

    fn animate(&mut self, dt: Duration) -> bool {
        let mut needs_repaint = false;
        if self.velocity.abs() > 0.5 {
            self.velocity *= 0.92f32.powf(dt.as_secs_f32() * 60.0);
            self.scroll_offset = (self.scroll_offset + self.velocity * dt.as_secs_f32())
                .clamp(0.0, self.max_scroll());
            if self.velocity.abs() < 0.5 {
                self.velocity = 0.0;
            }
            if !self.compositional {
                self.ensure_cached_for_viewport();
            }
            self.check_reach_top();
            self.scrollbar_fader.flash();
            needs_repaint = true;
        }
        let style = self.compose_scrollbar_style();
        self.scrollbar_fader.dragging = self.scrollbar_dragging;
        self.scrollbar_fader.hovered = self.scrollbar_hovered || self.scrollbar_fader.hovered;
        if self.scrollbar_fader.tick(dt.as_secs_f32(), &style) {
            needs_repaint = true;
        }
        needs_repaint
    }

    fn handle_event(&mut self, event: &Event, ctx: &mut EventContext) -> EventResult {
        match event {
            Event::MouseMove(pos) => {
                if self.scrollbar_dragging {
                    if let Some((_, _)) = self.scrollbar_rects() {
                        let track_h = self.bounds.size.height;
                        let thumb_h = (self.bounds.size.height / self.content_height() * track_h).max(20.0);
                        let max_s = self.max_scroll();
                        let relative_y = pos.y - self.bounds.y() - self.scrollbar_drag_offset;
                        let ratio = relative_y / (track_h - thumb_h);
                        let new_offset = (ratio * max_s).clamp(0.0, max_s);
                        self.scroll_offset = new_offset;
                        self.velocity = 0.0;
                        if !self.compositional {
                            self.ensure_cached_for_viewport();
                        }
                        self.check_reach_top();
                        ctx.set_cursor(CursorIcon::Default);
                        ctx.request_paint();
                    }
                    return EventResult::Handled;
                }

                if !self.bounds.contains(*pos) {
                    let changed = self.hovered_index.is_some() || self.scrollbar_hovered;
                    self.hovered_index = None;
                    self.scrollbar_hovered = false;
                    if changed {
                        if self.compositional {
                            self.needs_child_rebuild = true;
                        }
                        ctx.request_paint();
                    }
                    return EventResult::Ignored;
                }

                let sb_hovered = self.scrollbar_rects()
                    .map_or(false, |(track, _)| track.contains(*pos));
                if sb_hovered != self.scrollbar_hovered {
                    self.scrollbar_hovered = sb_hovered;
                    ctx.request_paint();
                }
                if sb_hovered {
                    ctx.set_cursor(CursorIcon::Default);
                    self.hovered_index = None;
                    return EventResult::Handled;
                }

                let new_hover = self.item_at_y(pos.y);
                if new_hover != self.hovered_index {
                    self.hovered_index = new_hover;
                    if self.compositional {
                        self.needs_child_rebuild = true;
                    }
                    ctx.request_paint();
                }
                if let Some(idx) = new_hover {
                    if !self.is_item_disabled(idx) {
                        ctx.set_cursor(CursorIcon::Pointer);
                    }
                }
                EventResult::Handled
            }
            Event::MouseDown { button, position } if *button == MouseButton::Left => {
                if !self.bounds.contains(*position) { return EventResult::Ignored; }

                if let Some((track, thumb)) = self.scrollbar_rects() {
                    if thumb.contains(*position) {
                        self.scrollbar_dragging = true;
                        self.scrollbar_drag_offset = position.y - thumb.y();
                        self.velocity = 0.0;
                        ctx.request_paint();
                        return EventResult::Handled;
                    }
                    if track.contains(*position) {
                        let track_h = self.bounds.size.height;
                        let thumb_h = thumb.size.height;
                        let max_s = self.max_scroll();
                        let relative_y = position.y - self.bounds.y() - thumb_h / 2.0;
                        let ratio = relative_y / (track_h - thumb_h);
                        self.scroll_offset = (ratio * max_s).clamp(0.0, max_s);
                        self.velocity = 0.0;
                        self.scrollbar_dragging = true;
                        self.scrollbar_drag_offset = thumb_h / 2.0;
                        self.ensure_cached_for_viewport();
                        self.check_reach_top();
                        ctx.request_paint();
                        return EventResult::Handled;
                    }
                }

                if let Some(idx) = self.item_at_y(position.y) {
                    if !self.is_item_disabled(idx) {
                        match self.selection_mode {
                            SelectionMode::None => {}
                            SelectionMode::Single => {
                                self.selected = vec![idx];
                                if let Some(sig) = self.selected_signal {
                                    sig.set(idx + self.selected_signal_offset);
                                }
                            }
                            SelectionMode::Multiple => {
                                if let Some(pos) = self.selected.iter().position(|&s| s == idx) {
                                    self.selected.remove(pos);
                                } else {
                                    self.selected.push(idx);
                                }
                            }
                        }
                        if let Some(ref cb) = self.on_select {
                            if let Ok(mut f) = cb.lock() { f(idx); }
                        }
                        if self.compositional {
                            self.needs_child_rebuild = true;
                        }
                        ctx.request_paint();
                    }
                }
                EventResult::Handled
            }
            Event::MouseUp { button, .. } if *button == MouseButton::Left => {
                if self.scrollbar_dragging {
                    self.scrollbar_dragging = false;
                    ctx.request_paint();
                    return EventResult::Handled;
                }
                EventResult::Ignored
            }
            Event::MouseWheel { delta, position, .. } => {
                if !self.bounds.contains(*position) { return EventResult::Ignored; }
                let scroll_amount = *delta;
                let new_offset = (self.scroll_offset - scroll_amount).clamp(0.0, self.max_scroll());
                if (new_offset - self.scroll_offset).abs() > 0.01 {
                    self.scroll_offset = new_offset;
                    self.velocity = 0.0;
                    if !self.compositional {
                        self.ensure_cached_for_viewport();
                    }
                    self.check_reach_top();
                    self.scrollbar_fader.flash();
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

    fn layout_hint(&self) -> LayoutHint {
        if self.compositional {
            LayoutHint::Scroll {
                left: 0.0, top: 0.0, right: 0.0, bottom: 0.0,
                unbounded_width: false,
                unbounded_height: true,
            }
        } else {
            LayoutHint::Center
        }
    }

    fn clip_content(&self) -> bool {
        false
    }

    fn scroll_offset(&self) -> Point {
        if self.compositional {
            Point::new(0.0, self.scroll_offset)
        } else {
            Point::zero()
        }
    }

    fn set_content_size(&mut self, size: Size) {
        if self.compositional {
            self.actual_content_height = size.height;
        }
    }

    fn manages_own_children(&self) -> bool {
        self.compositional
    }

    fn needs_rebuild(&self) -> bool {
        self.compositional && self.needs_child_rebuild
    }

    fn build_children(&self) -> Vec<Box<dyn Widget>> {
        if !self.compositional {
            return Vec::new();
        }
        let builder = match &self.item_widget_builder {
            Some(b) => b,
            None => return Vec::new(),
        };
        let count = self.item_count();
        let mut column = crate::widgets::containers::Column::new()
            .cross_axis_alignment(crate::layout::CrossAxisAlignment::Stretch);
        let effective_sel = self.effective_selected();
        for i in 0..count {
            let item = match &self.data {
                ListDataSource::Eager(items) => items.get(i).cloned(),
                ListDataSource::Virtual { item_builder, .. } => Some(item_builder(i)),
            };
            if let Some(item) = item {
                let is_selected = effective_sel.contains(&i);
                let is_hovered = self.hovered_index == Some(i);
                let child_widget = builder(i, &item, is_selected, is_hovered);
                column.children.push(child_widget);
            }
        }
        vec![Box::new(column)]
    }

    fn clear_rebuild(&mut self) {
        self.needs_child_rebuild = false;
    }

    fn set_classes(&mut self, classes: Vec<String>) {
        self.classes = classes;
        self.mark_dirty(DirtyFlags::RENDER);
    }

    fn get_classes(&self) -> &[String] { &self.classes }

    fn element_type_name(&self) -> &str { "ListView" }

    fn reset_mss_styles(&mut self) { self.mss.reset(); }
    fn mss(&self) -> Option<&crate::mss::MssFields> { Some(&self.mss) }
    fn apply_computed_style(&mut self, style: &ComputedStyle) {
        self.mss.apply(style);
        if let Some(w) = style.width() { self.width = Some(w); }
        if let Some(h) = style.height() { self.height = Some(h); }
        if let Some(ih) = style.get("item-height").and_then(|v| v.as_px()) {
            self.item_height = ih;
        }
        self.mark_dirty(DirtyFlags::LAYOUT | DirtyFlags::RENDER);
    }

    fn accessibility_info(&self) -> Option<crate::a11y::AccessibilityInfo> {
        Some(crate::a11y::AccessibilityInfo {
            role: crate::a11y::Role::ListBox,
            state: crate::a11y::NodeState::default(),
            properties: crate::a11y::NodeProperties {
                label: Some(format!("List with {} items", self.item_count())),
                ..Default::default()
            },
        })
    }
}

impl StyledElement for ListViewElement {
    fn apply_style(&mut self, style: &ComputedStyle) {
        if let Some(w) = style.width() { self.width = Some(w); }
        if let Some(h) = style.height() { self.height = Some(h); }
        self.mark_dirty(DirtyFlags::LAYOUT | DirtyFlags::RENDER);
    }

    fn classes(&self) -> &[String] { &self.classes }

    fn set_classes(&mut self, classes: Vec<String>) {
        self.classes = classes;
        self.mark_dirty(DirtyFlags::RENDER);
    }
}
