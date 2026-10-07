use super::IntoWidget;
use crate::core::sync::Mutex;
use crate::core::{Color, Point, Rect, RectExt, Size, Transform};
use crate::input::{CursorIcon, Event, EventResult, MouseButton};
use crate::layout::Constraints;
use crate::mss::ComputedStyle;
use crate::mss::MssFields;
use crate::render::DisplayList;
use crate::widget::context::{EventContext, EventContextExt};
use crate::widget::{
    DirtyFlags, Element, ElementId, ElementTree, LayoutHint, StyledElement, UpdateContext, Widget,
};
use std::any::Any;
use std::sync::Arc;
use std::time::Duration;

pub struct Carousel {
    /// Длительность перелистывания, мс (0 — мгновенно).
    slide_ms: u32,
    children: Vec<Box<dyn Widget>>,
    current_page: usize,
    auto_play: bool,
    auto_play_interval_ms: u32,
    show_indicators: bool,
    on_page_change: Option<Arc<Mutex<dyn FnMut(usize) + Send>>>,
    on_overscroll: Option<Arc<Mutex<dyn FnMut(i32) + Send>>>,
    page_signal: Option<crate::signal::RwSignal<usize>>,
    position_signal: Option<crate::signal::RwSignal<f32>>,
    show_arrows: bool,
}

impl Carousel {
    pub fn new() -> Self {
        Self {
            slide_ms: 350,
            children: Vec::new(),
            current_page: 0,
            auto_play: false,
            auto_play_interval_ms: 5000,
            show_indicators: true,
            on_page_change: None,
            on_overscroll: None,
            page_signal: None,
            position_signal: None,
            show_arrows: true,
        }
    }

    /// Положение ленты в страницах, дробное (1.5 — посередине между второй
    /// и третьей): и пока тянут пальцем, и во время доводки. Для параллакса —
    /// сдвига фона вслед за страницами.
    pub fn position_signal(mut self, sig: crate::signal::RwSignal<f32>) -> Self {
        self.position_signal = Some(sig);
        self
    }

    /// Текущая страница в сигнале: запись в него листает (при пересборке —
    /// оберните в `Reactive`), пролистывание пальцем пишет в него.
    pub fn page_signal(mut self, sig: crate::signal::RwSignal<usize>) -> Self {
        self.current_page = sig.get_untracked();
        self.page_signal = Some(sig);
        self
    }

    /// Стрелки по бокам (на сенсорных экранах обычно не нужны).
    /// Длительность перелистывания страницы, мс; 0 — без анимации (системная настройка «домашний экран» и т. п.).
    pub fn slide_duration_ms(mut self, ms: u32) -> Self {
        self.slide_ms = ms;
        self
    }

    pub fn show_arrows(mut self, show: bool) -> Self {
        self.show_arrows = show;
        self
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

    /// Пролистали пальцем за край: `1` — дальше последней страницы, `-1` —
    /// назад с первой (например, переход на соседний рабочий стол). С ним
    /// карусель ловит горизонтальный свайп и при одной странице.
    pub fn on_overscroll(mut self, f: impl FnMut(i32) + Send + 'static) -> Self {
        self.on_overscroll = Some(Arc::new(Mutex::new(f)));
        self
    }
}

impl Default for Carousel {
    fn default() -> Self {
        Self::new()
    }
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
            on_overscroll: self.on_overscroll.clone(),
            page_signal: self.page_signal,
            position_signal: self.position_signal,
            show_arrows: self.show_arrows,
            slide_ms: self.slide_ms,
            requested_page: self.current_page,
            touch: None,
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
        for child in &self.children {
            let el = child.create_element();
            let id = tree.insert_with_type_id(el, Some(parent_id), child.as_any().type_id());
            child.mount(tree, id);
        }
    }

    fn child_widgets(&self) -> Vec<&dyn Widget> {
        self.children
            .iter()
            .map(|c| c.as_ref() as &dyn Widget)
            .collect()
    }
}

const INDICATOR_SIZE: f32 = 8.0;
const INDICATOR_GAP: f32 = 8.0;
const INDICATOR_AREA_HEIGHT: f32 = 28.0;
const ARROW_SIZE: f32 = 36.0;

pub struct CarouselElement {
    id: ElementId,
    /// Длительность перелистывания, мс (0 — мгновенно).
    slide_ms: u32,
    page_count: usize,
    current_page: usize,
    auto_play: bool,
    auto_play_interval_ms: u32,
    show_indicators: bool,
    on_page_change: Option<Arc<Mutex<dyn FnMut(usize) + Send>>>,
    on_overscroll: Option<Arc<Mutex<dyn FnMut(i32) + Send>>>,
    page_signal: Option<crate::signal::RwSignal<usize>>,
    position_signal: Option<crate::signal::RwSignal<f32>>,
    show_arrows: bool,
    /// Страница, которую последней просил виджет: пересборка с той же
    /// страницей не сбрасывает пролистанное пальцем.
    requested_page: usize,
    touch: Option<CarouselTouch>,
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

#[derive(Debug)]
struct CarouselTouch {
    id: u64,
    start: Point,
    /// `None` — ось ещё не ясна; `Some(false)` — жест не наш (вертикальный).
    ours: Option<bool>,
    velocity: crate::input::VelocityTracker,
}

/// Бросок быстрее — листаем, даже если протащили меньше порога.
const FLING_VELOCITY: f32 = 400.0;

impl CarouselElement {
    /// Сдвиг пальцем с сопротивлением за крайними страницами.
    fn rubber_band(&self, raw: f32) -> f32 {
        let max = (self.page_count.saturating_sub(1)) as f32 * self.bounds.size.width;
        let pos = self.slide_offset + raw;
        if pos < 0.0 {
            -self.slide_offset + pos * 0.35
        } else if pos > max {
            max - self.slide_offset + (pos - max) * 0.35
        } else {
            raw
        }
    }

    /// Палец отпущен: листать по пути или броску, иначе вернуться.
    fn settle(&mut self, velocity: f32) {
        let threshold = self.bounds.size.width * 0.2;
        self.slide_offset += self.drag_offset;
        self.drag_start_x = None;
        self.drag_offset = 0.0;
        let moved = self.slide_offset - self.target_offset;
        let forward = moved > threshold || (velocity < -FLING_VELOCITY && moved > 0.0);
        let backward = -moved > threshold || (velocity > FLING_VELOCITY && moved < 0.0);
        if forward && self.current_page + 1 < self.page_count {
            self.go_to_page(self.current_page + 1);
        } else if backward && self.current_page > 0 {
            self.go_to_page(self.current_page - 1);
        } else {
            // За краем — сообщить (страница остаётся, резинка возвращается).
            // Путь за краем ужат резинкой, поэтому порог — по броску или
            // по уменьшенному пути.
            let over = if moved > threshold * 0.35 || (velocity < -FLING_VELOCITY && moved > 0.0) {
                1
            } else if -moved > threshold * 0.35 || (velocity > FLING_VELOCITY && moved < 0.0) {
                -1
            } else {
                0
            };
            if over != 0 {
                if let Some(ref cb) = self.on_overscroll {
                    if let Ok(mut f) = cb.lock() {
                        f(over);
                    }
                }
            }
            self.anim_start_offset = self.slide_offset;
            self.target_offset = self.current_page as f32 * self.bounds.size.width;
            self.anim_progress = 0.0;
            self.animating = true;
        }
    }

    /// Ловить свайп пальцем: есть куда листать или слушают выход за край.
    fn swipeable(&self) -> bool {
        self.page_count >= 2 || (self.page_count == 1 && self.on_overscroll.is_some())
    }

    fn begin_touch(&mut self, id: u64, at: Point) {
        let mut velocity = crate::input::VelocityTracker::new();
        velocity.add(at);
        self.touch = Some(CarouselTouch { id, start: at, ours: None, velocity });
    }

    fn fire_page_change(&self) {
        if let Some(sig) = self.page_signal {
            if sig.get_untracked() != self.current_page {
                sig.set(self.current_page);
            }
        }
        if let Some(ref cb) = self.on_page_change {
            if let Ok(mut f) = cb.lock() {
                f(self.current_page);
            }
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

    /// Точки страниц: размер, шаг и левый край. Если страниц много, точки
    /// и зазоры ужимаются, чтобы ряд помещался в ширину карусели.
    fn indicator_metrics(&self) -> (f32, f32, f32) {
        let n = self.page_count.max(1) as f32;
        let natural = n * INDICATOR_SIZE + (n - 1.0) * INDICATOR_GAP;
        let avail = (self.bounds.size.width - 16.0).max(0.0);
        let k = if natural > avail && natural > 0.0 { avail / natural } else { 1.0 };
        let size = (INDICATOR_SIZE * k).max(2.0);
        let step = (INDICATOR_SIZE + INDICATOR_GAP) * k;
        let total = (n - 1.0) * step + size;
        (size, step, self.bounds.x() + (self.bounds.size.width - total) / 2.0)
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
            Point::new(
                self.bounds.x() + 8.0,
                self.bounds.y() + (self.content_height() - ARROW_SIZE) / 2.0,
            ),
            Size::new(ARROW_SIZE, ARROW_SIZE),
        )
    }

    fn next_arrow_rect(&self) -> Rect {
        Rect::new(
            Point::new(
                self.bounds.x() + self.bounds.size.width - ARROW_SIZE - 8.0,
                self.bounds.y() + (self.content_height() - ARROW_SIZE) / 2.0,
            ),
            Size::new(ARROW_SIZE, ARROW_SIZE),
        )
    }
}

impl CarouselElement {
    fn handle_event_inner(&mut self, event: &Event, ctx: &mut EventContext) -> EventResult {
        match event {
            Event::MouseMove(pos) => {
                if self.bounds.contains(*pos) {
                    if let Some(start) = self.drag_start_x {
                        self.drag_offset = self.rubber_band(start - pos.x);
                        ctx.request_paint();
                        return EventResult::Handled;
                    }

                    let prev_h = self.show_arrows && self.prev_arrow_rect().contains(*pos) && self.current_page > 0;
                    let next_h = self.show_arrows
                        && self.next_arrow_rect().contains(*pos)
                        && self.current_page < self.page_count - 1;
                    if prev_h != self.prev_hover || next_h != self.next_hover {
                        self.prev_hover = prev_h;
                        self.next_hover = next_h;
                        if prev_h || next_h {
                            ctx.set_cursor(CursorIcon::Pointer);
                        }
                        ctx.request_paint();
                    }
                    return EventResult::Handled;
                }
                EventResult::Ignored
            }
            Event::MouseDown { button, position }
                if *button == MouseButton::Left && self.bounds.contains(*position) =>
            {
                if self.show_arrows && self.prev_arrow_rect().contains(*position) && self.current_page > 0 {
                    self.go_to_page(self.current_page - 1);
                    ctx.request_paint();
                    return EventResult::Handled;
                }
                if self.show_arrows
                    && self.next_arrow_rect().contains(*position)
                    && self.current_page < self.page_count - 1
                {
                    self.go_to_page(self.current_page + 1);
                    ctx.request_paint();
                    return EventResult::Handled;
                }

                if self.show_indicators && self.page_count > 1 {
                    let (size, step, start_x) = self.indicator_metrics();
                    let total_w = (self.page_count as f32 - 1.0) * step + size;
                    let ind_y = self.bounds.y() + self.bounds.size.height - INDICATOR_AREA_HEIGHT;
                    let ind_rect = Rect::new(
                        Point::new(start_x, ind_y),
                        Size::new(total_w, INDICATOR_AREA_HEIGHT),
                    );
                    if ind_rect.contains(*position) {
                        let idx =
                            ((position.x - start_x) / step) as usize;
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
            Event::MouseUp { button, .. }
                if *button == MouseButton::Left && self.drag_start_x.is_some() && self.touch.is_none() =>
            {
                self.settle(0.0);
                ctx.request_paint();
                EventResult::Handled
            }
            Event::TouchStart { id, position } => {
                if !self.swipeable() || !self.bounds.contains(*position) || self.touch.is_some() {
                    return EventResult::Ignored;
                }
                self.begin_touch(*id, *position);
                EventResult::Handled
            }
            Event::TouchMove { id, position } => {
                if self.touch.as_ref().map(|t| t.id) != Some(*id) {
                    // Жест отдала вложенная вертикальная прокрутка — подхватываем.
                    if !self.swipeable() || self.touch.is_some() || !self.bounds.contains(*position) {
                        return EventResult::Ignored;
                    }
                    self.begin_touch(*id, *position);
                    return EventResult::Handled;
                }
                let slop = crate::input::touch_config().tap_slop;
                let t = self.touch.as_mut().unwrap();
                t.velocity.add(*position);
                let (dx, dy) = (position.x - t.start.x, position.y - t.start.y);
                if t.ours.is_none() {
                    if dx.abs().max(dy.abs()) < slop {
                        return EventResult::Handled;
                    }
                    t.ours = Some(dx.abs() > dy.abs());
                    if t.ours == Some(true) {
                        // Отсчёт от точки, где ось стала ясна, — без рывка на пороге.
                        t.start = *position;
                        self.drag_start_x = Some(position.x);
                        self.drag_offset = 0.0;
                        self.animating = false;
                    }
                }
                if t.ours != Some(true) {
                    return EventResult::Ignored;
                }
                let raw = t.start.x - position.x;
                self.drag_offset = self.rubber_band(raw);
                ctx.request_paint();
                EventResult::Handled
            }
            Event::TouchEnd { id, .. } => {
                if self.touch.as_ref().map(|t| t.id) != Some(*id) {
                    return EventResult::Ignored;
                }
                let t = self.touch.take().unwrap();
                if t.ours != Some(true) {
                    return EventResult::Ignored;
                }
                self.settle(t.velocity.velocity().x);
                ctx.request_paint();
                EventResult::Handled
            }
            _ => EventResult::Ignored,
        }
    }


    /// Положение ленты в страницах (дробное) — в сигнал, если задан.
    fn publish_position(&self) {
        let Some(sig) = self.position_signal else { return };
        let w = self.bounds.size.width;
        if w <= 0.0 {
            return;
        }
        let pos = self.visible_offset() / w;
        if (sig.get_untracked() - pos).abs() > 0.0005 {
            sig.set(pos);
        }
    }
}

impl Element for CarouselElement {
    fn update(&mut self, widget: &dyn Widget, _ctx: &mut UpdateContext) {
        if let Some(c) = widget.as_any().downcast_ref::<Carousel>() {
            self.page_count = c.children.len();
            self.auto_play = c.auto_play;
            self.auto_play_interval_ms = c.auto_play_interval_ms;
            self.show_indicators = c.show_indicators;
            self.show_arrows = c.show_arrows;
            self.on_page_change = c.on_page_change.clone();
            self.on_overscroll = c.on_overscroll.clone();
            self.page_signal = c.page_signal;
            self.position_signal = c.position_signal;
            let want = c.page_signal.map(|s| s.get_untracked()).unwrap_or(c.current_page);
            if want != self.requested_page {
                self.requested_page = want;
                // Листаем с анимацией, а не прыжком.
                if want < self.page_count && want != self.current_page {
                    self.current_page = want;
                    self.anim_start_offset = self.slide_offset;
                    self.target_offset = want as f32 * self.bounds.size.width;
                    self.anim_progress = 0.0;
                    self.animating = true;
                }
            }
            if self.current_page >= self.page_count {
                self.current_page = self.page_count.saturating_sub(1);
                self.target_offset = self.current_page as f32 * self.bounds.size.width;
                self.slide_offset = self.target_offset;
            }
            self.mark_dirty(DirtyFlags::LAYOUT | DirtyFlags::RENDER);
        }
    }

    fn layout(&mut self, constraints: Constraints) -> Size {
        let w = constraints.max_width;
        // Высота из MSS (`height`), иначе — всё доступное место.
        let h = match self.mss.height.and_then(|d| d.resolve_opt(constraints.containing_block.height)) {
            Some(h) => h.max(constraints.min_height).min(constraints.max_height),
            None if constraints.max_height.is_finite() => constraints.max_height,
            None => 300.0,
        };
        let old_width = self.bounds.size.width;
        self.bounds = Rect::new(Point::zero(), Size::new(w, h));
        self.target_offset = self.current_page as f32 * w;
        if (old_width - w).abs() > 0.5 || old_width == 0.0 {
            self.slide_offset = self.target_offset;
        }
        self.publish_position();
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

        if self.page_count > 1 && self.show_arrows {
            if self.current_page > 0 {
                let prev = self.prev_arrow_rect();
                let bg = if self.prev_hover {
                    Color::BLACK.with_alpha(0.15)
                } else {
                    Color::BLACK.with_alpha(0.06)
                };
                list.push_rect(prev, bg, [ARROW_SIZE / 2.0; 4]);
                list.push_text_centered("\u{25C0}", prev, Color::WHITE, 14.0);
            }
            if self.current_page < self.page_count - 1 {
                let next = self.next_arrow_rect();
                let bg = if self.next_hover {
                    Color::BLACK.with_alpha(0.15)
                } else {
                    Color::BLACK.with_alpha(0.06)
                };
                list.push_rect(next, bg, [ARROW_SIZE / 2.0; 4]);
                list.push_text_centered("\u{25B6}", next, Color::WHITE, 14.0);
            }
        }

        if self.show_indicators && self.page_count > 1 {
            let (size, step, start_x) = self.indicator_metrics();
            let y = self.bounds.y() + self.bounds.size.height - INDICATOR_AREA_HEIGHT
                + (INDICATOR_AREA_HEIGHT - size) / 2.0;

            let active_color = self.mss.accent_color.unwrap_or(Color::from_hex("#3B82F6"));
            let inactive_color = self.mss.border_color.unwrap_or_else(crate::theme_fallback::fallback_border);
            for i in 0..self.page_count {
                let x = start_x + i as f32 * step;
                let r = Rect::new(Point::new(x, y), Size::new(size, size));
                let color = if i == self.current_page {
                    active_color
                } else {
                    inactive_color
                };
                list.push_rect(r, color, [size / 2.0; 4]);
            }
        }
    }

    /// Кадры нужны на время переезда слайда и постоянно — при автопрокрутке.
    fn wants_animate_tick(&self) -> bool {
        self.animating || (self.auto_play && self.page_count > 1)
    }

    fn animate(&mut self, dt: Duration) -> bool {
        // анимации выключены глобально — переход за один кадр
        let dt = crate::animation::effective_dt(dt);
        let mut needs_redraw = false;
        // 0 — без анимации: шаг больше любой длительности
        let slide_duration: f32 = (self.slide_ms as f32 / 1000.0).max(1e-4);

        if self.animating {
            self.anim_progress += dt.as_secs_f32() / slide_duration;
            if self.anim_progress >= 1.0 {
                self.anim_progress = 1.0;
                self.animating = false;
                self.slide_offset = self.target_offset;
            } else {
                let t = self.anim_progress;
                let ease = 1.0 - (1.0 - t) * (1.0 - t) * (1.0 - t);
                self.slide_offset =
                    self.anim_start_offset + (self.target_offset - self.anim_start_offset) * ease;
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

        if needs_redraw {
            self.publish_position();
        }
        needs_redraw || self.auto_play
    }

    fn handle_event(&mut self, event: &Event, ctx: &mut EventContext) -> EventResult {
        let r = self.handle_event_inner(event, ctx);
        self.publish_position();
        r
    }
    fn children(&self) -> &[ElementId] {
        &self.child_ids
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

    fn get_classes(&self) -> &[String] {
        &self.classes
    }

    fn element_type_name(&self) -> &str {
        "Carousel"
    }

    fn reset_mss_styles(&mut self) {
        self.mss.reset();
    }
    fn mss(&self) -> Option<&crate::mss::MssFields> {
        Some(&self.mss)
    }
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
        self.mss
            .apply_transitions(base, hover, active, focus, selected);
    }
}

impl StyledElement for CarouselElement {
    fn apply_style(&mut self, style: &ComputedStyle) {
        self.mss.apply(style);
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
mod touch_tests {
    use super::*;
    use crate::testing::TestHarness;
    use crate::widgets::{Column, DecoratedBox, ScrollView};

    fn pages(sig: crate::signal::RwSignal<usize>) -> Carousel {
        Carousel::new()
            .page_signal(sig)
            .show_indicators(false)
            .child(ScrollView::new().vertical().child(Column::new().height(3000.0)))
            .child(DecoratedBox::new())
            .child(DecoratedBox::new())
    }

    #[test]
    fn swipe_left_goes_to_next_page_through_vertical_scroll() {
        crate::signal::allow_signal_reads_on_this_thread();
        let sig = crate::signal::use_signal(0usize);
        let mut h = TestHarness::new(Box::new(pages(sig)));
        h.layout(400.0, 800.0);
        h.touch_down(1, Point::new(300.0, 400.0));
        for i in 1..=8 {
            std::thread::sleep(std::time::Duration::from_millis(4));
            h.touch_move(1, Point::new(300.0 - i as f32 * 25.0, 402.0));
        }
        h.touch_up(1);
        assert_eq!(sig.get_untracked(), 1);
    }

    #[test]
    fn vertical_gesture_stays_in_scroll() {
        crate::signal::allow_signal_reads_on_this_thread();
        let sig = crate::signal::use_signal(0usize);
        let mut h = TestHarness::new(Box::new(pages(sig)));
        h.layout(400.0, 800.0);
        h.touch_down(1, Point::new(200.0, 600.0));
        for i in 1..=8 {
            h.touch_move(1, Point::new(201.0, 600.0 - i as f32 * 30.0));
        }
        h.touch_up(1);
        assert_eq!(sig.get_untracked(), 0);
    }

    #[test]
    fn edge_page_does_not_overflow() {
        crate::signal::allow_signal_reads_on_this_thread();
        let sig = crate::signal::use_signal(0usize);
        let mut h = TestHarness::new(Box::new(pages(sig)));
        h.layout(400.0, 800.0);
        h.touch_down(1, Point::new(50.0, 400.0));
        for i in 1..=8 {
            h.touch_move(1, Point::new(50.0 + i as f32 * 40.0, 400.0));
        }
        h.touch_up(1);
        assert_eq!(sig.get_untracked(), 0);
    }
}
