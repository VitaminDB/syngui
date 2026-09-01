use crate::core::{Color, Point, Rect, RectExt, Size};
use crate::input::{Event, EventResult, MouseButton};
use crate::layout::Constraints;
use crate::mss::ComputedStyle;
use crate::mss::{IconState, MssFields};
use crate::render::{Border, DisplayList};
use crate::widget::context::{EventContext, EventContextExt};
use crate::widget::{DirtyFlags, Element, ElementId, ElementTree, StyledElement, UpdateContext, Widget};
use std::any::Any;
use std::cell::{Cell, RefCell};
use std::sync::Arc;
use crate::core::sync::Mutex;
use crate::signal::{RwSignal, use_signal};
use super::placement::{clamp_span, fit_span};

#[derive(Clone, Debug)]
pub struct MenuItem {
    pub id: String,
    pub label: String,
    pub icon: Option<String>,
    pub shortcut: Option<String>,
    pub disabled: bool,
    pub separator: bool,
    pub children: Vec<MenuItem>,
}

impl MenuItem {
    pub fn new(id: impl Into<String>, label: impl Into<String>) -> Self {
        Self {
            id: id.into(),
            label: label.into(),
            icon: None,
            shortcut: None,
            disabled: false,
            separator: false,
            children: Vec::new(),
        }
    }

    pub fn separator() -> Self {
        Self {
            id: String::new(),
            label: String::new(),
            icon: None,
            shortcut: None,
            disabled: false,
            separator: true,
            children: Vec::new(),
        }
    }

    pub fn icon(mut self, icon: impl Into<String>) -> Self {
        self.icon = Some(icon.into());
        self
    }

    pub fn shortcut(mut self, shortcut: impl Into<String>) -> Self {
        self.shortcut = Some(shortcut.into());
        self
    }

    pub fn disabled(mut self, disabled: bool) -> Self {
        self.disabled = disabled;
        self
    }

    pub fn children(mut self, children: Vec<MenuItem>) -> Self {
        self.children = children;
        self
    }

    pub fn has_submenu(&self) -> bool {
        !self.children.is_empty()
    }
}

#[derive(Clone, Copy, Debug, Default)]
pub enum PopupAnchor {
    #[default]
    Position,
    BottomStart,
    BottomEnd,
}

pub struct PopupMenu {
    pub items: Vec<MenuItem>,
    pub position: RwSignal<Point>,
    pub anchor_rect: RwSignal<Rect>,
    pub anchor: PopupAnchor,
    pub is_open: RwSignal<bool>,
    pub on_select: Option<Arc<Mutex<dyn FnMut(&str) + Send>>>,
    pub min_width: f32,
}

impl PopupMenu {
    pub fn new() -> Self {
        Self {
            items: Vec::new(),
            position: use_signal(Point::zero()),
            anchor_rect: use_signal(Rect::zero()),
            anchor: PopupAnchor::Position,
            is_open: use_signal(false),
            on_select: None,
            min_width: 180.0,
        }
    }

    pub fn items(mut self, items: Vec<MenuItem>) -> Self {
        self.items = items;
        self
    }

    pub fn position(mut self, pos: RwSignal<Point>) -> Self {
        self.position = pos;
        self
    }

    pub fn is_open(mut self, state: RwSignal<bool>) -> Self {
        self.is_open = state;
        self
    }

    pub fn on_select(mut self, callback: impl FnMut(&str) + Send + 'static) -> Self {
        self.on_select = Some(Arc::new(Mutex::new(callback)));
        self
    }

    pub fn min_width(mut self, width: f32) -> Self {
        self.min_width = width;
        self
    }

    pub fn anchor(mut self, anchor: PopupAnchor) -> Self {
        self.anchor = anchor;
        self
    }

    pub fn anchor_rect(mut self, rect: RwSignal<Rect>) -> Self {
        self.anchor_rect = rect;
        self
    }
}

impl Default for PopupMenu {
    fn default() -> Self { Self::new() }
}

impl Widget for PopupMenu {
    fn create_element(&self) -> Box<dyn Element> {
        Box::new(PopupMenuElement {
            id: ElementId::new(),
            items: self.items.clone(),
            position: self.position,
            anchor_rect: self.anchor_rect,
            anchor: self.anchor,
            is_open: self.is_open,
            on_select: self.on_select.clone(),
            min_width: self.min_width,
            bounds: Rect::zero(),
            viewport_size: Cell::new(Size::zero()),
            open_path: Vec::new(),
            classes: Vec::new(),
            dirty_flags: DirtyFlags::LAYOUT | DirtyFlags::RENDER,
            mss: MssFields::new(),
            mss_popup_bg: None,
            mss_popup_fg: None,
            mss_popup_border: None,
            mss_popup_hover_bg: None,
            mss_popup_hover_fg: None,
            mss_popup_arrow: None,
            text_measure: RefCell::new(None),
            level_widths: RefCell::new(Vec::new()),
        })
    }

    fn can_update(&self, other: &dyn Any) -> bool { other.is::<Self>() }
    fn as_any(&self) -> &dyn Any { self }
    fn as_any_mut(&mut self) -> &mut dyn Any { self }
    fn mount(&self, _tree: &mut ElementTree, _parent_id: ElementId) {}
}

const MENU_ITEM_HEIGHT: f32 = 32.0;
const SEPARATOR_HEIGHT: f32 = 9.0;
const MENU_PADDING: f32 = 4.0;

const ITEM_HORIZONTAL_PADDING: f32 = 4.0;
const ITEM_INNER_PADDING_LEFT: f32 = 8.0;
const ITEM_INNER_PADDING_RIGHT: f32 = 8.0;
const ITEM_ICON_BOX: f32 = 20.0;
const ITEM_ICON_GAP: f32 = 4.0;
const LABEL_FONT_SIZE: f32 = 13.0;
const SHORTCUT_FONT_SIZE: f32 = 12.0;
const SHORTCUT_LABEL_GAP: f32 = 16.0;
const CHEVRON_BOX: f32 = 14.0;
const CHEVRON_GLYPH: &str = "\u{E5CC}";

enum EnterAction {
    Select(String),
    OpenChild(usize),
}

struct PopupMenuElement {
    id: ElementId,
    items: Vec<MenuItem>,
    position: RwSignal<Point>,
    anchor_rect: RwSignal<Rect>,
    anchor: PopupAnchor,
    is_open: RwSignal<bool>,
    on_select: Option<Arc<Mutex<dyn FnMut(&str) + Send>>>,
    min_width: f32,
    bounds: Rect,
    viewport_size: Cell<Size>,
    open_path: Vec<usize>,
    classes: Vec<String>,
    dirty_flags: DirtyFlags,
    mss: MssFields,
    mss_popup_bg: Option<Color>,
    mss_popup_fg: Option<Color>,
    mss_popup_border: Option<Color>,
    mss_popup_hover_bg: Option<Color>,
    mss_popup_hover_fg: Option<Color>,
    mss_popup_arrow: Option<Color>,
    text_measure: RefCell<Option<Arc<dyn crate::widget::context::TextMeasure>>>,
    level_widths: RefCell<Vec<f32>>,
}

impl PopupMenuElement {
    fn estimate_text_width(&self, text: &str, font_size: f32) -> f32 {
        if let Some(tm) = self.text_measure.borrow().as_ref() {
            return tm.measure_text_width_styled(text, font_size, text.chars().count(), false, None);
        }
        text.chars().count() as f32 * font_size * 0.6
    }

    fn item_required_width(&self, item: &MenuItem) -> f32 {
        if item.separator {
            return 0.0;
        }
        let icon_w = if item.icon.is_some() { ITEM_ICON_BOX + ITEM_ICON_GAP } else { 0.0 };
        let label_w = self.estimate_text_width(&item.label, LABEL_FONT_SIZE);
        let trailing_w = if item.has_submenu() {
            CHEVRON_BOX + SHORTCUT_LABEL_GAP
        } else if let Some(s) = item.shortcut.as_ref() {
            self.estimate_text_width(s, SHORTCUT_FONT_SIZE) + SHORTCUT_LABEL_GAP
        } else {
            0.0
        };
        ITEM_HORIZONTAL_PADDING * 2.0
            + ITEM_INNER_PADDING_LEFT
            + icon_w
            + label_w
            + trailing_w
            + ITEM_INNER_PADDING_RIGHT
    }

    fn items_at_level(&self, level: usize) -> &[MenuItem] {
        let mut cur: &[MenuItem] = &self.items;
        for k in 0..level {
            let Some(&i) = self.open_path.get(k) else { return &[]; };
            let Some(item) = cur.get(i) else { return &[]; };
            cur = &item.children;
        }
        cur
    }

    fn level_width(&self, level: usize) -> f32 {
        {
            let widths = self.level_widths.borrow();
            if let Some(&w) = widths.get(level) {
                return w;
            }
        }
        let items = self.items_at_level(level);
        let max_item_w = items
            .iter()
            .map(|i| self.item_required_width(i))
            .fold(0.0f32, f32::max);
        let w = max_item_w.max(self.min_width).ceil();
        let mut widths = self.level_widths.borrow_mut();
        if widths.len() <= level {
            widths.resize(level + 1, 0.0);
        }
        widths[level] = w;
        w
    }

    fn level_height(&self, level: usize) -> f32 {
        let content: f32 = self.items_at_level(level).iter().map(|item| {
            if item.separator { SEPARATOR_HEIGHT } else { MENU_ITEM_HEIGHT }
        }).sum();
        content + MENU_PADDING * 2.0
    }

    fn ensure_text_measure(&self, tm: &Arc<dyn crate::widget::context::TextMeasure>) {
        let mut cur = self.text_measure.borrow_mut();
        if cur.is_none() {
            *cur = Some(tm.clone());
            drop(cur);
            self.level_widths.borrow_mut().clear();
        }
    }

    fn menu_rect(&self) -> Rect {
        let height = self.level_height(0);
        let viewport = self.viewport_size.get();
        let width = self.level_width(0);

        // `flip_up_to` — низ перевёрнутого варианта: меню раскроется вверх,
        // упершись в эту линию. Для `Position` это сама точка открытия
        // (курсор или край кнопки), для якорных вариантов — верх якоря.
        let (x, y, flip_up_to) = match self.anchor {
            PopupAnchor::Position => {
                let pos = self.position.get_untracked();
                (pos.x, pos.y, pos.y)
            }
            PopupAnchor::BottomStart => {
                let r = self.anchor_rect.get_untracked();
                (r.origin.x, r.origin.y + r.size.height, r.origin.y)
            }
            PopupAnchor::BottomEnd => {
                let r = self.anchor_rect.get_untracked();
                (r.origin.x + r.size.width - width, r.origin.y + r.size.height, r.origin.y)
            }
        };

        let x = clamp_span(x, width, viewport.width);
        let y = fit_span(y, height, flip_up_to, viewport.height);

        Rect::new(Point::new(x, y), Size::new(width, height))
    }

    fn level_rect(&self, level: usize) -> Rect {
        if level == 0 {
            return self.menu_rect();
        }
        let parent_level = self.level_rect(level - 1);
        let parent_idx = self.open_path.get(level - 1).copied().unwrap_or(0);
        let parent_item_rect = self.item_rect_at(level - 1, parent_idx);

        let width = self.level_width(level);
        let height = self.level_height(level);
        let viewport = self.viewport_size.get();

        // Подменю не переворачивается по вертикали — только прижимается;
        // по горизонтали уходит влево от родителя, если справа нет места.
        let y = clamp_span(parent_item_rect.y() - MENU_PADDING, height, viewport.height);
        let x = fit_span(parent_level.right(), width, parent_level.x(), viewport.width);

        Rect::new(Point::new(x, y), Size::new(width, height))
    }

    fn item_rect_at(&self, level: usize, index: usize) -> Rect {
        let lvl = self.level_rect(level);
        let mut y = lvl.y() + MENU_PADDING;
        for (i, item) in self.items_at_level(level).iter().enumerate() {
            let h = if item.separator { SEPARATOR_HEIGHT } else { MENU_ITEM_HEIGHT };
            if i == index {
                return Rect::new(Point::new(lvl.x(), y), Size::new(lvl.size.width, h));
            }
            y += h;
        }
        Rect::zero()
    }

    fn hover_at(&self, level: usize) -> Option<usize> {
        self.open_path.get(level).copied()
    }

    fn invalidate_widths_below(&self, stable_levels: usize) {
        let mut widths = self.level_widths.borrow_mut();
        if widths.len() > stable_levels {
            widths.truncate(stable_levels);
        }
    }

    fn first_selectable(items: &[MenuItem]) -> Option<usize> {
        items.iter().position(|i| !i.separator && !i.disabled)
    }

    fn close_menu(&mut self, ctx: &mut EventContext) {
        self.is_open.set(false);
        self.open_path.clear();
        self.invalidate_widths_below(0);
        ctx.request_paint();
    }

    fn draw_level(
        &self,
        list: &mut DisplayList,
        level: usize,
        bg: Color,
        fg: Color,
        border: Color,
        hover_bg: Color,
        arrow: Color,
    ) {
        let lvl = self.level_rect(level);

        list.push_shadow(lvl, Color::new(0.0, 0.0, 0.0, 0.12), 12.0, (0.0, 4.0), [8.0; 4]);
        list.push_rect_bordered(lvl, bg, [8.0; 4], Border { width: 1.0, color: border });

        let items = self.items_at_level(level);
        let hover_idx = self.hover_at(level);
        let mut y = lvl.y() + MENU_PADDING;
        for (i, item) in items.iter().enumerate() {
            if item.separator {
                let sep_rect = Rect::new(
                    Point::new(lvl.x() + 8.0, y + 4.0),
                    Size::new(lvl.size.width - 16.0, 1.0),
                );
                list.push_rect(sep_rect, border, [0.0; 4]);
                y += SEPARATOR_HEIGHT;
                continue;
            }

            let item_rect = Rect::new(
                Point::new(lvl.x() + 4.0, y),
                Size::new(lvl.size.width - 8.0, MENU_ITEM_HEIGHT),
            );

            let is_hover = hover_idx == Some(i);

            if is_hover && !item.disabled {
                list.push_rect(item_rect, hover_bg, [4.0; 4]);
            }

            let text_color = if item.disabled {
                fg.with_alpha(0.4)
            } else if is_hover {
                self.mss_popup_hover_fg.unwrap_or(fg)
            } else {
                fg
            };

            let mut text_x = item_rect.x() + ITEM_INNER_PADDING_LEFT;
            if let Some(ref icon) = item.icon {
                let icon_rect = Rect::new(
                    Point::new(text_x, item_rect.y()),
                    Size::new(ITEM_ICON_BOX, item_rect.size.height),
                );
                let icon_state = if item.disabled {
                    IconState::Disabled
                } else if is_hover {
                    IconState::Hover
                } else {
                    IconState::Normal
                };
                let icon_color = self.mss.icon_color(icon_state, text_color);
                list.push_text_centered(icon, icon_rect, icon_color, 14.0);
                text_x += ITEM_ICON_BOX + ITEM_ICON_GAP;
            }

            let item_right = item_rect.x() + item_rect.size.width;

            let (trailing_block_w, trailing_pad_after_label) = if item.has_submenu() {
                (CHEVRON_BOX, SHORTCUT_LABEL_GAP)
            } else if let Some(ref shortcut) = item.shortcut {
                let w = self.estimate_text_width(shortcut, SHORTCUT_FONT_SIZE).ceil();
                let _ = shortcut;
                (w, SHORTCUT_LABEL_GAP)
            } else {
                (0.0, 0.0)
            };

            if item.has_submenu() {
                let cx = item_right - ITEM_INNER_PADDING_RIGHT - CHEVRON_BOX;
                let cr = Rect::new(
                    Point::new(cx, item_rect.y()),
                    Size::new(CHEVRON_BOX, item_rect.size.height),
                );
                list.push_text_centered(CHEVRON_GLYPH, cr, arrow, 14.0);
            } else if let Some(ref shortcut) = item.shortcut {
                let sc_x = item_right - ITEM_INNER_PADDING_RIGHT - trailing_block_w;
                let sc_rect = Rect::new(
                    Point::new(sc_x, item_rect.y()),
                    Size::new(trailing_block_w, item_rect.size.height),
                );
                list.push_text(shortcut, sc_rect, fg.with_alpha(0.4), SHORTCUT_FONT_SIZE);
            }

            let label_right_edge = if trailing_block_w > 0.0 {
                item_right - ITEM_INNER_PADDING_RIGHT - trailing_block_w - trailing_pad_after_label
            } else {
                item_right - ITEM_INNER_PADDING_RIGHT
            };
            let label_w = (label_right_edge - text_x).max(0.0);
            let label_rect = Rect::new(
                Point::new(text_x, item_rect.y()),
                Size::new(label_w, item_rect.size.height),
            );
            list.push_text(&item.label, label_rect, text_color, LABEL_FONT_SIZE);

            y += MENU_ITEM_HEIGHT;
        }
    }
}

impl Element for PopupMenuElement {
    fn update(&mut self, widget: &dyn Widget, _ctx: &mut UpdateContext) {
        if let Some(m) = widget.as_any().downcast_ref::<PopupMenu>() {
            self.items = m.items.clone();
            self.position = m.position;
            self.anchor_rect = m.anchor_rect;
            self.anchor = m.anchor;
            self.is_open = m.is_open;
            self.on_select = m.on_select.clone();
            self.min_width = m.min_width;
            self.open_path.clear();
            self.level_widths.borrow_mut().clear();
            self.mark_dirty(DirtyFlags::RENDER);
        }
    }

    fn layout(&mut self, constraints: Constraints) -> Size {
        let w = if constraints.max_width.is_finite() { constraints.max_width } else { 0.0 };
        let h = if constraints.max_height.is_finite() { constraints.max_height } else { 0.0 };
        self.bounds = Rect::new(Point::zero(), Size::new(w, h));
        if w > 0.0 && h > 0.0 {
            self.viewport_size.set(Size::new(w, h));
        }
        Size::zero()
    }

    fn build_display_list(&self, list: &mut DisplayList, _clip: Rect) {
        if !self.is_open.get_untracked() {
            return;
        }

        list.begin_overlay();
        self.viewport_size.set(list.surface_size());

        let bg = self.mss_popup_bg
            .or(self.mss.background_color)
            .unwrap_or(Color::WHITE);
        let fg = self.mss_popup_fg
            .or(self.mss.color)
            .unwrap_or(Color::from_hex("#374151"));
        let border = self.mss_popup_border
            .or(self.mss.border_color)
            .unwrap_or(Color::from_hex("#E5E7EB"));
        let hover_bg = self.mss_popup_hover_bg.unwrap_or_else(|| bg.darken(0.05));
        let arrow = self.mss_popup_arrow.unwrap_or_else(|| fg.with_alpha(0.55));

        let visible_levels = self.open_path.len().max(1);
        for level in 0..visible_levels {
            self.draw_level(list, level, bg, fg, border, hover_bg, arrow);
            let Some(idx) = self.open_path.get(level).copied() else { break; };
            let items = self.items_at_level(level);
            let Some(item) = items.get(idx) else { break; };
            if !item.has_submenu() { break; }
            if level + 1 >= self.open_path.len() { break; }
        }

        list.end_overlay();
    }

    fn handle_event(&mut self, event: &Event, ctx: &mut EventContext) -> EventResult {
        let is_open = self.is_open.get_untracked();

        if !is_open {
            return EventResult::Ignored;
        }

        if let Some(tm) = ctx.text_measure.as_ref() {
            self.ensure_text_measure(tm);
        }

        match event {
            Event::MouseMove(pos) => {
                let visible_levels = self.open_path.len().max(1);
                let mut hit: Option<(usize, usize)> = None;
                for level in (0..visible_levels).rev() {
                    let items_len = self.items_at_level(level).len();
                    for i in 0..items_len {
                        let item = &self.items_at_level(level)[i];
                        if item.separator || item.disabled { continue; }
                        let r = self.item_rect_at(level, i);
                        if r.contains(*pos) {
                            hit = Some((level, i));
                            break;
                        }
                    }
                    if hit.is_some() { break; }
                }

                if let Some((level, idx)) = hit {
                    let item_has_children = self
                        .items_at_level(level)
                        .get(idx)
                        .map(|i| i.has_submenu())
                        .unwrap_or(false);

                    let mut new_path = self.open_path.clone();
                    new_path.truncate(level);
                    new_path.push(idx);

                    if item_has_children {
                        let first_child = {
                            let items = self.items_at_level(level);
                            items.get(idx)
                                .and_then(|p| Self::first_selectable(&p.children))
                        };
                        if let Some(ci) = first_child {
                            new_path.push(ci);
                        }
                    }

                    if new_path != self.open_path {
                        self.open_path = new_path;
                        self.invalidate_widths_below(level + 2);
                        ctx.request_paint();
                    }
                }
                EventResult::Handled
            }
            Event::MouseDown { button, position } => {
                if *button == MouseButton::Left {
                    let visible_levels = self.open_path.len().max(1);
                    for level in (0..visible_levels).rev() {
                        let items_len = self.items_at_level(level).len();
                        for i in 0..items_len {
                            let (is_sep, is_disabled, is_leaf, item_id) = {
                                let item = &self.items_at_level(level)[i];
                                (item.separator, item.disabled, item.children.is_empty(), item.id.clone())
                            };
                            if is_sep || is_disabled { continue; }
                            let r = self.item_rect_at(level, i);
                            if r.contains(*position) {
                                if is_leaf {
                                    if let Some(ref cb) = self.on_select {
                                        if let Ok(mut f) = cb.lock() { f(&item_id); }
                                    }
                                    self.close_menu(ctx);
                                }
                                return EventResult::Handled;
                            }
                        }
                    }
                    self.close_menu(ctx);
                    return EventResult::Handled;
                }
                EventResult::Handled
            }
            Event::KeyDown(crate::input::Key::Escape) | Event::BackPressed => {
                self.close_menu(ctx);
                EventResult::Handled
            }
            Event::KeyDown(crate::input::Key::Right) => {
                if !self.open_path.is_empty() {
                    let level = self.open_path.len() - 1;
                    let idx = self.open_path[level];
                    let first_child = {
                        let items = self.items_at_level(level);
                        items.get(idx).and_then(|p| {
                            if p.has_submenu() { Self::first_selectable(&p.children) }
                            else { None }
                        })
                    };
                    if let Some(c) = first_child {
                        self.open_path.push(c);
                        self.invalidate_widths_below(self.open_path.len());
                        ctx.request_paint();
                    }
                }
                EventResult::Handled
            }
            Event::KeyDown(crate::input::Key::Left) => {
                if self.open_path.len() > 1 {
                    self.open_path.pop();
                    self.invalidate_widths_below(self.open_path.len());
                    ctx.request_paint();
                }
                EventResult::Handled
            }
            Event::KeyDown(crate::input::Key::Up) => {
                if self.open_path.is_empty() {
                    if let Some(idx) = Self::first_selectable(self.items_at_level(0)) {
                        self.open_path.push(idx);
                        ctx.request_paint();
                    }
                    return EventResult::Handled;
                }
                let level = self.open_path.len() - 1;
                let cur = self.open_path[level];
                let new_idx = {
                    let items = self.items_at_level(level);
                    let mut found = None;
                    for i in (0..cur).rev() {
                        if !items[i].separator && !items[i].disabled { found = Some(i); break; }
                    }
                    found
                };
                if let Some(i) = new_idx {
                    self.open_path[level] = i;
                    self.invalidate_widths_below(level + 1);
                    ctx.request_paint();
                }
                EventResult::Handled
            }
            Event::KeyDown(crate::input::Key::Down) => {
                if self.open_path.is_empty() {
                    if let Some(idx) = Self::first_selectable(self.items_at_level(0)) {
                        self.open_path.push(idx);
                        ctx.request_paint();
                    }
                    return EventResult::Handled;
                }
                let level = self.open_path.len() - 1;
                let cur = self.open_path[level];
                let new_idx = {
                    let items = self.items_at_level(level);
                    let mut found = None;
                    for i in (cur + 1)..items.len() {
                        if !items[i].separator && !items[i].disabled { found = Some(i); break; }
                    }
                    found
                };
                if let Some(i) = new_idx {
                    self.open_path[level] = i;
                    self.invalidate_widths_below(level + 1);
                    ctx.request_paint();
                }
                EventResult::Handled
            }
            Event::KeyDown(crate::input::Key::Enter) => {
                if !self.open_path.is_empty() {
                    let level = self.open_path.len() - 1;
                    let idx = self.open_path[level];
                    let action = {
                        let item = &self.items_at_level(level)[idx];
                        if item.disabled || item.separator { None }
                        else if item.has_submenu() {
                            Self::first_selectable(&item.children).map(EnterAction::OpenChild)
                        } else {
                            Some(EnterAction::Select(item.id.clone()))
                        }
                    };
                    match action {
                        Some(EnterAction::Select(id)) => {
                            if let Some(ref cb) = self.on_select {
                                if let Ok(mut f) = cb.lock() { f(&id); }
                            }
                            self.close_menu(ctx);
                        }
                        Some(EnterAction::OpenChild(c)) => {
                            self.open_path.push(c);
                            self.invalidate_widths_below(self.open_path.len());
                            ctx.request_paint();
                        }
                        None => {}
                    }
                }
                EventResult::Handled
            }
            _ => EventResult::Handled,
        }
    }

    fn children(&self) -> &[ElementId] { &[] }
    fn bounds(&self) -> Rect { self.bounds }

    fn hit_test(&self, _point: Point) -> bool {
        self.is_open.get_untracked()
    }

    fn overlay_request(&self) -> Option<(Rect, bool)> {
        if !self.is_open.get_untracked() { return None; }
        let viewport = self.viewport_size.get();
        if viewport.width <= 0.0 || viewport.height <= 0.0 { return None; }
        Some((Rect::new(Point::zero(), viewport), true))
    }
    fn set_position(&mut self, pos: Point) { self.bounds.origin = pos; }
    fn mark_dirty(&mut self, flags: DirtyFlags) { self.dirty_flags |= flags; }
    fn clear_dirty(&mut self, flags: DirtyFlags) { self.dirty_flags.remove(flags); }
    fn is_dirty(&self, flags: DirtyFlags) -> bool { self.dirty_flags.contains(flags) }
    fn id(&self) -> ElementId { self.id }
    fn set_id(&mut self, id: ElementId) { self.id = id; }
    fn mount(&mut self, _tree: &mut ElementTree) {}
    fn set_classes(&mut self, classes: Vec<String>) { self.classes = classes; self.mark_dirty(DirtyFlags::RENDER); }
    fn get_classes(&self) -> &[String] { &self.classes }
    fn element_type_name(&self) -> &str { "PopupMenu" }
    fn reset_mss_styles(&mut self) { self.mss.reset(); }
    fn mss(&self) -> Option<&crate::mss::MssFields> { Some(&self.mss) }
    fn apply_computed_style(&mut self, style: &ComputedStyle) {
        self.mss.apply(style);
        use crate::animation::transition::mss_color_to_core;
        if let Some(c) = style.get("--popup-background").and_then(|v| v.as_color()) { self.mss_popup_bg = Some(mss_color_to_core(c)); }
        if let Some(c) = style.get("--popup-color").and_then(|v| v.as_color()) { self.mss_popup_fg = Some(mss_color_to_core(c)); }
        if let Some(c) = style.get("--popup-border").and_then(|v| v.as_color()) { self.mss_popup_border = Some(mss_color_to_core(c)); }
        if let Some(c) = style.get("--popup-hover-background").and_then(|v| v.as_color()) { self.mss_popup_hover_bg = Some(mss_color_to_core(c)); }
        if let Some(c) = style.get("--popup-hover-color").and_then(|v| v.as_color()) { self.mss_popup_hover_fg = Some(mss_color_to_core(c)); }
        if let Some(c) = style.get("--popup-submenu-arrow-color").and_then(|v| v.as_color()) { self.mss_popup_arrow = Some(mss_color_to_core(c)); }
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

    fn accessibility_info(&self) -> Option<crate::a11y::AccessibilityInfo> {
        Some(crate::a11y::AccessibilityInfo {
            role: crate::a11y::Role::Menu,
            state: crate::a11y::NodeState {
                hidden: !self.is_open.get_untracked(),
                ..Default::default()
            },
            properties: crate::a11y::NodeProperties {
                label: Some(format!("Menu with {} items", self.items.iter().filter(|i| !i.separator).count())),
                ..Default::default()
            },
        })
    }

    fn set_content_size(&mut self, size: Size) {
        if size.width > 0.0 && size.height > 0.0 {
            self.viewport_size.set(size);
        }
    }
}

impl StyledElement for PopupMenuElement {
    fn apply_style(&mut self, _style: &ComputedStyle) { self.mark_dirty(DirtyFlags::RENDER); }
    fn classes(&self) -> &[String] { &self.classes }
    fn set_classes(&mut self, classes: Vec<String>) { self.classes = classes; self.mark_dirty(DirtyFlags::RENDER); }
}
