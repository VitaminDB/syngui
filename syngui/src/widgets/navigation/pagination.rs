use crate::core::{Color, Point, Rect, RectExt, Size};
use crate::input::{CursorIcon, Event, EventResult, MouseButton};
use crate::layout::Constraints;
use crate::mss::ComputedStyle;
use crate::mss::MssFields;
use crate::render::DisplayList;
use crate::widget::context::{EventContext, EventContextExt};
use crate::widget::{DirtyFlags, Element, ElementId, ElementTree, StyledElement, UpdateContext, Widget};
use std::any::Any;
use std::sync::Arc;
use crate::core::sync::Mutex;

pub struct Pagination {
    total_pages: usize,
    current_page: usize,
    max_visible: usize,
    on_page_change: Option<Arc<Mutex<dyn FnMut(usize) + Send>>>,
}

impl Pagination {
    pub fn new(total_pages: usize, current_page: usize) -> Self {
        Self {
            total_pages,
            current_page,
            max_visible: 7,
            on_page_change: None,
        }
    }

    pub fn max_visible(mut self, n: usize) -> Self {
        self.max_visible = n;
        self
    }

    pub fn on_page_change(mut self, f: impl FnMut(usize) + Send + 'static) -> Self {
        self.on_page_change = Some(Arc::new(Mutex::new(f)));
        self
    }
}

impl Widget for Pagination {
    fn create_element(&self) -> Box<dyn Element> {
        Box::new(PaginationElement {
            id: ElementId::new(),
            total_pages: self.total_pages,
            current_page: self.current_page,
            max_visible: self.max_visible,
            on_page_change: self.on_page_change.clone(),
            hover_index: None,
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

const BTN_SIZE: f32 = 36.0;
const BTN_GAP: f32 = 4.0;

pub struct PaginationElement {
    id: ElementId,
    total_pages: usize,
    current_page: usize,
    max_visible: usize,
    on_page_change: Option<Arc<Mutex<dyn FnMut(usize) + Send>>>,
    hover_index: Option<usize>,
    bounds: Rect,
    classes: Vec<String>,
    dirty_flags: DirtyFlags,
    mss: MssFields,
    text_measure: Option<std::sync::Arc<dyn crate::widget::context::TextMeasure>>,
}

#[derive(Clone, Debug)]
enum PageButton {
    Prev,
    Page(usize),
    Ellipsis,
    Next,
}

impl PaginationElement {
    fn buttons(&self) -> Vec<PageButton> {
        let mut btns = vec![PageButton::Prev];
        let total = self.total_pages;
        let current = self.current_page;
        let max_vis = self.max_visible;

        if total <= max_vis {
            for i in 1..=total {
                btns.push(PageButton::Page(i));
            }
        } else {
            btns.push(PageButton::Page(1));

            let half = (max_vis - 2) / 2;
            let start = if current <= half + 2 {
                2
            } else if current >= total - half - 1 {
                total - max_vis + 3
            } else {
                current - half
            };
            let end = (start + max_vis - 3).min(total - 1);

            if start > 2 {
                btns.push(PageButton::Ellipsis);
            }
            for i in start..=end {
                btns.push(PageButton::Page(i));
            }
            if end < total - 1 {
                btns.push(PageButton::Ellipsis);
            }

            btns.push(PageButton::Page(total));
        }

        btns.push(PageButton::Next);
        btns
    }

    fn btn_rect(&self, index: usize) -> Rect {
        let x = self.bounds.x() + index as f32 * (BTN_SIZE + BTN_GAP);
        Rect::new(Point::new(x, self.bounds.y()), Size::new(BTN_SIZE, BTN_SIZE))
    }

    fn fire_change(&self, page: usize) {
        if let Some(ref cb) = self.on_page_change {
            if let Ok(mut f) = cb.lock() { f(page); }
        }
    }
}

impl Element for PaginationElement {
    fn update(&mut self, widget: &dyn Widget, _ctx: &mut UpdateContext) {
        if let Some(p) = widget.as_any().downcast_ref::<Pagination>() {
            self.total_pages = p.total_pages;
            self.current_page = p.current_page;
            self.max_visible = p.max_visible;
            self.on_page_change = p.on_page_change.clone();
            self.mark_dirty(DirtyFlags::LAYOUT | DirtyFlags::RENDER);
        }
    }

    fn layout(&mut self, _constraints: Constraints) -> Size {
        let btns = self.buttons();
        let w = btns.len() as f32 * BTN_SIZE + (btns.len() as f32 - 1.0).max(0.0) * BTN_GAP;
        self.bounds = Rect::new(Point::zero(), Size::new(w, BTN_SIZE));
        Size::new(w, BTN_SIZE)
    }

    fn build_display_list(&self, list: &mut DisplayList, _clip: Rect) {
        let btns = self.buttons();
        let page_font_size = self.mss.font_size_or(13.0);
        let arrow_font_size = (page_font_size * 12.0 / 13.0).round();
        let font_weight = self.mss.font_weight_or(400);
        let btn_radius = self.mss.border_radius_uniform(BTN_SIZE, 6.0);

        for (i, btn) in btns.iter().enumerate() {
            let rect = self.btn_rect(i);
            let is_hovered = self.hover_index == Some(i);

            let hover_bg = self.mss.background_color.unwrap_or(Color::from_hex("#F3F4F6"));
            let disabled_color = self.mss.border_color.unwrap_or(Color::from_hex("#D1D5DB"));
            let active_arrow = self.mss.color.map(|c| c.with_alpha(0.6)).unwrap_or(Color::from_hex("#6B7280"));
            let accent = self.mss.accent_color.unwrap_or(Color::from_hex("#3B82F6"));
            let text_color_base = self.mss.color.unwrap_or(Color::from_hex("#374151"));
            let ellipsis_color = self.mss.color.map(|c| c.with_alpha(0.5)).unwrap_or(Color::from_hex("#9CA3AF"));

            match btn {
                PageButton::Prev => {
                    let bg = if is_hovered { hover_bg } else { Color::TRANSPARENT };
                    list.push_rect(rect, bg, [btn_radius; 4]);
                    let tr = Rect::new(
                        Point::new(rect.x() + 10.0, rect.y() + (BTN_SIZE - 14.0) / 2.0),
                        Size::new(16.0, 14.0),
                    );
                    let color = if self.current_page <= 1 { disabled_color } else { active_arrow };
                    list.push_text_styled("◀", tr, color, arrow_font_size,
                        crate::mss::TextAlign::DEFAULT, crate::mss::TextDecoration::None,
                        font_weight, self.mss.font_family.clone());
                }
                PageButton::Next => {
                    let bg = if is_hovered { hover_bg } else { Color::TRANSPARENT };
                    list.push_rect(rect, bg, [btn_radius; 4]);
                    let tr = Rect::new(
                        Point::new(rect.x() + 10.0, rect.y() + (BTN_SIZE - 14.0) / 2.0),
                        Size::new(16.0, 14.0),
                    );
                    let color = if self.current_page >= self.total_pages { disabled_color } else { active_arrow };
                    list.push_text_styled("▶", tr, color, arrow_font_size,
                        crate::mss::TextAlign::DEFAULT, crate::mss::TextDecoration::None,
                        font_weight, self.mss.font_family.clone());
                }
                PageButton::Page(n) => {
                    let is_current = *n == self.current_page;
                    let bg = if is_current {
                        accent
                    } else if is_hovered {
                        hover_bg
                    } else {
                        Color::TRANSPARENT
                    };
                    list.push_rect(rect, bg, [btn_radius; 4]);
                    let text_color = if is_current { Color::WHITE } else { text_color_base };
                    let label = n.to_string();
                    let tr = Rect::new(
                        Point::new(rect.x(), rect.y() + (BTN_SIZE - 14.0) / 2.0),
                        Size::new(BTN_SIZE, 14.0),
                    );
                    list.push_text_styled(&label, tr, text_color, page_font_size,
                        crate::mss::TextAlign::CENTER, crate::mss::TextDecoration::None,
                        font_weight, self.mss.font_family.clone());
                }
                PageButton::Ellipsis => {
                    let tr = Rect::new(
                        Point::new(rect.x(), rect.y() + (BTN_SIZE - 14.0) / 2.0),
                        Size::new(BTN_SIZE, 14.0),
                    );
                    list.push_text_styled("…", tr, ellipsis_color, page_font_size,
                        crate::mss::TextAlign::CENTER, crate::mss::TextDecoration::None,
                        font_weight, self.mss.font_family.clone());
                }
            }
        }
    }

    fn handle_event(&mut self, event: &Event, ctx: &mut EventContext) -> EventResult {
        let btns = self.buttons();
        match event {
            Event::MouseMove(pos) => {
                let mut found = None;
                for i in 0..btns.len() {
                    if self.btn_rect(i).contains(*pos) {
                        found = Some(i);
                        break;
                    }
                }
                if found != self.hover_index {
                    self.hover_index = found;
                    if found.is_some() { ctx.set_cursor(CursorIcon::Pointer); }
                    ctx.request_paint();
                }
                if found.is_some() { EventResult::Handled } else { EventResult::Ignored }
            }
            Event::MouseDown { button, position } if *button == MouseButton::Left => {
                for (i, btn) in btns.iter().enumerate() {
                    if self.btn_rect(i).contains(*position) {
                        match btn {
                            PageButton::Prev if self.current_page > 1 => {
                                self.fire_change(self.current_page - 1);
                            }
                            PageButton::Next if self.current_page < self.total_pages => {
                                self.fire_change(self.current_page + 1);
                            }
                            PageButton::Page(n) => {
                                self.fire_change(*n);
                            }
                            _ => {}
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

    fn element_type_name(&self) -> &str { "Pagination" }

    fn reset_mss_styles(&mut self) { self.mss.reset(); }
    fn mss(&self) -> Option<&crate::mss::MssFields> { Some(&self.mss) }
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
        self.mss.apply_transitions(base, hover, active, focus, selected);
    }
}

impl StyledElement for PaginationElement {
    fn apply_style(&mut self, _style: &ComputedStyle) {
        self.mark_dirty(DirtyFlags::RENDER);
    }

    fn classes(&self) -> &[String] { &self.classes }

    fn set_classes(&mut self, classes: Vec<String>) {
        self.classes = classes;
        self.mark_dirty(DirtyFlags::RENDER);
    }
}
