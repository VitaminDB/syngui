//! Многоуровневое кольцо (sunburst, как у Baobab и Filelight): в центре — корень,
//! каждое кольцо — следующий уровень, сектор ребёнка лежит под сектором родителя
//! и занимает его долю по значению. Наведение и щелчки по секторам отдаются
//! приложению (`on_hover`, `on_click`, `on_secondary_click`), щелчок по центру —
//! `on_center_click` (обычно «на уровень вверх»). Подсветка — своя (под
//! указателем) и внешняя (`highlight`, например от строки списка).
//!
//! MSS: `color` — текст в центре, `center-color` — заливка центра,
//! `label-font-size` — размер текста в центре, `separator-color` — тонкие
//! границы между секторами и кольцами (обычно — цвет фона под графиком).

use crate::core::canvas::CanvasContext;
use crate::core::{Color, Point, Rect, Size};
use crate::input::{Event, EventResult, MouseButton};
use crate::layout::Constraints;
use crate::mss::MssFields;
use crate::mss::{ComputedStyle, Dimension};
use crate::render::DisplayList;
use crate::widget::{DirtyFlags, Element, ElementId, ElementTree, StyledElement, UpdateContext, Widget};
use crate::CursorIcon;
use std::any::Any;
use std::f32::consts::{FRAC_PI_2, TAU};
use std::sync::Arc;

use super::math::polar_to_cartesian;
use super::types::palette_color;

/// Узел кольца: `id` возвращается в обработчики, `value` — вес (сумма детей
/// может быть меньше — остаток сектора остаётся пустым).
#[derive(Debug, Clone, Default)]
pub struct SunburstNode {
    pub id: u64,
    pub label: String,
    pub value: f64,
    pub color: Option<Color>,
    pub children: Vec<SunburstNode>,
}

impl SunburstNode {
    pub fn new(id: u64, label: impl Into<String>, value: f64) -> Self {
        Self { id, label: label.into(), value, color: None, children: Vec::new() }
    }

    pub fn color(mut self, c: impl Into<Color>) -> Self {
        self.color = Some(c.into());
        self
    }

    pub fn child(mut self, c: SunburstNode) -> Self {
        self.children.push(c);
        self
    }

    pub fn children(mut self, c: Vec<SunburstNode>) -> Self {
        self.children = c;
        self
    }
}

type HoverCb = Arc<dyn Fn(Option<u64>) + Send + Sync>;
type ClickCb = Arc<dyn Fn(u64) + Send + Sync>;
type CenterCb = Arc<dyn Fn() + Send + Sync>;
type SecondaryCb = Arc<dyn Fn(Option<u64>, Point) + Send + Sync>;

pub struct SunburstChart {
    nodes: Vec<SunburstNode>,
    total: Option<f64>,
    levels: usize,
    center_ratio: f32,
    min_angle_deg: f32,
    gap: f32,
    center: Vec<String>,
    highlight: Option<u64>,
    width: Option<Dimension>,
    height: Option<Dimension>,
    on_hover: Option<HoverCb>,
    on_click: Option<ClickCb>,
    on_center_click: Option<CenterCb>,
    on_secondary: Option<SecondaryCb>,
    classes: Vec<String>,
}

impl Default for SunburstChart {
    fn default() -> Self {
        Self::new()
    }
}

impl SunburstChart {
    pub fn new() -> Self {
        Self {
            nodes: Vec::new(),
            total: None,
            levels: 4,
            center_ratio: 0.26,
            min_angle_deg: 0.5,
            gap: 1.0,
            center: Vec::new(),
            highlight: None,
            width: None,
            height: None,
            on_hover: None,
            on_click: None,
            on_center_click: None,
            on_secondary: None,
            classes: Vec::new(),
        }
    }

    /// Узлы первого кольца.
    pub fn nodes(mut self, nodes: Vec<SunburstNode>) -> Self {
        self.nodes = nodes;
        self
    }

    /// Вес полного круга (по умолчанию — сумма узлов первого кольца).
    pub fn total(mut self, total: f64) -> Self {
        self.total = Some(total);
        self
    }

    /// Сколько колец рисовать (глубже — не показывается).
    pub fn levels(mut self, n: usize) -> Self {
        self.levels = n.max(1);
        self
    }

    /// Радиус центра — доля внешнего радиуса.
    pub fn center_ratio(mut self, r: f32) -> Self {
        self.center_ratio = r.clamp(0.0, 0.8);
        self
    }

    /// Секторы уже этого угла не рисуются (и их дети тоже).
    pub fn min_angle(mut self, deg: f32) -> Self {
        self.min_angle_deg = deg.max(0.0);
        self
    }

    /// Щель между секторами, px.
    pub fn gap(mut self, px: f32) -> Self {
        self.gap = px.max(0.0);
        self
    }

    /// Строки текста в центре (первая — крупнее).
    pub fn center_text(mut self, lines: Vec<String>) -> Self {
        self.center = lines;
        self
    }

    /// Подсветить сектор снаружи (например, строку списка под указателем).
    pub fn highlight(mut self, id: Option<u64>) -> Self {
        self.highlight = id;
        self
    }

    pub fn size(mut self, w: f32, h: f32) -> Self {
        self.width = Some(Dimension::Px(w));
        self.height = Some(Dimension::Px(h));
        self
    }

    pub fn on_hover(mut self, f: impl Fn(Option<u64>) + Send + Sync + 'static) -> Self {
        self.on_hover = Some(Arc::new(f));
        self
    }

    pub fn on_click(mut self, f: impl Fn(u64) + Send + Sync + 'static) -> Self {
        self.on_click = Some(Arc::new(f));
        self
    }

    pub fn on_center_click(mut self, f: impl Fn() + Send + Sync + 'static) -> Self {
        self.on_center_click = Some(Arc::new(f));
        self
    }

    /// Правая кнопка: сектор (или `None` — центр) и точка окна (для меню).
    pub fn on_secondary_click(mut self, f: impl Fn(Option<u64>, Point) + Send + Sync + 'static) -> Self {
        self.on_secondary = Some(Arc::new(f));
        self
    }

    pub fn class(mut self, cls: impl Into<String>) -> Self {
        crate::widget::push_classes(&mut self.classes, cls.into());
        self
    }
}

/// Сектор на экране: кольцо (0 — первое), доли круга по часовой от верха.
#[derive(Debug, Clone, Copy)]
struct Seg {
    id: u64,
    ring: usize,
    t0: f32,
    t1: f32,
    color: Color,
}

fn flatten(nodes: &[SunburstNode], total: f64, ring: usize, t0: f32, t1: f32, base: Option<Color>, levels: usize, min_t: f32, out: &mut Vec<Seg>) {
    if ring >= levels || total <= 0.0 {
        return;
    }
    let mut t = t0;
    let span = t1 - t0;
    for (i, n) in nodes.iter().enumerate() {
        let w = (n.value.max(0.0) / total) as f32 * span;
        let (a, b) = (t, (t + w).min(t1));
        t = b;
        if b - a < min_t {
            continue;
        }
        let color = n.color.unwrap_or_else(|| match base {
            // Глубже — светлее цвет родителя.
            Some(c) => c.lerp(&Color::WHITE, 0.16),
            None => palette_color(i),
        });
        out.push(Seg { id: n.id, ring, t0: a, t1: b, color });
        if !n.children.is_empty() {
            flatten(&n.children, n.value, ring + 1, a, b, Some(color), levels, min_t, out);
        }
    }
}

/// Сектор уже этого по внешней дуге (px) не рисуется — иначе «иголки» и шум.
const MIN_ARC_PX: f32 = 2.0;

fn seg_shown(s: &Seg, r0: f32, ring: f32) -> bool {
    (s.t1 - s.t0) * TAU * (r0 + ring * (s.ring + 1) as f32) >= MIN_ARC_PX
}

/// Угол для доли `t` по часовой от верха (в системе `polar_to_cartesian`).
fn angle(t: f32) -> f32 {
    FRAC_PI_2 - t * TAU
}

struct SunburstElement {
    id: ElementId,
    w: SunburstChart,
    segs: Vec<Seg>,
    bounds: Rect,
    hovered: Option<Option<u64>>,
    pressed: Option<Option<u64>>,
    classes: Vec<String>,
    dirty: DirtyFlags,
    mss: MssFields,
    center_color: Option<Color>,
    separator: Option<Color>,
    font: Option<f32>,
}

impl SunburstElement {
    fn rebuild(&mut self) {
        self.segs.clear();
        let total = self.w.total.unwrap_or_else(|| self.w.nodes.iter().map(|n| n.value.max(0.0)).sum());
        let min_t = self.w.min_angle_deg / 360.0;
        flatten(&self.w.nodes, total, 0, 0.0, 1.0, None, self.w.levels, min_t, &mut self.segs);
    }

    /// Центр, радиус центра, ширина кольца.
    fn geometry(&self) -> (f32, f32, f32, f32) {
        let b = self.bounds;
        let outer = (b.size.width.min(b.size.height) * 0.5 - 4.0).max(10.0);
        let r0 = outer * self.w.center_ratio;
        let ring = (outer - r0) / self.w.levels as f32;
        (b.origin.x + b.size.width * 0.5, b.origin.y + b.size.height * 0.5, r0, ring)
    }

    /// Что под точкой: `Some(Some(id))` — сектор, `Some(None)` — центр, `None` — пусто.
    fn hit(&self, p: Point) -> Option<Option<u64>> {
        let (cx, cy, r0, ring) = self.geometry();
        let (dx, dy) = (p.x - cx, p.y - cy);
        let r = (dx * dx + dy * dy).sqrt();
        if r < r0 {
            return Some(None);
        }
        let k = ((r - r0) / ring) as usize;
        if k >= self.w.levels {
            return None;
        }
        // Доля по часовой от верха (y экрана — вниз).
        let mut t = dx.atan2(-dy) / TAU;
        if t < 0.0 {
            t += 1.0;
        }
        self.segs.iter().find(|s| s.ring == k && t >= s.t0 && t < s.t1 && seg_shown(s, r0, ring)).map(|s| Some(s.id))
    }

    fn set_hover(&mut self, h: Option<Option<u64>>) {
        if h == self.hovered {
            return;
        }
        let before = self.hovered.flatten();
        self.hovered = h;
        if before != h.flatten() {
            if let Some(cb) = &self.w.on_hover {
                cb(h.flatten());
            }
        }
        self.mark_dirty(DirtyFlags::RENDER);
    }
}

impl Widget for SunburstChart {
    fn create_element(&self) -> Box<dyn Element> {
        let mut e = SunburstElement {
            id: ElementId::new(),
            w: self.clone_shallow(),
            segs: Vec::new(),
            bounds: Rect::zero(),
            hovered: None,
            pressed: None,
            classes: self.classes.clone(),
            dirty: DirtyFlags::LAYOUT | DirtyFlags::RENDER,
            mss: MssFields::new(),
            center_color: None,
            separator: None,
            font: None,
        };
        e.rebuild();
        Box::new(e)
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

impl SunburstChart {
    fn clone_shallow(&self) -> SunburstChart {
        SunburstChart {
            nodes: self.nodes.clone(),
            total: self.total,
            levels: self.levels,
            center_ratio: self.center_ratio,
            min_angle_deg: self.min_angle_deg,
            gap: self.gap,
            center: self.center.clone(),
            highlight: self.highlight,
            width: self.width,
            height: self.height,
            on_hover: self.on_hover.clone(),
            on_click: self.on_click.clone(),
            on_center_click: self.on_center_click.clone(),
            on_secondary: self.on_secondary.clone(),
            classes: self.classes.clone(),
        }
    }
}

impl Element for SunburstElement {
    fn update(&mut self, widget: &dyn Widget, _ctx: &mut UpdateContext) {
        if let Some(w) = widget.as_any().downcast_ref::<SunburstChart>() {
            self.w = w.clone_shallow();
            self.rebuild();
            // Прежний сектор под указателем мог исчезнуть.
            if let Some(Some(id)) = self.hovered {
                if !self.segs.iter().any(|s| s.id == id) {
                    self.hovered = None;
                }
            }
            self.mark_dirty(DirtyFlags::LAYOUT | DirtyFlags::RENDER);
        }
    }

    fn layout(&mut self, c: Constraints) -> Size {
        let fallback = |m: f32| if m.is_finite() { m } else { 300.0 };
        let w = self.mss.width.or(self.w.width).map(|d| d.resolve(c.max_width)).unwrap_or_else(|| fallback(c.max_width)).min(c.max_width);
        let h = self.mss.height.or(self.w.height).map(|d| d.resolve(c.max_height)).unwrap_or_else(|| fallback(c.max_height).min(w.max(1.0))).min(c.max_height);
        self.bounds = Rect::new(Point::zero(), Size::new(w, h));
        Size::new(w, h)
    }

    fn build_display_list(&self, list: &mut DisplayList, _clip: Rect) {
        if let Some(bg) = self.mss.background_color {
            list.push_rect(self.bounds, bg, self.mss.border_radius_resolved(self.bounds.size.width, 0.0));
        }
        let (cx, cy, r0, ring) = self.geometry();
        let mut ctx = CanvasContext::new(self.bounds.origin, self.bounds.size);
        let (lx, ly) = (cx - self.bounds.origin.x, cy - self.bounds.origin.y);
        let lit = self.hovered.flatten().or(self.w.highlight);
        // Притушить — к тёмному в тёмной теме (светлый текст), к светлому в светлой:
        // прозрачность проявляла бы стыки кусочков дуги.
        let fg0 = self.mss.color.unwrap_or(Color::from_hex("#e6e6e6"));
        let dark = fg0.r + fg0.g + fg0.b > 1.5;
        let dim_to = if dark { Color::from_hex("#202226") } else { Color::from_hex("#f4f5f7") };
        let gap = self.w.gap;
        let sep = self.separator.unwrap_or(dim_to);
        let shown: Vec<&Seg> = self.segs.iter().filter(|s| seg_shown(s, r0, ring)).collect();
        let color_of = |s: &Seg| match lit {
            // Под указателем — ярче, остальные — притушены (видно и на светлых кольцах).
            Some(id) if id == s.id => s.color.lerp(&Color::WHITE, 0.3),
            Some(_) => s.color.lerp(&dim_to, 0.5),
            None => s.color,
        };
        // Точки дуги радиуса `r` от доли t0 до t1 — не реже чем через ~2 px.
        let steps = |r: f32, t0: f32, t1: f32| (((t1 - t0) * TAU * r / 2.0).ceil() as usize).clamp(1, 720);
        let arc_n = |r: f32, t0: f32, t1: f32, n: usize| -> Vec<(f32, f32)> {
            (0..=n).map(|k| polar_to_cartesian(lx, ly, r, angle(t0 + (t1 - t0) * k as f32 / n as f32))).collect()
        };
        let arc = |r: f32, t0: f32, t1: f32| arc_n(r, t0, t1, steps(r, t0, t1));
        // 1. Заливка секторов встык (без сглаживания внутри — стыки кусочков не видны).
        ctx.set_anti_alias(0.0);
        for s in &shown {
            let inner = r0 + ring * s.ring as f32;
            let outer = inner + ring;
            ctx.set_color(color_of(s));
            let n = steps(outer, s.t0, s.t1);
            let (o, i) = (arc_n(outer, s.t0, s.t1, n), arc_n(inner, s.t0, s.t1, n));
            for k in 0..o.len() - 1 {
                ctx.fill_polygon(&[o[k], o[k + 1], i[k + 1], i[k]]);
            }
        }
        // 2. Внешний край каждого сектора — сглаженной дугой своего цвета.
        ctx.set_anti_alias(1.0);
        ctx.set_stroke_width(1.0);
        for s in &shown {
            let outer = r0 + ring * (s.ring + 1) as f32;
            ctx.set_color(color_of(s));
            ctx.stroke_path(&arc(outer - 0.5, s.t0, s.t1), false);
        }
        // 3. Границы — тонкими сглаженными линиями цвета фона: дуга по внутреннему
        //    радиусу и радиальная черта в начале сектора (если сектор не слишком узкий).
        if gap > 0.0 {
            ctx.set_color(sep);
            ctx.set_stroke_width(gap);
            for s in &shown {
                let inner = r0 + ring * s.ring as f32;
                let outer = inner + ring;
                ctx.stroke_path(&arc(inner, s.t0, s.t1), false);
                if (s.t1 - s.t0) * TAU * inner >= gap * 3.0 && (s.t1 - s.t0) < 0.9999 {
                    let a = angle(s.t0);
                    ctx.stroke_path(&[polar_to_cartesian(lx, ly, inner, a), polar_to_cartesian(lx, ly, outer + 0.5, a)], false);
                    let b = angle(s.t1);
                    ctx.stroke_path(&[polar_to_cartesian(lx, ly, inner, b), polar_to_cartesian(lx, ly, outer + 0.5, b)], false);
                }
            }
        }
        // Центр: заливка (под указателем — ярче, если по нему щёлкают).
        let fg = self.mss.color.unwrap_or(Color::from_hex("#e6e6e6"));
        let mut center = self.center_color.unwrap_or(fg.with_alpha(0.07));
        if self.hovered == Some(None) && self.w.on_center_click.is_some() {
            center = center.lerp(&fg, 0.1);
        }
        ctx.set_color(center);
        ctx.fill_circle(lx, ly, (r0 - gap).max(0.0));
        ctx.flush(list);
        // Текст в центре: первая строка крупнее.
        let font = self.font.unwrap_or(13.0);
        let lines: Vec<(&String, f32)> = self.w.center.iter().enumerate().map(|(i, l)| (l, if i == 0 { font + 3.0 } else { font })).collect();
        let total_h: f32 = lines.iter().map(|(_, f)| f * 1.3).sum();
        let mut y = cy - total_h * 0.5;
        let width = (r0 * 1.7).max(20.0);
        for (i, (text, f)) in lines.iter().enumerate() {
            let color = if i == 0 { fg } else { fg.with_alpha(0.65) };
            list.push_text_centered(text, Rect::new(Point::new(cx - width * 0.5, y), Size::new(width, f * 1.3)), color, *f);
            y += f * 1.3;
        }
    }

    fn handle_event(&mut self, event: &Event, ctx: &mut crate::widget::context::EventContext) -> EventResult {
        match event {
            Event::MouseMove(p) => {
                if !self.bounds.contains(*p) {
                    self.set_hover(None);
                    return EventResult::Ignored;
                }
                let h = self.hit(*p);
                self.set_hover(h);
                let clickable = match h {
                    Some(Some(_)) => self.w.on_click.is_some(),
                    Some(None) => self.w.on_center_click.is_some(),
                    None => false,
                };
                if clickable {
                    ctx.set_cursor(CursorIcon::Pointer);
                }
                if h.is_some() {
                    return EventResult::Handled;
                }
                EventResult::Ignored
            }
            Event::MouseDown { button, position } if self.bounds.contains(*position) => {
                let h = self.hit(*position);
                match button {
                    MouseButton::Left => {
                        self.pressed = h;
                        if h.is_some() {
                            return EventResult::Handled;
                        }
                    }
                    MouseButton::Right => {
                        if let (Some(h), Some(cb)) = (h, &self.w.on_secondary) {
                            cb(h, *position);
                            return EventResult::Handled;
                        }
                    }
                    _ => {}
                }
                EventResult::Ignored
            }
            Event::MouseUp { button: MouseButton::Left, position } => {
                let pressed = self.pressed.take();
                if !self.bounds.contains(*position) {
                    return EventResult::Ignored;
                }
                let h = self.hit(*position);
                if h.is_none() || h != pressed {
                    return EventResult::Ignored;
                }
                match h {
                    Some(Some(id)) => {
                        if let Some(cb) = &self.w.on_click {
                            cb(id);
                        }
                    }
                    Some(None) => {
                        if let Some(cb) = &self.w.on_center_click {
                            cb();
                        }
                    }
                    None => {}
                }
                EventResult::Handled
            }
            Event::LongPress { position } if self.bounds.contains(*position) => {
                if let (Some(h), Some(cb)) = (self.hit(*position), &self.w.on_secondary) {
                    cb(h, *position);
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
        self.dirty |= flags;
    }
    fn clear_dirty(&mut self, flags: DirtyFlags) {
        self.dirty.remove(flags);
    }
    fn is_dirty(&self, flags: DirtyFlags) -> bool {
        self.dirty.contains(flags)
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
        "SunburstChart"
    }
    fn reset_mss_styles(&mut self) {
        self.mss.reset();
    }
    fn mss(&self) -> Option<&MssFields> {
        Some(&self.mss)
    }
    fn mss_mut(&mut self) -> Option<&mut MssFields> {
        Some(&mut self.mss)
    }
    fn apply_computed_style(&mut self, style: &ComputedStyle) {
        self.mss.apply(style);
        let color = |k: &str| style.get(k).and_then(|v| v.as_color()).map(crate::animation::transition::mss_color_to_core);
        self.center_color = color("center-color");
        self.separator = color("separator-color");
        self.font = style.get("label-font-size").and_then(|v| v.as_px());
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
}

impl StyledElement for SunburstElement {
    fn apply_style(&mut self, _style: &ComputedStyle) {
        self.mark_dirty(DirtyFlags::LAYOUT | DirtyFlags::RENDER);
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

    fn chart() -> SunburstElement {
        let w = SunburstChart::new()
            .levels(2)
            .total(100.0)
            .nodes(vec![SunburstNode::new(1, "a", 50.0).child(SunburstNode::new(11, "a1", 25.0)), SunburstNode::new(2, "b", 25.0)]);
        let mut e = SunburstElement {
            id: ElementId::new(),
            w,
            segs: Vec::new(),
            bounds: Rect::new(Point::zero(), Size::new(208.0, 208.0)),
            hovered: None,
            pressed: None,
            classes: Vec::new(),
            dirty: DirtyFlags::empty(),
            mss: MssFields::new(),
            center_color: None,
            separator: None,
            font: None,
        };
        e.rebuild();
        e
    }

    #[test]
    fn layout_and_hit() {
        let e = chart();
        // a: 0..0.5, a1: 0..0.25 во втором кольце, b: 0.5..0.75.
        assert_eq!(e.segs.len(), 3);
        let (cx, cy, r0, ring) = e.geometry();
        assert_eq!(e.hit(Point::new(cx, cy)), Some(None));
        // Справа от центра — четверть круга по часовой (a), первое кольцо.
        assert_eq!(e.hit(Point::new(cx + r0 + ring * 0.5, cy)), Some(Some(1)));
        // Чуть выше правой точки во втором кольце — ещё a1 (до 0.25).
        assert_eq!(e.hit(Point::new(cx + r0 + ring * 1.5, cy - 5.0)), Some(Some(11)));
        // Снизу (0.5) — b; слева (0.75) — пусто.
        assert_eq!(e.hit(Point::new(cx, cy + r0 + ring * 0.5)), Some(Some(2)));
        assert_eq!(e.hit(Point::new(cx - r0 - ring * 0.5, cy - 2.0)), None);
    }
}
