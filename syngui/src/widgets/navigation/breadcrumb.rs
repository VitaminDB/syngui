use crate::core::{Color, Point, Rect, RectExt, Size};
use crate::input::{Event, EventResult, MouseButton};
use crate::layout::Constraints;
use crate::mss::ComputedStyle;
use crate::mss::{IconState, MssFields};
use crate::render::DisplayList;
use crate::widget::context::{EventContext, EventContextExt};
use crate::widget::{DirtyFlags, Element, ElementId, ElementTree, StyledElement, UpdateContext, Widget};
use std::any::Any;
use std::sync::Arc;
use crate::core::sync::Mutex;

#[derive(Clone, Debug, Default)]
pub struct BreadcrumbItem {
    pub text: String,
    pub icon: Option<String>,
}

impl BreadcrumbItem {
    pub fn new(text: impl Into<String>) -> Self {
        Self { text: text.into(), icon: None }
    }

    pub fn icon(mut self, icon: impl Into<String>) -> Self {
        self.icon = Some(icon.into());
        self
    }
}

pub struct Breadcrumb {
    pub items: Vec<BreadcrumbItem>,
    pub separator: String,
    pub separator_icon: Option<String>,
    pub on_click: Option<Arc<Mutex<dyn FnMut(usize) + Send>>>,
}

impl Breadcrumb {
    pub fn new() -> Self {
        Self {
            items: Vec::new(),
            separator: ">".to_string(),
            separator_icon: None,
            on_click: None,
        }
    }

    pub fn item(mut self, item: impl Into<String>) -> Self {
        self.items.push(BreadcrumbItem::new(item));
        self
    }

    pub fn rich_item(mut self, item: BreadcrumbItem) -> Self {
        self.items.push(item);
        self
    }

    pub fn icon_item(mut self, icon: impl Into<String>, text: impl Into<String>) -> Self {
        self.items.push(BreadcrumbItem::new(text).icon(icon));
        self
    }

    pub fn separator(mut self, sep: impl Into<String>) -> Self {
        self.separator = sep.into();
        self
    }

    pub fn separator_icon(mut self, icon: impl Into<String>) -> Self {
        self.separator_icon = Some(icon.into());
        self
    }

    pub fn on_click(mut self, callback: impl FnMut(usize) + Send + 'static) -> Self {
        self.on_click = Some(Arc::new(Mutex::new(callback)));
        self
    }
}

impl Default for Breadcrumb {
    fn default() -> Self {
        Self::new()
    }
}

impl Widget for Breadcrumb {
    fn create_element(&self) -> Box<dyn Element> {
        Box::new(BreadcrumbElement {
            id: ElementId::new(),
            bounds: Rect::zero(),
            items: self.items.clone(),
            separator: self.separator.clone(),
            separator_icon: self.separator_icon.clone(),
            on_click: self.on_click.clone(),
            item_rects: Vec::new(),
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

    fn as_any(&self) -> &dyn Any {
        self
    }

    fn as_any_mut(&mut self) -> &mut dyn Any {
        self
    }

    fn mount(&self, _tree: &mut ElementTree, _parent_id: ElementId) {}
}

pub struct BreadcrumbElement {
    id: ElementId,
    bounds: Rect,
    items: Vec<BreadcrumbItem>,
    separator: String,
    separator_icon: Option<String>,
    on_click: Option<Arc<Mutex<dyn FnMut(usize) + Send>>>,
    item_rects: Vec<Rect>,
    hover_index: Option<usize>,
    classes: Vec<String>,
    dirty_flags: DirtyFlags,
    mss: MssFields,
    text_measure: Option<std::sync::Arc<dyn crate::widget::context::TextMeasure>>,
}

impl BreadcrumbElement {
    const FONT_SIZE: f32 = 14.0;
    const CHAR_WIDTH: f32 = 9.5;
    const PADDING_H: f32 = 6.0;
    const PADDING_V: f32 = 4.0;
    const SEPARATOR_PADDING: f32 = 6.0;
    const ICON_RATIO: f32 = 1.15;
    const ICON_GAP: f32 = 4.0;

    fn font_size(&self) -> f32 {
        self.mss.font_size_or(Self::FONT_SIZE)
    }

    fn icon_size(&self) -> f32 {
        self.mss.icon_size
            .unwrap_or_else(|| self.font_size() * Self::ICON_RATIO)
    }

    fn item_height(&self) -> f32 {
        self.font_size().max(self.icon_size()) + Self::PADDING_V * 2.0
    }

    fn icon_gap(&self) -> f32 {
        self.mss.gap.unwrap_or(Self::ICON_GAP)
    }

    fn text_width(&self, text: &str, bold: bool) -> f32 {
        let fs = self.font_size();
        self.text_measure
            .as_ref()
            .map(|tm| tm.measure_text_width_styled(text, fs, text.chars().count(), bold, self.mss.font_family.as_deref()))
            .unwrap_or_else(|| text.chars().count() as f32 * (fs * Self::CHAR_WIDTH / Self::FONT_SIZE))
    }

    fn item_content_width(&self, item: &BreadcrumbItem, bold: bool) -> f32 {
        let text_w = self.text_width(&item.text, bold);
        if item.icon.is_some() {
            self.icon_size() + self.icon_gap() + text_w
        } else {
            text_w
        }
    }

    fn item_box_width(&self, item: &BreadcrumbItem, bold: bool) -> f32 {
        self.item_content_width(item, bold) + Self::PADDING_H * 2.0
    }

    fn separator_width(&self) -> f32 {
        if self.separator_icon.is_some() {
            self.icon_size() + Self::SEPARATOR_PADDING * 2.0
        } else {
            self.text_width(&self.separator, false) + Self::SEPARATOR_PADDING * 2.0
        }
    }

    fn hit_test_item(&self, pos: Point) -> Option<usize> {
        for (i, rect) in self.item_rects.iter().enumerate() {
            let offset_rect = Rect::new(
                Point::new(rect.x() + self.bounds.x(), rect.y() + self.bounds.y()),
                rect.size,
            );
            if offset_rect.contains(pos) {
                return Some(i);
            }
        }
        None
    }
}

impl Element for BreadcrumbElement {
    fn update(&mut self, widget: &dyn Widget, _ctx: &mut UpdateContext) {
        if let Some(bc) = widget.as_any().downcast_ref::<Breadcrumb>() {
            self.items = bc.items.clone();
            self.separator = bc.separator.clone();
            self.separator_icon = bc.separator_icon.clone();
            self.on_click = bc.on_click.clone();
            self.mark_dirty(DirtyFlags::LAYOUT | DirtyFlags::RENDER);
        }
    }

    fn layout(&mut self, constraints: Constraints) -> Size {
        let mut total_width: f32 = 0.0;
        self.item_rects.clear();
        let bold = self.mss.font_weight_or(400) >= 700;
        let sep_w = self.separator_width();
        let item_h = self.item_height();

        for (i, item) in self.items.iter().enumerate() {
            let item_width = self.item_box_width(item, bold);
            let item_rect = Rect::new(
                Point::new(total_width, 0.0),
                Size::new(item_width, item_h),
            );
            self.item_rects.push(item_rect);
            total_width += item_width;

            if i < self.items.len() - 1 {
                total_width += sep_w;
            }
        }

        let width = total_width.min(constraints.max_width).max(constraints.min_width);
        let height = item_h.min(constraints.max_height);

        self.bounds = Rect::new(Point::zero(), Size::new(width, height));
        Size::new(width, height)
    }

    fn build_display_list(&self, list: &mut DisplayList, _clip: Rect) {
        let gray_500 = self.mss.color.map(|c| c.with_alpha(0.6)).unwrap_or(Color::from_hex("#6B7280"));
        let gray_400 = self.mss.border_color.map(|c| c.with_alpha(0.7)).unwrap_or(Color::from_hex("#9CA3AF"));
        let gray_900 = self.mss.color.unwrap_or(Color::from_hex("#111827"));
        let primary = self.mss.accent_color.unwrap_or(Color::from_hex("#3B82F6"));
        let hover_bg = self.mss.background_color.unwrap_or(Color::from_hex("#F3F4F6"));
        let font_size = self.font_size();
        let font_weight = self.mss.font_weight_or(400);
        let bold = font_weight >= 700;
        let icon_size = self.icon_size();
        let item_h = self.item_height();
        let sep_w = self.separator_width();

        let last_index = if self.items.is_empty() { 0 } else { self.items.len() - 1 };
        let mut x = self.bounds.x();

        for (i, item) in self.items.iter().enumerate() {
            let is_last = i == last_index;
            let is_hover = self.hover_index == Some(i) && !is_last;

            let item_width = self.item_box_width(item, bold);
            let item_rect = Rect::new(
                Point::new(x, self.bounds.y()),
                Size::new(item_width, item_h),
            );

            if is_hover {
                list.push_rect(item_rect, hover_bg, [4.0; 4]);
            }

            let text_color = if is_last {
                gray_900
            } else if is_hover {
                primary
            } else {
                gray_500
            };

            let mut content_x = x + Self::PADDING_H;

            if let Some(ref icon) = item.icon {
                let icon_rect = Rect::new(
                    Point::new(content_x, self.bounds.y()),
                    Size::new(icon_size, item_h),
                );
                let icon_state = if is_last {
                    IconState::Selected
                } else if is_hover {
                    IconState::Hover
                } else {
                    IconState::Normal
                };
                let icon_color = self.mss.icon_color(icon_state, text_color);
                list.push_text_centered(icon, icon_rect, icon_color, icon_size);
                content_x += icon_size + self.icon_gap();
            }

            let text_rect = Rect::new(
                Point::new(content_x, self.bounds.y()),
                Size::new(item_width - (content_x - x) - Self::PADDING_H, item_h),
            );
            list.push_text_styled(
                &item.text,
                text_rect,
                text_color,
                font_size,
                crate::mss::TextAlign::DEFAULT,
                crate::mss::TextDecoration::None,
                if is_last { font_weight.max(600) } else { font_weight },
                self.mss.font_family.clone(),
            );

            x += item_width;

            if !is_last {
                let sep_rect = Rect::new(
                    Point::new(x + Self::SEPARATOR_PADDING, self.bounds.y()),
                    Size::new(sep_w - Self::SEPARATOR_PADDING * 2.0, item_h),
                );
                if let Some(ref sep_icon) = self.separator_icon {
                    list.push_text_centered(sep_icon, sep_rect, gray_400, icon_size);
                } else {
                    list.push_text_styled(
                        &self.separator,
                        sep_rect,
                        gray_400,
                        font_size,
                        crate::mss::TextAlign::DEFAULT,
                        crate::mss::TextDecoration::None,
                        font_weight,
                        self.mss.font_family.clone(),
                    );
                }
                x += sep_w;
            }
        }
    }

    fn handle_event(&mut self, event: &Event, ctx: &mut EventContext) -> EventResult {
        match event {
            Event::MouseMove(pos) => {
                let old_hover = self.hover_index;
                self.hover_index = if self.bounds.contains(*pos) {
                    self.hit_test_item(*pos)
                } else {
                    None
                };
                if self.hover_index != old_hover {
                    ctx.request_paint();
                    return EventResult::Handled;
                }
                EventResult::Ignored
            }
            Event::MouseDown { button, position } => {
                if *button == MouseButton::Left && self.bounds.contains(*position) {
                    if let Some(i) = self.hit_test_item(*position) {
                        if i + 1 < self.items.len() {
                            if let Some(ref callback) = self.on_click {
                                if let Ok(mut cb) = callback.lock() {
                                    cb(i);
                                }
                            }
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

    fn element_type_name(&self) -> &str { "Breadcrumb" }

    fn set_classes(&mut self, classes: Vec<String>) {
        self.classes = classes;
        self.mark_dirty(DirtyFlags::RENDER);
    }

    fn get_classes(&self) -> &[String] {
        &self.classes
    }

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

impl StyledElement for BreadcrumbElement {
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
