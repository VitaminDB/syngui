use crate::core::{Point, Rect, Size};
use crate::input::{Event, EventResult};
use crate::layout::Constraints;
use crate::mss::{ComputedStyle, MssFields};
use crate::render::DisplayList;
use crate::widget::context::{EventContext, TextMeasure};
use crate::widget::{
    DirtyFlags, Element, ElementId, ElementTree, StyledElement, UpdateContext, Widget,
};
use std::any::Any;
use std::sync::Arc;

use super::{paint_markers, MapMarker, MapViewport};

/// Метки отдельным слоем поверх карты — для случая, когда между картой и
/// метками лежит другой слой (`HeatOverlay`, `BuildingOverlay`) и метки с
/// подписями не должны уходить под него. Рисуются так же, как метки
/// [`MapView`](super::MapView); положение карты приходит через `viewport`
/// (из `MapView::on_viewport_change`). Слой прозрачен для ввода: щелчки по
/// меткам и анимации появления/исчезания остаются за `MapView`.
pub struct MarkerOverlay {
    markers: Vec<MapMarker>,
    viewport: MapViewport,
}

impl MarkerOverlay {
    pub fn new() -> Self {
        Self {
            markers: Vec::new(),
            viewport: MapViewport {
                center_lat: 0.0,
                center_lng: 0.0,
                zoom: 1,
                viewport_w: 0.0,
                viewport_h: 0.0,
            },
        }
    }

    pub fn markers(mut self, markers: Vec<MapMarker>) -> Self {
        self.markers = markers;
        self
    }

    pub fn viewport(mut self, viewport: MapViewport) -> Self {
        self.viewport = viewport;
        self
    }
}

impl Default for MarkerOverlay {
    fn default() -> Self {
        Self::new()
    }
}

impl Widget for MarkerOverlay {
    fn create_element(&self) -> Box<dyn Element> {
        Box::new(MarkerOverlayElement {
            id: ElementId::new(),
            markers: self.markers.clone(),
            viewport: self.viewport,
            bounds: Rect::zero(),
            text_measure: None,
            classes: Vec::new(),
            dirty_flags: DirtyFlags::LAYOUT | DirtyFlags::RENDER,
            mss: MssFields::new(),
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

pub struct MarkerOverlayElement {
    id: ElementId,
    markers: Vec<MapMarker>,
    viewport: MapViewport,
    bounds: Rect,
    text_measure: Option<Arc<dyn TextMeasure>>,
    classes: Vec<String>,
    dirty_flags: DirtyFlags,
    mss: MssFields,
}

impl Element for MarkerOverlayElement {
    fn update(&mut self, widget: &dyn Widget, _ctx: &mut UpdateContext) {
        if let Some(w) = widget.as_any().downcast_ref::<MarkerOverlay>() {
            self.markers = w.markers.clone();
            self.viewport = w.viewport;
            self.mark_dirty(DirtyFlags::RENDER);
        }
    }

    fn layout(&mut self, constraints: Constraints) -> Size {
        let w = if constraints.max_width.is_finite() {
            constraints.max_width
        } else {
            400.0
        };
        let h = if constraints.max_height.is_finite() {
            constraints.max_height
        } else {
            400.0
        };
        self.bounds = Rect::new(self.bounds.origin, Size::new(w, h));
        Size::new(w, h)
    }

    fn build_display_list(&self, list: &mut DisplayList, _clip: Rect) {
        let vp = self.viewport;
        if vp.viewport_w <= 1.0 || vp.viewport_h <= 1.0 {
            return;
        }
        paint_markers(
            list,
            &self.markers,
            self.bounds,
            (vp.center_lat, vp.center_lng, vp.zoom),
            self.text_measure.as_deref(),
        );
    }

    fn handle_event(&mut self, _event: &Event, _ctx: &mut EventContext) -> EventResult {
        EventResult::Ignored
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

    fn passthrough_hit_test(&self) -> bool {
        true
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

    fn element_type_name(&self) -> &str {
        "MarkerOverlay"
    }

    fn set_classes(&mut self, classes: Vec<String>) {
        self.classes = classes;
        self.mark_dirty(DirtyFlags::RENDER);
    }

    fn get_classes(&self) -> &[String] {
        &self.classes
    }

    fn reset_mss_styles(&mut self) {
        self.mss.reset();
    }

    fn mss(&self) -> Option<&MssFields> {
        Some(&self.mss)
    }

    fn apply_computed_style(&mut self, style: &ComputedStyle) {
        self.mss.apply(style);
        self.mark_dirty(DirtyFlags::RENDER);
    }
}

impl StyledElement for MarkerOverlayElement {
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
