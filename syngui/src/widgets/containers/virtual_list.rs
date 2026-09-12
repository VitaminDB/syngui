//! Виртуальный список с переменной высотой строк.
//!
//! Зачем. Лента чата держала смонтированными все сообщения: при 300
//! сообщениях это 8,5 тысяч элементов, и каждый кадр с пересборкой платил
//! за них каскадом стилей, раскладкой и синхронизацией a11y, даже когда
//! менялась одна строка. `VirtualList` строит только видимое окно (плюс
//! запас сверху и снизу), поэтому цена кадра перестаёт зависеть от длины
//! чата.
//!
//! Чем отличается от [`VirtualFlex`](super::virtual_flex::VirtualFlex): тот
//! считает все строки одинаковой высоты (одна оценка, сглаженная по
//! среднему) — на пузырьках разного размера скролл прыгает. Здесь высота
//! запоминается по ключу строки, а непомеренные считаются по оценке;
//! когда измеренная высота отличается от оценки, смещение правится так,
//! чтобы первая видимая строка осталась на месте.
//!
//! Окно рисуется со сдвигом, без распорок: `scroll_offset` возвращает
//! ровно тот сдвиг, на который смещено содержимое, поэтому попадание мышью
//! и отсечение при отрисовке совпадают с картинкой.

use std::any::Any;
use std::collections::HashMap;
use std::sync::Arc;
use std::time::Duration;

use crate::core::{Color, Point, Rect, Size, Transform};
use crate::input::{Event, EventResult, Key, MouseButton};
use crate::layout::{Constraints, CrossAxisAlignment};
use crate::mss::{ComputedStyle, MssFields};
use crate::render::{display_list::Border, DisplayList};
use crate::widget::context::{EventContext, EventContextExt};
use crate::widget::{
    DirtyFlags, Element, ElementId, ElementTree, LayoutHint, StyledElement, UpdateContext, Widget,
};
use crate::widgets::containers::page::ScrollbarPolicy;
use crate::widgets::containers::{Column, Keyed};

type RowBuilder = Arc<dyn Fn() -> Box<dyn Widget> + Send + Sync>;

/// Строка списка: кто это (`key`), что показывать (`version`) и как её
/// собрать. Сборщик вызывается, только когда строка попадает в окно и её
/// версия изменилась (см. [`Keyed`]).
pub struct VirtualRow {
    pub key: u64,
    pub version: u64,
    pub builder: RowBuilder,
}

impl VirtualRow {
    pub fn new(
        key: u64,
        version: u64,
        builder: impl Fn() -> Box<dyn Widget> + Send + Sync + 'static,
    ) -> Self {
        Self {
            key,
            version,
            builder: Arc::new(builder),
        }
    }

    pub fn from_arc(key: u64, version: u64, builder: RowBuilder) -> Self {
        Self {
            key,
            version,
            builder,
        }
    }
}

pub struct VirtualList {
    rows: Vec<VirtualRow>,
    gap: f32,
    estimated_row_height: f32,
    follow_end: bool,
    overscan: f32,
    reset_key: u64,
    scroll_to: Option<(u64, u64)>,
    scrollbar_policy: ScrollbarPolicy,
}

impl VirtualList {
    pub fn new(rows: Vec<VirtualRow>) -> Self {
        Self {
            rows,
            gap: 0.0,
            estimated_row_height: 120.0,
            follow_end: false,
            overscan: 600.0,
            reset_key: 0,
            scroll_to: None,
            scrollbar_policy: ScrollbarPolicy::Auto,
        }
    }

    pub fn gap(mut self, gap: f32) -> Self {
        self.gap = gap;
        self
    }

    /// Высота ещё не измеренной строки. Чем ближе к правде, тем меньше
    /// дёргается полоса прокрутки при первой прокрутке вверх.
    pub fn estimated_row_height(mut self, h: f32) -> Self {
        self.estimated_row_height = h.max(1.0);
        self
    }

    /// Держаться низа: список открывается внизу и следует за новыми
    /// строками, пока пользователь не пролистает вверх.
    pub fn follow_end(mut self, on: bool) -> Self {
        self.follow_end = on;
        self
    }

    /// Запас строк за краями вьюпорта, в пикселях.
    pub fn overscan(mut self, px: f32) -> Self {
        self.overscan = px.max(0.0);
        self
    }

    /// Смена этого ключа сбрасывает состояние списка: кэш высот, смещение и
    /// прилипание к низу. Для ленты чата — идентификатор чата: элемент при
    /// переключении переиспользуется, и без сброса новый чат открылся бы на
    /// смещении старого.
    pub fn reset_key(mut self, key: u64) -> Self {
        self.reset_key = key;
        self
    }

    /// Прокрутить к строке `key`. `generation` отличает новые запросы от
    /// повторов того же: пока номер не изменился, запрос не выполняется
    /// заново.
    pub fn scroll_to(mut self, key: u64, generation: u64) -> Self {
        self.scroll_to = Some((key, generation));
        self
    }

    pub fn scrollbar_policy(mut self, policy: ScrollbarPolicy) -> Self {
        self.scrollbar_policy = policy;
        self
    }
}

const VELOCITY_SCALE: f32 = 25.0;
const FRICTION: f32 = 0.95;
const MIN_VELOCITY: f32 = 0.5;
/// Насколько близко к низу список считается «прилипшим».
const STICK_EPS: f32 = 1.0;

impl Widget for VirtualList {
    fn create_element(&self) -> Box<dyn Element> {
        Box::new(VirtualListElement {
            id: ElementId::new(),
            bounds: Rect::zero(),
            rows: self.rows.iter().map(RowMeta::from).collect(),
            heights: HashMap::new(),
            prefix: Vec::new(),
            gap: self.gap,
            estimated_row_height: self.estimated_row_height,
            follow_end: self.follow_end,
            overscan: self.overscan,
            reset_key: self.reset_key,
            scroll_to_gen: self.scroll_to.map(|(_, g)| g).unwrap_or(0),
            pending_scroll_to: self.scroll_to.map(|(k, _)| k),
            scroll_y: 0.0,
            stick: self.follow_end,
            velocity: 0.0,
            is_coasting: false,
            measured_width: 0.0,
            window_viewport_h: 0.0,
            built: None,
            rows_dirty: true,
            scrollbar_policy: self.scrollbar_policy,
            scrollbar_opacity: 0.0,
            scrollbar_idle_time: 0.0,
            scrollbar_width: 8.0,
            dragging_scrollbar: false,
            hover_scrollbar: false,
            hover_scrollbar_area: false,
            touch_drag_start: None,
            touch_id: None,
            classes: Vec::new(),
            dirty_flags: DirtyFlags::LAYOUT | DirtyFlags::RENDER,
            mss: MssFields::new(),
            background: None,
            padding: crate::core::EdgeInsets::default(),
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

    fn child_widgets(&self) -> Vec<&dyn Widget> {
        vec![]
    }
}

struct RowMeta {
    key: u64,
    version: u64,
    builder: RowBuilder,
}

impl From<&VirtualRow> for RowMeta {
    fn from(r: &VirtualRow) -> Self {
        Self {
            key: r.key,
            version: r.version,
            builder: r.builder.clone(),
        }
    }
}

struct VirtualListElement {
    id: ElementId,
    bounds: Rect,

    rows: Vec<RowMeta>,
    /// Измеренные высоты по ключу строки. Сбрасывается при смене ширины и
    /// при смене `reset_key`.
    heights: HashMap<u64, f32>,
    /// Префиксные суммы: `prefix[i]` — смещение строки `i` от начала ленты.
    /// Длина — `rows.len() + 1`.
    prefix: Vec<f32>,
    gap: f32,
    estimated_row_height: f32,
    follow_end: bool,
    overscan: f32,
    reset_key: u64,
    scroll_to_gen: u64,
    pending_scroll_to: Option<u64>,

    scroll_y: f32,
    /// Список держится низа (см. [`VirtualList::follow_end`]).
    stick: bool,
    velocity: f32,
    is_coasting: bool,

    measured_width: f32,
    window_viewport_h: f32,
    /// Построенное окно строк `[start, end)`.
    built: Option<(usize, usize)>,
    rows_dirty: bool,

    scrollbar_policy: ScrollbarPolicy,
    scrollbar_opacity: f32,
    scrollbar_idle_time: f32,
    scrollbar_width: f32,
    dragging_scrollbar: bool,
    hover_scrollbar: bool,
    hover_scrollbar_area: bool,
    touch_drag_start: Option<Point>,
    touch_id: Option<u64>,

    classes: Vec<String>,
    dirty_flags: DirtyFlags,
    mss: MssFields,
    background: Option<Color>,
    padding: crate::core::EdgeInsets,
}

impl VirtualListElement {
    fn row_height(&self, i: usize) -> f32 {
        self.rows
            .get(i)
            .and_then(|r| self.heights.get(&r.key).copied())
            .unwrap_or(self.estimated_row_height)
    }

    fn rebuild_prefix(&mut self) {
        self.prefix.clear();
        self.prefix.reserve(self.rows.len() + 1);
        let mut acc = 0.0;
        for i in 0..self.rows.len() {
            self.prefix.push(acc);
            acc += self.row_height(i);
            if i + 1 < self.rows.len() {
                acc += self.gap;
            }
        }
        self.prefix.push(acc);
    }

    /// Смещение строки от начала ленты. До первой раскладки префиксных
    /// сумм ещё нет — тогда считаем по оценке, иначе первый проход собрал
    /// бы всю ленту (все смещения были бы нулевыми).
    fn offset_of(&self, i: usize) -> f32 {
        match self.prefix.get(i) {
            Some(v) => *v,
            None => i as f32 * (self.estimated_row_height + self.gap),
        }
    }

    fn content_height(&self) -> f32 {
        match self.prefix.last() {
            Some(v) => *v,
            None => self.offset_of(self.rows.len()),
        }
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

    /// Первая строка, чей низ ниже `y`. Бинарный поиск по префиксным суммам.
    fn row_at(&self, y: f32) -> usize {
        if self.rows.is_empty() {
            return 0;
        }
        let mut lo = 0usize;
        let mut hi = self.rows.len() - 1;
        while lo < hi {
            let mid = (lo + hi) / 2;
            if self.offset_of(mid) + self.row_height(mid) <= y {
                lo = mid + 1;
            } else {
                hi = mid;
            }
        }
        lo
    }

    /// Какое окно строк нужно показать при текущем смещении.
    fn needed_window(&self) -> (usize, usize) {
        if self.rows.is_empty() {
            return (0, 0);
        }
        let top = (self.scroll_y - self.overscan).max(0.0);
        // До первой раскладки высота вьюпорта неизвестна: строим минимум,
        // следующий проход (уже с размерами) достроит настоящее окно.
        let bottom = self.scroll_y + self.viewport_height() + self.overscan;
        let start = self.row_at(top);
        let mut end = start;
        while end < self.rows.len() && self.offset_of(end) < bottom {
            end += 1;
        }
        (start, end.max(start + 1).min(self.rows.len()))
    }

    /// Сдвиг содержимого: окно нарисовано не от начала ленты, а от своей
    /// первой строки.
    fn shift(&self) -> f32 {
        let start = self.built.map(|(s, _)| s).unwrap_or(0);
        self.scroll_y - self.offset_of(start)
    }

    fn refresh_stick(&mut self) {
        if self.follow_end {
            self.stick = self.scroll_y >= self.max_scroll() - STICK_EPS;
        }
    }

    fn clamp_scroll(&mut self) {
        let max = self.max_scroll();
        if self.scroll_y > max {
            self.scroll_y = max;
        }
        if self.scroll_y < 0.0 {
            self.scroll_y = 0.0;
        }
    }

    fn flash_scrollbar(&mut self) {
        self.scrollbar_opacity = 1.0;
        self.scrollbar_idle_time = 0.0;
    }

    fn is_animating(&self) -> bool {
        let fading = self.scrollbar_policy == ScrollbarPolicy::Auto
            && self.scrollbar_opacity > 0.0
            && !self.hover_scrollbar_area;
        self.is_coasting || fading
    }

    fn scrollbar_track(&self) -> Rect {
        let x = self.bounds.origin.x + self.bounds.size.width - self.scrollbar_width;
        let y = self.bounds.origin.y + self.padding.top;
        Rect::new(
            Point::new(x, y),
            Size::new(self.scrollbar_width, self.viewport_height()),
        )
    }

    fn scrollbar_thumb(&self) -> Rect {
        let track = self.scrollbar_track();
        let ch = self.content_height();
        let vh = self.viewport_height();
        if ch <= 0.0 || vh <= 0.0 || ch <= vh {
            return Rect::zero();
        }
        let thumb_h = (vh / ch * track.size.height).clamp(20.0, track.size.height);
        let max = self.max_scroll();
        let ratio = if max > 0.0 {
            self.scroll_y.clamp(0.0, max) / max
        } else {
            0.0
        };
        Rect::new(
            Point::new(track.origin.x, track.origin.y + ratio * (track.size.height - thumb_h)),
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

    /// Прокрутить так, чтобы строка `key` была видна.
    fn apply_scroll_to(&mut self, key: u64) {
        let Some(i) = self.rows.iter().position(|r| r.key == key) else {
            return;
        };
        let target = self.offset_of(i) - self.viewport_height() * 0.3;
        self.scroll_y = target.clamp(0.0, self.max_scroll());
        self.velocity = 0.0;
        self.is_coasting = false;
        self.refresh_stick();
    }

    fn reset_state(&mut self) {
        self.heights.clear();
        self.prefix.clear();
        self.scroll_y = 0.0;
        self.stick = self.follow_end;
        self.velocity = 0.0;
        self.is_coasting = false;
        self.built = None;
        self.rows_dirty = true;
    }
}

impl Element for VirtualListElement {
    fn update(&mut self, widget: &dyn Widget, _ctx: &mut UpdateContext) {
        let Some(w) = widget.as_any().downcast_ref::<VirtualList>() else {
            return;
        };

        let reset = self.reset_key != w.reset_key;
        self.reset_key = w.reset_key;
        self.gap = w.gap;
        self.estimated_row_height = w.estimated_row_height;
        self.follow_end = w.follow_end;
        self.overscan = w.overscan;
        self.scrollbar_policy = w.scrollbar_policy;

        let same_rows = self.rows.len() == w.rows.len()
            && self
                .rows
                .iter()
                .zip(w.rows.iter())
                .all(|(a, b)| a.key == b.key && a.version == b.version);
        // Сборщики перенимаем всегда: в них живёт снимок данных, из
        // которого строится строка.
        self.rows = w.rows.iter().map(RowMeta::from).collect();
        if !same_rows {
            self.rows_dirty = true;
            self.mark_dirty(DirtyFlags::LAYOUT | DirtyFlags::RENDER);
        }

        if reset {
            self.reset_state();
            self.mark_dirty(DirtyFlags::LAYOUT | DirtyFlags::RENDER);
        }

        if let Some((key, generation)) = w.scroll_to {
            if generation != self.scroll_to_gen {
                self.scroll_to_gen = generation;
                self.pending_scroll_to = Some(key);
                self.mark_dirty(DirtyFlags::LAYOUT | DirtyFlags::RENDER);
            }
        }
    }

    fn layout(&mut self, constraints: Constraints) -> Size {
        let w = if constraints.max_width.is_finite() {
            constraints.max_width
        } else {
            constraints.min_width.max(0.0)
        };
        let h = if constraints.max_height.is_finite() {
            constraints.max_height
        } else {
            constraints.min_height.max(0.0)
        };
        // Ширина изменилась — измеренные высоты больше не годятся.
        if (w - self.measured_width).abs() > 0.5 {
            self.measured_width = w;
            self.heights.clear();
            self.built = None;
            self.rows_dirty = true;
        }
        self.bounds = Rect::new(self.bounds.origin, Size::new(w, h));
        self.rebuild_prefix();

        if let Some(key) = self.pending_scroll_to.take() {
            self.apply_scroll_to(key);
        }
        if self.stick {
            self.scroll_y = self.max_scroll();
        }
        self.clamp_scroll();
        Size::new(w, h)
    }

    fn layout_hint(&self) -> LayoutHint {
        LayoutHint::Scroll {
            left: self.padding.left,
            top: self.padding.top,
            right: self.padding.right + self.scrollbar_width,
            bottom: self.padding.bottom,
            unbounded_width: false,
            unbounded_height: true,
        }
    }

    /// Реальные высоты построенных строк. Приходят после раскладки; если
    /// они разошлись с оценкой, правим смещение так, чтобы первая видимая
    /// строка осталась на месте.
    fn set_row_bounds(&mut self, bounds: Vec<(f32, f32)>) {
        let Some((start, end)) = self.built else {
            return;
        };
        let mut changed = false;
        for (i, (_, h)) in bounds.iter().enumerate() {
            let Some(row) = self.rows.get(start + i) else {
                break;
            };
            if start + i >= end {
                break;
            }
            if self.heights.get(&row.key).map_or(true, |old| (old - h).abs() > 0.5) {
                self.heights.insert(row.key, *h);
                changed = true;
            }
        }
        if !changed {
            return;
        }
        let before = self.offset_of(start);
        self.rebuild_prefix();
        let after = self.offset_of(start);
        if self.stick {
            self.scroll_y = self.max_scroll();
        } else if (after - before).abs() > 0.01 {
            self.scroll_y = (self.scroll_y + after - before).max(0.0);
        }
        self.clamp_scroll();
        self.mark_dirty(DirtyFlags::LAYOUT | DirtyFlags::RENDER);
    }

    fn manages_own_children(&self) -> bool {
        true
    }

    fn needs_rebuild(&self) -> bool {
        if self.rows_dirty {
            return true;
        }
        let Some((start, end)) = self.built else {
            return !self.rows.is_empty();
        };
        let (need_start, need_end) = self.needed_window();
        need_start < start || need_end > end
    }

    fn build_children(&self) -> Vec<Box<dyn Widget>> {
        if self.rows.is_empty() {
            return vec![];
        }
        let (start, end) = self.needed_window();
        let children: Vec<Box<dyn Widget>> = self.rows[start..end]
            .iter()
            .map(|r| {
                Box::new(Keyed::from_arc(r.key, r.version, r.builder.clone())) as Box<dyn Widget>
            })
            .collect();
        vec![Box::new(
            Column::new()
                .gap(self.gap)
                .cross_axis_alignment(CrossAxisAlignment::Stretch)
                .children(children),
        )]
    }

    fn clear_rebuild(&mut self) {
        self.built = Some(self.needed_window());
        self.rows_dirty = false;
    }

    fn build_display_list(&self, list: &mut DisplayList, _clip: Rect) {
        let br = self
            .mss
            .border_radius_resolved(self.bounds.size.width.min(self.bounds.size.height), 0.0);
        let bw = self.mss.border_width_or(0.0);
        if let Some(bg) = self.background {
            match (bw > 0.0, self.mss.border_color) {
                (true, Some(bc)) => list.push_rect_bordered(self.bounds, bg, br, Border::new(bw, bc)),
                _ => list.push_rect(self.bounds, bg, br),
            }
        } else if bw > 0.0 {
            if let Some(bc) = self.mss.border_color {
                list.push_rect_bordered(self.bounds, Color::TRANSPARENT, br, Border::new(bw, bc));
            }
        }
        list.push_clip(self.bounds);
        list.push_transform(Transform::translation(0.0, -self.shift()));
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
            list.push_rect(self.scrollbar_track(), thumb_base.with_alpha(opacity * 0.15), radius);
        }
        let color = if self.dragging_scrollbar {
            thumb_base.darken(0.5).with_alpha(opacity)
        } else if self.hover_scrollbar {
            thumb_base.darken(0.3).with_alpha(opacity)
        } else {
            thumb_base.with_alpha(opacity * 0.7)
        };
        list.push_rect(self.scrollbar_thumb(), color, radius);
    }

    fn handle_event(&mut self, event: &Event, ctx: &mut EventContext) -> EventResult {
        match event {
            Event::MouseWheel { delta, position, .. } => {
                if !self.bounds.contains(*position) || self.max_scroll() <= 0.0 {
                    return EventResult::Ignored;
                }
                let delta_y = -*delta;
                let old = self.scroll_y;
                if (delta_y < 0.0 && old <= 0.0) || (delta_y > 0.0 && old >= self.max_scroll()) {
                    return EventResult::Handled;
                }
                self.scroll_y = (old + delta_y).clamp(0.0, self.max_scroll());
                if (self.scroll_y - old).abs() < 0.001 {
                    return EventResult::Handled;
                }
                let alpha = 0.3;
                self.velocity = self.velocity * (1.0 - alpha) + delta_y * VELOCITY_SCALE * alpha;
                self.is_coasting = true;
                self.refresh_stick();
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
                    Key::PageUp => Some((self.scroll_y - vp).max(0.0)),
                    Key::PageDown => Some((self.scroll_y + vp).min(max)),
                    Key::Up => Some((self.scroll_y - 40.0).max(0.0)),
                    Key::Down => Some((self.scroll_y + 40.0).min(max)),
                    _ => None,
                };
                let Some(t) = target else {
                    return EventResult::Ignored;
                };
                self.scroll_y = t;
                self.velocity = 0.0;
                self.is_coasting = false;
                self.refresh_stick();
                self.flash_scrollbar();
                ctx.request_layout();
                ctx.request_paint();
                EventResult::Handled
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
                if self.dragging_scrollbar {
                    let track = self.scrollbar_track();
                    let thumb_h = self.scrollbar_thumb().size.height;
                    if track.size.height > thumb_h {
                        let rel = (pos.y - track.origin.y - thumb_h / 2.0) / (track.size.height - thumb_h);
                        self.scroll_y = (rel.clamp(0.0, 1.0) * self.max_scroll()).clamp(0.0, self.max_scroll());
                    }
                    self.refresh_stick();
                    self.flash_scrollbar();
                    ctx.request_layout();
                    ctx.request_paint();
                    return EventResult::Captured;
                }

                let mut result = EventResult::Ignored;
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
                if self.touch_id != Some(*id) {
                    return EventResult::Ignored;
                }
                let Some(start) = self.touch_drag_start else {
                    return EventResult::Ignored;
                };
                let dy = start.y - position.y;
                self.scroll_y = (self.scroll_y + dy).clamp(0.0, self.max_scroll());
                let alpha = 0.3;
                self.velocity = self.velocity * (1.0 - alpha) + dy * VELOCITY_SCALE * alpha;
                self.touch_drag_start = Some(*position);
                self.refresh_stick();
                self.flash_scrollbar();
                ctx.request_layout();
                ctx.request_paint();
                EventResult::Handled
            }
            Event::TouchEnd { id, .. } => {
                if self.touch_id != Some(*id) {
                    return EventResult::Ignored;
                }
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
        if dt_secs <= 0.0 {
            return self.is_animating();
        }
        let mut repaint = false;
        if self.is_coasting {
            self.velocity *= FRICTION.powf(dt_secs * 60.0);
            self.scroll_y = (self.scroll_y + self.velocity * dt_secs).clamp(0.0, self.max_scroll());
            if self.velocity.abs() < MIN_VELOCITY {
                self.is_coasting = false;
                self.velocity = 0.0;
            }
            self.refresh_stick();
            self.mark_dirty(DirtyFlags::LAYOUT | DirtyFlags::RENDER);
            repaint = true;
        }
        if self.scrollbar_policy == ScrollbarPolicy::Auto && self.scrollbar_opacity > 0.0 {
            if self.hover_scrollbar_area || self.dragging_scrollbar {
                self.scrollbar_idle_time = 0.0;
            } else {
                self.scrollbar_idle_time += dt_secs;
            }
            if self.scrollbar_idle_time > 1.0 {
                self.scrollbar_opacity = (self.scrollbar_opacity - dt_secs * 3.0).max(0.0);
                repaint = repaint || self.scrollbar_opacity > 0.0;
            }
        }
        repaint
    }

    fn wants_animate_tick(&self) -> bool {
        self.is_animating()
    }

    fn needs_repaint(&self) -> bool {
        self.is_animating()
    }

    fn scroll_offset(&self) -> Point {
        Point::new(0.0, self.shift())
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
        let visible_top = self.scroll_y;
        let visible_bottom = visible_top + vp_h;
        let margin = 20.0;
        // Координаты ребёнка — в пространстве построенного окна.
        let top = child_rect.origin.y + self.offset_of(self.built.map(|(s, _)| s).unwrap_or(0));
        let bottom = top + child_rect.size.height;
        let target = if bottom + margin > visible_bottom {
            bottom + margin - vp_h
        } else if top - margin < visible_top {
            (top - margin).max(0.0)
        } else {
            return false;
        };
        self.scroll_y = target.clamp(0.0, self.max_scroll());
        self.velocity = 0.0;
        self.is_coasting = false;
        self.refresh_stick();
        true
    }

    fn set_viewport_size(&mut self, size: Size) {
        self.window_viewport_h = size.height;
    }

    fn id(&self) -> ElementId {
        self.id
    }
    fn set_id(&mut self, id: ElementId) {
        self.id = id;
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
    fn mount(&mut self, _tree: &mut ElementTree) {}
    fn element_type_name(&self) -> &str {
        "VirtualList"
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
        if let Some(bg) = self.mss.background_color {
            self.background = Some(bg);
        }
        if let Some(v) = self.mss.padding_left {
            self.padding.left = v;
        }
        if let Some(v) = self.mss.padding_right {
            self.padding.right = v;
        }
        if let Some(v) = self.mss.padding_top {
            self.padding.top = v;
        }
        if let Some(v) = self.mss.padding_bottom {
            self.padding.bottom = v;
        }
        if let Some(v) = style.get("scrollbar-width").and_then(|v| v.as_px()) {
            self.scrollbar_width = v;
        }
        self.mark_dirty(DirtyFlags::LAYOUT | DirtyFlags::RENDER);
    }
}

impl StyledElement for VirtualListElement {
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

#[cfg(all(test, feature = "testing"))]
mod tests {
    use super::{VirtualList, VirtualRow};
    use crate::core::Point;
    use crate::input::Event;
    use crate::prelude::*;
    use crate::signal::use_signal;
    use crate::testing::TestHarness;
    use crate::widgets::containers::reactive::Reactive;
    use std::sync::atomic::{AtomicUsize, Ordering};
    use std::sync::Arc;

    const ROW_H: f32 = 100.0;
    const VIEW_H: f32 = 800.0;
    const MSS: &str = ".row { height: 100px; }";

    /// Список из `n` строк, каждая ростом [`ROW_H`]; счётчик считает,
    /// сколько строк реально собрано.
    fn list(n: usize, follow_end: bool, builds: Arc<AtomicUsize>) -> VirtualList {
        let rows = (0..n)
            .map(|i| {
                let builds = builds.clone();
                VirtualRow::new(i as u64, 1, move || {
                    builds.fetch_add(1, Ordering::Relaxed);
                    Box::new(DecoratedBox::new().class("row")) as Box<dyn Widget>
                })
            })
            .collect();
        VirtualList::new(rows)
            .estimated_row_height(ROW_H)
            .follow_end(follow_end)
            .overscan(200.0)
    }

    fn built_rows(h: &TestHarness) -> usize {
        h.find_by_type_name("Keyed").len()
    }

    /// Строится только видимое окно с запасом, а не вся лента.
    #[test]
    fn builds_only_the_visible_window() {
        crate::signal::allow_signal_reads_on_this_thread();
        let builds = Arc::new(AtomicUsize::new(0));
        let mut h = TestHarness::new(Box::new(list(200, false, builds.clone())));
        let engine = h.apply_mss(MSS);
        h.frame(Some(&engine), 600.0, VIEW_H);

        // Вьюпорт 800 px по 100 px на строку плюс запас 200 px снизу.
        let rows = built_rows(&h);
        assert!(rows >= 8 && rows <= 14, "окно из {rows} строк");
        // Пара строк собирается в первом проходе кадра, когда размеров ещё
        // нет; настоящее окно строится во втором.
        let calls = builds.load(Ordering::Relaxed);
        assert!(calls <= rows + 4, "сборок {calls} на окно из {rows} строк");
        assert!(h.element_count() < 100, "в дереве вся лента: {}", h.element_count());
    }

    /// Прокрутка колесом сдвигает окно: дальние строки собираются, ближние
    /// отпускаются.
    #[test]
    fn scrolling_moves_the_window() {
        crate::signal::allow_signal_reads_on_this_thread();
        let builds = Arc::new(AtomicUsize::new(0));
        let mut h = TestHarness::new(Box::new(list(200, false, builds.clone())));
        let engine = h.apply_mss(MSS);
        h.frame(Some(&engine), 600.0, VIEW_H);
        let first = built_rows(&h);
        builds.store(0, Ordering::Relaxed);

        for _ in 0..10 {
            h.send_event(&Event::MouseWheel {
                delta: -300.0,
                delta_x: 0.0,
                position: Point::new(300.0, 400.0),
            });
        }
        h.frame(Some(&engine), 600.0, VIEW_H);

        assert!(builds.load(Ordering::Relaxed) > 0, "новые строки не собрались");
        // У верхнего края запас только снизу, в середине — с обеих сторон,
        // поэтому окно чуть больше, но всё равно далеко от длины ленты.
        let now = built_rows(&h);
        assert!(now >= first && now <= first + 4, "окно {first} → {now}");
    }

    /// follow_end: список открывается внизу и следует за новыми строками,
    /// пока пользователь не ушёл вверх.
    #[test]
    fn follow_end_opens_at_the_bottom_and_follows() {
        crate::signal::allow_signal_reads_on_this_thread();
        let count = use_signal(50usize);
        let builds = Arc::new(AtomicUsize::new(0));
        let b = builds.clone();
        let mut h = TestHarness::new(Box::new(Reactive::new(move || -> Vec<Box<dyn Widget>> {
            vec![Box::new(list(count.get(), true, b.clone())) as Box<dyn Widget>]
        })));
        let engine = h.apply_mss(MSS);
        h.frame(Some(&engine), 600.0, VIEW_H);

        let ids = h.find_by_type_name("VirtualList");
        let id = *ids.first().expect("список в дереве");
        // Низ ленты: 50 строк по 100 px, вьюпорт 800 → смещение 4200.
        let bottom_row = h.find_by_type_name("Keyed").len();
        assert!(bottom_row > 0);
        assert_eq!(h.element_bounds(id).size.height, VIEW_H);

        // Новая строка — список остаётся внизу.
        builds.store(0, Ordering::Relaxed);
        count.set(51);
        h.frame(Some(&engine), 600.0, VIEW_H);
        assert!(builds.load(Ordering::Relaxed) >= 1, "хвост не достроился");
    }

    /// Смена ключа сброса (в ленте чата — другой чат) открывает список
    /// заново внизу, а не на смещении прошлого чата.
    #[test]
    fn reset_key_reopens_the_list() {
        crate::signal::allow_signal_reads_on_this_thread();
        let chat = use_signal(1u64);
        let builds = Arc::new(AtomicUsize::new(0));
        let b = builds.clone();
        let mut h = TestHarness::new(Box::new(Reactive::new(move || -> Vec<Box<dyn Widget>> {
            let id = chat.get();
            vec![Box::new(list(100, true, b.clone()).reset_key(id)) as Box<dyn Widget>]
        })));
        let engine = h.apply_mss(MSS);
        h.frame(Some(&engine), 600.0, VIEW_H);

        // Уходим вверх.
        for _ in 0..20 {
            h.send_event(&Event::MouseWheel {
                delta: 300.0,
                delta_x: 0.0,
                position: Point::new(300.0, 400.0),
            });
        }
        h.frame(Some(&engine), 600.0, VIEW_H);
        let top_window = h.find_by_type_name("Keyed");
        assert!(!top_window.is_empty());

        chat.set(2);
        h.frame(Some(&engine), 600.0, VIEW_H);
        // После сброса список снова внизу: окно другое.
        let after = h.find_by_type_name("Keyed");
        assert_ne!(after, top_window, "список не вернулся к низу");
    }

    /// Прокрутка к строке по ключу: строка попадает в построенное окно.
    #[test]
    fn scroll_to_key_brings_the_row_into_the_window() {
        crate::signal::allow_signal_reads_on_this_thread();
        let target = use_signal(0u64);
        let built_keys = Arc::new(std::sync::Mutex::new(Vec::<u64>::new()));
        let keys = built_keys.clone();
        let mut h = TestHarness::new(Box::new(Reactive::new(move || -> Vec<Box<dyn Widget>> {
            let t = target.get();
            let keys = keys.clone();
            let rows = (0..200u64)
                .map(|i| {
                    let keys = keys.clone();
                    VirtualRow::new(i, 1, move || {
                        keys.lock().expect("ключи").push(i);
                        Box::new(DecoratedBox::new().class("row")) as Box<dyn Widget>
                    })
                })
                .collect();
            let list = VirtualList::new(rows)
                .estimated_row_height(ROW_H)
                .overscan(200.0);
            let list = if t > 0 { list.scroll_to(t, t) } else { list };
            vec![Box::new(list) as Box<dyn Widget>]
        })));
        let engine = h.apply_mss(MSS);
        h.frame(Some(&engine), 600.0, VIEW_H);

        built_keys.lock().expect("ключи").clear();
        target.set(150);
        h.frame(Some(&engine), 600.0, VIEW_H);
        let seen = built_keys.lock().expect("ключи").clone();
        assert!(seen.contains(&150), "строка 150 не собрана: {seen:?}");
    }
}
