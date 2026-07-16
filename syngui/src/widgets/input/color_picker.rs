use crate::core::{Color, Point, Rect, RectExt, Size};
use crate::input::{CursorIcon, Event, EventResult, MouseButton};
use crate::layout::Constraints;
use crate::mss::{ComputedStyle, Dimension};
use crate::mss::MssFields;
use crate::render::{Border, DisplayList};
use crate::widget::context::{EventContext, EventContextExt};
use crate::widget::{DirtyFlags, Element, ElementId, ElementTree, LayoutHint, StyledElement, UpdateContext, Widget};
use crate::widgets::containers::IntoWidget;
use std::any::Any;
use std::sync::Arc;
use crate::core::sync::Mutex;

#[derive(Clone, Copy, Debug, PartialEq)]
pub struct ColorValue {
    pub r: u8,
    pub g: u8,
    pub b: u8,
    pub a: u8,
}

impl ColorValue {
    pub fn new(r: u8, g: u8, b: u8) -> Self {
        Self { r, g, b, a: 255 }
    }

    pub fn with_alpha(mut self, a: u8) -> Self {
        self.a = a;
        self
    }

    pub fn from_hex(hex: &str) -> Option<Self> {
        let hex = hex.trim_start_matches('#');
        if hex.len() == 6 {
            let r = u8::from_str_radix(&hex[0..2], 16).ok()?;
            let g = u8::from_str_radix(&hex[2..4], 16).ok()?;
            let b = u8::from_str_radix(&hex[4..6], 16).ok()?;
            Some(Self { r, g, b, a: 255 })
        } else if hex.len() == 8 {
            let r = u8::from_str_radix(&hex[0..2], 16).ok()?;
            let g = u8::from_str_radix(&hex[2..4], 16).ok()?;
            let b = u8::from_str_radix(&hex[4..6], 16).ok()?;
            let a = u8::from_str_radix(&hex[6..8], 16).ok()?;
            Some(Self { r, g, b, a })
        } else {
            None
        }
    }

    pub fn to_hex(&self) -> String {
        format!("#{:02X}{:02X}{:02X}", self.r, self.g, self.b)
    }

    pub fn to_color(&self) -> Color {
        Color::new(
            self.r as f32 / 255.0,
            self.g as f32 / 255.0,
            self.b as f32 / 255.0,
            self.a as f32 / 255.0,
        )
    }

    pub fn from_color(c: Color) -> Self {
        Self {
            r: (c.r * 255.0).round() as u8,
            g: (c.g * 255.0).round() as u8,
            b: (c.b * 255.0).round() as u8,
            a: (c.a * 255.0).round() as u8,
        }
    }

    pub fn to_hsv(&self) -> (f32, f32, f32) {
        let r = self.r as f32 / 255.0;
        let g = self.g as f32 / 255.0;
        let b = self.b as f32 / 255.0;
        let max = r.max(g).max(b);
        let min = r.min(g).min(b);
        let delta = max - min;

        let h = if delta < 1e-6 {
            0.0
        } else if (max - r).abs() < 1e-6 {
            60.0 * (((g - b) / delta) % 6.0)
        } else if (max - g).abs() < 1e-6 {
            60.0 * ((b - r) / delta + 2.0)
        } else {
            60.0 * ((r - g) / delta + 4.0)
        };
        let h = if h < 0.0 { h + 360.0 } else { h };
        let s = if max < 1e-6 { 0.0 } else { delta / max };
        (h, s, max)
    }

    pub fn from_hsv(h: f32, s: f32, v: f32) -> Self {
        let c = v * s;
        let x = c * (1.0 - ((h / 60.0) % 2.0 - 1.0).abs());
        let m = v - c;

        let (r, g, b) = if h < 60.0 {
            (c, x, 0.0)
        } else if h < 120.0 {
            (x, c, 0.0)
        } else if h < 180.0 {
            (0.0, c, x)
        } else if h < 240.0 {
            (0.0, x, c)
        } else if h < 300.0 {
            (x, 0.0, c)
        } else {
            (c, 0.0, x)
        };

        Self {
            r: ((r + m) * 255.0).round() as u8,
            g: ((g + m) * 255.0).round() as u8,
            b: ((b + m) * 255.0).round() as u8,
            a: 255,
        }
    }
}

pub struct ColorPicker {
    color: ColorValue,
    on_change: Option<Arc<Mutex<dyn FnMut(ColorValue) + Send>>>,
    width: Option<Dimension>,
    show_alpha: bool,
    child: Option<Box<dyn Widget>>,
}

impl ColorPicker {
    pub fn new() -> Self {
        Self {
            color: ColorValue::new(59, 130, 246),
            on_change: None,
            width: None,
            show_alpha: false,
            child: None,
        }
    }

    pub fn color(mut self, c: ColorValue) -> Self {
        self.color = c;
        self
    }

    pub fn on_change(mut self, f: impl FnMut(ColorValue) + Send + 'static) -> Self {
        self.on_change = Some(Arc::new(Mutex::new(f)));
        self
    }

    pub fn width(mut self, w: f32) -> Self {
        self.width = Some(Dimension::Px(w));
        self
    }

    pub fn show_alpha(mut self, v: bool) -> Self {
        self.show_alpha = v;
        self
    }

    pub fn child<M>(mut self, child: impl IntoWidget<M>) -> Self {
        self.child = Some(child.into_widget());
        self
    }
}

impl Default for ColorPicker {
    fn default() -> Self { Self::new() }
}

impl Widget for ColorPicker {
    fn create_element(&self) -> Box<dyn Element> {
        let (h, s, v) = self.color.to_hsv();
        Box::new(ColorPickerElement {
            id: ElementId::new(),
            color: self.color,
            on_change: self.on_change.clone(),
            width: self.width,
            show_alpha: self.show_alpha,
            has_child: self.child.is_some(),
            is_open: false,
            hue: h,
            sat: s,
            val: v,
            hex_input: self.color.to_hex(),
            drag_target: DragTarget::None,
            opens_upward: false,
            bounds: Rect::zero(),
            child_ids: Vec::new(),
            classes: Vec::new(),
            dirty_flags: DirtyFlags::LAYOUT | DirtyFlags::RENDER,
            mss: MssFields::new(),
        })
    }

    fn can_update(&self, other: &dyn Any) -> bool { other.is::<Self>() }
    fn as_any(&self) -> &dyn Any { self }
    fn as_any_mut(&mut self) -> &mut dyn Any { self }

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
}

const INPUT_HEIGHT: f32 = 40.0;
const POPUP_WIDTH: f32 = 260.0;
const SV_SIZE: f32 = 200.0;
const HUE_BAR_WIDTH: f32 = 20.0;
const HUE_BAR_GAP: f32 = 12.0;
const POPUP_PADDING: f32 = 16.0;
const SLIDER_HEIGHT: f32 = 20.0;
const SLIDER_GAP: f32 = 6.0;
const HEX_ROW_HEIGHT: f32 = 32.0;
const PREVIEW_HEIGHT: f32 = 32.0;

#[derive(Clone, Copy, Debug, PartialEq)]
enum DragTarget {
    None,
    SvField,
    HueBar,
}

pub struct ColorPickerElement {
    id: ElementId,
    color: ColorValue,
    on_change: Option<Arc<Mutex<dyn FnMut(ColorValue) + Send>>>,
    width: Option<Dimension>,
    show_alpha: bool,
    has_child: bool,
    is_open: bool,
    hue: f32,
    sat: f32,
    val: f32,
    hex_input: String,
    drag_target: DragTarget,
    opens_upward: bool,
    bounds: Rect,
    child_ids: Vec<ElementId>,
    classes: Vec<String>,
    dirty_flags: DirtyFlags,
    mss: MssFields,
}

impl ColorPickerElement {
    fn popup_height(&self) -> f32 {
        POPUP_PADDING * 2.0 + SV_SIZE + SLIDER_GAP
            + 3.0 * (SLIDER_HEIGHT + SLIDER_GAP)
            + HEX_ROW_HEIGHT + SLIDER_GAP
            + PREVIEW_HEIGHT
    }

    fn popup_rect(&self) -> Rect {
        let h = self.popup_height();
        let trigger_h = self.bounds.size.height;
        let y = if self.opens_upward {
            self.bounds.y() - h - 4.0
        } else {
            self.bounds.y() + trigger_h + 4.0
        };
        Rect::new(
            Point::new(self.bounds.x(), y),
            Size::new(POPUP_WIDTH, h),
        )
    }

    fn sv_field_rect(&self, popup: Rect) -> Rect {
        Rect::new(
            Point::new(popup.x() + POPUP_PADDING, popup.y() + POPUP_PADDING),
            Size::new(SV_SIZE, SV_SIZE),
        )
    }

    fn hue_bar_rect(&self, popup: Rect) -> Rect {
        Rect::new(
            Point::new(popup.x() + POPUP_PADDING + SV_SIZE + HUE_BAR_GAP, popup.y() + POPUP_PADDING),
            Size::new(HUE_BAR_WIDTH, SV_SIZE),
        )
    }

    fn rgb_slider_rect(&self, popup: Rect, index: usize) -> Rect {
        let y = popup.y() + POPUP_PADDING + SV_SIZE + SLIDER_GAP + index as f32 * (SLIDER_HEIGHT + SLIDER_GAP);
        Rect::new(
            Point::new(popup.x() + POPUP_PADDING, y),
            Size::new(POPUP_WIDTH - POPUP_PADDING * 2.0, SLIDER_HEIGHT),
        )
    }

    fn hex_row_rect(&self, popup: Rect) -> Rect {
        let y = popup.y() + POPUP_PADDING + SV_SIZE + SLIDER_GAP + 3.0 * (SLIDER_HEIGHT + SLIDER_GAP);
        Rect::new(
            Point::new(popup.x() + POPUP_PADDING, y),
            Size::new(POPUP_WIDTH - POPUP_PADDING * 2.0, HEX_ROW_HEIGHT),
        )
    }

    fn preview_rect(&self, popup: Rect) -> Rect {
        let y = popup.y() + POPUP_PADDING + SV_SIZE + SLIDER_GAP
            + 3.0 * (SLIDER_HEIGHT + SLIDER_GAP)
            + HEX_ROW_HEIGHT + SLIDER_GAP;
        Rect::new(
            Point::new(popup.x() + POPUP_PADDING, y),
            Size::new(POPUP_WIDTH - POPUP_PADDING * 2.0, PREVIEW_HEIGHT),
        )
    }

    fn update_from_hsv(&mut self) {
        self.color = ColorValue::from_hsv(self.hue, self.sat, self.val);
        self.hex_input = self.color.to_hex();
    }

    fn fire_change(&self) {
        if let Some(ref cb) = self.on_change {
            if let Ok(mut f) = cb.lock() { f(self.color); }
        }
    }

    fn handle_sv_drag(&mut self, pos: Point, popup: Rect) {
        let sv = self.sv_field_rect(popup);
        let sx = ((pos.x - sv.x()) / sv.size.width).clamp(0.0, 1.0);
        let sy = 1.0 - ((pos.y - sv.y()) / sv.size.height).clamp(0.0, 1.0);
        self.sat = sx;
        self.val = sy;
        self.update_from_hsv();
    }

    fn handle_hue_drag(&mut self, pos: Point, popup: Rect) {
        let hb = self.hue_bar_rect(popup);
        let t = ((pos.y - hb.y()) / hb.size.height).clamp(0.0, 1.0);
        self.hue = t * 360.0;
        self.update_from_hsv();
    }

    fn draw_sv_field(&self, list: &mut DisplayList, rect: Rect) {
        let steps = 16;
        let cell_w = rect.size.width / steps as f32;
        let cell_h = rect.size.height / steps as f32;

        for yi in 0..steps {
            for xi in 0..steps {
                let s = (xi as f32 + 0.5) / steps as f32;
                let v = 1.0 - (yi as f32 + 0.5) / steps as f32;
                let c = ColorValue::from_hsv(self.hue, s, v);
                let cell_rect = Rect::new(
                    Point::new(rect.x() + xi as f32 * cell_w, rect.y() + yi as f32 * cell_h),
                    Size::new(cell_w + 0.5, cell_h + 0.5),
                );
                list.push_rect(cell_rect, c.to_color(), [0.0; 4]);
            }
        }

        let cx = rect.x() + self.sat * rect.size.width;
        let cy = rect.y() + (1.0 - self.val) * rect.size.height;
        let cursor_r = 6.0;
        let cursor_rect = Rect::new(
            Point::new(cx - cursor_r, cy - cursor_r),
            Size::new(cursor_r * 2.0, cursor_r * 2.0),
        );
        list.push_rect_bordered(cursor_rect, Color::TRANSPARENT, [cursor_r; 4], Border::new(2.0, Color::WHITE));
        list.push_rect_bordered(cursor_rect, Color::TRANSPARENT, [cursor_r; 4], Border::new(1.0, Color::BLACK.with_alpha(0.3)));
    }

    fn draw_hue_bar(&self, list: &mut DisplayList, rect: Rect) {
        let steps = 12;
        let cell_h = rect.size.height / steps as f32;
        let hues = [0.0, 30.0, 60.0, 90.0, 120.0, 150.0, 180.0, 210.0, 240.0, 270.0, 300.0, 330.0];

        for (i, &h) in hues.iter().enumerate() {
            let c = ColorValue::from_hsv(h, 1.0, 1.0);
            let cell_rect = Rect::new(
                Point::new(rect.x(), rect.y() + i as f32 * cell_h),
                Size::new(rect.size.width, cell_h + 0.5),
            );
            list.push_rect(cell_rect, c.to_color(), [0.0; 4]);
        }

        let cy = rect.y() + (self.hue / 360.0) * rect.size.height;
        let cursor_rect = Rect::new(
            Point::new(rect.x() - 2.0, cy - 3.0),
            Size::new(rect.size.width + 4.0, 6.0),
        );
        list.push_rect_bordered(cursor_rect, Color::TRANSPARENT, [3.0; 4], Border::new(2.0, Color::WHITE));
    }

    fn draw_rgb_slider(&self, list: &mut DisplayList, rect: Rect, label: &str, value: u8, channel_color: Color, fg: Color) {
        let label_w = 20.0;
        let value_w = 36.0;
        let bar_x = rect.x() + label_w;
        let bar_w = rect.size.width - label_w - value_w;

        let label_rect = Rect::new(rect.origin, Size::new(label_w, rect.size.height));
        list.push_text(label, label_rect, fg.with_alpha(0.6), 11.0);

        let bar_rect = Rect::new(
            Point::new(bar_x, rect.y() + (rect.size.height - 8.0) / 2.0),
            Size::new(bar_w, 8.0),
        );
        let bar_bg = self.mss.border_color.map(|c| c.lighten(0.2)).unwrap_or(Color::from_hex("#E5E7EB"));
        list.push_rect(bar_rect, bar_bg, [4.0; 4]);

        let fill_w = (value as f32 / 255.0) * bar_w;
        let fill_rect = Rect::new(bar_rect.origin, Size::new(fill_w, 8.0));
        list.push_rect(fill_rect, channel_color, [4.0; 4]);

        let thumb_x = bar_x + fill_w - 5.0;
        let thumb_rect = Rect::new(
            Point::new(thumb_x, rect.y() + (rect.size.height - 14.0) / 2.0),
            Size::new(10.0, 14.0),
        );
        list.push_rect_bordered(thumb_rect, Color::WHITE, [5.0; 4], Border::new(2.0, channel_color));

        let val_rect = Rect::new(
            Point::new(rect.x() + rect.size.width - value_w, rect.y()),
            Size::new(value_w, rect.size.height),
        );
        list.push_text(&value.to_string(), val_rect, fg, 11.0);
    }
}

impl Element for ColorPickerElement {
    fn update(&mut self, widget: &dyn Widget, _ctx: &mut UpdateContext) {
        if let Some(cp) = widget.as_any().downcast_ref::<ColorPicker>() {
            self.color = cp.color;
            self.on_change = cp.on_change.clone();
            self.width = cp.width;
            self.show_alpha = cp.show_alpha;
            self.has_child = cp.child.is_some();
            let (h, s, v) = cp.color.to_hsv();
            self.hue = h;
            self.sat = s;
            self.val = v;
            self.hex_input = cp.color.to_hex();
            self.mark_dirty(DirtyFlags::LAYOUT | DirtyFlags::RENDER);
        }
    }

    fn layout(&mut self, constraints: Constraints) -> Size {
        if self.has_child {
            let w = if constraints.max_width.is_finite() { constraints.max_width } else { 0.0 };
            let h = if constraints.max_height.is_finite() { constraints.max_height } else { 0.0 };
            self.bounds = Rect::new(Point::zero(), Size::new(w, h));
            Size::new(w, h)
        } else {
            let w = self.width.map(|d| d.resolve(constraints.max_width)).unwrap_or(POPUP_WIDTH).min(constraints.max_width);
            let h = self.mss.height.map(|d| d.resolve(constraints.max_height.max(0.0))).unwrap_or(INPUT_HEIGHT);
            self.bounds = Rect::new(Point::zero(), Size::new(w, h));
            Size::new(w, h)
        }
    }

    fn build_display_list(&self, list: &mut DisplayList, _clip: Rect) {
        if !self.has_child {
            let border_color = if self.is_open {
                self.mss.accent_color.unwrap_or(Color::from_hex("#3B82F6"))
            } else {
                self.mss.border_color.unwrap_or(Color::from_hex("#D1D5DB"))
            };
            let bg_color = self.mss.background_color.unwrap_or(Color::WHITE);

            let h = self.bounds.size.height;
            let v_pad = if h < 32.0 { 4.0 } else { 8.0 };
            let swatch_size = (h - 2.0 * v_pad).max(0.0);
            let h_pad = v_pad;
            let radius = if h < 32.0 { 6.0 } else { 8.0 };

            list.push_rect_bordered(
                self.bounds, bg_color, [radius; 4],
                Border::new(if self.is_open { 2.0 } else { 1.0 }, border_color),
            );

            let swatch_rect = Rect::new(
                Point::new(self.bounds.x() + h_pad, self.bounds.y() + v_pad),
                Size::new(swatch_size, swatch_size),
            );
            let swatch_radius = (swatch_size * 0.18).clamp(2.0, 4.0);
            list.push_rect(swatch_rect, self.color.to_color(), [swatch_radius; 4]);

            let font_size = self.mss.font_size_or(if h < 32.0 { 11.0 } else { 14.0 });
            let text_x = self.bounds.x() + h_pad + swatch_size + h_pad;
            let text_rect = Rect::new(
                Point::new(text_x, self.bounds.y() + (h - font_size) / 2.0),
                Size::new((self.bounds.right() - h_pad - text_x).max(0.0), font_size + 2.0),
            );
            let text_color = self.mss.color.unwrap_or(Color::from_hex("#1F2937"));
            list.push_text(&self.color.to_hex(), text_rect, text_color, font_size);
        }

        if !self.is_open { return; }

        let popup = self.popup_rect();

        list.begin_overlay();

        list.push_shadow(popup, Color::BLACK.with_alpha(0.15), 16.0, (0.0, 4.0), [12.0; 4]);
        let popup_bg = self.mss.background_color.unwrap_or(Color::WHITE);
        let popup_fg = self.mss.color.unwrap_or(Color::from_hex("#1F2937"));
        let popup_border = self.mss.border_color.map(|c| c.lighten(0.2)).unwrap_or(Color::from_hex("#E5E7EB"));
        list.push_rect_bordered(popup, popup_bg, [12.0; 4], Border::new(1.0, popup_border));

        let sv_rect = self.sv_field_rect(popup);
        list.push_rect_bordered(sv_rect, Color::TRANSPARENT, [4.0; 4], Border::new(1.0, popup_border));
        self.draw_sv_field(list, sv_rect);

        let hue_rect = self.hue_bar_rect(popup);
        list.push_rect_bordered(hue_rect, Color::TRANSPARENT, [4.0; 4], Border::new(1.0, popup_border));
        self.draw_hue_bar(list, hue_rect);

        let r_rect = self.rgb_slider_rect(popup, 0);
        self.draw_rgb_slider(list, r_rect, "R", self.color.r, Color::from_hex("#EF4444"), popup_fg);

        let g_rect = self.rgb_slider_rect(popup, 1);
        self.draw_rgb_slider(list, g_rect, "G", self.color.g, Color::from_hex("#22C55E"), popup_fg);

        let b_rect = self.rgb_slider_rect(popup, 2);
        self.draw_rgb_slider(list, b_rect, "B", self.color.b, self.mss.accent_color.unwrap_or(Color::from_hex("#3B82F6")), popup_fg);

        let hex_rect = self.hex_row_rect(popup);
        let hex_label_rect = Rect::new(hex_rect.origin, Size::new(36.0, hex_rect.size.height));
        list.push_text("HEX", hex_label_rect, popup_fg.with_alpha(0.6), 11.0);

        let hex_input_rect = Rect::new(
            Point::new(hex_rect.x() + 36.0, hex_rect.y() + 2.0),
            Size::new(hex_rect.size.width - 36.0, hex_rect.size.height - 4.0),
        );
        let hex_input_bg = self.mss.background_color.map(|c| c.darken(0.03)).unwrap_or(Color::from_hex("#F9FAFB"));
        let hex_input_border = self.mss.border_color.unwrap_or(Color::from_hex("#D1D5DB"));
        list.push_rect_bordered(hex_input_rect, hex_input_bg, [4.0; 4], Border::new(1.0, hex_input_border));
        let hex_text_rect = Rect::new(
            Point::new(hex_input_rect.x() + 8.0, hex_input_rect.y() + (hex_input_rect.size.height - 12.0) / 2.0),
            Size::new(hex_input_rect.size.width - 16.0, 14.0),
        );
        list.push_text(&self.hex_input, hex_text_rect, popup_fg, 12.0);

        let preview = self.preview_rect(popup);
        let half_w = preview.size.width / 2.0;
        let old_rect = Rect::new(preview.origin, Size::new(half_w, preview.size.height));
        let new_rect = Rect::new(
            Point::new(preview.x() + half_w, preview.y()),
            Size::new(half_w, preview.size.height),
        );
        list.push_rect(old_rect, self.color.to_color(), [4.0, 0.0, 0.0, 4.0]);
        let new_color = ColorValue::from_hsv(self.hue, self.sat, self.val);
        list.push_rect(new_rect, new_color.to_color(), [0.0, 4.0, 4.0, 0.0]);

        list.end_overlay();
    }

    fn handle_event(&mut self, event: &Event, ctx: &mut EventContext) -> EventResult {
        match event {
            Event::MouseMove(pos) => {
                if self.drag_target != DragTarget::None && self.is_open {
                    let popup = self.popup_rect();
                    match self.drag_target {
                        DragTarget::SvField => self.handle_sv_drag(*pos, popup),
                        DragTarget::HueBar => self.handle_hue_drag(*pos, popup),
                        DragTarget::None => {}
                    }
                    self.fire_change();
                    ctx.request_paint();
                    return EventResult::Handled;
                }

                if self.bounds.contains(*pos) {
                    ctx.set_cursor(CursorIcon::Pointer);
                    return EventResult::Handled;
                }

                if self.is_open {
                    let popup = self.popup_rect();
                    if popup.contains(*pos) {
                        ctx.set_cursor(CursorIcon::Crosshair);
                        return EventResult::Handled;
                    }
                }
                EventResult::Ignored
            }
            Event::MouseDown { button, position } if *button == MouseButton::Left => {
                if self.bounds.contains(*position) {
                    self.is_open = !self.is_open;
                    if self.is_open {
                        let popup_h = self.popup_height();
                        let trigger_h = self.bounds.size.height;
                        self.opens_upward = self.bounds.y() + trigger_h + 4.0 + popup_h > ctx.viewport_size().height
                            && self.bounds.y() >= popup_h + 4.0;
                        let popup = self.popup_rect();
                        let overlay_bounds = if self.opens_upward {
                            Rect::new(
                                Point::new(self.bounds.x(), popup.y()),
                                Size::new(POPUP_WIDTH, popup.size.height + 4.0 + trigger_h),
                            )
                        } else {
                            Rect::new(
                                self.bounds.origin,
                                Size::new(POPUP_WIDTH, trigger_h + 4.0 + popup.size.height),
                            )
                        };
                        ctx.register_overlay(overlay_bounds, false);
                    } else {
                        ctx.unregister_overlay();
                    }
                    ctx.request_paint();
                    return EventResult::Handled;
                }

                if self.is_open {
                    let popup = self.popup_rect();

                    let sv = self.sv_field_rect(popup);
                    if sv.contains(*position) {
                        self.drag_target = DragTarget::SvField;
                        self.handle_sv_drag(*position, popup);
                        self.fire_change();
                        ctx.request_paint();
                        return EventResult::Handled;
                    }

                    let hue = self.hue_bar_rect(popup);
                    if hue.contains(*position) {
                        self.drag_target = DragTarget::HueBar;
                        self.handle_hue_drag(*position, popup);
                        self.fire_change();
                        ctx.request_paint();
                        return EventResult::Handled;
                    }

                    for ch in 0..3 {
                        let slider_rect = self.rgb_slider_rect(popup, ch);
                        if slider_rect.contains(*position) {
                            let label_w = 20.0;
                            let value_w = 36.0;
                            let bar_x = slider_rect.x() + label_w;
                            let bar_w = slider_rect.size.width - label_w - value_w;
                            let t = ((position.x - bar_x) / bar_w).clamp(0.0, 1.0);
                            let val = (t * 255.0).round() as u8;
                            match ch {
                                0 => self.color.r = val,
                                1 => self.color.g = val,
                                2 => self.color.b = val,
                                _ => {}
                            }
                            let (h, s, v) = self.color.to_hsv();
                            self.hue = h;
                            self.sat = s;
                            self.val = v;
                            self.hex_input = self.color.to_hex();
                            self.fire_change();
                            ctx.request_paint();
                            return EventResult::Handled;
                        }
                    }

                    if popup.contains(*position) {
                        return EventResult::Handled;
                    }

                    self.is_open = false;
                    ctx.unregister_overlay();
                    ctx.request_paint();
                    return EventResult::Handled;
                }
                EventResult::Ignored
            }
            Event::MouseUp { button, .. } if *button == MouseButton::Left => {
                if self.drag_target != DragTarget::None {
                    self.drag_target = DragTarget::None;
                    return EventResult::Handled;
                }
                EventResult::Ignored
            }
            Event::CharInput(ch) if self.is_open => {
                if *ch == '\x08' {
                    if self.hex_input.len() > 1 {
                        self.hex_input.pop();
                        if let Some(c) = ColorValue::from_hex(&self.hex_input) {
                            self.color = c;
                            let (h, s, v) = c.to_hsv();
                            self.hue = h;
                            self.sat = s;
                            self.val = v;
                            self.fire_change();
                        }
                        ctx.request_paint();
                        return EventResult::Handled;
                    }
                } else if ch.is_ascii_hexdigit() || *ch == '#' {
                    if self.hex_input.len() < 7 {
                        self.hex_input.push(*ch);
                        if let Some(c) = ColorValue::from_hex(&self.hex_input) {
                            self.color = c;
                            let (h, s, v) = c.to_hsv();
                            self.hue = h;
                            self.sat = s;
                            self.val = v;
                            self.fire_change();
                        }
                        ctx.request_paint();
                        return EventResult::Handled;
                    }
                }
                EventResult::Ignored
            }
            _ => EventResult::Ignored,
        }
    }

    fn children(&self) -> &[ElementId] { &self.child_ids }
    fn bounds(&self) -> Rect { self.bounds }
    fn set_position(&mut self, pos: Point) { self.bounds.origin = pos; }
    fn mark_dirty(&mut self, flags: DirtyFlags) { self.dirty_flags |= flags; }
    fn clear_dirty(&mut self, flags: DirtyFlags) { self.dirty_flags.remove(flags); }
    fn is_dirty(&self, flags: DirtyFlags) -> bool { self.dirty_flags.contains(flags) }
    fn id(&self) -> ElementId { self.id }
    fn set_id(&mut self, id: ElementId) { self.id = id; }
    fn mount(&mut self, _tree: &mut ElementTree) {}

    fn layout_hint(&self) -> LayoutHint {
        if self.has_child {
            LayoutHint::Padding { left: 0.0, top: 0.0, right: 0.0, bottom: 0.0 }
        } else {
            LayoutHint::default()
        }
    }

    fn set_classes(&mut self, classes: Vec<String>) {
        self.classes = classes;
        self.mark_dirty(DirtyFlags::RENDER);
    }

    fn get_classes(&self) -> &[String] { &self.classes }

    fn element_type_name(&self) -> &str { "ColorPicker" }

    fn reset_mss_styles(&mut self) { self.mss.reset(); }
    fn mss(&self) -> Option<&crate::mss::MssFields> { Some(&self.mss) }
    fn apply_computed_style(&mut self, style: &ComputedStyle) {
        self.mss.apply(style);
        if let Some(w) = self.mss.width { self.width = Some(w); }
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

impl StyledElement for ColorPickerElement {
    fn apply_style(&mut self, _style: &ComputedStyle) {
        self.mark_dirty(DirtyFlags::LAYOUT | DirtyFlags::RENDER);
    }

    fn classes(&self) -> &[String] { &self.classes }

    fn set_classes(&mut self, classes: Vec<String>) {
        self.classes = classes;
        self.mark_dirty(DirtyFlags::RENDER);
    }
}
