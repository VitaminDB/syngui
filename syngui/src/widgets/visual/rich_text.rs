use crate::core::{Color, Point, Rect, RectExt, Size};
use crate::layout::Constraints;
use crate::mss::{ComputedStyle, Dimension};
use crate::mss::MssFields;
use crate::render::DisplayList;
use crate::mss::{TextAlign, TextDecoration};
use crate::widget::{DirtyFlags, Element, ElementId, ElementTree, StyledElement, UpdateContext, Widget};
use crate::widget::context::TextMeasure;
use std::any::Any;
use std::sync::Arc;

#[derive(Clone, Debug)]
pub struct TextSpan {
    pub text: String,
    pub color: Option<Color>,
    pub font_size: Option<f32>,
    pub bold: bool,
    pub italic: bool,
    pub underline: bool,
}

impl TextSpan {
    pub fn new(text: impl Into<String>) -> Self {
        Self {
            text: text.into(),
            color: None,
            font_size: None,
            bold: false,
            italic: false,
            underline: false,
        }
    }

    pub fn color(mut self, c: Color) -> Self { self.color = Some(c); self }
    pub fn font_size(mut self, s: f32) -> Self { self.font_size = Some(s); self }
    pub fn bold(mut self) -> Self { self.bold = true; self }
    pub fn italic(mut self) -> Self { self.italic = true; self }
    pub fn underline(mut self) -> Self { self.underline = true; self }
}

pub struct RichText {
    spans: Vec<TextSpan>,
    default_color: Color,
    default_font_size: f32,
    line_height: f32,
    wrap: bool,
    max_width: Option<Dimension>,
}

impl RichText {
    pub fn new() -> Self {
        Self {
            spans: Vec::new(),
            default_color: Color::from_hex("#1F2937"),
            default_font_size: 14.0,
            line_height: 1.4,
            wrap: true,
            max_width: None,
        }
    }

    pub fn span(mut self, text: impl Into<String>, style_fn: impl FnOnce(TextSpan) -> TextSpan) -> Self {
        let span = TextSpan::new(text);
        self.spans.push(style_fn(span));
        self
    }

    pub fn text(mut self, text: impl Into<String>) -> Self {
        self.spans.push(TextSpan::new(text));
        self
    }

    pub fn default_color(mut self, c: Color) -> Self { self.default_color = c; self }
    pub fn default_font_size(mut self, s: f32) -> Self { self.default_font_size = s; self }
    pub fn line_height(mut self, h: f32) -> Self { self.line_height = h; self }
    pub fn wrap(mut self, w: bool) -> Self { self.wrap = w; self }
    pub fn max_width(mut self, w: f32) -> Self { self.max_width = Some(Dimension::Px(w)); self }
}

impl Default for RichText {
    fn default() -> Self { Self::new() }
}

impl Widget for RichText {
    fn create_element(&self) -> Box<dyn Element> {
        Box::new(RichTextElement {
            id: ElementId::new(),
            spans: self.spans.clone(),
            default_color: self.default_color,
            default_font_size: self.default_font_size,
            line_height: self.line_height,
            wrap: self.wrap,
            max_width: self.max_width,
            bounds: Rect::zero(),
            classes: Vec::new(),
            dirty_flags: DirtyFlags::LAYOUT | DirtyFlags::RENDER,
            mss: MssFields::new(),
            text_measure: None,
        })
    }

    fn can_update(&self, other: &dyn Any) -> bool { other.is::<Self>() }
    fn as_any(&self) -> &dyn Any { self }
    fn as_any_mut(&mut self) -> &mut dyn Any { self }
    fn mount(&self, _tree: &mut ElementTree, _parent_id: ElementId) {}
}

const CHAR_WIDTH_FACTOR: f32 = 0.6;

pub struct RichTextElement {
    id: ElementId,
    spans: Vec<TextSpan>,
    default_color: Color,
    default_font_size: f32,
    line_height: f32,
    wrap: bool,
    max_width: Option<Dimension>,
    bounds: Rect,
    classes: Vec<String>,
    dirty_flags: DirtyFlags,
    mss: MssFields,
    text_measure: Option<Arc<dyn TextMeasure>>,
}

impl RichTextElement {
    fn span_width(&self, span: &TextSpan, default_size: f32) -> f32 {
        let fs = span.font_size.unwrap_or(default_size);
        self.text_measure.as_ref()
            .map(|tm| tm.measure_text_width_styled(&span.text, fs, span.text.chars().count(), span.bold, None))
            .unwrap_or_else(|| {
                let factor = if span.bold { CHAR_WIDTH_FACTOR * 1.1 } else { CHAR_WIDTH_FACTOR };
                span.text.chars().count() as f32 * fs * factor
            })
    }

    fn compute_lines(&self, max_w: f32) -> Vec<Vec<(usize, usize, usize)>> {
        if self.spans.is_empty() { return vec![]; }

        let mut lines: Vec<Vec<usize>> = vec![vec![]];
        let mut current_x = 0.0f32;

        for (i, span) in self.spans.iter().enumerate() {
            let sw = self.span_width(span, self.default_font_size);

            if self.wrap && current_x + sw > max_w && current_x > 0.0 {
                lines.push(vec![]);
                current_x = 0.0;
            }

            lines.last_mut().unwrap().push(i);
            current_x += sw;
        }

        lines.into_iter().map(|line| {
            line.into_iter().map(|i| (i, 0, self.spans[i].text.len())).collect()
        }).collect()
    }

    fn line_max_font_size(&self, span_indices: &[(usize, usize, usize)]) -> f32 {
        span_indices.iter()
            .map(|(i, _, _)| self.spans[*i].font_size.unwrap_or(self.default_font_size))
            .fold(0.0f32, f32::max)
    }
}

impl Element for RichTextElement {
    fn update(&mut self, widget: &dyn Widget, _ctx: &mut UpdateContext) {
        if let Some(rt) = widget.as_any().downcast_ref::<RichText>() {
            self.spans = rt.spans.clone();
            self.default_color = rt.default_color;
            self.default_font_size = rt.default_font_size;
            self.line_height = rt.line_height;
            self.wrap = rt.wrap;
            self.max_width = rt.max_width;
            self.mark_dirty(DirtyFlags::LAYOUT | DirtyFlags::RENDER);
        }
    }

    fn layout(&mut self, constraints: Constraints) -> Size {
        let max_w = self.max_width.map(|d| d.resolve(constraints.max_width)).unwrap_or(constraints.max_width).min(constraints.max_width);
        let lines = self.compute_lines(max_w);

        let mut total_height = 0.0f32;
        let mut max_line_width = 0.0f32;

        for line in &lines {
            let line_fs = self.line_max_font_size(line);
            let line_h = line_fs * self.line_height;
            total_height += line_h;

            let line_w: f32 = line.iter()
                .map(|(i, _, _)| self.span_width(&self.spans[*i], self.default_font_size))
                .sum();
            max_line_width = max_line_width.max(line_w);
        }

        if total_height == 0.0 {
            total_height = self.default_font_size * self.line_height;
        }

        let w = if self.wrap { max_w } else { max_line_width }.min(constraints.max_width);
        let h = total_height.min(constraints.max_height);

        self.bounds = Rect::new(Point::zero(), Size::new(w, h));
        Size::new(w, h)
    }

    fn build_display_list(&self, list: &mut DisplayList, _clip: Rect) {
        if self.spans.is_empty() { return; }

        let effective_default = self.mss.color.unwrap_or(self.default_color);
        let max_w = self.bounds.size.width;
        let lines = self.compute_lines(max_w);
        let mut y = self.bounds.y();

        for line in &lines {
            let line_fs = self.line_max_font_size(line);
            let line_h = line_fs * self.line_height;
            let mut x = self.bounds.x();

            for &(span_idx, _, _) in line {
                let span = &self.spans[span_idx];
                let fs = span.font_size.unwrap_or(self.default_font_size);
                let color = span.color.unwrap_or(effective_default);
                let sw = self.span_width(span, self.default_font_size);

                let text_y = y + (line_h - fs) / 2.0;
                let font_weight: u16 = if span.bold { 700 } else { 400 };
                let baseline_y = y + (line_h - fs) / 2.0;
                let rect = Rect::new(Point::new(x, baseline_y), Size::new(sw, 0.0));
                list.push_text_aligned(&span.text, rect, color, fs, TextAlign::DEFAULT, TextDecoration::None, font_weight);

                if span.underline {
                    let underline_y = text_y + fs + 1.0;
                    let underline_rect = Rect::new(
                        Point::new(x, underline_y),
                        Size::new(sw, 1.0),
                    );
                    list.push_rect(underline_rect, color, [0.0; 4]);
                }

                x += sw;
            }

            y += line_h;
        }
    }

    fn handle_event(&mut self, _event: &crate::input::Event, _ctx: &mut crate::widget::context::EventContext) -> crate::input::EventResult {
        crate::input::EventResult::Ignored
    }

    fn children(&self) -> &[ElementId] { &[] }
    fn bounds(&self) -> Rect { self.bounds }
    fn set_position(&mut self, pos: Point) { self.bounds.origin = pos; }
    fn mark_dirty(&mut self, flags: DirtyFlags) { self.dirty_flags |= flags; }
    fn clear_dirty(&mut self, flags: DirtyFlags) { self.dirty_flags.remove(flags); }
    fn is_dirty(&self, flags: DirtyFlags) -> bool { self.dirty_flags.contains(flags) }
    fn id(&self) -> ElementId { self.id }
    fn set_id(&mut self, id: ElementId) { self.id = id; }
    fn mount(&mut self, tree: &mut ElementTree) {
        self.text_measure = tree.text_measure.clone();
    }

    fn set_classes(&mut self, classes: Vec<String>) {
        self.classes = classes;
        self.mark_dirty(DirtyFlags::RENDER);
    }

    fn get_classes(&self) -> &[String] { &self.classes }

    fn element_type_name(&self) -> &str { "RichText" }

    fn reset_mss_styles(&mut self) { self.mss.reset(); }
    fn mss(&self) -> Option<&crate::mss::MssFields> { Some(&self.mss) }
    fn apply_computed_style(&mut self, style: &ComputedStyle) {
        self.mss.apply(style);
        if let Some(d) = self.mss.width { self.max_width = Some(d); }
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

impl StyledElement for RichTextElement {
    fn apply_style(&mut self, _style: &ComputedStyle) {
        self.mark_dirty(DirtyFlags::LAYOUT | DirtyFlags::RENDER);
    }

    fn classes(&self) -> &[String] { &self.classes }

    fn set_classes(&mut self, classes: Vec<String>) {
        self.classes = classes;
        self.mark_dirty(DirtyFlags::RENDER);
    }
}
