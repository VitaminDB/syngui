use crate::core::{Color, Point, Rect, RectExt, Size};
use crate::input::{Event, EventResult, MouseButton};
use crate::layout::Constraints;
use crate::mss::{ComputedStyle, TextAlign, TextDecoration};
use crate::mss::MssFields;
use crate::render::display_list::Border;
use crate::render::DisplayList;
use crate::widget::context::{EventContext, EventContextExt};
use crate::widget::{DirtyFlags, Element, ElementId, ElementTree, StyledElement, UpdateContext, Widget};
use std::any::Any;
use std::sync::Arc;
use crate::core::sync::Mutex;

#[derive(Clone, Debug)]
pub struct StepInfo {
    pub label: String,
    pub support_text: Option<String>,
    pub icon: Option<String>,
    pub status_text: Option<String>,
}

#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum StepState {
    Completed,
    Active,
    Pending,
    Disabled,
}

#[derive(Clone, Copy, PartialEq, Eq, Debug)]
enum StepperVariant {
    Pill,
    Radio,
    Numbered,
    Icon,
    Status,
}

pub struct Stepper {
    pub steps: Vec<StepInfo>,
    pub current_step: usize,
    pub on_step_click: Option<Arc<Mutex<dyn FnMut(usize) + Send>>>,
    pub allow_click_navigation: bool,
    pub disabled_steps: Vec<usize>,
}

impl Stepper {
    pub fn new() -> Self {
        Self {
            steps: Vec::new(),
            current_step: 0,
            on_step_click: None,
            allow_click_navigation: false,
            disabled_steps: Vec::new(),
        }
    }

    pub fn step(mut self, label: impl Into<String>, support: Option<&str>) -> Self {
        self.steps.push(StepInfo {
            label: label.into(),
            support_text: support.map(|s| s.to_string()),
            icon: None,
            status_text: None,
        });
        self
    }

    pub fn step_with_icon(mut self, label: impl Into<String>, icon: impl Into<String>, support: Option<&str>) -> Self {
        self.steps.push(StepInfo {
            label: label.into(),
            support_text: support.map(|s| s.to_string()),
            icon: Some(icon.into()),
            status_text: None,
        });
        self
    }

    pub fn step_with_status(mut self, label: impl Into<String>, title: impl Into<String>, status: impl Into<String>) -> Self {
        self.steps.push(StepInfo {
            label: title.into(),
            support_text: Some(label.into()),
            icon: None,
            status_text: Some(status.into()),
        });
        self
    }

    pub fn current(mut self, index: usize) -> Self {
        self.current_step = index;
        self
    }

    pub fn on_step_click(mut self, callback: impl FnMut(usize) + Send + 'static) -> Self {
        self.on_step_click = Some(Arc::new(Mutex::new(callback)));
        self
    }

    pub fn allow_navigation(mut self, allow: bool) -> Self {
        self.allow_click_navigation = allow;
        self
    }

    pub fn disable_step(mut self, index: usize) -> Self {
        self.disabled_steps.push(index);
        self
    }
}

impl Default for Stepper {
    fn default() -> Self {
        Self::new()
    }
}

impl Widget for Stepper {
    fn create_element(&self) -> Box<dyn Element> {
        Box::new(StepperElement {
            id: ElementId::new(),
            bounds: Rect::zero(),
            steps: self.steps.clone(),
            current_step: self.current_step,
            disabled_steps: self.disabled_steps.clone(),
            allow_click_navigation: self.allow_click_navigation,
            on_step_click: self.on_step_click.clone(),
            step_rects: Vec::new(),
            hover_index: None,
            classes: Vec::new(),
            dirty_flags: DirtyFlags::LAYOUT | DirtyFlags::RENDER,
            mss: MssFields::new(),
            text_measure: None,
        })
    }

    fn can_update(&self, other: &dyn Any) -> bool {
        other.is::<Self>()
    }

    fn as_any(&self) -> &dyn Any { self }
    fn as_any_mut(&mut self) -> &mut dyn Any { self }
    fn mount(&self, _tree: &mut ElementTree, _parent_id: ElementId) {}
}

pub struct StepperElement {
    id: ElementId,
    bounds: Rect,
    steps: Vec<StepInfo>,
    current_step: usize,
    disabled_steps: Vec<usize>,
    allow_click_navigation: bool,
    on_step_click: Option<Arc<Mutex<dyn FnMut(usize) + Send>>>,
    step_rects: Vec<Rect>,
    hover_index: Option<usize>,
    classes: Vec<String>,
    dirty_flags: DirtyFlags,
    mss: MssFields,
    text_measure: Option<Arc<dyn crate::widget::context::TextMeasure>>,
}

impl StepperElement {
    fn step_state(&self, index: usize) -> StepState {
        if self.disabled_steps.contains(&index) {
            StepState::Disabled
        } else if index < self.current_step {
            StepState::Completed
        } else if index == self.current_step {
            StepState::Active
        } else {
            StepState::Pending
        }
    }

    fn detected_variant(&self) -> StepperVariant {
        for c in &self.classes {
            match c.as_str() {
                "pill" => return StepperVariant::Pill,
                "radio" => return StepperVariant::Radio,
                "numbered" => return StepperVariant::Numbered,
                "icon" => return StepperVariant::Icon,
                "status" => return StepperVariant::Status,
                _ => {}
            }
        }
        StepperVariant::Numbered
    }

    fn accent(&self) -> Color { self.mss.accent_color.unwrap_or(Color::from_hex("#6366F1")) }
    fn accent_light(&self) -> Color { self.accent().with_alpha(0.15) }
    fn text_color(&self) -> Color { self.mss.color.unwrap_or(Color::from_hex("#1F2937")) }
    fn text_muted(&self) -> Color { self.text_color().with_alpha(0.5) }
    fn border_color(&self) -> Color { self.mss.border_color.unwrap_or(Color::from_hex("#D1D5DB")) }
    fn bg_color(&self) -> Color { self.mss.background_color.unwrap_or(Color::from_hex("#F3F4F6")) }
    fn white(&self) -> Color { Color::new(1.0, 1.0, 1.0, 1.0) }
    fn completed_color(&self) -> Color { Color::from_hex("#10B981") }

    fn font_size(&self) -> f32 { self.mss.font_size_or(13.0) }
    fn font_weight(&self) -> u16 { self.mss.font_weight_or(400) }
    fn icon_size(&self) -> f32 { self.mss.icon_size.unwrap_or(32.0) }
    fn gap(&self) -> f32 { self.mss.gap.unwrap_or(16.0) }
    fn border_width(&self) -> f32 { self.mss.border_width.unwrap_or(2.0) }

    fn measure_text(&self, text: &str, size: f32, bold: bool) -> f32 {
        self.text_measure.as_ref()
            .map(|tm| tm.measure_text_width_styled(text, size, text.chars().count(), bold, self.mss.font_family.as_deref()))
            .unwrap_or(text.chars().count() as f32 * size * 0.6)
    }

    fn layout_pill(&mut self, constraints: Constraints) -> Size {
        let font_size = self.font_size();
        let support_size = font_size * 0.85;
        let pad_h = self.mss.padding_left.unwrap_or(16.0);
        let pad_v = self.mss.padding_top.unwrap_or(8.0);
        let gap = self.gap();
        let arrow_w = 12.0;

        let mut total_w: f32 = 0.0;
        let mut max_h: f32 = 0.0;
        self.step_rects.clear();

        for (i, step) in self.steps.iter().enumerate() {
            let label_w = self.measure_text(&step.label, font_size, true);
            let support_w = step.support_text.as_ref()
                .map(|s| self.measure_text(s, support_size, false))
                .unwrap_or(0.0);
            let content_w = label_w.max(support_w);
            let pill_w = content_w + pad_h * 2.0;
            let pill_h = if step.support_text.is_some() {
                font_size + support_size + pad_v * 2.0 + 4.0
            } else {
                font_size + pad_v * 2.0
            };

            self.step_rects.push(Rect::new(
                Point::new(total_w, 0.0),
                Size::new(pill_w, pill_h),
            ));
            total_w += pill_w;
            max_h = max_h.max(pill_h);

            if i < self.steps.len() - 1 {
                total_w += arrow_w + gap;
            }
        }

        let w = total_w.min(constraints.max_width).max(constraints.min_width);
        let h = max_h.min(constraints.max_height).max(constraints.min_height);
        self.bounds = Rect::new(Point::zero(), Size::new(w, h));
        Size::new(w, h)
    }

    fn layout_circle_variant(&mut self, constraints: Constraints, text_below: bool) -> Size {
        let font_size = self.font_size();
        let support_size = font_size * 0.85;
        let circle_d = self.icon_size();
        let gap = self.gap();

        self.step_rects.clear();

        let mut step_widths: Vec<f32> = Vec::new();
        for step in &self.steps {
            let label_w = self.measure_text(&step.label, font_size, false);
            let support_w = step.support_text.as_ref()
                .map(|s| self.measure_text(s, support_size, false))
                .unwrap_or(0.0);
            let status_w = step.status_text.as_ref()
                .map(|s| self.measure_text(s, support_size, false))
                .unwrap_or(0.0);
            let text_w = label_w.max(support_w).max(status_w);
            let w = if text_below { text_w.max(circle_d) } else { circle_d + 8.0 + label_w };
            step_widths.push(w);
        }

        let total_w: f32 = step_widths.iter().sum::<f32>() + gap * (self.steps.len().saturating_sub(1)) as f32;
        let text_height = if text_below {
            let base = font_size + 6.0;
            let support_extra = if self.steps.iter().any(|s| s.support_text.is_some()) { support_size + 2.0 } else { 0.0 };
            let status_extra = if self.steps.iter().any(|s| s.status_text.is_some()) { support_size + 2.0 } else { 0.0 };
            base + support_extra + status_extra
        } else {
            0.0
        };
        let total_h = circle_d + text_height;

        let w = total_w.min(constraints.max_width).max(constraints.min_width);
        let h = total_h.min(constraints.max_height).max(constraints.min_height);

        if w > total_w && self.steps.len() > 1 && text_below {
            let step_w = w / self.steps.len() as f32;
            for i in 0..self.steps.len() {
                self.step_rects.push(Rect::new(
                    Point::new(i as f32 * step_w, 0.0),
                    Size::new(step_w, total_h),
                ));
            }
        } else {
            let mut x: f32 = 0.0;
            for (i, sw) in step_widths.iter().enumerate() {
                self.step_rects.push(Rect::new(
                    Point::new(x, 0.0),
                    Size::new(*sw, total_h),
                ));
                x += sw;
                if i < self.steps.len() - 1 {
                    x += gap;
                }
            }
        }

        self.bounds = Rect::new(Point::zero(), Size::new(w, h));
        Size::new(w, h)
    }

    fn layout_radio(&mut self, constraints: Constraints) -> Size {
        self.layout_circle_variant(constraints, false)
    }

    fn layout_numbered(&mut self, constraints: Constraints) -> Size {
        self.layout_circle_variant(constraints, true)
    }

    fn layout_icon(&mut self, constraints: Constraints) -> Size {
        self.layout_circle_variant(constraints, true)
    }

    fn layout_status(&mut self, constraints: Constraints) -> Size {
        self.layout_circle_variant(constraints, true)
    }

    fn render_pill(&self, list: &mut DisplayList) {
        let font_size = self.font_size();
        let support_size = font_size * 0.85;
        let font_weight = self.font_weight();
        let pad_h = self.mss.padding_left.unwrap_or(16.0);
        let pad_v = self.mss.padding_top.unwrap_or(8.0);
        let radius = self.mss.border_radius.map(|r| r.map(|d| d.resolve(0.0))).unwrap_or([6.0; 4]);
        let ox = self.bounds.x();
        let oy = self.bounds.y();

        for (i, step) in self.steps.iter().enumerate() {
            if i >= self.step_rects.len() { break; }
            let r = self.step_rects[i];
            let state = self.step_state(i);
            let is_hover = self.hover_index == Some(i);

            let pill_rect = Rect::new(
                Point::new(r.x() + ox, r.y() + oy),
                r.size,
            );

            let (bg, text_col) = match state {
                StepState::Active => (self.accent(), self.white()),
                StepState::Completed => (self.accent_light(), self.accent()),
                StepState::Pending => if is_hover {
                    (self.bg_color(), self.text_color())
                } else {
                    (Color::TRANSPARENT, self.text_muted())
                },
                StepState::Disabled => (Color::TRANSPARENT, self.text_muted().with_alpha(0.3)),
            };

            list.push_rect(pill_rect, bg, radius);

            let label_rect = Rect::new(
                Point::new(pill_rect.x() + pad_h, pill_rect.y() + pad_v),
                Size::new(pill_rect.width() - pad_h * 2.0, font_size + 2.0),
            );
            list.push_text_styled(&step.label, label_rect, text_col, font_size,
                TextAlign::DEFAULT, TextDecoration::None, 600, self.mss.font_family.clone());

            if let Some(ref support) = step.support_text {
                let support_col = match state {
                    StepState::Active => self.white().with_alpha(0.8),
                    _ => text_col.with_alpha(0.6),
                };
                let sup_rect = Rect::new(
                    Point::new(pill_rect.x() + pad_h, label_rect.y() + font_size + 4.0),
                    Size::new(pill_rect.width() - pad_h * 2.0, support_size + 2.0),
                );
                list.push_text_styled(support, sup_rect, support_col, support_size,
                    TextAlign::DEFAULT, TextDecoration::None, font_weight, self.mss.font_family.clone());
            }

            if i < self.steps.len() - 1 {
                let arrow_x = pill_rect.x() + pill_rect.width() + 2.0;
                let arrow_y = pill_rect.y();
                let arrow_rect = Rect::new(
                    Point::new(arrow_x, arrow_y),
                    Size::new(12.0, pill_rect.height()),
                );
                list.push_text_centered("›", arrow_rect, self.border_color(), font_size + 4.0);
            }
        }
    }

    fn render_radio(&self, list: &mut DisplayList) {
        let font_size = self.font_size();
        let font_weight = self.font_weight();
        let circle_d = self.icon_size();
        let circle_r = circle_d / 2.0;
        let bw = self.border_width();
        let ox = self.bounds.x();
        let oy = self.bounds.y();
        let center_y = oy + circle_r;

        let connector_h = 2.0;
        for i in 0..self.steps.len().saturating_sub(1) {
            if i >= self.step_rects.len() || i + 1 >= self.step_rects.len() { break; }
            let r0 = self.step_rects[i];
            let r1 = self.step_rects[i + 1];
            let x0 = ox + r0.x() + circle_d;
            let x1 = ox + r1.x();
            let state = self.step_state(i + 1);
            let color = if state == StepState::Completed || state == StepState::Active {
                self.accent()
            } else {
                self.border_color()
            };
            let conn = Rect::new(
                Point::new(x0 + 4.0, center_y - connector_h / 2.0),
                Size::new((x1 - x0 - 8.0).max(0.0), connector_h),
            );
            list.push_rect(conn, color, [0.0; 4]);
        }

        for (i, step) in self.steps.iter().enumerate() {
            if i >= self.step_rects.len() { break; }
            let r = self.step_rects[i];
            let state = self.step_state(i);
            let cx = ox + r.x();

            let circle_rect = Rect::new(
                Point::new(cx, oy),
                Size::new(circle_d, circle_d),
            );
            let full_r = [circle_r; 4];

            match state {
                StepState::Completed => {
                    list.push_rect(circle_rect, self.accent(), full_r);
                    list.push_text_centered("\u{E5CA}", circle_rect, self.white(), circle_d * 0.5);
                }
                StepState::Active => {
                    list.push_rect(circle_rect, self.accent(), full_r);
                    let dot_d = circle_d * 0.4;
                    let dot_rect = Rect::new(
                        Point::new(cx + (circle_d - dot_d) / 2.0, oy + (circle_d - dot_d) / 2.0),
                        Size::new(dot_d, dot_d),
                    );
                    list.push_rect(dot_rect, self.white(), [dot_d / 2.0; 4]);
                }
                StepState::Pending | StepState::Disabled => {
                    let border = Border { width: bw, color: self.border_color() };
                    list.push_rect_bordered(circle_rect, Color::TRANSPARENT, full_r, border);
                }
            }

            let label_x = cx + circle_d + 8.0;
            let label_rect = Rect::new(
                Point::new(label_x, oy + (circle_d - font_size) / 2.0),
                Size::new(r.width() - circle_d - 8.0, font_size + 2.0),
            );
            let label_col = if state == StepState::Disabled { self.text_muted().with_alpha(0.3) }
                else if state == StepState::Pending { self.text_muted() }
                else { self.text_color() };
            list.push_text_styled(&step.label, label_rect, label_col, font_size,
                TextAlign::DEFAULT, TextDecoration::None, font_weight, self.mss.font_family.clone());
        }
    }

    fn render_numbered(&self, list: &mut DisplayList) {
        let font_size = self.font_size();
        let support_size = font_size * 0.85;
        let font_weight = self.font_weight();
        let circle_d = self.icon_size();
        let circle_r = circle_d / 2.0;
        let bw = self.border_width();
        let ox = self.bounds.x();
        let oy = self.bounds.y();

        let connector_h = 2.0;
        for i in 0..self.steps.len().saturating_sub(1) {
            if i >= self.step_rects.len() || i + 1 >= self.step_rects.len() { break; }
            let r0 = self.step_rects[i];
            let r1 = self.step_rects[i + 1];
            let cx0 = ox + r0.x() + r0.width() / 2.0;
            let cx1 = ox + r1.x() + r1.width() / 2.0;
            let x0 = cx0 + circle_r + 4.0;
            let x1 = cx1 - circle_r - 4.0;
            let state = self.step_state(i + 1);
            let color = if state == StepState::Completed || state == StepState::Active {
                self.accent()
            } else {
                self.border_color()
            };
            let conn = Rect::new(
                Point::new(x0, oy + circle_r - connector_h / 2.0),
                Size::new((x1 - x0).max(0.0), connector_h),
            );
            list.push_rect(conn, color, [0.0; 4]);
        }

        for (i, step) in self.steps.iter().enumerate() {
            if i >= self.step_rects.len() { break; }
            let r = self.step_rects[i];
            let state = self.step_state(i);
            let center_x = ox + r.x() + r.width() / 2.0;

            let circle_rect = Rect::new(
                Point::new(center_x - circle_r, oy),
                Size::new(circle_d, circle_d),
            );
            let full_r = [circle_r; 4];

            match state {
                StepState::Completed => {
                    list.push_rect(circle_rect, self.accent(), full_r);
                    list.push_text_centered("\u{E5CA}", circle_rect, self.white(), circle_d * 0.45);
                }
                StepState::Active => {
                    let border = Border { width: bw, color: self.accent() };
                    list.push_rect_bordered(circle_rect, self.accent_light(), full_r, border);
                    let num = format!("{:02}", i + 1);
                    list.push_text_centered(&num, circle_rect, self.accent(), circle_d * 0.4);
                }
                StepState::Pending | StepState::Disabled => {
                    let border_col = if state == StepState::Disabled { self.border_color().with_alpha(0.3) } else { self.border_color() };
                    let border = Border { width: bw, color: border_col };
                    list.push_rect_bordered(circle_rect, Color::TRANSPARENT, full_r, border);
                    let num = format!("{:02}", i + 1);
                    let text_col = if state == StepState::Disabled { self.text_muted().with_alpha(0.3) } else { self.text_muted() };
                    list.push_text_centered(&num, circle_rect, text_col, circle_d * 0.4);
                }
            }

            let label_y = oy + circle_d + 8.0;
            let label_rect = Rect::new(
                Point::new(ox + r.x(), label_y),
                Size::new(r.width(), font_size + 2.0),
            );
            let label_col = match state {
                StepState::Active => self.text_color(),
                StepState::Completed => self.text_color(),
                StepState::Disabled => self.text_muted().with_alpha(0.3),
                StepState::Pending => self.text_muted(),
            };
            list.push_text_styled(&step.label, label_rect, label_col, font_size,
                TextAlign::HCENTER, TextDecoration::None,
                if state == StepState::Active { 600 } else { font_weight },
                self.mss.font_family.clone());

            if let Some(ref support) = step.support_text {
                let sup_rect = Rect::new(
                    Point::new(ox + r.x(), label_y + font_size + 4.0),
                    Size::new(r.width(), support_size + 2.0),
                );
                list.push_text_styled(support, sup_rect, self.text_muted(), support_size,
                    TextAlign::HCENTER, TextDecoration::None, font_weight, self.mss.font_family.clone());
            }
        }
    }

    fn render_icon(&self, list: &mut DisplayList) {
        let font_size = self.font_size();
        let support_size = font_size * 0.85;
        let font_weight = self.font_weight();
        let circle_d = self.icon_size();
        let circle_r = circle_d / 2.0;
        let ox = self.bounds.x();
        let oy = self.bounds.y();

        let connector_h = 4.0;
        for i in 0..self.steps.len().saturating_sub(1) {
            if i >= self.step_rects.len() || i + 1 >= self.step_rects.len() { break; }
            let r0 = self.step_rects[i];
            let r1 = self.step_rects[i + 1];
            let cx0 = ox + r0.x() + r0.width() / 2.0;
            let cx1 = ox + r1.x() + r1.width() / 2.0;
            let x0 = cx0 + circle_r + 2.0;
            let x1 = cx1 - circle_r - 2.0;
            let state = self.step_state(i + 1);
            let color = if state == StepState::Completed || state == StepState::Active {
                self.accent()
            } else {
                self.border_color()
            };
            let conn = Rect::new(
                Point::new(x0, oy + circle_r - connector_h / 2.0),
                Size::new((x1 - x0).max(0.0), connector_h),
            );
            list.push_rect(conn, color, [connector_h / 2.0; 4]);
        }

        for (i, step) in self.steps.iter().enumerate() {
            if i >= self.step_rects.len() { break; }
            let r = self.step_rects[i];
            let state = self.step_state(i);
            let center_x = ox + r.x() + r.width() / 2.0;

            let circle_rect = Rect::new(
                Point::new(center_x - circle_r, oy),
                Size::new(circle_d, circle_d),
            );
            let full_r = [circle_r; 4];

            let (bg, icon_col) = match state {
                StepState::Completed | StepState::Active => {
                    let ic = self.mss.icon_color_selected.unwrap_or_else(|| self.white());
                    (self.accent(), ic)
                }
                StepState::Pending => {
                    let ic = self.mss.icon_color
                        .or(self.mss.color)
                        .unwrap_or_else(|| self.text_muted());
                    (self.bg_color(), ic)
                }
                StepState::Disabled => {
                    let ic = self.mss.icon_color_disabled
                        .unwrap_or_else(|| self.text_muted().with_alpha(0.3));
                    (self.bg_color().with_alpha(0.3), ic)
                }
            };
            list.push_rect(circle_rect, bg, full_r);

            let fallback = format!("{}", i + 1);
            let icon_text = step.icon.as_deref().unwrap_or(&fallback);
            list.push_text_centered(icon_text, circle_rect, icon_col, circle_d * 0.45);

            let label_y = oy + circle_d + 8.0;
            let label_rect = Rect::new(
                Point::new(ox + r.x(), label_y),
                Size::new(r.width(), font_size + 2.0),
            );
            let label_col = match state {
                StepState::Active | StepState::Completed => self.accent(),
                StepState::Disabled => self.text_muted().with_alpha(0.3),
                StepState::Pending => self.text_muted(),
            };
            list.push_text_styled(&step.label, label_rect, label_col, font_size,
                TextAlign::HCENTER, TextDecoration::None, 600, self.mss.font_family.clone());

            if let Some(ref support) = step.support_text {
                let sup_rect = Rect::new(
                    Point::new(ox + r.x(), label_y + font_size + 4.0),
                    Size::new(r.width(), support_size + 2.0),
                );
                list.push_text_styled(support, sup_rect, self.text_muted(), support_size,
                    TextAlign::HCENTER, TextDecoration::None, font_weight, self.mss.font_family.clone());
            }
        }
    }

    fn render_status(&self, list: &mut DisplayList) {
        let font_size = self.font_size();
        let support_size = font_size * 0.85;
        let font_weight = self.font_weight();
        let circle_d = self.icon_size();
        let circle_r = circle_d / 2.0;
        let bw = self.border_width();
        let ox = self.bounds.x();
        let oy = self.bounds.y();

        let connector_h = 2.0;
        for i in 0..self.steps.len().saturating_sub(1) {
            if i >= self.step_rects.len() || i + 1 >= self.step_rects.len() { break; }
            let r0 = self.step_rects[i];
            let r1 = self.step_rects[i + 1];
            let cx0 = ox + r0.x() + r0.width() / 2.0;
            let cx1 = ox + r1.x() + r1.width() / 2.0;
            let x0 = cx0 + circle_r + 4.0;
            let x1 = cx1 - circle_r - 4.0;
            let state = self.step_state(i + 1);
            let color = if state == StepState::Completed || state == StepState::Active {
                self.accent()
            } else {
                self.border_color()
            };
            let conn = Rect::new(
                Point::new(x0, oy + circle_r - connector_h / 2.0),
                Size::new((x1 - x0).max(0.0), connector_h),
            );
            list.push_rect(conn, color, [0.0; 4]);
        }

        for (i, step) in self.steps.iter().enumerate() {
            if i >= self.step_rects.len() { break; }
            let r = self.step_rects[i];
            let state = self.step_state(i);
            let center_x = ox + r.x() + r.width() / 2.0;

            let circle_rect = Rect::new(
                Point::new(center_x - circle_r, oy),
                Size::new(circle_d, circle_d),
            );
            let full_r = [circle_r; 4];

            match state {
                StepState::Completed => {
                    list.push_rect(circle_rect, self.accent(), full_r);
                    list.push_text_centered("\u{E5CA}", circle_rect, self.white(), circle_d * 0.6);
                }
                StepState::Active => {
                    let border = Border { width: bw, color: self.accent() };
                    list.push_rect_bordered(circle_rect, Color::TRANSPARENT, full_r, border);
                    let dot_d = circle_d * 0.5;
                    let dot_rect = Rect::new(
                        Point::new(center_x - dot_d / 2.0, oy + (circle_d - dot_d) / 2.0),
                        Size::new(dot_d, dot_d),
                    );
                    list.push_rect(dot_rect, self.accent(), [dot_d / 2.0; 4]);
                }
                StepState::Pending | StepState::Disabled => {
                    let border_col = if state == StepState::Disabled { self.border_color().with_alpha(0.3) } else { self.border_color() };
                    let border = Border { width: bw, color: border_col };
                    list.push_rect_bordered(circle_rect, Color::TRANSPARENT, full_r, border);
                }
            }

            let fallback_label = format!("Step {}", i + 1);
            let step_label = step.support_text.as_deref().unwrap_or(&fallback_label);
            let label_y = oy + circle_d + 8.0;
            let step_label_rect = Rect::new(
                Point::new(ox + r.x(), label_y),
                Size::new(r.width(), support_size + 2.0),
            );
            list.push_text_styled(step_label, step_label_rect, self.text_muted(), support_size,
                TextAlign::HCENTER, TextDecoration::None, font_weight, self.mss.font_family.clone());

            let title_y = label_y + support_size + 4.0;
            let title_rect = Rect::new(
                Point::new(ox + r.x(), title_y),
                Size::new(r.width(), font_size + 2.0),
            );
            let title_col = match state {
                StepState::Active | StepState::Completed => self.text_color(),
                _ => self.text_muted(),
            };
            list.push_text_styled(&step.label, title_rect, title_col, font_size,
                TextAlign::HCENTER, TextDecoration::None, 600, self.mss.font_family.clone());

            if let Some(ref status) = step.status_text {
                let status_y = title_y + font_size + 4.0;
                let status_rect = Rect::new(
                    Point::new(ox + r.x(), status_y),
                    Size::new(r.width(), support_size + 2.0),
                );
                let status_col = match state {
                    StepState::Completed => self.completed_color(),
                    StepState::Active => self.accent(),
                    _ => self.text_muted(),
                };
                list.push_text_styled(status, status_rect, status_col, support_size,
                    TextAlign::HCENTER, TextDecoration::None, font_weight, self.mss.font_family.clone());
            }
        }
    }
}

impl Element for StepperElement {
    fn update(&mut self, widget: &dyn Widget, _ctx: &mut UpdateContext) {
        if let Some(s) = widget.as_any().downcast_ref::<Stepper>() {
            self.steps = s.steps.clone();
            self.current_step = s.current_step;
            self.disabled_steps = s.disabled_steps.clone();
            self.allow_click_navigation = s.allow_click_navigation;
            self.on_step_click = s.on_step_click.clone();
            self.mark_dirty(DirtyFlags::LAYOUT | DirtyFlags::RENDER);
        }
    }

    fn layout(&mut self, constraints: Constraints) -> Size {
        match self.detected_variant() {
            StepperVariant::Pill => self.layout_pill(constraints),
            StepperVariant::Radio => self.layout_radio(constraints),
            StepperVariant::Numbered => self.layout_numbered(constraints),
            StepperVariant::Icon => self.layout_icon(constraints),
            StepperVariant::Status => self.layout_status(constraints),
        }
    }

    fn build_display_list(&self, list: &mut DisplayList, _clip: Rect) {
        match self.detected_variant() {
            StepperVariant::Pill => self.render_pill(list),
            StepperVariant::Radio => self.render_radio(list),
            StepperVariant::Numbered => self.render_numbered(list),
            StepperVariant::Icon => self.render_icon(list),
            StepperVariant::Status => self.render_status(list),
        }
    }

    fn handle_event(&mut self, event: &Event, ctx: &mut EventContext) -> EventResult {
        match event {
            Event::MouseMove(pos) => {
                let old_hover = self.hover_index;
                if self.bounds.contains(*pos) {
                    self.hover_index = None;
                    for (i, rect) in self.step_rects.iter().enumerate() {
                        let offset_rect = Rect::new(
                            Point::new(rect.x() + self.bounds.x(), rect.y() + self.bounds.y()),
                            rect.size,
                        );
                        if offset_rect.contains(*pos) {
                            self.hover_index = Some(i);
                            break;
                        }
                    }
                } else {
                    self.hover_index = None;
                }
                if self.hover_index != old_hover {
                    ctx.request_paint();
                    return EventResult::Handled;
                }
                EventResult::Ignored
            }
            Event::MouseDown { button, position } => {
                if *button == MouseButton::Left && self.bounds.contains(*position) {
                    for (i, rect) in self.step_rects.iter().enumerate() {
                        let offset_rect = Rect::new(
                            Point::new(rect.x() + self.bounds.x(), rect.y() + self.bounds.y()),
                            rect.size,
                        );
                        if offset_rect.contains(*position) && self.step_state(i) != StepState::Disabled {
                            if let Some(ref callback) = self.on_step_click {
                                if let Ok(mut cb) = callback.lock() {
                                    cb(i);
                                }
                            }
                            ctx.request_paint();
                            return EventResult::Handled;
                        }
                    }
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

    fn mount(&mut self, tree: &mut ElementTree) {
        self.text_measure = tree.text_measure.clone();
    }

    fn element_type_name(&self) -> &str { "Stepper" }

    fn set_classes(&mut self, classes: Vec<String>) {
        self.classes = classes;
        self.mark_dirty(DirtyFlags::LAYOUT | DirtyFlags::RENDER);
    }

    fn get_classes(&self) -> &[String] { &self.classes }

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
}

impl StyledElement for StepperElement {
    fn apply_style(&mut self, _style: &ComputedStyle) {
        self.mark_dirty(DirtyFlags::RENDER);
    }

    fn classes(&self) -> &[String] { &self.classes }

    fn set_classes(&mut self, classes: Vec<String>) {
        self.classes = classes;
        self.mark_dirty(DirtyFlags::RENDER);
    }
}
