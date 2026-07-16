use crate::core::canvas::CanvasContext;
use crate::core::{Color, Rect, Size};
use crate::input::{Event, EventResult};
use crate::layout::Constraints;
use crate::mss::{ComputedStyle, MssFields};
use crate::render::DisplayList;
use crate::widget::context::EventContext;
use crate::widget::{DirtyFlags, Element, ElementId, ElementTree, StyledElement, UpdateContext, Widget};
use std::any::Any;

use super::{tile_math, MapViewport};

#[derive(Clone, PartialEq)]
pub struct BuildingShape {
    pub polygon: Vec<(f64, f64)>,
    pub fill: Color,
    pub has_data: bool,
}

impl BuildingShape {
    pub fn new(polygon: Vec<(f64, f64)>, fill: Color, has_data: bool) -> Self {
        Self { polygon, fill, has_data }
    }
}

pub struct BuildingOverlay {
    buildings: Vec<BuildingShape>,
    viewport: MapViewport,
    outline: Color,
    opacity: f32,
}

impl BuildingOverlay {
    pub fn new() -> Self {
        Self {
            buildings: Vec::new(),
            viewport: MapViewport {
                center_lat: 0.0,
                center_lng: 0.0,
                zoom: 1,
                viewport_w: 0.0,
                viewport_h: 0.0,
            },
            outline: Color::new(0.06, 0.09, 0.16, 0.85),
            opacity: 0.78,
        }
    }

    pub fn buildings(mut self, buildings: Vec<BuildingShape>) -> Self {
        self.buildings = buildings;
        self
    }

    pub fn viewport(mut self, viewport: MapViewport) -> Self {
        self.viewport = viewport;
        self
    }

    pub fn outline(mut self, color: Color) -> Self {
        self.outline = color;
        self
    }

    pub fn opacity(mut self, opacity: f32) -> Self {
        self.opacity = opacity.clamp(0.0, 1.0);
        self
    }
}

impl Default for BuildingOverlay {
    fn default() -> Self {
        Self::new()
    }
}

impl Widget for BuildingOverlay {
    fn create_element(&self) -> Box<dyn Element> {
        Box::new(BuildingOverlayElement {
            id: ElementId::new(),
            buildings: self.buildings.clone(),
            viewport: self.viewport,
            outline: self.outline,
            opacity: self.opacity,
            bounds: Rect::zero(),
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

pub struct BuildingOverlayElement {
    id: ElementId,
    buildings: Vec<BuildingShape>,
    viewport: MapViewport,
    outline: Color,
    opacity: f32,
    bounds: Rect,
    classes: Vec<String>,
    dirty_flags: DirtyFlags,
    mss: MssFields,
}

impl Element for BuildingOverlayElement {
    fn update(&mut self, widget: &dyn Widget, _ctx: &mut UpdateContext) {
        if let Some(w) = widget.as_any().downcast_ref::<BuildingOverlay>() {
            self.buildings = w.buildings.clone();
            self.viewport = w.viewport;
            self.outline = w.outline;
            self.opacity = w.opacity;
            self.mark_dirty(DirtyFlags::RENDER);
        }
    }

    fn layout(&mut self, constraints: Constraints) -> Size {
        let w = if constraints.max_width.is_finite() { constraints.max_width } else { 400.0 };
        let h = if constraints.max_height.is_finite() { constraints.max_height } else { 400.0 };
        self.bounds = Rect::new(self.bounds.origin, Size::new(w, h));
        Size::new(w, h)
    }

    fn build_display_list(&self, list: &mut DisplayList, _clip: Rect) {
        let vp = self.viewport;
        if vp.viewport_w <= 1.0 || vp.viewport_h <= 1.0 || self.buildings.is_empty() {
            return;
        }

        list.push_clip(self.bounds);
        let mut ctx = CanvasContext::new(self.bounds.origin, self.bounds.size);

        for b in &self.buildings {
            if b.polygon.len() < 3 {
                continue;
            }
            let pts: Vec<(f32, f32)> = b
                .polygon
                .iter()
                .map(|&(lat, lng)| {
                    tile_math::geo_to_pixel(
                        lat, lng, vp.center_lat, vp.center_lng, vp.zoom,
                        vp.viewport_w, vp.viewport_h,
                    )
                })
                .collect();

            let (mut min_x, mut min_y) = (f32::INFINITY, f32::INFINITY);
            let (mut max_x, mut max_y) = (f32::NEG_INFINITY, f32::NEG_INFINITY);
            for &(x, y) in &pts {
                min_x = min_x.min(x);
                min_y = min_y.min(y);
                max_x = max_x.max(x);
                max_y = max_y.max(y);
            }
            if max_x < 0.0 || min_x > vp.viewport_w || max_y < 0.0 || min_y > vp.viewport_h {
                continue;
            }

            let fill = b.fill.with_alpha(b.fill.a * self.opacity);
            ctx.set_anti_alias(0.0);
            ctx.set_color(fill);
            ctx.fill_polygon_concave(&pts);

            let mut closed = pts;
            if let Some(&first) = closed.first() {
                closed.push(first);
            }
            ctx.set_anti_alias(0.75);
            ctx.set_stroke_width(if b.has_data { 1.4 } else { 1.0 });
            ctx.set_color(self.outline.with_alpha(self.outline.a * self.opacity));
            ctx.draw_polyline(&closed);
        }

        ctx.flush(list);
        list.pop_clip();
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

    fn set_position(&mut self, pos: crate::core::Point) {
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

    fn mount(&mut self, _tree: &mut ElementTree) {}

    fn element_type_name(&self) -> &str {
        "BuildingOverlay"
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

impl StyledElement for BuildingOverlayElement {
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
