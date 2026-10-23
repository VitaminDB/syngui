//! `OptionGroup` — группа взаимоисключающих вариантов (радиокнопки) одним элементом.
//!
//! В отличие от [`RadioButton`](super::RadioButton), у группы есть обработчик выбора, а смена
//! варианта сразу перерисовывает всю группу. У каждого варианта — подпись, необязательные
//! пояснение второй строкой и метка справа («рекомендуется», «установлено»); вариант можно
//! выключить. Управление: щелчок, стрелки (с пропуском выключенных), Home/End.
//!
//! ```ignore
//! OptionGroup::new(vec![
//!     OptionItem::new("pacman", "pacman").description("extra/zig 0.14.1"),
//!     OptionItem::new("download", "Скачать").description("ziglang.org, 48 МБ"),
//! ])
//! .selected("pacman")
//! .on_change(|v| println!("{v}"))
//! ```
//!
//! Выбранное значение — свойство: если родитель передал новое `selected`, группа его принимает;
//! если не передаёт (неуправляемый режим), группа помнит выбор сама.

use crate::core::sync::Mutex;
use crate::core::{Color, Point, Rect, RectExt, Size};
use crate::input::{CursorIcon, Event, EventResult, Key, MouseButton};
use crate::layout::Constraints;
use crate::mss::{ComputedStyle, MssFields, TextAlign, TextDecoration};
use crate::render::{Border, DisplayList};
use crate::widget::context::{EventContext, EventContextExt};
use crate::widget::{
    DirtyFlags, Element, ElementId, ElementTree, StyledElement, UpdateContext, Widget,
};
use std::any::Any;
use std::sync::Arc;

const RADIO: f32 = 18.0;
const GAP: f32 = 8.0;
const ROW_PAD: f32 = 4.0;

/// Один вариант группы.
#[derive(Clone, Debug, PartialEq)]
pub struct OptionItem {
    pub value: String,
    pub label: String,
    pub description: Option<String>,
    pub badge: Option<String>,
    pub disabled: bool,
}

impl OptionItem {
    pub fn new(value: impl Into<String>, label: impl Into<String>) -> Self {
        Self {
            value: value.into(),
            label: label.into(),
            description: None,
            badge: None,
            disabled: false,
        }
    }

    /// Пояснение второй строкой (приглушённым цветом).
    pub fn description(mut self, d: impl Into<String>) -> Self {
        let d = d.into();
        self.description = (!d.is_empty()).then_some(d);
        self
    }

    /// Метка справа от подписи (цвет акцента).
    pub fn badge(mut self, b: impl Into<String>) -> Self {
        let b = b.into();
        self.badge = (!b.is_empty()).then_some(b);
        self
    }

    pub fn disabled(mut self, disabled: bool) -> Self {
        self.disabled = disabled;
        self
    }
}

type OnChange = Arc<Mutex<dyn FnMut(&str) + Send>>;

pub struct OptionGroup {
    pub items: Vec<OptionItem>,
    pub selected: Option<String>,
    pub horizontal: bool,
    pub disabled: bool,
    pub gap: f32,
    pub on_change: Option<OnChange>,
}

impl OptionGroup {
    pub fn new(items: Vec<OptionItem>) -> Self {
        Self {
            items,
            selected: None,
            horizontal: false,
            disabled: false,
            gap: 2.0,
            on_change: None,
        }
    }

    pub fn selected(mut self, value: impl Into<String>) -> Self {
        self.selected = Some(value.into());
        self
    }

    /// Варианты в строку (по умолчанию — столбиком).
    pub fn horizontal(mut self) -> Self {
        self.horizontal = true;
        self
    }

    pub fn disabled(mut self, disabled: bool) -> Self {
        self.disabled = disabled;
        self
    }

    /// Промежуток между вариантами (по умолчанию 2 по вертикали; по горизонтали к нему +16).
    pub fn gap(mut self, gap: f32) -> Self {
        self.gap = gap;
        self
    }

    /// Вызывается при выборе варианта пользователем (значение `value`).
    pub fn on_change(mut self, cb: impl FnMut(&str) + Send + 'static) -> Self {
        self.on_change = Some(Arc::new(Mutex::new(cb)));
        self
    }
}

impl Widget for OptionGroup {
    fn create_element(&self) -> Box<dyn Element> {
        Box::new(self.element())
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
}

impl OptionGroup {
    fn element(&self) -> OptionGroupElement {
        let selected = self.selected.clone().unwrap_or_default();
        OptionGroupElement {
            id: ElementId::new(),
            items: self.items.clone(),
            selected: selected.clone(),
            prop_selected: self.selected.clone(),
            horizontal: self.horizontal,
            disabled: self.disabled,
            gap: self.gap,
            on_change: self.on_change.clone(),
            bounds: Rect::zero(),
            rows: Vec::new(),
            hover: None,
            focused: false,
            classes: Vec::new(),
            dirty_flags: DirtyFlags::LAYOUT | DirtyFlags::RENDER,
            mss: MssFields::new(),
            text_measure: None,
        }
    }
}

pub struct OptionGroupElement {
    id: ElementId,
    items: Vec<OptionItem>,
    selected: String,
    /// Последнее `selected`, пришедшее от родителя: новое значение принимается, прежнее
    /// не перебивает выбор пользователя.
    prop_selected: Option<String>,
    horizontal: bool,
    disabled: bool,
    gap: f32,
    on_change: Option<OnChange>,
    bounds: Rect,
    /// Прямоугольники вариантов относительно `bounds.origin`.
    rows: Vec<Rect>,
    hover: Option<usize>,
    focused: bool,
    classes: Vec<String>,
    dirty_flags: DirtyFlags,
    mss: MssFields,
    text_measure: Option<Arc<dyn crate::widget::context::TextMeasure>>,
}

impl OptionGroupElement {
    fn font_size(&self) -> f32 {
        self.mss.font_size.unwrap_or(14.0)
    }

    fn desc_size(&self) -> f32 {
        (self.font_size() - 2.0).max(9.0)
    }

    fn measure(&self, text: &str, size: f32, weight: u16) -> f32 {
        self.text_measure
            .as_ref()
            .map(|tm| {
                tm.measure_text_width_weight(
                    text,
                    size,
                    text.chars().count(),
                    weight,
                    self.mss.font_family.as_deref(),
                )
            })
            .unwrap_or(text.chars().count() as f32 * size * 0.6)
    }

    fn line_h(&self) -> f32 {
        (self.font_size() * 1.35).max(RADIO)
    }

    fn row_size(&self, it: &OptionItem) -> Size {
        let fs = self.font_size();
        let mut w = self.measure(&it.label, fs, self.mss.font_weight.unwrap_or(400));
        if let Some(b) = &it.badge {
            w += GAP + self.measure(b, self.desc_size(), 500) + 12.0;
        }
        let mut h = self.line_h();
        if let Some(d) = &it.description {
            w = w.max(self.measure(d, self.desc_size(), 400));
            h += self.desc_size() * 1.3;
        }
        Size::new(RADIO + GAP + w + 4.0, h + ROW_PAD * 2.0)
    }

    fn index_of(&self, value: &str) -> Option<usize> {
        self.items.iter().position(|it| it.value == value)
    }

    fn row_at(&self, pos: Point) -> Option<usize> {
        let o = self.bounds.origin;
        self.rows
            .iter()
            .position(|r| r.shifted(o.x, o.y).contains(pos))
    }

    /// Выбрать вариант `i` пользователем: запомнить и вызвать обработчик.
    fn choose(&mut self, i: usize, ctx: &mut EventContext) -> bool {
        let Some(it) = self.items.get(i) else { return false };
        if it.disabled || it.value == self.selected {
            return false;
        }
        self.selected = it.value.clone();
        let v = self.selected.clone();
        if let Some(cb) = &self.on_change {
            if let Ok(mut cb) = cb.lock() {
                cb(&v);
            }
        }
        ctx.request_paint();
        true
    }

    /// Следующий включённый вариант от текущего в направлении `dir` (по кругу).
    fn step(&self, dir: isize) -> Option<usize> {
        let n = self.items.len() as isize;
        if n == 0 {
            return None;
        }
        let start = self.index_of(&self.selected).map(|i| i as isize).unwrap_or(if dir > 0 { -1 } else { n });
        let mut i = start;
        for _ in 0..n {
            i = (i + dir).rem_euclid(n);
            if !self.items[i as usize].disabled {
                return Some(i as usize);
            }
        }
        None
    }
}

trait Shifted {
    fn shifted(&self, dx: f32, dy: f32) -> Rect;
}

impl Shifted for Rect {
    fn shifted(&self, dx: f32, dy: f32) -> Rect {
        Rect::new(Point::new(self.x() + dx, self.y() + dy), self.size)
    }
}

impl Element for OptionGroupElement {
    fn update(&mut self, widget: &dyn Widget, _ctx: &mut UpdateContext) {
        if let Some(w) = widget.as_any().downcast_ref::<OptionGroup>() {
            if w.items != self.items || w.horizontal != self.horizontal || w.gap != self.gap {
                self.mark_dirty(DirtyFlags::LAYOUT);
            }
            self.items = w.items.clone();
            self.horizontal = w.horizontal;
            self.disabled = w.disabled;
            self.gap = w.gap;
            self.on_change = w.on_change.clone();
            if w.selected != self.prop_selected {
                self.prop_selected = w.selected.clone();
                if let Some(s) = &w.selected {
                    self.selected = s.clone();
                }
            }
            if self.hover.is_some_and(|h| h >= self.items.len()) {
                self.hover = None;
            }
            self.mark_dirty(DirtyFlags::RENDER);
        }
    }

    fn layout(&mut self, constraints: Constraints) -> Size {
        let sizes: Vec<Size> = self.items.iter().map(|it| self.row_size(it)).collect();
        self.rows.clear();
        let (mut w, mut h) = (0.0f32, 0.0f32);
        if self.horizontal {
            let gap = self.gap + 16.0;
            let (mut x, mut y, mut line_h) = (0.0f32, 0.0f32, 0.0f32);
            for s in &sizes {
                if x > 0.0 && x + s.width > constraints.max_width {
                    x = 0.0;
                    y += line_h + self.gap;
                    line_h = 0.0;
                }
                self.rows.push(Rect::new(Point::new(x, y), *s));
                x += s.width + gap;
                line_h = line_h.max(s.height);
                w = w.max(x - gap);
            }
            h = y + line_h;
        } else {
            let full = sizes.iter().map(|s| s.width).fold(0.0f32, f32::max).min(constraints.max_width);
            // по ширине группы: строка вариантов — во всю ширину (щелчок по пустому месту строки)
            let row_w = if constraints.max_width.is_finite() { constraints.max_width.max(full) } else { full };
            for (i, s) in sizes.iter().enumerate() {
                if i > 0 {
                    h += self.gap;
                }
                self.rows.push(Rect::new(Point::new(0.0, h), Size::new(row_w, s.height)));
                h += s.height;
            }
            w = full;
        }
        let size = Size::new(
            w.max(constraints.min_width).min(constraints.max_width),
            h.max(constraints.min_height).min(constraints.max_height),
        );
        if !self.horizontal {
            for r in &mut self.rows {
                r.size.width = size.width;
            }
        }
        self.bounds.size = size;
        size
    }

    fn build_display_list(&self, list: &mut DisplayList, _clip: Rect) {
        let fg = self.mss.color.unwrap_or_else(crate::theme_fallback::fallback_fg);
        let default_border = self.mss.border_color.unwrap_or_else(|| match self.mss.color {
            Some(c) => c.with_alpha(c.a * 0.55),
            None => crate::theme_fallback::fallback_border(),
        });
        let primary = self.mss.accent_color.unwrap_or(Color::from_hex("#3B82F6"));
        let fs = self.font_size();
        let ds = self.desc_size();
        let weight = self.mss.font_weight.unwrap_or(400);
        let lh = self.line_h();
        let o = self.bounds.origin;
        list.push_clip(self.bounds);
        for (i, (it, r)) in self.items.iter().zip(&self.rows).enumerate() {
            let r = r.shifted(o.x, o.y);
            let off = self.disabled || it.disabled;
            let sel = it.value == self.selected;
            let hov = self.hover == Some(i) && !off;
            if hov {
                list.push_rect(r, fg.with_alpha(0.06), [4.0; 4]);
            }
            let cy = r.y() + ROW_PAD + (lh - RADIO) / 2.0;
            let radio = Rect::new(Point::new(r.x() + 2.0, cy), Size::new(RADIO, RADIO));
            if self.focused && sel {
                let ring = Rect::new(Point::new(radio.x() - 3.0, radio.y() - 3.0), Size::new(RADIO + 6.0, RADIO + 6.0));
                list.push_rect_bordered(ring, Color::TRANSPARENT, [RADIO; 4], Border { width: 2.0, color: primary.with_alpha(0.45) });
            }
            let border = if off {
                default_border.with_alpha(default_border.a * 0.5)
            } else if sel || hov {
                primary
            } else {
                default_border
            };
            list.push_rect_bordered(
                radio,
                self.mss.background_color.unwrap_or(Color::TRANSPARENT),
                [RADIO / 2.0; 4],
                Border { width: self.mss.border_width_or(2.0), color: border },
            );
            if sel {
                let d = RADIO * 0.5;
                let dot = Rect::new(Point::new(radio.x() + (RADIO - d) / 2.0, radio.y() + (RADIO - d) / 2.0), Size::new(d, d));
                list.push_rect(dot, if off { fg.with_alpha(0.4) } else { primary }, [d / 2.0; 4]);
            }
            let tx = radio.x() + RADIO + GAP;
            let text_color = if off { fg.with_alpha(0.45) } else { fg };
            let lw = self.measure(&it.label, fs, weight) + 4.0;
            let ty = r.y() + ROW_PAD + (lh - fs * 1.2) / 2.0;
            list.push_text_styled(
                &it.label,
                Rect::new(Point::new(tx, ty), Size::new(lw, fs * 1.25)),
                text_color,
                fs,
                TextAlign::DEFAULT,
                TextDecoration::None,
                weight,
                self.mss.font_family.clone(),
            );
            if let Some(b) = &it.badge {
                let bw = self.measure(b, ds, 500) + 12.0;
                let bx = tx + lw + GAP - 4.0;
                let bh = ds * 1.45;
                let by = r.y() + ROW_PAD + (lh - bh) / 2.0;
                let bc = if off { fg.with_alpha(0.4) } else { primary };
                list.push_rect(Rect::new(Point::new(bx, by), Size::new(bw, bh)), bc.with_alpha(0.14), [bh / 2.0; 4]);
                list.push_text_styled(
                    b,
                    Rect::new(Point::new(bx + 6.0, by + (bh - ds * 1.2) / 2.0), Size::new(bw, ds * 1.25)),
                    bc,
                    ds,
                    TextAlign::DEFAULT,
                    TextDecoration::None,
                    500,
                    self.mss.font_family.clone(),
                );
            }
            if let Some(d) = &it.description {
                let dw = self.measure(d, ds, 400) + 4.0;
                list.push_text_styled(
                    d,
                    Rect::new(Point::new(tx, r.y() + ROW_PAD + lh), Size::new(dw, ds * 1.3)),
                    fg.with_alpha(if off { 0.35 } else { 0.62 }),
                    ds,
                    TextAlign::DEFAULT,
                    TextDecoration::None,
                    400,
                    self.mss.font_family.clone(),
                );
            }
        }
        list.pop_clip();
    }

    fn handle_event(&mut self, event: &Event, ctx: &mut EventContext) -> EventResult {
        if self.disabled {
            return EventResult::Ignored;
        }
        match event {
            Event::MouseMove(pos) => {
                let h = self.row_at(*pos).filter(|i| !self.items[*i].disabled);
                if h.is_some() {
                    ctx.set_cursor(CursorIcon::Pointer);
                }
                if h != self.hover {
                    self.hover = h;
                    ctx.request_paint();
                }
                if h.is_some() { EventResult::Handled } else { EventResult::Ignored }
            }
            Event::MouseDown { button: MouseButton::Left, position } => match self.row_at(*position) {
                Some(i) => {
                    self.choose(i, ctx);
                    EventResult::Handled
                }
                None => EventResult::Ignored,
            },
            Event::KeyDown(k) if self.focused => {
                let next = match k {
                    Key::Down | Key::Right => self.step(1),
                    Key::Up | Key::Left => self.step(-1),
                    Key::Home => self.items.iter().position(|it| !it.disabled),
                    Key::End => self.items.iter().rposition(|it| !it.disabled),
                    Key::Space | Key::Enter => {
                        // ничего не выбрано — выбрать первый доступный
                        if self.index_of(&self.selected).is_none() {
                            self.step(1)
                        } else {
                            return EventResult::Handled;
                        }
                    }
                    _ => return EventResult::Ignored,
                };
                if let Some(i) = next {
                    self.choose(i, ctx);
                }
                EventResult::Handled
            }
            Event::FocusGained => {
                self.focused = true;
                ctx.request_paint();
                EventResult::Handled
            }
            Event::FocusLost => {
                self.focused = false;
                ctx.request_paint();
                EventResult::Handled
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
        "OptionGroup"
    }

    fn reset_mss_styles(&mut self) {
        self.mss.reset();
    }

    fn mss(&self) -> Option<&MssFields> {
        Some(&self.mss)
    }

    fn apply_computed_style(&mut self, style: &ComputedStyle) {
        self.mss.apply(style);
        if let Some(f) = style.get("font-family").and_then(|v| v.as_string().map(|s| s.to_string())) {
            self.mss.font_family = Some(f);
        }
        self.mark_dirty(DirtyFlags::LAYOUT | DirtyFlags::RENDER);
    }

    fn accessibility_info(&self) -> Option<crate::a11y::AccessibilityInfo> {
        let label = self
            .items
            .iter()
            .find(|it| it.value == self.selected)
            .map(|it| it.label.clone());
        Some(crate::a11y::AccessibilityInfo {
            role: crate::a11y::Role::RadioGroup,
            state: crate::a11y::NodeState {
                disabled: self.disabled,
                focused: self.focused,
                ..Default::default()
            },
            properties: crate::a11y::NodeProperties {
                label,
                ..Default::default()
            },
        })
    }
}

impl StyledElement for OptionGroupElement {
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

    fn el(items: Vec<OptionItem>, sel: &str) -> OptionGroupElement {
        OptionGroup::new(items).selected(sel).element()
    }

    #[test]
    fn step_skips_disabled_and_wraps() {
        let e = el(
            vec![
                OptionItem::new("a", "A"),
                OptionItem::new("b", "B").disabled(true),
                OptionItem::new("c", "C"),
            ],
            "a",
        );
        assert_eq!(e.step(1), Some(2));
        assert_eq!(e.step(-1), Some(2));
    }

    #[test]
    fn vertical_layout_rows() {
        let mut e = el(
            vec![OptionItem::new("a", "A").description("пояснение"), OptionItem::new("b", "B")],
            "b",
        );
        let s = e.layout(Constraints::loose(Size::new(300.0, 500.0)));
        assert_eq!(e.rows.len(), 2);
        assert!(e.rows[0].size.height > e.rows[1].size.height, "у варианта с пояснением строка выше");
        assert!((e.rows[1].y() - (e.rows[0].size.height + e.gap)).abs() < 0.01);
        assert!(s.height >= e.rows[1].y() + e.rows[1].size.height - 0.01);
    }
}
