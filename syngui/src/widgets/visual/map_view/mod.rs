pub mod tile_math;
pub mod provider;
pub mod tile_loader;
pub mod tile_cache;
#[cfg(all(target_arch = "wasm32", feature = "map"))]
mod tile_cache_idb;
pub mod marker;
pub mod heat_overlay;
pub mod building_overlay;

pub use provider::TileProvider;
pub use marker::MapMarker;
pub use tile_cache::TileCache;
pub use heat_overlay::{HeatOverlay, HeatPoint};
pub use building_overlay::{BuildingOverlay, BuildingShape};
pub use tile_math::{geo_to_pixel, pixel_to_geo, lng_to_tile_x, lat_to_tile_y, tile_x_to_lng, tile_y_to_lat};

#[derive(Clone, Copy, Debug, PartialEq)]
pub struct MapViewport {
    pub center_lat: f64,
    pub center_lng: f64,
    pub zoom: u8,
    pub viewport_w: f32,
    pub viewport_h: f32,
}

use crate::animation::{Animation, Easing};
use crate::core::{Color, Point, Rect, Size};
use crate::gpu::tile_atlas::{TileAtlas, TileKey};
use crate::input::{CursorIcon, Event, EventResult};
use crate::layout::Constraints;
use crate::render::{DisplayList, TextureId};
use crate::mss::{ComputedStyle, MssFields};
use crate::widget::{DirtyFlags, Element, ElementId, ElementTree, Widget};
use crate::widget::context::TextMeasure;

use std::any::Any;
use std::sync::Arc;
use crate::core::sync::Mutex;
use tile_loader::{TileLoader, TileState};

pub struct MapView {
    center_lat: f64,
    center_lng: f64,
    zoom: u8,
    provider: TileProvider,
    markers: Vec<MapMarker>,
    width: Option<f32>,
    height: Option<f32>,
    provider_source: Option<Arc<Mutex<TileProvider>>>,
    tile_cache: Option<Arc<TileCache>>,
    animate_target: Option<(f64, f64, u8)>,
    animate_duration_ms: u32,
    animate_easing: Easing,
    on_viewport_change: Option<Arc<Mutex<dyn FnMut(MapViewport) + Send>>>,
}

impl MapView {
    pub fn new() -> Self {
        Self {
            center_lat: 55.7558,
            center_lng: 37.6173,
            zoom: 10,
            provider: TileProvider::osm(),
            markers: Vec::new(),
            width: None,
            height: None,
            provider_source: None,
            tile_cache: None,
            animate_target: None,
            animate_duration_ms: 1000,
            animate_easing: Easing::EaseInOutCubic,
            on_viewport_change: None,
        }
    }

    pub fn center(mut self, lat: f64, lng: f64) -> Self {
        self.center_lat = lat;
        self.center_lng = lng;
        self
    }

    pub fn zoom(mut self, z: u8) -> Self {
        self.zoom = z.clamp(1, 19);
        self
    }

    pub fn provider(mut self, p: TileProvider) -> Self {
        self.provider = p;
        self
    }

    pub fn marker(mut self, m: MapMarker) -> Self {
        self.markers.push(m);
        self
    }

    pub fn markers(mut self, ms: Vec<MapMarker>) -> Self {
        self.markers = ms;
        self
    }

    pub fn size(mut self, w: f32, h: f32) -> Self {
        self.width = Some(w);
        self.height = Some(h);
        self
    }

    pub fn width(mut self, w: f32) -> Self {
        self.width = Some(w);
        self
    }

    pub fn height(mut self, h: f32) -> Self {
        self.height = Some(h);
        self
    }

    pub fn provider_source(mut self, source: Arc<Mutex<TileProvider>>) -> Self {
        self.provider_source = Some(source);
        self
    }

    pub fn tile_cache(mut self, cache: TileCache) -> Self {
        self.tile_cache = Some(Arc::new(cache));
        self
    }

    pub fn tile_cache_arc(mut self, cache: Arc<TileCache>) -> Self {
        self.tile_cache = Some(cache);
        self
    }

    pub fn animate_to(mut self, lat: f64, lng: f64, zoom: u8) -> Self {
        self.animate_target = Some((lat, lng, zoom.clamp(1, 19)));
        self
    }

    pub fn animate_duration_ms(mut self, ms: u32) -> Self {
        self.animate_duration_ms = ms;
        self
    }

    pub fn animate_easing(mut self, easing: Easing) -> Self {
        self.animate_easing = easing;
        self
    }

    pub fn on_viewport_change(mut self, cb: impl FnMut(MapViewport) + Send + 'static) -> Self {
        self.on_viewport_change = Some(Arc::new(Mutex::new(cb)));
        self
    }
}

impl Widget for MapView {
    fn create_element(&self) -> Box<dyn Element> {
        Box::new(MapViewElement {
            id: ElementId::new(),
            center_lat: self.center_lat,
            center_lng: self.center_lng,
            zoom: self.zoom,
            provider: self.provider.clone(),
            markers: self.markers.clone(),
            preferred_width: self.width,
            preferred_height: self.height,
            bounds: Rect::zero(),
            dragging: false,
            drag_start: Point::zero(),
            drag_center_lat: 0.0,
            drag_center_lng: 0.0,
            zoom_accumulator: 0.0,
            tile_loader: Arc::new(match &self.tile_cache {
                Some(cache) => TileLoader::with_cache(Arc::clone(cache)),
                None => TileLoader::new(),
            }),
            tile_atlas: None,
            provider_source: self.provider_source.clone(),
            classes: Vec::new(),
            dirty_flags: DirtyFlags::LAYOUT | DirtyFlags::RENDER,
            touches: std::collections::HashMap::new(),
            pinch_distance: None,
            pinch_center: Point::zero(),
            fly_animation: None,
            fly_from: (0.0, 0.0, 0.0),
            fly_to: (0.0, 0.0, 0.0),
            mss: MssFields::new(),
            text_measure: None,
            on_viewport_change: self.on_viewport_change.clone(),
            last_viewport: None,
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

pub struct MapViewElement {
    id: ElementId,
    center_lat: f64,
    center_lng: f64,
    zoom: u8,
    provider: TileProvider,
    markers: Vec<MapMarker>,
    preferred_width: Option<f32>,
    preferred_height: Option<f32>,
    bounds: Rect,
    dragging: bool,
    drag_start: Point,
    drag_center_lat: f64,
    drag_center_lng: f64,
    zoom_accumulator: f32,
    tile_loader: Arc<TileLoader>,
    tile_atlas: Option<Arc<Mutex<TileAtlas>>>,
    provider_source: Option<Arc<Mutex<TileProvider>>>,
    classes: Vec<String>,
    dirty_flags: DirtyFlags,
    touches: std::collections::HashMap<u64, Point>,
    pinch_distance: Option<f32>,
    pinch_center: Point,
    fly_animation: Option<Animation>,
    fly_from: (f64, f64, f64),
    fly_to: (f64, f64, f64),
    mss: MssFields,
    text_measure: Option<Arc<dyn TextMeasure>>,
    on_viewport_change: Option<Arc<Mutex<dyn FnMut(MapViewport) + Send>>>,
    last_viewport: Option<MapViewport>,
}

impl MapViewElement {
    fn active_filter(&self, target: &crate::animation::transition::AnimatedPropertyMap) -> Option<Vec<crate::effects::FilterEffect>> {
        if let Some(ref anim) = self.mss.keyframe_animation {
            if anim.is_running() {
                if let Some(filter) = anim.current_values().filter() {
                    return Some(filter);
                }
            }
        }
        if let Some(chain) = self.mss.transition.filter_chain() {
            if !chain.is_empty() { return Some(chain); }
            return None;
        }
        target.filter().or_else(|| self.mss.filter.clone())
    }

    fn has_filter_effects(&self, target: &crate::animation::transition::AnimatedPropertyMap) -> bool {
        self.active_filter(target).map_or(false, |f| !f.is_empty())
    }

    fn build_filter_effect(&self, target: &crate::animation::transition::AnimatedPropertyMap) -> crate::render::display_list::Effect {
        use crate::render::display_list::Effect;
        let mut effects: Vec<Effect> = Vec::new();
        if let Some(filters) = self.active_filter(target) {
            for f in &filters {
                let e = f.to_effect();
                if !e.is_identity() {
                    effects.push(e);
                }
            }
        }
        match effects.len() {
            0 => Effect::None,
            1 => effects.remove(0),
            _ => Effect::Chain(effects),
        }
    }

    fn ensure_atlas(&mut self, tree: &ElementTree) {
        if self.tile_atlas.is_none() {
            self.tile_atlas = tree.tile_atlas.clone();
        }
    }

    fn emit_viewport(&mut self) {
        let Some(cb) = self.on_viewport_change.clone() else { return };
        let vp = MapViewport {
            center_lat: self.center_lat,
            center_lng: self.center_lng,
            zoom: self.zoom,
            viewport_w: self.bounds.size.width,
            viewport_h: self.bounds.size.height,
        };
        if self.last_viewport == Some(vp) {
            return;
        }
        self.last_viewport = Some(vp);
        if let Ok(mut f) = cb.lock() {
            f(vp);
        };
    }
}

impl Element for MapViewElement {
    fn update(&mut self, widget: &dyn Widget, _ctx: &mut crate::widget::UpdateContext) {
        if let Some(m) = widget.as_any().downcast_ref::<MapView>() {
            if m.provider.id != self.provider.id {
                self.tile_loader.clear_provider(self.provider.id);
                if let Some(ref atlas) = self.tile_atlas {
                    if let Ok(mut a) = atlas.lock() {
                        a.clear_provider(self.provider.id);
                    }
                }
                self.provider = m.provider.clone();
            }
            self.markers = m.markers.clone();
            self.preferred_width = m.width;
            self.preferred_height = m.height;
            self.on_viewport_change = m.on_viewport_change.clone();

            if let Some((target_lat, target_lng, target_zoom)) = m.animate_target {
                let needs_anim = (target_lat - self.fly_to.0).abs() > 1e-8
                    || (target_lng - self.fly_to.1).abs() > 1e-8
                    || (target_zoom as f64 - self.fly_to.2).abs() > 0.5
                    || self.fly_animation.is_none();

                if needs_anim {
                    self.fly_from = (self.center_lat, self.center_lng, self.zoom as f64);
                    self.fly_to = (target_lat, target_lng, target_zoom as f64);
                    self.fly_animation = Some(
                        Animation::tween(m.animate_easing)
                            .from(0.0)
                            .to(1.0)
                            .duration_ms(m.animate_duration_ms)
                            .build()
                    );
                }
            }

            self.mark_dirty(DirtyFlags::RENDER);
        }
    }

    fn layout(&mut self, constraints: Constraints) -> Size {
        let w = self.preferred_width.unwrap_or(constraints.max_width).min(constraints.max_width);
        let h = self.preferred_height
            .unwrap_or_else(|| if constraints.max_height.is_finite() { constraints.max_height } else { 400.0 })
            .min(constraints.max_height);
        self.bounds = Rect::new(Point::zero(), Size::new(w, h));
        Size::new(w, h)
    }

    fn build_display_list(&self, list: &mut DisplayList, _clip: Rect) {
        let bounds = self.bounds;

        list.push_clip(bounds);

        let target = self.mss.target_props(false, false, false, false);
        if self.has_filter_effects(&target) {
            list.push_effect_layer(self.build_filter_effect(&target), bounds);
        }

        list.push_rect(bounds, Color::new(0.678, 0.847, 0.902, 1.0), [0.0; 4]);

        if let Some(ref tile_atlas) = self.tile_atlas {
            self.render_tiles(list, tile_atlas);
        }

        list.push_z_barrier();

        self.render_markers(list);

        let attr_text = self.provider.attribution;
        let attr_rect = Rect::new(
            Point::new(bounds.origin.x + 4.0, bounds.origin.y + bounds.size.height - 16.0),
            Size::new(bounds.size.width - 8.0, 14.0),
        );
        list.push_rect(
            Rect::new(
                Point::new(bounds.origin.x, bounds.origin.y + bounds.size.height - 18.0),
                Size::new(bounds.size.width, 18.0),
            ),
            Color::new(1.0, 1.0, 1.0, 0.7),
            [0.0; 4],
        );
        list.push_text(attr_text, attr_rect, Color::new(0.2, 0.2, 0.2, 1.0), 10.0);

        if self.has_filter_effects(&target) {
            list.pop_effect_layer();
        }

        list.pop_clip();
    }

    fn handle_event(&mut self, event: &Event, ctx: &mut crate::widget::context::EventContext) -> EventResult {
        match event {
            Event::MouseDown { position, .. } => {
                if self.bounds.contains(*position) {
                    self.fly_animation = None;
                    self.dragging = true;
                    self.drag_start = *position;
                    self.drag_center_lat = self.center_lat;
                    self.drag_center_lng = self.center_lng;
                    ctx.set_cursor(CursorIcon::Grabbing);
                    return EventResult::Handled;
                }
            }
            Event::MouseMove(position) => {
                if self.dragging {
                    let dx = position.x - self.drag_start.x;
                    let dy = position.y - self.drag_start.y;

                    let tile_size = 256.0_f64;
                    let n = (1u64 << self.zoom) as f64;
                    let total_pixels = n * tile_size;

                    let lng_per_pixel = 360.0 / total_pixels;
                    self.center_lng = self.drag_center_lng - (dx as f64) * lng_per_pixel;

                    let center_tile_y = tile_math::lat_to_tile_y(self.drag_center_lat, self.zoom);
                    let new_tile_y = center_tile_y - (dy as f64) / tile_size;
                    self.center_lat = tile_math::tile_y_to_lat(new_tile_y, self.zoom);

                    self.center_lat = self.center_lat.clamp(-85.05, 85.05);

                    ctx.set_cursor(CursorIcon::Grabbing);
                    self.mark_dirty(DirtyFlags::RENDER);
                    self.emit_viewport();
                    return EventResult::Handled;
                } else if self.bounds.contains(*position) {
                    ctx.set_cursor(CursorIcon::Grab);
                }
            }
            Event::MouseUp { .. } => {
                if self.dragging {
                    self.dragging = false;
                    ctx.set_cursor(CursorIcon::Default);
                    return EventResult::Handled;
                }
            }
            Event::MouseWheel { position, delta, .. } => {
                if self.bounds.contains(*position) {
                    self.fly_animation = None;
                    const ZOOM_THRESHOLD: f32 = 12.0;

                    self.zoom_accumulator += *delta;

                    if self.zoom_accumulator.abs() >= ZOOM_THRESHOLD {
                        let (lat_at_cursor, lng_at_cursor) = tile_math::pixel_to_geo(
                            position.x - self.bounds.origin.x,
                            position.y - self.bounds.origin.y,
                            self.center_lat,
                            self.center_lng,
                            self.zoom,
                            self.bounds.size.width,
                            self.bounds.size.height,
                        );

                        let old_zoom = self.zoom;
                        if self.zoom_accumulator > 0.0 {
                            self.zoom = (self.zoom + 1).min(self.provider.max_zoom);
                        } else {
                            self.zoom = self.zoom.saturating_sub(1).max(1);
                        }
                        self.zoom_accumulator = 0.0;

                        if self.zoom != old_zoom {
                            let (new_lat, new_lng) = tile_math::pixel_to_geo(
                                position.x - self.bounds.origin.x,
                                position.y - self.bounds.origin.y,
                                self.center_lat,
                                self.center_lng,
                                self.zoom,
                                self.bounds.size.width,
                                self.bounds.size.height,
                            );
                            self.center_lat += lat_at_cursor - new_lat;
                            self.center_lng += lng_at_cursor - new_lng;
                            self.center_lat = self.center_lat.clamp(-85.05, 85.05);

                            self.mark_dirty(DirtyFlags::RENDER);
                        }
                    }
                    self.emit_viewport();
                    return EventResult::Handled;
                }
            }
            Event::TouchStart { id, position } => {
                if self.bounds.contains(*position) {
                    self.fly_animation = None;
                    self.touches.insert(*id, *position);
                    if self.touches.len() == 1 {
                        self.dragging = true;
                        self.drag_start = *position;
                        self.drag_center_lat = self.center_lat;
                        self.drag_center_lng = self.center_lng;
                    } else if self.touches.len() == 2 {
                        self.dragging = false;
                        let pts: Vec<&Point> = self.touches.values().collect();
                        let dx = pts[1].x - pts[0].x;
                        let dy = pts[1].y - pts[0].y;
                        self.pinch_distance = Some((dx * dx + dy * dy).sqrt());
                        self.pinch_center = Point::new(
                            (pts[0].x + pts[1].x) / 2.0,
                            (pts[0].y + pts[1].y) / 2.0,
                        );
                    }
                    return EventResult::Handled;
                }
            }
            Event::TouchMove { id, position } => {
                if self.touches.contains_key(id) {
                    self.touches.insert(*id, *position);

                    if self.touches.len() == 1 && self.dragging {
                        let dx = position.x - self.drag_start.x;
                        let dy = position.y - self.drag_start.y;

                        let tile_size = 256.0_f64;
                        let n = (1u64 << self.zoom) as f64;
                        let total_pixels = n * tile_size;

                        let lng_per_pixel = 360.0 / total_pixels;
                        self.center_lng = self.drag_center_lng - (dx as f64) * lng_per_pixel;

                        let center_tile_y = tile_math::lat_to_tile_y(self.drag_center_lat, self.zoom);
                        let new_tile_y = center_tile_y - (dy as f64) / tile_size;
                        self.center_lat = tile_math::tile_y_to_lat(new_tile_y, self.zoom);
                        self.center_lat = self.center_lat.clamp(-85.05, 85.05);

                        self.mark_dirty(DirtyFlags::RENDER);
                        self.emit_viewport();
                        return EventResult::Handled;
                    } else if self.touches.len() == 2 {
                        let pts: Vec<&Point> = self.touches.values().collect();
                        let dx = pts[1].x - pts[0].x;
                        let dy = pts[1].y - pts[0].y;
                        let new_distance = (dx * dx + dy * dy).sqrt();
                        let center = Point::new(
                            (pts[0].x + pts[1].x) / 2.0,
                            (pts[0].y + pts[1].y) / 2.0,
                        );

                        if let Some(prev_dist) = self.pinch_distance {
                            let ratio = new_distance / prev_dist;
                            if ratio > 1.5 {
                                let (lat_c, lng_c) = tile_math::pixel_to_geo(
                                    center.x - self.bounds.origin.x,
                                    center.y - self.bounds.origin.y,
                                    self.center_lat, self.center_lng,
                                    self.zoom, self.bounds.size.width, self.bounds.size.height,
                                );
                                let old_zoom = self.zoom;
                                self.zoom = (self.zoom + 1).min(self.provider.max_zoom);
                                if self.zoom != old_zoom {
                                    let (new_lat, new_lng) = tile_math::pixel_to_geo(
                                        center.x - self.bounds.origin.x,
                                        center.y - self.bounds.origin.y,
                                        self.center_lat, self.center_lng,
                                        self.zoom, self.bounds.size.width, self.bounds.size.height,
                                    );
                                    self.center_lat += lat_c - new_lat;
                                    self.center_lng += lng_c - new_lng;
                                    self.center_lat = self.center_lat.clamp(-85.05, 85.05);
                                }
                                self.pinch_distance = Some(new_distance);
                                self.mark_dirty(DirtyFlags::RENDER);
                            } else if ratio < 0.67 {
                                let (lat_c, lng_c) = tile_math::pixel_to_geo(
                                    center.x - self.bounds.origin.x,
                                    center.y - self.bounds.origin.y,
                                    self.center_lat, self.center_lng,
                                    self.zoom, self.bounds.size.width, self.bounds.size.height,
                                );
                                let old_zoom = self.zoom;
                                self.zoom = self.zoom.saturating_sub(1).max(1);
                                if self.zoom != old_zoom {
                                    let (new_lat, new_lng) = tile_math::pixel_to_geo(
                                        center.x - self.bounds.origin.x,
                                        center.y - self.bounds.origin.y,
                                        self.center_lat, self.center_lng,
                                        self.zoom, self.bounds.size.width, self.bounds.size.height,
                                    );
                                    self.center_lat += lat_c - new_lat;
                                    self.center_lng += lng_c - new_lng;
                                    self.center_lat = self.center_lat.clamp(-85.05, 85.05);
                                }
                                self.pinch_distance = Some(new_distance);
                                self.mark_dirty(DirtyFlags::RENDER);
                            } else {
                                let pdx = center.x - self.pinch_center.x;
                                let pdy = center.y - self.pinch_center.y;
                                if pdx.abs() > 2.0 || pdy.abs() > 2.0 {
                                    let tile_size = 256.0_f64;
                                    let n = (1u64 << self.zoom) as f64;
                                    let total_pixels = n * tile_size;
                                    let lng_per_pixel = 360.0 / total_pixels;
                                    self.center_lng -= (pdx as f64) * lng_per_pixel;
                                    let center_tile_y = tile_math::lat_to_tile_y(self.center_lat, self.zoom);
                                    let new_tile_y = center_tile_y - (pdy as f64) / tile_size;
                                    self.center_lat = tile_math::tile_y_to_lat(new_tile_y, self.zoom);
                                    self.center_lat = self.center_lat.clamp(-85.05, 85.05);
                                    self.pinch_center = center;
                                    self.mark_dirty(DirtyFlags::RENDER);
                                }
                            }
                        }
                        self.emit_viewport();
                        return EventResult::Handled;
                    }
                }
            }
            Event::TouchEnd { id, .. } => {
                if self.touches.remove(id).is_some() {
                    if self.touches.is_empty() {
                        self.dragging = false;
                        self.pinch_distance = None;
                    } else if self.touches.len() == 1 {
                        self.pinch_distance = None;
                        self.dragging = true;
                        let remaining = *self.touches.values().next().unwrap();
                        self.drag_start = remaining;
                        self.drag_center_lat = self.center_lat;
                        self.drag_center_lng = self.center_lng;
                    }
                    return EventResult::Handled;
                }
            }
            _ => {}
        }
        EventResult::Ignored
    }

    /// Кадры нужны на перелёте камеры, пока подгружаются тайлы, пока
    /// анимируются маркеры и пока провайдер может смениться снаружи.
    fn wants_animate_tick(&self) -> bool {
        self.fly_animation.is_some()
            || self.tile_loader.has_pending()
            || self.provider_source.is_some()
            || self
                .markers
                .iter()
                .any(|m| m.is_animating(web_time::Instant::now()))
    }

    fn animate(&mut self, dt: std::time::Duration) -> bool {
        let mut needs_frame = false;

        if let Some(ref mut anim) = self.fly_animation {
            let still_running = anim.tick(dt);
            let t = anim.current_value() as f64;

            self.center_lat = self.fly_from.0 + (self.fly_to.0 - self.fly_from.0) * t;
            self.center_lng = self.fly_from.1 + (self.fly_to.1 - self.fly_from.1) * t;
            let zoom_f64 = self.fly_from.2 + (self.fly_to.2 - self.fly_from.2) * t;
            self.zoom = zoom_f64.round().clamp(1.0, 19.0) as u8;
            self.center_lat = self.center_lat.clamp(-85.05, 85.05);
            self.mark_dirty(DirtyFlags::RENDER);

            if !still_running {
                self.center_lat = self.fly_to.0;
                self.center_lng = self.fly_to.1;
                self.zoom = self.fly_to.2.round().clamp(1.0, 19.0) as u8;
                self.fly_animation = None;
            } else {
                needs_frame = true;
            }
        }

        if let Some(ref source) = self.provider_source {
            let new_provider = source.lock().unwrap().clone();
            if new_provider.id != self.provider.id {
                self.tile_loader.clear_provider(self.provider.id);
                if let Some(ref atlas) = self.tile_atlas {
                    if let Ok(mut a) = atlas.lock() {
                        a.clear_provider(self.provider.id);
                    }
                }
                self.provider = new_provider;
                self.mark_dirty(DirtyFlags::RENDER);
            }
        }

        let transition_active = self.mss.transition.tick(dt.as_secs_f32());
        let keyframe_active = self.mss.keyframe_animation
            .as_mut()
            .map(|a| a.tick(dt.as_secs_f32()))
            .unwrap_or(false);

        let now_inst = web_time::Instant::now();
        let markers_animating = self.markers.iter().any(|m| m.is_animating(now_inst));
        if markers_animating {
            self.mark_dirty(DirtyFlags::RENDER);
        }

        let tiles_pending = self.tile_loader.has_pending();
        let tiles_deferred = self.tile_loader.take_deferred();
        if tiles_pending || tiles_deferred {
            self.mark_dirty(DirtyFlags::RENDER);
        }

        self.emit_viewport();

        needs_frame || tiles_pending || tiles_deferred || self.provider_source.is_some()
            || transition_active || keyframe_active || markers_animating
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

    fn clip_content(&self) -> bool {
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
        self.ensure_atlas(tree);
    }

    fn element_type_name(&self) -> &str { "MapView" }

    fn set_classes(&mut self, classes: Vec<String>) {
        self.classes = classes;
        self.mark_dirty(DirtyFlags::RENDER);
    }

    fn get_classes(&self) -> &[String] {
        &self.classes
    }

    fn mss(&self) -> Option<&crate::mss::MssFields> { Some(&self.mss) }

    fn reset_mss_styles(&mut self) {
        self.mss.reset();
    }

    fn apply_computed_style(&mut self, style: &ComputedStyle) {
        self.mss.apply(style);
        self.mark_dirty(DirtyFlags::RENDER);
    }

    fn apply_transition_styles(
        &mut self,
        base: &ComputedStyle,
        hover: Option<&ComputedStyle>,
        _active: Option<&ComputedStyle>,
        _focus: Option<&ComputedStyle>,
        _selected: Option<&ComputedStyle>,
        _checked: Option<&ComputedStyle>,
    ) {
        self.mss.apply_transitions(base, hover, None, None, None);
    }

    fn setup_keyframe_animation(
        &mut self,
        style: &ComputedStyle,
        stylesheet: &crate::mss::StyleSheet,
    ) {
        self.mss.setup_keyframe_animation(style, stylesheet);
    }
}

impl MapViewElement {
    fn render_tiles(&self, list: &mut DisplayList, tile_atlas: &Arc<Mutex<TileAtlas>>) {
        let bounds = self.bounds;
        let vw = bounds.size.width;
        let vh = bounds.size.height;
        let zoom = self.zoom;
        let tile_size = 256.0_f32;

        let center_tx = tile_math::lng_to_tile_x(self.center_lng, zoom);
        let center_ty = tile_math::lat_to_tile_y(self.center_lat, zoom);

        let center_px = (center_tx.fract() * tile_size as f64) as f32;
        let center_py = (center_ty.fract() * tile_size as f64) as f32;

        let center_tile_x = center_tx.floor() as i32;
        let center_tile_y = center_ty.floor() as i32;

        let dx_min = ((center_px - vw / 2.0) / tile_size).floor() as i32;
        let dx_max = ((center_px + vw / 2.0) / tile_size).ceil() as i32 - 1;
        let dy_min = ((center_py - vh / 2.0) / tile_size).floor() as i32;
        let dy_max = ((center_py + vh / 2.0) / tile_size).ceil() as i32 - 1;

        let max_tile = (1i32 << zoom) - 1;

        let mut atlas = match tile_atlas.lock() {
            Ok(a) => a,
            Err(_) => return,
        };

        for dy in dy_min..=dy_max {
            for dx in dx_min..=dx_max {
                let tx = center_tile_x + dx;
                let ty = center_tile_y + dy;

                if ty < 0 || ty > max_tile {
                    continue;
                }
                let wrapped_tx = ((tx % (max_tile + 1)) + (max_tile + 1)) % (max_tile + 1);

                let key = TileKey {
                    x: wrapped_tx as u32,
                    y: ty as u32,
                    z: zoom,
                    provider_id: self.provider.id,
                };

                let screen_x = bounds.origin.x + vw / 2.0 - center_px + (dx as f32) * tile_size;
                let screen_y = bounds.origin.y + vh / 2.0 - center_py + (dy as f32) * tile_size;
                let tile_rect = Rect::new(
                    Point::new(screen_x, screen_y),
                    Size::new(tile_size, tile_size),
                );

                if let Some(slot) = atlas.get_tile(&key) {
                    let uv_rect = Rect::new(
                        Point::new(slot.uv_x, slot.uv_y),
                        Size::new(slot.uv_w, slot.uv_h),
                    );
                    list.push_image(tile_rect, TextureId(0), uv_rect, Color::WHITE);
                    continue;
                }

                let url = self.provider.tile_url(key.x, key.y, key.z);
                let slot = match self.tile_loader.request_tile(key, url) {
                    TileState::Loaded(rgba) => atlas.insert_tile(key, &rgba),
                    TileState::Loading | TileState::Failed => None,
                };

                match slot {
                    Some(slot) => {
                        let uv_rect = Rect::new(
                            Point::new(slot.uv_x, slot.uv_y),
                            Size::new(slot.uv_w, slot.uv_h),
                        );
                        list.push_image(tile_rect, TextureId(0), uv_rect, Color::WHITE);
                    }
                    None => {
                        if !Self::draw_parent_tile(list, &mut atlas, &key, tile_rect, self.provider.id) {
                            list.push_rect(tile_rect, Color::new(0.9, 0.9, 0.9, 1.0), [0.0; 4]);
                        }
                    }
                }
            }
        }
    }

    fn draw_parent_tile(
        list: &mut DisplayList,
        atlas: &mut TileAtlas,
        key: &TileKey,
        tile_rect: Rect,
        provider_id: u8,
    ) -> bool {
        let mut px = key.x;
        let mut py = key.y;
        let mut pz = key.z;
        let orig_x = key.x;
        let orig_y = key.y;
        let orig_z = key.z;

        for _ in 0..3 {
            if pz == 0 { break; }
            px /= 2;
            py /= 2;
            pz -= 1;

            let parent_key = TileKey { x: px, y: py, z: pz, provider_id };
            if let Some(slot) = atlas.get_tile(&parent_key) {
                let depth = orig_z - pz;
                let scale = 1.0 / (1u32 << depth) as f32;

                let sub_x = (orig_x % (1u32 << depth)) as f32 * scale;
                let sub_y = (orig_y % (1u32 << depth)) as f32 * scale;

                let uv_rect = Rect::new(
                    Point::new(
                        slot.uv_x + sub_x * slot.uv_w,
                        slot.uv_y + sub_y * slot.uv_h,
                    ),
                    Size::new(slot.uv_w * scale, slot.uv_h * scale),
                );
                list.push_image(tile_rect, TextureId(0), uv_rect, Color::WHITE);
                return true;
            }
        }
        false
    }

    fn render_markers(&self, list: &mut DisplayList) {
        let bounds = self.bounds;
        let now = web_time::Instant::now();

        for marker in &self.markers {
            if marker.is_expired(now) { continue; }

            let opacity = marker.current_opacity(now);
            if opacity <= 0.001 { continue; }
            let scale = marker.current_scale(now);
            let effective_size = marker.size * scale;

            let (px, py) = tile_math::geo_to_pixel(
                marker.lat,
                marker.lng,
                self.center_lat,
                self.center_lng,
                self.zoom,
                bounds.size.width,
                bounds.size.height,
            );

            let screen_x = bounds.origin.x + px;
            let screen_y = bounds.origin.y + py;

            if screen_x < bounds.origin.x - effective_size
                || screen_x > bounds.origin.x + bounds.size.width + effective_size
                || screen_y < bounds.origin.y - effective_size
                || screen_y > bounds.origin.y + bounds.size.height + effective_size
            {
                continue;
            }

            let r = effective_size / 2.0;

            let pin_color = marker.color.with_alpha(marker.color.a * opacity);
            let pin_rect = Rect::new(
                Point::new(screen_x - r, screen_y - r),
                Size::new(effective_size, effective_size),
            );
            list.push_rect(pin_rect, pin_color, [r, r, r, r]);

            let dot_r = r * 0.4;
            let dot_rect = Rect::new(
                Point::new(screen_x - dot_r, screen_y - dot_r),
                Size::new(dot_r * 2.0, dot_r * 2.0),
            );
            let dot_color = Color::new(1.0, 1.0, 1.0, opacity);
            list.push_rect(dot_rect, dot_color, [dot_r, dot_r, dot_r, dot_r]);

            if let Some(ref label) = marker.label {
                let label_font = 12.0;
                let label_w = self.text_measure.as_ref()
                    .map(|tm| tm.measure_text_width(label, label_font, label.chars().count()))
                    .unwrap_or_else(|| label.chars().count() as f32 * label_font * 0.6)
                    + 8.0;
                let label_h = 18.0;
                let lx = screen_x - label_w / 2.0;
                let ly = screen_y - r - label_h - 4.0;

                let label_bg = Rect::new(
                    Point::new(lx, ly),
                    Size::new(label_w, label_h),
                );
                list.push_rect(label_bg, Color::new(0.15, 0.15, 0.15, 0.85 * opacity), [4.0; 4]);

                let text_rect = Rect::new(
                    Point::new(lx + 4.0, ly + 1.0),
                    Size::new(label_w - 8.0, label_h - 2.0),
                );
                list.push_text_centered(label, text_rect, Color::new(1.0, 1.0, 1.0, opacity), 11.0);
            }
        }
    }
}
