use crate::core::{Point, Rect, Size};
use crate::input::{Event, EventResult};
use crate::layout::{Constraints, MainAxisAlignment, CrossAxisAlignment};
use crate::render::DisplayList;
use crate::widget::{DirtyFlags, ElementId, UpdateContext, Widget};
use crate::widget::context::EventContext;
use std::any::Any;
use std::time::Duration;

#[derive(Debug, Clone)]
pub enum LayoutHint {
    Center,
    Column { gap: f32, cross_align: CrossAxisAlignment, main_align: MainAxisAlignment, padding_left: f32, padding_top: f32, padding_right: f32, padding_bottom: f32, expand: bool },
    Row { gap: f32, offset_x: f32, cross_align: CrossAxisAlignment, main_align: MainAxisAlignment, padding_left: f32, padding_top: f32, padding_right: f32, padding_bottom: f32 },
    Stack { expand: bool },
    Padding { left: f32, top: f32, right: f32, bottom: f32 },
    Grid { columns: usize, row_gap: f32, col_gap: f32, masonry: bool },
    Scroll {
        left: f32, top: f32, right: f32, bottom: f32,
        unbounded_width: bool,
        unbounded_height: bool,
    },
    HorizontalPages,
    Split { horizontal: bool, ratio: f32, divider: f32 },
    AnimatedSize,
    Container { left: f32, top: f32, right: f32, bottom: f32 },
    Loose,
    Portal { anchor: u8, margin_a: f32, margin_b: f32 },
    FloatingWindow { x: f32, y: f32 },
    Flex { col_gap: f32, row_gap: f32, justify: MainAxisAlignment, align_items: CrossAxisAlignment },
    Tooltip {
        position: u8,
        gap: f32,
        padding_l: f32,
        padding_t: f32,
        padding_r: f32,
        padding_b: f32,
    },
    TabBar { equal_width: bool, gap: f32 },
    Positioned { x: f32, y: f32 },
    PanZoom,
}

impl Default for LayoutHint {
    fn default() -> Self {
        LayoutHint::Center
    }
}

pub trait Element: Send {
    fn update(&mut self, widget: &dyn Widget, ctx: &mut UpdateContext);

    fn layout(&mut self, constraints: Constraints) -> Size;

    fn build_display_list(&self, list: &mut DisplayList, clip: Rect);

    fn post_build_display_list(&self, _list: &mut DisplayList, _clip: Rect) {}

    fn handle_event(&mut self, event: &Event, ctx: &mut EventContext) -> EventResult;

    fn animate(&mut self, _dt: Duration) -> bool {
        false
    }

    fn needs_repaint(&self) -> bool {
        false
    }

    fn children(&self) -> &[ElementId];

    fn bounds(&self) -> Rect;

    fn set_position(&mut self, pos: Point);

    fn mark_dirty(&mut self, flags: DirtyFlags);

    fn clear_dirty(&mut self, flags: DirtyFlags);

    fn is_dirty(&self, flags: DirtyFlags) -> bool;

    fn id(&self) -> ElementId;

    fn set_id(&mut self, id: ElementId);

    fn hit_test(&self, point: Point) -> bool {
        self.bounds().contains(point)
    }

    fn wants_tab(&self) -> bool {
        false
    }

    fn passthrough_hit_test(&self) -> bool {
        false
    }

    /// Принимает ли элемент-ввод фокус по клику в этой точке. По умолчанию —
    /// во всей своей области (`find_text_input_at` уже проверил `hit_test`).
    /// Контейнер с чужими виджетами внутри (редактор документа с
    /// врезками-объектами: доска, диаграмма) отдаёт `false` над ними —
    /// тогда клик по врезке снимает фокус и каретку с контейнера, а не
    /// оставляет их в соседнем блоке.
    fn text_input_hit(&self, _point: Point) -> bool {
        true
    }

    fn overlay_request(&self) -> Option<(Rect, bool)> {
        None
    }

    fn mount(&mut self, tree: &mut super::ElementTree);

    fn take_focus_request(&mut self) -> bool {
        false
    }

    fn layout_hint(&self) -> LayoutHint {
        LayoutHint::Center
    }

    fn explicit_dimensions(&self, parent_width: f32, parent_height: f32) -> (Option<f32>, Option<f32>) {
        let _ = (parent_width, parent_height);
        (None, None)
    }

    fn min_max_dimensions(&self, parent_width: f32, parent_height: f32)
        -> (Option<f32>, Option<f32>, Option<f32>, Option<f32>)
    {
        let _ = (parent_width, parent_height);
        (None, None, None, None)
    }

    fn margin(&self) -> crate::core::EdgeInsets {
        crate::core::EdgeInsets::default()
    }

    fn set_classes(&mut self, _classes: Vec<String>) {
    }

    fn get_classes(&self) -> &[String] {
        &[]
    }

    fn set_inline_styles(&mut self, _styles: Vec<(String, crate::mss::StyleValue)>) {
    }

    fn get_inline_styles(&self) -> &[(String, crate::mss::StyleValue)] {
        &[]
    }

    fn apply_computed_style(&mut self, _style: &crate::mss::ComputedStyle) {
    }

    fn mss(&self) -> Option<&crate::mss::MssFields> {
        None
    }

    fn reset_mss_styles(&mut self) {
    }

    fn element_type_name(&self) -> &str {
        ""
    }

    fn apply_transition_styles(
        &mut self,
        _base: &crate::mss::ComputedStyle,
        _hover: Option<&crate::mss::ComputedStyle>,
        _active: Option<&crate::mss::ComputedStyle>,
        _focus: Option<&crate::mss::ComputedStyle>,
        _selected: Option<&crate::mss::ComputedStyle>,
        _checked: Option<&crate::mss::ComputedStyle>,
    ) {
    }

    fn setup_keyframe_animation(
        &mut self,
        _style: &crate::mss::ComputedStyle,
        _stylesheet: &crate::mss::StyleSheet,
    ) {
    }

    fn is_visible(&self) -> bool {
        true
    }

    fn active_child_count(&self) -> usize {
        usize::MAX
    }

    fn intercepts_child_events(&self) -> bool {
        false
    }

    fn set_content_size(&mut self, _size: Size) {
    }

    fn set_viewport_size(&mut self, _size: Size) {
    }

    fn accessibility_info(&self) -> Option<crate::a11y::AccessibilityInfo> {
        None
    }

    fn clip_content(&self) -> bool {
        false
    }

    fn clip_corner_radius(&self) -> [f32; 4] {
        [0.0; 4]
    }

    fn scroll_offset(&self) -> Point {
        Point::zero()
    }

    fn event_scale(&self) -> f32 {
        1.0
    }

    fn is_scroll_container(&self) -> bool {
        false
    }

    fn ensure_visible(&mut self, _child_rect: Rect) -> bool {
        false
    }

    fn manages_own_children(&self) -> bool {
        false
    }

    fn needs_rebuild(&self) -> bool {
        false
    }

    fn build_children(&self) -> Vec<Box<dyn super::Widget>> {
        Vec::new()
    }

    fn clear_rebuild(&mut self) {}

    fn set_row_bounds(&mut self, _bounds: Vec<(f32, f32)>) {}

    fn is_relayout_boundary(&self) -> bool {
        false
    }

    fn visible_child_indices(&self, _cull: Rect, _out: &mut Vec<usize>) -> bool {
        false
    }

    fn child_at_position(&self, _pos: Point) -> ChildHit {
        ChildHit::Unknown
    }

    fn as_any_mut(&mut self) -> Option<&mut dyn Any> {
        None
    }

    fn wants_animate_tick(&self) -> bool {
        false
    }
}

#[derive(Debug, Clone, Copy)]
pub enum ChildHit {
    Unknown,
    None,
    Index(usize),
}

impl Element for Box<dyn Element> {
    fn update(&mut self, widget: &dyn Widget, ctx: &mut UpdateContext) {
        self.as_mut().update(widget, ctx)
    }

    fn layout(&mut self, constraints: Constraints) -> Size {
        self.as_mut().layout(constraints)
    }

    fn take_focus_request(&mut self) -> bool {
        self.as_mut().take_focus_request()
    }

    fn build_display_list(&self, list: &mut DisplayList, clip: Rect) {
        self.as_ref().build_display_list(list, clip)
    }

    fn post_build_display_list(&self, list: &mut DisplayList, clip: Rect) {
        self.as_ref().post_build_display_list(list, clip)
    }

    fn handle_event(&mut self, event: &Event, ctx: &mut EventContext) -> EventResult {
        self.as_mut().handle_event(event, ctx)
    }

    fn animate(&mut self, dt: Duration) -> bool {
        self.as_mut().animate(dt)
    }

    fn needs_repaint(&self) -> bool {
        self.as_ref().needs_repaint()
    }

    fn children(&self) -> &[ElementId] {
        self.as_ref().children()
    }

    fn bounds(&self) -> Rect {
        self.as_ref().bounds()
    }

    fn hit_test(&self, point: Point) -> bool {
        self.as_ref().hit_test(point)
    }

    fn passthrough_hit_test(&self) -> bool {
        self.as_ref().passthrough_hit_test()
    }

    fn text_input_hit(&self, point: Point) -> bool {
        self.as_ref().text_input_hit(point)
    }

    fn overlay_request(&self) -> Option<(Rect, bool)> {
        self.as_ref().overlay_request()
    }

    fn set_position(&mut self, pos: Point) {
        self.as_mut().set_position(pos)
    }

    fn mark_dirty(&mut self, flags: DirtyFlags) {
        self.as_mut().mark_dirty(flags)
    }

    fn clear_dirty(&mut self, flags: DirtyFlags) {
        self.as_mut().clear_dirty(flags)
    }

    fn is_dirty(&self, flags: DirtyFlags) -> bool {
        self.as_ref().is_dirty(flags)
    }

    fn id(&self) -> ElementId {
        self.as_ref().id()
    }

    fn set_id(&mut self, id: ElementId) {
        self.as_mut().set_id(id)
    }

    fn mount(&mut self, tree: &mut super::ElementTree) {
        self.as_mut().mount(tree)
    }

    fn layout_hint(&self) -> LayoutHint {
        self.as_ref().layout_hint()
    }

    fn explicit_dimensions(&self, parent_width: f32, parent_height: f32) -> (Option<f32>, Option<f32>) {
        self.as_ref().explicit_dimensions(parent_width, parent_height)
    }

    fn margin(&self) -> crate::core::EdgeInsets {
        self.as_ref().margin()
    }

    fn set_classes(&mut self, classes: Vec<String>) {
        self.as_mut().set_classes(classes)
    }

    fn get_classes(&self) -> &[String] {
        self.as_ref().get_classes()
    }

    fn set_inline_styles(&mut self, styles: Vec<(String, crate::mss::StyleValue)>) {
        self.as_mut().set_inline_styles(styles)
    }

    fn get_inline_styles(&self) -> &[(String, crate::mss::StyleValue)] {
        self.as_ref().get_inline_styles()
    }

    fn apply_computed_style(&mut self, style: &crate::mss::ComputedStyle) {
        self.as_mut().apply_computed_style(style)
    }

    fn mss(&self) -> Option<&crate::mss::MssFields> {
        self.as_ref().mss()
    }

    fn reset_mss_styles(&mut self) {
        self.as_mut().reset_mss_styles()
    }

    fn element_type_name(&self) -> &str {
        self.as_ref().element_type_name()
    }

    fn apply_transition_styles(
        &mut self,
        base: &crate::mss::ComputedStyle,
        hover: Option<&crate::mss::ComputedStyle>,
        active: Option<&crate::mss::ComputedStyle>,
        focus: Option<&crate::mss::ComputedStyle>,
        selected: Option<&crate::mss::ComputedStyle>,
        checked: Option<&crate::mss::ComputedStyle>,
    ) {
        self.as_mut().apply_transition_styles(base, hover, active, focus, selected, checked)
    }

    fn is_visible(&self) -> bool {
        self.as_ref().is_visible()
    }

    fn active_child_count(&self) -> usize {
        self.as_ref().active_child_count()
    }

    fn intercepts_child_events(&self) -> bool {
        self.as_ref().intercepts_child_events()
    }

    fn set_content_size(&mut self, size: Size) {
        self.as_mut().set_content_size(size)
    }

    fn set_viewport_size(&mut self, size: Size) {
        self.as_mut().set_viewport_size(size)
    }

    fn accessibility_info(&self) -> Option<crate::a11y::AccessibilityInfo> {
        self.as_ref().accessibility_info()
    }

    fn clip_content(&self) -> bool {
        self.as_ref().clip_content()
    }

    fn clip_corner_radius(&self) -> [f32; 4] {
        self.as_ref().clip_corner_radius()
    }

    fn scroll_offset(&self) -> Point {
        self.as_ref().scroll_offset()
    }

    fn event_scale(&self) -> f32 {
        self.as_ref().event_scale()
    }

    fn is_scroll_container(&self) -> bool {
        self.as_ref().is_scroll_container()
    }

    fn ensure_visible(&mut self, child_rect: Rect) -> bool {
        self.as_mut().ensure_visible(child_rect)
    }

    fn manages_own_children(&self) -> bool {
        self.as_ref().manages_own_children()
    }

    fn needs_rebuild(&self) -> bool {
        self.as_ref().needs_rebuild()
    }

    fn build_children(&self) -> Vec<Box<dyn super::Widget>> {
        self.as_ref().build_children()
    }

    fn clear_rebuild(&mut self) {
        self.as_mut().clear_rebuild()
    }

    fn set_row_bounds(&mut self, bounds: Vec<(f32, f32)>) {
        self.as_mut().set_row_bounds(bounds)
    }

    fn is_relayout_boundary(&self) -> bool {
        self.as_ref().is_relayout_boundary()
    }

    fn visible_child_indices(&self, cull: Rect, out: &mut Vec<usize>) -> bool {
        self.as_ref().visible_child_indices(cull, out)
    }

    fn child_at_position(&self, pos: Point) -> ChildHit {
        self.as_ref().child_at_position(pos)
    }

    fn as_any_mut(&mut self) -> Option<&mut dyn Any> {
        self.as_mut().as_any_mut()
    }

    fn wants_animate_tick(&self) -> bool {
        self.as_ref().wants_animate_tick()
    }
}

pub trait EventContextExt {
    fn request_paint(&mut self);
    
    fn request_layout(&mut self);
    
    fn emit<E: Any>(&mut self, event: E);
}

impl EventContextExt for EventContext {
    fn request_paint(&mut self) {
        self.capture();
    }
    
    fn request_layout(&mut self) {
        self.capture();
    }
    
    fn emit<E: Any>(&mut self, _event: E) {
        self.capture();
    }
}
