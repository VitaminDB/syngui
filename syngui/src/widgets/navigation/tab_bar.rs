use crate::core::{Point, Rect, RectExt, Size};
use crate::input::{Event, EventResult};
use crate::layout::Constraints;
use crate::mss::ComputedStyle;
use crate::mss::MssFields;
use crate::render::DisplayList;
use crate::widget::context::EventContext;
use crate::widget::{DirtyFlags, Element, ElementId, ElementTree, LayoutHint, StyledElement, UpdateContext, Widget};
use std::any::Any;
use crate::signal::use_signal;

use super::tab::{Tab, TabState};

#[derive(Clone, Copy, Debug, Default)]
pub enum TabPosition {
    #[default]
    Top,
    Bottom,
    Left,
    Right,
}

pub struct TabBar {
    pub tabs: Vec<Box<dyn Widget>>,
    pub selected_state: TabState,
    pub position: TabPosition,
}

impl TabBar {
    pub fn new() -> Self {
        Self {
            tabs: Vec::new(),
            selected_state: use_signal(0),
            position: TabPosition::default(),
        }
    }

    pub fn tab(mut self, tab: Tab) -> Self {
        self.tabs.push(Box::new(tab));
        self
    }

    pub fn position(mut self, position: TabPosition) -> Self {
        self.position = position;
        self
    }

    pub fn selected_index(&self) -> usize {
        self.selected_state.get_untracked()
    }
}

impl Default for TabBar {
    fn default() -> Self {
        Self::new()
    }
}

impl Widget for TabBar {
    fn create_element(&self) -> Box<dyn Element> {
        Box::new(TabBarElement {
            id: ElementId::new(),
            bounds: Rect::zero(),
            position: self.position,
            tab_ids: Vec::new(),
            classes: Vec::new(),
            dirty_flags: DirtyFlags::LAYOUT | DirtyFlags::RENDER,
            mss: MssFields::new(),
            equal_width: false,
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
        for tab in &self.tabs {
            let tab_element = tab.create_element();
            let tab_id = tree.insert_with_type_id(tab_element, Some(parent_id), tab.as_any().type_id());
            tab.mount(tree, tab_id);
        }
    }

    fn child_widgets(&self) -> Vec<&dyn Widget> {
        self.tabs.iter().map(|c| c.as_ref() as &dyn Widget).collect()
    }
}

pub struct TabBarElement {
    id: ElementId,
    bounds: Rect,
    position: TabPosition,
    tab_ids: Vec<ElementId>,
    classes: Vec<String>,
    dirty_flags: DirtyFlags,
    mss: MssFields,
    equal_width: bool,
}

impl Element for TabBarElement {
    fn update(&mut self, widget: &dyn Widget, _ctx: &mut UpdateContext) {
        if let Some(tab_bar) = widget.as_any().downcast_ref::<TabBar>() {
            self.position = tab_bar.position;
            self.mark_dirty(DirtyFlags::RENDER);
        }
    }

    fn layout(&mut self, constraints: Constraints) -> Size {
        let height = self.mss.height.map(|d| d.resolve(constraints.max_height)).unwrap_or(44.0);
        let width = constraints.max_width;

        self.bounds = Rect::new(Point::zero(), Size::new(width, height));
        Size::new(width, height)
    }

    fn build_display_list(&self, list: &mut DisplayList, _clip: Rect) {
        if let Some(bg_color) = self.mss.background_color {
            list.push_rect(self.bounds, bg_color, [0.0; 4]);
        }
        if let Some(border_color) = self.mss.border_color {
            let bottom_line = Rect::new(
                Point::new(self.bounds.x(), self.bounds.y() + self.bounds.size.height - 1.0),
                Size::new(self.bounds.size.width, 1.0),
            );
            list.push_rect(bottom_line, border_color, [0.0; 4]);
        }
    }

    fn handle_event(&mut self, _event: &Event, _ctx: &mut EventContext) -> EventResult {
        EventResult::Ignored
    }

    fn children(&self) -> &[ElementId] {
        &self.tab_ids
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

    fn layout_hint(&self) -> LayoutHint {
        LayoutHint::TabBar { equal_width: self.equal_width, gap: 0.0 }
    }

    fn set_classes(&mut self, classes: Vec<String>) {
        self.classes = classes;
        self.mark_dirty(DirtyFlags::RENDER);
    }

    fn get_classes(&self) -> &[String] {
        &self.classes
    }

    fn element_type_name(&self) -> &str { "TabBar" }

    fn reset_mss_styles(&mut self) { self.mss.reset(); }
    fn mss(&self) -> Option<&crate::mss::MssFields> { Some(&self.mss) }
    fn apply_computed_style(&mut self, style: &ComputedStyle) {
        self.mss.apply(style);
        let fill = style.get("--tab-fill")
            .and_then(|v| v.as_string())
            .map(|s| s.trim().to_ascii_lowercase());
        self.equal_width = matches!(fill.as_deref(), Some("equal"));
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

    fn accessibility_info(&self) -> Option<crate::a11y::AccessibilityInfo> {
        Some(crate::a11y::AccessibilityInfo {
            role: crate::a11y::Role::TabList,
            state: crate::a11y::NodeState::default(),
            properties: crate::a11y::NodeProperties::default(),
        })
    }
}

impl StyledElement for TabBarElement {
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
