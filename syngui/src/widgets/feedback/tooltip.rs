use crate::core::{Color, Point, Rect, RectExt, Size};
use crate::input::{Event, EventResult};
use crate::layout::Constraints;
use crate::mss::ComputedStyle;
use crate::mss::MssFields;
use crate::render::DisplayList;
use crate::widget::context::TextMeasure;
use crate::widget::context::{EventContext, EventContextExt};
use crate::widget::{
    DirtyFlags, Element, ElementId, ElementTree, LayoutHint, StyledElement, UpdateContext, Widget,
};
use crate::Border;
use std::any::Any;
use std::cell::Cell;
use std::sync::Arc;
use std::time::Duration;

const DEFAULT_PAD: f32 = 8.0;

#[derive(Clone, Copy, Debug, Default, PartialEq)]
pub enum TooltipPosition {
    #[default]
    Below,
    Above,
    Left,
    Right,
}

impl TooltipPosition {
    fn to_u8(self) -> u8 {
        match self {
            Self::Below => 0,
            Self::Above => 1,
            Self::Left => 2,
            Self::Right => 3,
        }
    }
}

enum TooltipContent {
    Text(String),
    Rich(Box<dyn Widget>),
}

pub struct Tooltip {
    child: Box<dyn Widget>,
    content: TooltipContent,
    position: TooltipPosition,
    delay_ms: u32,
    max_width: f32,
}

impl Tooltip {
    pub fn new(child: impl Widget + 'static, text: impl Into<String>) -> Self {
        Self {
            child: Box::new(child),
            content: TooltipContent::Text(text.into()),
            position: TooltipPosition::default(),
            delay_ms: 500,
            max_width: 300.0,
        }
    }

    pub fn rich(child: impl Widget + 'static, content: impl Widget + 'static) -> Self {
        Self {
            child: Box::new(child),
            content: TooltipContent::Rich(Box::new(content)),
            position: TooltipPosition::default(),
            delay_ms: 500,
            max_width: 300.0,
        }
    }

    pub fn position(mut self, pos: TooltipPosition) -> Self {
        self.position = pos;
        self
    }

    pub fn delay_ms(mut self, ms: u32) -> Self {
        self.delay_ms = ms;
        self
    }

    pub fn max_width(mut self, w: f32) -> Self {
        self.max_width = w;
        self
    }
}

impl Widget for Tooltip {
    fn create_element(&self) -> Box<dyn Element> {
        let is_rich = matches!(&self.content, TooltipContent::Rich(_));
        let text = match &self.content {
            TooltipContent::Text(s) => s.clone(),
            TooltipContent::Rich(_) => String::new(),
        };
        Box::new(TooltipElement {
            id: ElementId::new(),
            text,
            is_rich,
            position: self.position,
            delay_ms: self.delay_ms,
            max_width: self.max_width,
            hovered: false,
            hover_elapsed: Duration::ZERO,
            visible: false,
            child_ids: Vec::new(),
            bounds: Rect::zero(),
            content_size: Cell::new(Size::zero()),
            classes: Vec::new(),
            dirty_flags: DirtyFlags::LAYOUT | DirtyFlags::RENDER,
            mss: MssFields::new(),
            text_measure: None,
            overlay_registered: false,
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
        let child_element = self.child.create_element();
        let child_id = tree.insert_with_type_id(
            child_element,
            Some(parent_id),
            self.child.as_any().type_id(),
        );
        self.child.mount(tree, child_id);

        if let TooltipContent::Rich(ref content) = self.content {
            let content_element = content.create_element();
            let content_id = tree.insert_with_type_id(
                content_element,
                Some(parent_id),
                content.as_any().type_id(),
            );
            content.mount(tree, content_id);
        }
    }

    fn child_widgets(&self) -> Vec<&dyn Widget> {
        let mut result = vec![self.child.as_ref() as &dyn Widget];
        if let TooltipContent::Rich(ref content) = self.content {
            result.push(content.as_ref() as &dyn Widget);
        }
        result
    }
}

pub struct TooltipElement {
    id: ElementId,
    text: String,
    is_rich: bool,
    position: TooltipPosition,
    delay_ms: u32,
    max_width: f32,
    hovered: bool,
    hover_elapsed: Duration,
    visible: bool,
    child_ids: Vec<ElementId>,
    bounds: Rect,
    content_size: Cell<Size>,
    classes: Vec<String>,
    dirty_flags: DirtyFlags,
    mss: MssFields,
    text_measure: Option<Arc<dyn TextMeasure>>,
    overlay_registered: bool,
}

impl TooltipElement {
    fn text_width(&self, s: &str, font_size: f32) -> f32 {
        self.text_measure
            .as_ref()
            .map(|tm| tm.measure_text_width(s, font_size, s.chars().count()))
            .unwrap_or_else(|| s.chars().count() as f32 * font_size * 0.6)
    }

    /// Отступы текста простой подсказки: из MSS, иначе 10 по бокам и 6 сверху/снизу.
    fn text_pads(&self) -> (f32, f32, f32, f32) {
        (
            self.mss.padding_left.unwrap_or(10.0),
            self.mss.padding_top.unwrap_or(6.0),
            self.mss.padding_right.unwrap_or(10.0),
            self.mss.padding_bottom.unwrap_or(6.0),
        )
    }

    /// Строки простой подсказки с переносом по словам в ширину `max_width`
    /// за вычетом отступов. Раньше текст делился только по переводам строк: длинная
    /// фраза переносилась уже при отрисовке, вылезала из пузыря, отступы
    /// пропадали. Возвращает строки и ширину самой длинной.
    fn wrapped_lines(&self, font_size: f32) -> (Vec<String>, f32) {
        let (pl, _, pr, _) = self.text_pads();
        let max_w = self
            .mss
            .max_width
            .map(|d| d.resolve(self.bounds.size.width))
            .unwrap_or(self.max_width);
        let avail = (max_w - pl - pr).max(40.0);
        let mut out: Vec<String> = Vec::new();
        for para in self.text.lines() {
            let mut cur = String::new();
            for word in para.split(' ') {
                let cand = if cur.is_empty() { word.to_string() } else { format!("{cur} {word}") };
                if !cur.is_empty() && self.text_width(&cand, font_size) > avail {
                    out.push(std::mem::take(&mut cur));
                    cur = word.to_string();
                } else {
                    cur = cand;
                }
                // слово шире строки (путь файла без пробелов) — делится по разделителям `/ \ _ . -`, иначе по
                // символам: раньше оно оставалось целым, пузырь обрезался до max_width, и текст переносила уже
                // отрисовка посреди слова («Shape|Lab»), упираясь в рамку — отступов не было видно
                while self.text_width(&cur, font_size) > avail {
                    let chars: Vec<char> = cur.chars().collect();
                    let mut fit = 0;
                    while fit < chars.len() && self.text_width(&chars[..=fit].iter().collect::<String>(), font_size) <= avail {
                        fit += 1;
                    }
                    let fit = fit.max(1);
                    let cut = (1..fit).rev().find(|&i| matches!(chars[i - 1], '/' | '\\' | '_' | '.' | '-')).unwrap_or(fit);
                    out.push(chars[..cut].iter().collect());
                    cur = chars[cut..].iter().collect();
                }
            }
            out.push(cur);
        }
        if out.is_empty() {
            out.push(String::new());
        }
        let widest = out.iter().map(|l| self.text_width(l, font_size)).fold(0.0f32, f32::max).min(avail);
        (out, widest)
    }

    fn text_tooltip_rect(&self) -> Rect {
        let font_size = self.mss.font_size_or(12.0);
        let (pl, pt, pr, pb) = self.text_pads();
        let line_height = font_size + 4.0;
        let (lines, widest) = self.wrapped_lines(font_size);
        let tooltip_w = widest.ceil() + pl + pr;
        let mut tooltip_h = lines.len() as f32 * line_height + pt + pb;

        if let Some(mh) = self.mss.max_height {
            let resolved = mh.resolve(self.bounds.size.height);
            tooltip_h = tooltip_h.min(resolved);
        }

        let gap = 6.0;
        let (x, y) = match self.position {
            TooltipPosition::Below => (
                self.bounds.x() + (self.bounds.size.width - tooltip_w) / 2.0,
                self.bounds.y() + self.bounds.size.height + gap,
            ),
            TooltipPosition::Above => (
                self.bounds.x() + (self.bounds.size.width - tooltip_w) / 2.0,
                self.bounds.y() - tooltip_h - gap,
            ),
            TooltipPosition::Right => (
                self.bounds.x() + self.bounds.size.width + gap,
                self.bounds.y() + (self.bounds.size.height - tooltip_h) / 2.0,
            ),
            TooltipPosition::Left => (
                self.bounds.x() - tooltip_w - gap,
                self.bounds.y() + (self.bounds.size.height - tooltip_h) / 2.0,
            ),
        };

        Rect::new(Point::new(x, y), Size::new(tooltip_w, tooltip_h))
    }

    fn rich_tooltip_rect(&self) -> Rect {
        let content = self.content_size.get();
        let pad_l = self.mss.padding_left.unwrap_or(DEFAULT_PAD);
        let pad_r = self.mss.padding_right.unwrap_or(DEFAULT_PAD);
        let pad_t = self.mss.padding_top.unwrap_or(DEFAULT_PAD);
        let pad_b = self.mss.padding_bottom.unwrap_or(DEFAULT_PAD);

        let max_w = self
            .mss
            .max_width
            .map(|d| d.resolve(self.bounds.size.width))
            .unwrap_or(self.max_width);
        let max_h = self
            .mss
            .max_height
            .map(|d| d.resolve(self.bounds.size.height))
            .unwrap_or(300.0);

        let tooltip_w = (content.width + pad_l + pad_r).min(max_w);
        let tooltip_h = (content.height + pad_t + pad_b).min(max_h);

        let gap = 6.0;
        let (x, y) = match self.position {
            TooltipPosition::Below => (
                self.bounds.x(),
                self.bounds.y() + self.bounds.size.height + gap,
            ),
            TooltipPosition::Above => (self.bounds.x(), self.bounds.y() - tooltip_h - gap),
            TooltipPosition::Right => (
                self.bounds.x() + self.bounds.size.width + gap,
                self.bounds.y(),
            ),
            TooltipPosition::Left => (self.bounds.x() - tooltip_w - gap, self.bounds.y()),
        };

        Rect::new(Point::new(x, y), Size::new(tooltip_w, tooltip_h))
    }
}

impl Element for TooltipElement {
    fn update(&mut self, widget: &dyn Widget, _ctx: &mut UpdateContext) {
        if let Some(t) = widget.as_any().downcast_ref::<Tooltip>() {
            self.text = match &t.content {
                TooltipContent::Text(s) => s.clone(),
                TooltipContent::Rich(_) => String::new(),
            };
            self.is_rich = matches!(&t.content, TooltipContent::Rich(_));
            self.position = t.position;
            self.delay_ms = t.delay_ms;
            self.max_width = t.max_width;
            self.mark_dirty(DirtyFlags::RENDER);
        }
    }

    fn layout(&mut self, constraints: Constraints) -> Size {
        let w = if constraints.max_width.is_finite() {
            constraints.max_width
        } else {
            constraints.min_width.max(40.0)
        };
        let h = if constraints.max_height.is_finite() {
            constraints.max_height
        } else {
            constraints.min_height.max(40.0)
        };
        self.bounds = Rect::new(Point::zero(), Size::new(w, h));
        Size::new(w, h)
    }

    fn layout_hint(&self) -> LayoutHint {
        if self.is_rich {
            LayoutHint::Tooltip {
                position: self.position.to_u8(),
                gap: 6.0,
                padding_l: self.mss.padding_left.unwrap_or(DEFAULT_PAD),
                padding_t: self.mss.padding_top.unwrap_or(DEFAULT_PAD),
                padding_r: self.mss.padding_right.unwrap_or(DEFAULT_PAD),
                padding_b: self.mss.padding_bottom.unwrap_or(DEFAULT_PAD),
            }
        } else {
            LayoutHint::Padding {
                left: 0.0,
                top: 0.0,
                right: 0.0,
                bottom: 0.0,
            }
        }
    }

    fn is_relayout_boundary(&self) -> bool {
        self.is_rich
    }

    fn set_content_size(&mut self, size: Size) {
        self.content_size.set(size);
    }

    fn explicit_dimensions(
        &self,
        _parent_width: f32,
        _parent_height: f32,
    ) -> (Option<f32>, Option<f32>) {
        if !self.is_rich {
            return (None, None);
        }
        let w = self
            .mss
            .max_width
            .map(|d| d.resolve(self.bounds.size.width))
            .or(Some(self.max_width));
        let h = self
            .mss
            .max_height
            .map(|d| d.resolve(self.bounds.size.height));
        (w, h)
    }

    fn build_display_list(&self, list: &mut DisplayList, _clip: Rect) {
        if self.is_rich {
            if !self.visible {
                return;
            }
            list.begin_overlay();

            let tip = self.rich_tooltip_rect();
            let bg = self
                .mss
                .background_color
                .unwrap_or(Color::from_hex("#1E1F22"));
            let radius = self
                .mss
                .border_radius_uniform(tip.size.width.min(tip.size.height), 10.0);

            if let Some(ref shadows) = self.mss.box_shadow {
                for sh in &shadows.0 {
                    if !sh.inset {
                        list.push_shadow(
                            tip,
                            sh.color,
                            sh.blur_radius,
                            (sh.offset_x, sh.offset_y),
                            [radius; 4],
                        );
                    }
                }
            }
            if let Some(bc) = self.mss.border_color {
                let bw = self.mss.border_width.unwrap_or(1.0);
                list.push_rect_bordered(tip, bg, [radius; 4], Border::new(bw, bc));
            } else {
                list.push_rect_bordered(
                    tip,
                    bg,
                    [radius; 4],
                    Border::new(1.0, Color::from_hex("#3F4147")),
                );
            }
            list.push_clip(tip);
        }
    }

    fn post_build_display_list(&self, list: &mut DisplayList, _clip: Rect) {
        if self.is_rich {
            if self.visible {
                list.pop_clip();
                list.end_overlay();
            }
            return;
        }

        if !self.visible {
            return;
        }

        let tip = self.text_tooltip_rect();
        let bg = self
            .mss
            .background_color
            .unwrap_or(Color::from_hex("#1E1F22"));
        let text_color = self.mss.color.unwrap_or(Color::WHITE);
        let font_size = self.mss.font_size_or(12.0);
        let font_weight = self.mss.font_weight_or(400);
        let radius = self
            .mss
            .border_radius_uniform(tip.size.width.min(tip.size.height), 10.0);
        let (pad_l, pad_t, pad_r, _) = self.text_pads();
        let line_height = font_size + 4.0;

        list.begin_overlay();
        if let Some(ref shadows) = self.mss.box_shadow {
            for sh in &shadows.0 {
                if !sh.inset {
                    list.push_shadow(
                        tip,
                        sh.color,
                        sh.blur_radius,
                        (sh.offset_x, sh.offset_y),
                        [radius; 4],
                    );
                }
            }
        }
        if let Some(bc) = self.mss.border_color {
            let bw = self.mss.border_width.unwrap_or(1.0);
            list.push_rect_bordered(tip, bg, [radius; 4], Border::new(bw, bc));
        } else {
            list.push_rect_bordered(
                tip,
                bg,
                [radius; 4],
                Border::new(1.0, Color::from_hex("#3F4147")),
            );
        }
        let (lines, _) = self.wrapped_lines(font_size);
        let max_h = self
            .mss
            .max_height
            .map(|d| d.resolve(self.bounds.size.height))
            .unwrap_or(f32::INFINITY);
        for (i, line) in lines.iter().enumerate() {
            let y = tip.y() + pad_t + i as f32 * line_height;
            if y + line_height > tip.y() + max_h {
                break;
            }
            let text_rect = Rect::new(
                Point::new(tip.x() + pad_l, y),
                Size::new(tip.size.width - pad_l - pad_r + 2.0, line_height),
            );
            list.push_text_styled(
                line,
                text_rect,
                text_color,
                font_size,
                crate::mss::TextAlign::DEFAULT,
                crate::mss::TextDecoration::None,
                font_weight,
                self.mss.font_family.clone(),
            );
        }
        list.end_overlay();
    }

    /// Тик отсчитывает задержку до показа подсказки под курсором.
    fn wants_animate_tick(&self) -> bool {
        self.hovered && !self.visible
    }

    fn animate(&mut self, dt: Duration) -> bool {
        if self.hovered && !self.visible {
            self.hover_elapsed += dt;
            if self.hover_elapsed >= Duration::from_millis(self.delay_ms as u64) {
                self.visible = true;
                self.mark_dirty(DirtyFlags::LAYOUT);
                return true;
            }
            return true;
        }
        false
    }

    fn active_child_count(&self) -> usize {
        if self.is_rich && !self.visible {
            1
        } else {
            usize::MAX
        }
    }

    fn handle_event(&mut self, event: &Event, ctx: &mut EventContext) -> EventResult {
        match event {
            Event::MouseMove(pos) => {
                let now_hovered = self.bounds.contains(*pos);
                if now_hovered != self.hovered {
                    self.hovered = now_hovered;
                    if !now_hovered {
                        self.visible = false;
                        self.hover_elapsed = Duration::ZERO;
                        if self.overlay_registered {
                            ctx.unregister_overlay();
                            self.overlay_registered = false;
                        }
                    }
                    ctx.request_paint();
                }
                if self.is_rich && self.visible && !self.overlay_registered {
                    let tip = self.rich_tooltip_rect();
                    ctx.register_overlay(tip, false);
                    self.overlay_registered = true;
                }
                EventResult::Ignored
            }
            _ => EventResult::Ignored,
        }
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
    fn mount(&mut self, tree: &mut ElementTree) {
        self.text_measure = tree.text_measure.clone();
    }

    fn clip_content(&self) -> bool {
        false
    }

    fn set_classes(&mut self, classes: Vec<String>) {
        self.classes = classes;
        self.mark_dirty(DirtyFlags::RENDER);
    }

    fn get_classes(&self) -> &[String] {
        &self.classes
    }

    fn element_type_name(&self) -> &str {
        "Tooltip"
    }

    fn reset_mss_styles(&mut self) {
        self.mss.reset();
    }
    fn mss(&self) -> Option<&crate::mss::MssFields> {
        Some(&self.mss)
    }
    fn mss_mut(&mut self) -> Option<&mut crate::mss::MssFields> {
        Some(&mut self.mss)
    }
    fn apply_computed_style(&mut self, style: &ComputedStyle) {
        self.mss.apply(style);
        self.mark_dirty(DirtyFlags::RENDER);
    }

    fn accessibility_info(&self) -> Option<crate::a11y::AccessibilityInfo> {
        Some(crate::a11y::AccessibilityInfo {
            role: crate::a11y::Role::StaticText,
            state: crate::a11y::NodeState {
                hidden: !self.visible,
                ..Default::default()
            },
            properties: crate::a11y::NodeProperties {
                label: Some(self.text.clone()),
                ..Default::default()
            },
        })
    }
}

impl StyledElement for TooltipElement {
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

    fn element(text: &str) -> TooltipElement {
        TooltipElement {
            id: ElementId::new(),
            text: text.to_string(),
            is_rich: false,
            position: TooltipPosition::Below,
            delay_ms: 0,
            max_width: 300.0,
            hovered: false,
            hover_elapsed: Duration::ZERO,
            visible: true,
            child_ids: Vec::new(),
            bounds: Rect::new(Point::new(100.0, 100.0), Size::new(30.0, 30.0)),
            content_size: Cell::new(Size::zero()),
            classes: Vec::new(),
            dirty_flags: DirtyFlags::empty(),
            mss: MssFields::new(),
            text_measure: None,
            overlay_registered: false,
        }
    }

    #[test]
    fn long_text_wraps_inside_bubble() {
        let e = element("На свой рабочий стол: машина займёт свободный рабочий стол целиком");
        let (lines, widest) = e.wrapped_lines(12.0);
        assert!(lines.len() >= 2, "{lines:?}");
        // Каждая строка помещается в ширину без отступов.
        for l in &lines {
            assert!(e.text_width(l, 12.0) <= 300.0 - 20.0 + 0.01, "{l}");
        }
        let r = e.text_tooltip_rect();
        assert!((r.size.width - (widest.ceil() + 20.0)).abs() < 0.01);
        // Высота — на все строки и отступы сверху и снизу.
        assert!((r.size.height - (lines.len() as f32 * 16.0 + 12.0)).abs() < 0.01);
        // Слова не теряются.
        assert_eq!(lines.join(" "), "На свой рабочий стол: машина займёт свободный рабочий стол целиком");
    }

    /// Путь файла без пробелов шире пузыря — делится по «/», каждая строка помещается с отступами, символы
    /// не теряются (раньше оставался одним словом и переносился отрисовкой посреди слова — «Shape|Lab»).
    #[test]
    fn long_path_breaks_at_separators() {
        let p = "~/Storage/Проекты/2027/hexspire/test_re/ShapeLab/ShapeLab.exe";
        let e = element(p);
        let (lines, _) = e.wrapped_lines(12.0);
        assert!(lines.len() >= 2, "{lines:?}");
        for l in &lines {
            assert!(e.text_width(l, 12.0) <= 300.0 - 20.0 + 0.01, "{l}");
        }
        // перенос — после разделителя
        for l in &lines[..lines.len() - 1] {
            assert!(l.ends_with('/') || l.ends_with('_') || l.ends_with('.'), "{lines:?}");
        }
        assert_eq!(lines.concat(), p);
    }

    #[test]
    fn short_text_one_line_and_newlines_kept() {
        let e = element("Пауза");
        assert_eq!(e.wrapped_lines(12.0).0, vec!["Пауза".to_string()]);
        let e = element("Строка один\nСтрока два");
        assert_eq!(e.wrapped_lines(12.0).0.len(), 2);
    }
}
