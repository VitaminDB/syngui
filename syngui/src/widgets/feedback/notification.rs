use crate::core::{Color, Point, Rect, RectExt, Size};
use crate::input::{CursorIcon, Event, EventResult, MouseButton};
use crate::layout::Constraints;
use crate::mss::{ComputedStyle, MssFields};
use crate::render::DisplayList;
use crate::widget::context::{EventContext, EventContextExt};
use crate::widget::{
    DirtyFlags, Element, ElementId, ElementTree, StyledElement, UpdateContext, Widget,
};
use std::any::Any;
use std::sync::Arc;
use std::time::Duration;

use crate::core::sync::Mutex;

#[derive(Clone, Copy, Debug, Default, PartialEq)]
pub enum NotificationSeverity {
    #[default]
    Info,
    Success,
    Warning,
    Error,
}

impl NotificationSeverity {
    pub fn css_class(&self) -> &'static str {
        match self {
            Self::Info => "severity-info",
            Self::Success => "severity-success",
            Self::Warning => "severity-warning",
            Self::Error => "severity-error",
        }
    }

    fn fallback_accent(&self) -> Color {
        match self {
            Self::Info => Color::from_hex("#3B82F6"),
            Self::Success => Color::from_hex("#22C55E"),
            Self::Warning => Color::from_hex("#F59E0B"),
            Self::Error => Color::from_hex("#EF4444"),
        }
    }

    fn icon(&self) -> &'static str {
        match self {
            Self::Info => "\u{E88E}",
            Self::Success => "\u{E5CA}",
            Self::Warning => "\u{E002}",
            Self::Error => "\u{E5CD}",
        }
    }
}

#[derive(Clone)]
pub struct NotificationItem {
    pub title: String,
    pub message: Option<String>,
    pub severity: NotificationSeverity,
    pub duration_ms: u32,
}

impl NotificationItem {
    pub fn info(title: impl Into<String>) -> Self {
        Self::with_severity(title, NotificationSeverity::Info)
    }
    pub fn success(title: impl Into<String>) -> Self {
        Self::with_severity(title, NotificationSeverity::Success)
    }
    pub fn warning(title: impl Into<String>) -> Self {
        Self::with_severity(title, NotificationSeverity::Warning)
    }
    pub fn error(title: impl Into<String>) -> Self {
        Self::with_severity(title, NotificationSeverity::Error)
    }

    fn with_severity(title: impl Into<String>, severity: NotificationSeverity) -> Self {
        Self {
            title: title.into(),
            message: None,
            severity,
            duration_ms: 0,
        }
    }

    pub fn message(mut self, msg: impl Into<String>) -> Self {
        self.message = Some(msg.into());
        self
    }

    pub fn duration_ms(mut self, ms: u32) -> Self {
        self.duration_ms = ms;
        self
    }
}

#[derive(Clone)]
pub struct NotificationCtx {
    items: Arc<Mutex<Vec<NotificationItem>>>,
    default_duration_ms: u32,
}

impl Default for NotificationCtx {
    fn default() -> Self {
        Self::new()
    }
}

impl NotificationCtx {
    pub fn new() -> Self {
        Self::with_default_duration(5000)
    }

    pub fn with_default_duration(default_duration_ms: u32) -> Self {
        Self {
            items: Arc::new(Mutex::new(Vec::new())),
            default_duration_ms,
        }
    }

    pub fn items_handle(&self) -> Arc<Mutex<Vec<NotificationItem>>> {
        self.items.clone()
    }

    pub fn default_duration_ms(&self) -> u32 {
        self.default_duration_ms
    }

    pub fn show(&self, mut item: NotificationItem) {
        if item.duration_ms == 0 {
            item.duration_ms = self.default_duration_ms;
        }
        if let Ok(mut q) = self.items.lock() {
            q.push(item);
        }
    }

    pub fn info(&self, title: impl Into<String>) {
        self.show(NotificationItem::info(title));
    }
    pub fn success(&self, title: impl Into<String>) {
        self.show(NotificationItem::success(title));
    }
    pub fn warning(&self, title: impl Into<String>) {
        self.show(NotificationItem::warning(title));
    }
    pub fn error(&self, title: impl Into<String>) {
        self.show(NotificationItem::error(title));
    }
}

pub struct NotificationHost {
    ctx: NotificationCtx,
    classes: Vec<String>,
    grow_up: bool,
}

impl NotificationHost {
    pub fn new(ctx: NotificationCtx) -> Self {
        Self { ctx, classes: Vec::new(), grow_up: false }
    }

    pub fn class(mut self, class: impl Into<String>) -> Self {
        self.classes.push(class.into());
        self
    }

    /// Стек растёт ВВЕРХ — для хоста, приклеенного к нижнему краю
    /// (`PortalAnchor::BottomEnd`): свежая карточка появляется снизу,
    /// прежние уезжают выше, а колода переполнения выглядывает над верхней
    /// карточкой, а не под нижней. Без этого при нижнем якоре колода лезла
    /// бы за край окна.
    pub fn grow_up(mut self, yes: bool) -> Self {
        self.grow_up = yes;
        self
    }
}

impl Widget for NotificationHost {
    fn create_element(&self) -> Box<dyn Element> {
        let mut elem = NotificationHostElement {
            id: ElementId::new(),
            items: self.ctx.items_handle(),
            active: Vec::new(),
            bounds: Rect::zero(),
            classes: self.classes.clone(),
            grow_up: self.grow_up,
            dirty_flags: DirtyFlags::LAYOUT | DirtyFlags::RENDER,
            mss: MssFields::new(),
            text_measure: None,
        };
        if !self.classes.is_empty() {
            elem.classes = self.classes.clone();
        }
        Box::new(elem)
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

const MAX_VISIBLE: usize = 3;
const DECK_DEPTH: usize = 2;
const CARD_OFFSET_PX: f32 = 6.0;
const CARD_SCALE_STEP: f32 = 0.04;
const CARD_OPACITY_STEP: f32 = 0.30;

const FADE_DURATION: f32 = 0.25;
const DEFAULT_MAX_WIDTH: f32 = 360.0;
const DEFAULT_PADDING: f32 = 14.0;
const DEFAULT_GAP: f32 = 12.0;
const DEFAULT_BORDER_RADIUS: f32 = 12.0;
const DEFAULT_FONT_SIZE: f32 = 14.0;
const ACCENT_BORDER_WIDTH: f32 = 3.0;
const ICON_SIZE: f32 = 18.0;
const CLOSE_ICON_SIZE: f32 = 14.0;

struct ActiveNotification {
    item: NotificationItem,
    elapsed: Duration,
    opacity: f32,
    dismissing: bool,
    close_hovered: bool,
    cached_height: f32,
}

pub struct NotificationHostElement {
    id: ElementId,
    items: Arc<Mutex<Vec<NotificationItem>>>,
    active: Vec<ActiveNotification>,
    bounds: Rect,
    classes: Vec<String>,
    /// См. [`NotificationHost::grow_up`].
    grow_up: bool,
    dirty_flags: DirtyFlags,
    mss: MssFields,
    text_measure: Option<Arc<dyn crate::widget::context::TextMeasure>>,
}

impl NotificationHostElement {
    fn effective_padding(&self) -> f32 {
        self.mss.padding_top.unwrap_or(DEFAULT_PADDING)
    }
    fn effective_gap(&self) -> f32 {
        self.mss.gap.unwrap_or(DEFAULT_GAP)
    }
    fn effective_max_width(&self, parent_w: f32) -> f32 {
        if let Some(d) = self.mss.max_width {
            let resolved = d.resolve(parent_w);
            if resolved > 0.0 {
                return resolved;
            }
        }
        DEFAULT_MAX_WIDTH
    }
    fn effective_font_size(&self) -> f32 {
        self.mss.font_size.unwrap_or(DEFAULT_FONT_SIZE)
    }
    fn effective_border_radius(&self) -> f32 {
        self.mss
            .border_radius_uniform(self.bounds.size.width.min(self.bounds.size.height), DEFAULT_BORDER_RADIUS)
    }

    fn font_family_str(&self) -> Option<&str> {
        self.mss.font_family.as_deref()
    }

    fn measure_width(&self, text: &str, font_size: f32, bold: bool) -> f32 {
        self.text_measure
            .as_ref()
            .map(|tm| {
                tm.measure_text_width_styled(
                    text,
                    font_size,
                    text.chars().count(),
                    bold,
                    self.font_family_str(),
                )
            })
            .unwrap_or_else(|| text.chars().count() as f32 * font_size * 0.6)
    }

    fn count_visual_lines(&self, text: &str, max_w: f32, font_size: f32, bold: bool) -> usize {
        if text.is_empty() {
            return 1;
        }
        let space_w = self.measure_width(" ", font_size, bold).max(font_size * 0.25);
        let mut total = 0usize;
        for paragraph in text.split('\n') {
            if paragraph.is_empty() {
                total += 1;
                continue;
            }
            let mut lines = 1usize;
            let mut x = 0.0f32;
            for word in paragraph.split(' ') {
                if word.is_empty() {
                    x += space_w;
                    continue;
                }
                let w = self.measure_width(word, font_size, bold);
                if x > 0.0 {
                    if x + space_w + w > max_w {
                        lines += 1;
                        x = w;
                    } else {
                        x += space_w + w;
                    }
                } else {
                    x = w;
                }
            }
            total += lines;
        }
        total.max(1)
    }

    /// Сколько карточек переполнения показывается «колодой» за видимыми.
    fn deck_layers(&self) -> usize {
        self.active.len().saturating_sub(MAX_VISIBLE).min(DECK_DEPTH)
    }

    /// Y первой видимой карточки. В режиме [`NotificationHost::grow_up`]
    /// сверху резервируется полоса под выглядывающую колоду — она входит в
    /// высоту хоста (см. `layout`), поэтому видимый стек сдвигается вниз.
    fn visible_origin_y(&self) -> f32 {
        if self.grow_up {
            self.bounds.origin.y + self.deck_layers() as f32 * CARD_OFFSET_PX
        } else {
            self.bounds.origin.y
        }
    }

    fn measure_item(&self, item: &NotificationItem, max_w: f32) -> (f32, f32) {
        let pad = self.effective_padding();
        let font_size = self.effective_font_size();
        let icon_slot = ICON_SIZE + 12.0;
        let close_slot = CLOSE_ICON_SIZE + 8.0;
        let title_avail = (max_w - pad * 2.0 - icon_slot - close_slot).max(40.0);

        let title_line_h = font_size * 1.4;
        let title_lines = self.count_visual_lines(&item.title, title_avail, font_size, true);
        let title_h = title_line_h * title_lines as f32;

        let (msg_h, msg_gap) = if let Some(ref m) = item.message {
            let msg_font = (font_size - 1.0).max(11.0);
            let line_h = msg_font * 1.4;
            let lines = self.count_visual_lines(m, title_avail, msg_font, false);
            (line_h * lines as f32, 4.0)
        } else {
            (0.0, 0.0)
        };

        let content_h = title_h + msg_gap + msg_h;
        let h = pad * 2.0 + content_h.max(ICON_SIZE);

        let title_w = self.measure_width(&item.title, font_size, true);
        let msg_w = item
            .message
            .as_ref()
            .map(|m| self.measure_width(m, (font_size - 1.0).max(11.0), false))
            .unwrap_or(0.0);
        let content_w = title_w.max(msg_w).min(title_avail);
        let w = (pad * 2.0 + icon_slot + close_slot + content_w).min(max_w);
        (w, h)
    }
}

impl Element for NotificationHostElement {
    fn update(&mut self, widget: &dyn Widget, _ctx: &mut UpdateContext) {
        if let Some(host) = widget.as_any().downcast_ref::<NotificationHost>() {
            self.items = host.ctx.items_handle();
            if self.grow_up != host.grow_up {
                self.grow_up = host.grow_up;
                self.mark_dirty(DirtyFlags::LAYOUT);
            }
            self.mark_dirty(DirtyFlags::RENDER);
        }
    }

    fn layout(&mut self, constraints: Constraints) -> Size {
        let parent_w = if constraints.containing_block.width > 0.0 {
            constraints.containing_block.width
        } else if constraints.max_width.is_finite() {
            constraints.max_width
        } else {
            800.0
        };
        let max_w = self.effective_max_width(parent_w);

        let measurements: Vec<(f32, f32)> = self
            .active
            .iter()
            .map(|n| self.measure_item(&n.item, max_w))
            .collect();
        let mut max_item_w: f32 = 0.0;
        for ((w, h), n) in measurements.iter().zip(self.active.iter_mut()) {
            n.cached_height = *h;
            if *w > max_item_w {
                max_item_w = *w;
            }
        }
        let host_w = max_item_w;

        let gap = self.effective_gap();
        let visible_count = self.active.len().min(MAX_VISIBLE);
        let mut host_h: f32 = 0.0;
        for i in 0..visible_count {
            host_h += self.active[i].cached_height;
            if i + 1 < visible_count {
                host_h += gap;
            }
        }
        host_h += self.deck_layers() as f32 * CARD_OFFSET_PX;

        self.bounds = Rect::new(self.bounds.origin, Size::new(host_w, host_h));
        Size::new(host_w, host_h)
    }

    fn build_display_list(&self, list: &mut DisplayList, _clip: Rect) {
        if self.active.is_empty() {
            return;
        }

        let pad = self.effective_padding();
        let gap = self.effective_gap();
        let radius = self.effective_border_radius();
        let font_size = self.effective_font_size();
        let font_family_owned = self.mss.font_family.clone();
        let title_color = self.mss.color.unwrap_or(Color::from_hex("#1F2937"));
        let msg_color = title_color.with_alpha(0.7);
        let close_color_idle = title_color.with_alpha(0.4);
        let close_color_hover = title_color;
        let bg_default = self.mss.background_color.unwrap_or(Color::from_hex("#FFFFFF"));
        let host_w = self.bounds.size.width;
        let origin_x = self.bounds.origin.x;
        let origin_y = self.bounds.origin.y;

        list.begin_overlay();

        let mut visible_y: Vec<f32> = Vec::with_capacity(self.active.len().min(MAX_VISIBLE));
        let mut cur_y = self.visible_origin_y();
        for i in 0..self.active.len().min(MAX_VISIBLE) {
            visible_y.push(cur_y);
            cur_y += self.active[i].cached_height + gap;
        }

        let deck_range_end = (MAX_VISIBLE + DECK_DEPTH).min(self.active.len());
        for idx in (MAX_VISIBLE..deck_range_end).rev() {
            let depth = (idx - MAX_VISIBLE + 1) as f32;
            let scale = (1.0 - CARD_SCALE_STEP * depth).max(0.5);
            let opacity = self.active[idx].opacity * (1.0 - CARD_OPACITY_STEP * depth).max(0.0);
            if opacity <= 0.01 {
                continue;
            }
            let y = if self.grow_up {
                // Колода выглядывает НАД верхней карточкой: её верхняя кромка
                // поднимается на depth * offset, остальное перекрыто видимой
                // карточкой (колода рисуется до неё).
                visible_y.first().copied().unwrap_or(origin_y) - depth * CARD_OFFSET_PX
            } else {
                let last_visible_y = visible_y.last().copied().unwrap_or(origin_y);
                let last_h = self
                    .active
                    .get(MAX_VISIBLE - 1)
                    .map(|n| n.cached_height)
                    .unwrap_or(0.0);
                last_visible_y + last_h - last_h * scale + depth * CARD_OFFSET_PX
            };
            let scaled_w = host_w * scale;
            let x = origin_x + (host_w - scaled_w) * 0.5;
            let scaled_h = self.active[idx].cached_height * scale;
            let card_rect = Rect::new(Point::new(x, y), Size::new(scaled_w, scaled_h));
            self.render_card(
                list,
                &self.active[idx],
                card_rect,
                opacity,
                radius,
                pad,
                font_size,
                font_family_owned.as_deref(),
                title_color,
                msg_color,
                close_color_idle,
                close_color_hover,
                bg_default,
                 false,
            );
        }

        for i in 0..self.active.len().min(MAX_VISIBLE) {
            let n = &self.active[i];
            if n.opacity <= 0.01 {
                continue;
            }
            let y = visible_y[i];
            let item_rect = Rect::new(Point::new(origin_x, y), Size::new(host_w, n.cached_height));
            self.render_card(
                list,
                n,
                item_rect,
                n.opacity,
                radius,
                pad,
                font_size,
                font_family_owned.as_deref(),
                title_color,
                msg_color,
                close_color_idle,
                close_color_hover,
                bg_default,
                 true,
            );
        }

        list.end_overlay();
    }

    fn animate(&mut self, dt: Duration) -> bool {
        let cap = MAX_VISIBLE + DECK_DEPTH + 4;
        let pulled: Vec<NotificationItem> = if let Ok(mut items) = self.items.lock() {
            let take_n = cap.saturating_sub(self.active.len()).min(items.len());
            items.drain(0..take_n).collect()
        } else {
            Vec::new()
        };
        if !pulled.is_empty() {
            for item in pulled {
                self.active.push(ActiveNotification {
                    item,
                    elapsed: Duration::ZERO,
                    opacity: 0.0,
                    dismissing: false,
                    close_hovered: false,
                    cached_height: 0.0,
                });
            }
            self.mark_dirty(DirtyFlags::LAYOUT | DirtyFlags::RENDER);
        }

        if self.active.is_empty() {
            return false;
        }

        let dt_secs = dt.as_secs_f32();
        let mut any_active = false;

        for notif in &mut self.active {
            notif.elapsed += dt;
            if notif.dismissing {
                notif.opacity = (notif.opacity - dt_secs / FADE_DURATION).max(0.0);
            } else {
                if notif.opacity < 1.0 {
                    notif.opacity = (notif.opacity + dt_secs / FADE_DURATION).min(1.0);
                }
                if notif.item.duration_ms > 0
                    && notif.elapsed >= Duration::from_millis(notif.item.duration_ms as u64)
                {
                    notif.dismissing = true;
                }
            }
            if notif.opacity > 0.01 || !notif.dismissing {
                any_active = true;
            }
        }

        let count_before = self.active.len();
        self.active.retain(|n| !(n.dismissing && n.opacity <= 0.01));
        if self.active.len() < count_before {
            self.mark_dirty(DirtyFlags::LAYOUT | DirtyFlags::RENDER);
        }

        any_active
    }

    fn handle_event(&mut self, event: &Event, ctx: &mut EventContext) -> EventResult {
        if self.active.is_empty() {
            return EventResult::Ignored;
        }
        let pad = self.effective_padding();
        let gap = self.effective_gap();
        let host_w = self.bounds.size.width;
        let origin_x = self.bounds.origin.x;
        let visible_count = self.active.len().min(MAX_VISIBLE);
        let mut rects: Vec<Rect> = Vec::with_capacity(visible_count);
        let mut cur_y = self.visible_origin_y();
        for i in 0..visible_count {
            rects.push(Rect::new(
                Point::new(origin_x, cur_y),
                Size::new(host_w, self.active[i].cached_height),
            ));
            cur_y += self.active[i].cached_height + gap;
        }

        match event {
            Event::MouseMove(pos) => {
                let mut hit = false;
                for (i, rect) in rects.iter().enumerate() {
                    let close_rect = self.close_rect_for(*rect, pad);
                    let hovering = close_rect.contains(*pos);
                    if hovering != self.active[i].close_hovered {
                        self.active[i].close_hovered = hovering;
                        if hovering {
                            ctx.set_cursor(CursorIcon::Pointer);
                        }
                        ctx.request_paint();
                    }
                    if rect.contains(*pos) {
                        hit = true;
                    }
                }
                if hit {
                    EventResult::Handled
                } else {
                    EventResult::Ignored
                }
            }
            Event::MouseDown { button, position } if *button == MouseButton::Left => {
                for (i, rect) in rects.iter().enumerate() {
                    if !rect.contains(*position) {
                        continue;
                    }
                    let close_rect = self.close_rect_for(*rect, pad);
                    if close_rect.contains(*position) {
                        self.active[i].dismissing = true;
                        ctx.request_paint();
                    }
                    return EventResult::Handled;
                }
                EventResult::Ignored
            }
            _ => EventResult::Ignored,
        }
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
        self.text_measure = tree.text_measure.clone();
    }

    fn set_classes(&mut self, classes: Vec<String>) {
        self.classes = classes;
        self.mark_dirty(DirtyFlags::RENDER);
    }
    fn get_classes(&self) -> &[String] {
        &self.classes
    }

    fn element_type_name(&self) -> &str {
        "Notification"
    }

    fn reset_mss_styles(&mut self) {
        self.mss.reset();
    }
    fn mss(&self) -> Option<&MssFields> {
        Some(&self.mss)
    }
    fn apply_computed_style(&mut self, style: &ComputedStyle) {
        self.mss.apply(style);
        self.mark_dirty(DirtyFlags::LAYOUT | DirtyFlags::RENDER);
    }

    fn wants_animate_tick(&self) -> bool {
        true
    }
}

impl NotificationHostElement {
    fn close_rect_for(&self, item_rect: Rect, pad: f32) -> Rect {
        Rect::new(
            Point::new(
                item_rect.x() + item_rect.size.width - pad - CLOSE_ICON_SIZE,
                item_rect.y() + pad,
            ),
            Size::new(CLOSE_ICON_SIZE, CLOSE_ICON_SIZE),
        )
    }

    #[allow(clippy::too_many_arguments)]
    fn render_card(
        &self,
        list: &mut DisplayList,
        n: &ActiveNotification,
        rect: Rect,
        opacity: f32,
        radius: f32,
        pad: f32,
        font_size: f32,
        font_family: Option<&str>,
        title_color: Color,
        msg_color: Color,
        close_color_idle: Color,
        close_color_hover: Color,
        bg_default: Color,
        draw_text: bool,
    ) {
        let accent = self
            .mss
            .accent_color
            .unwrap_or_else(|| n.item.severity.fallback_accent());

        list.push_shadow(
            rect,
            Color::BLACK.with_alpha(0.12 * opacity),
            12.0,
            (0.0, 4.0),
            [radius; 4],
        );
        list.push_rect_per_side_border(
            rect,
            bg_default.with_alpha(opacity),
            [radius; 4],
            None,
            crate::render::PerSideBorder {
                widths: [ACCENT_BORDER_WIDTH, 0.0, 0.0, 0.0],
                color: accent.with_alpha(opacity),
            },
        );

        if !draw_text {
            return;
        }

        let icon_rect = Rect::new(
            Point::new(rect.x() + pad, rect.y() + pad),
            Size::new(ICON_SIZE, ICON_SIZE),
        );
        list.push_text_centered(n.item.severity.icon(), icon_rect, accent.with_alpha(opacity), ICON_SIZE);

        let title_x = rect.x() + pad + ICON_SIZE + 12.0;
        let title_w = (rect.size.width - (title_x - rect.x()) - pad - CLOSE_ICON_SIZE - 8.0).max(40.0);
        let title_line_h = font_size * 1.4;
        let title_lines = self.count_visual_lines(&n.item.title, title_w, font_size, true);
        let title_rect = Rect::new(
            Point::new(title_x, rect.y() + pad),
            Size::new(title_w, title_line_h * title_lines as f32),
        );
        list.push_text_styled(
            &n.item.title,
            title_rect,
            title_color.with_alpha(opacity),
            font_size,
            crate::mss::TextAlign::DEFAULT,
            crate::mss::TextDecoration::None,
            self.mss.font_weight_or(600),
            font_family.map(|s| s.to_string()),
        );

        if let Some(ref msg) = n.item.message {
            let msg_font_size = (font_size - 1.0).max(11.0);
            let msg_line_h = msg_font_size * 1.4;
            let msg_lines = self.count_visual_lines(msg, title_w, msg_font_size, false);
            let msg_y = title_rect.y() + title_rect.size.height + 4.0;
            let msg_rect = Rect::new(
                Point::new(title_x, msg_y),
                Size::new(title_w, msg_line_h * msg_lines as f32),
            );
            list.push_text_styled(
                msg,
                msg_rect,
                msg_color.with_alpha(opacity),
                msg_font_size,
                crate::mss::TextAlign::DEFAULT,
                crate::mss::TextDecoration::None,
                self.mss.font_weight_or(400),
                font_family.map(|s| s.to_string()),
            );
        }

        let close_rect = self.close_rect_for(rect, pad);
        let close_color = if n.close_hovered {
            close_color_hover
        } else {
            close_color_idle
        };
        list.push_text_centered("\u{E5CD}", close_rect, close_color.with_alpha(opacity), CLOSE_ICON_SIZE);
    }
}

impl StyledElement for NotificationHostElement {
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
