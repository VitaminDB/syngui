pub mod building_overlay;
pub mod heat_overlay;
pub mod marker;
pub mod marker_overlay;
pub mod provider;
pub mod tile_cache;
#[cfg(all(target_arch = "wasm32", feature = "map"))]
mod tile_cache_idb;
pub mod tile_loader;
pub mod tile_math;

pub use building_overlay::{BuildingOverlay, BuildingShape};
pub use heat_overlay::{HeatOverlay, HeatPoint};
pub use marker::MapMarker;
pub use marker_overlay::MarkerOverlay;
pub use provider::TileProvider;
pub use tile_cache::TileCache;
pub use tile_math::{
    geo_to_pixel, geo_to_pixel_f, lat_to_tile_y, lng_to_tile_x, pixel_to_geo, pixel_to_geo_f,
    tile_x_to_lng, tile_y_to_lat,
};

/// Положение карты. `zoom` — округлённый масштаб (для старого кода), `zoom_level` — точный (дробный при
/// плавном масштабировании); оверлеи считают по `zoom_level`.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct MapViewport {
    pub center_lat: f64,
    pub center_lng: f64,
    pub zoom: u8,
    pub zoom_level: f64,
    pub viewport_w: f32,
    pub viewport_h: f32,
}

impl Default for MapViewport {
    fn default() -> Self {
        Self { center_lat: 0.0, center_lng: 0.0, zoom: 1, zoom_level: 1.0, viewport_w: 0.0, viewport_h: 0.0 }
    }
}

/// Линия поверх карты (маршрут, трек): точки (широта, долгота), цвет и толщина в логических пикселях.
/// `outline` — подложка шире линии (контур маршрута), рисуется под ней.
#[derive(Clone, Debug, PartialEq)]
pub struct MapPolyline {
    pub points: Vec<(f64, f64)>,
    pub color: Color,
    pub width: f32,
    pub outline: Option<(Color, f32)>,
}

impl MapPolyline {
    pub fn new(points: Vec<(f64, f64)>) -> Self {
        Self { points, color: Color::new(0.1, 0.45, 0.95, 1.0), width: 5.0, outline: None }
    }
    pub fn color(mut self, c: Color) -> Self {
        self.color = c;
        self
    }
    pub fn width(mut self, w: f32) -> Self {
        self.width = w;
        self
    }
    pub fn outline(mut self, c: Color, w: f32) -> Self {
        self.outline = Some((c, w));
        self
    }
}

/// Запрос перемещения камеры снаружи (кнопка «моё место», найденный адрес, маршрут целиком). Выполняется,
/// когда меняется `seq`: так повторное нажатие с той же целью снова переносит карту, даже если её сдвинули.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct MapCamera {
    pub lat: f64,
    pub lng: f64,
    pub zoom: f64,
    pub seq: u64,
    pub animate: bool,
}

use crate::animation::{Animation, Easing};
use crate::core::{Color, Point, Rect, Size};
use crate::gpu::tile_atlas::{TileAtlas, TileKey};
use crate::input::{CursorIcon, Event, EventResult};
use crate::layout::Constraints;
use crate::mss::{ComputedStyle, MssFields};
use crate::render::{DisplayList, TextureId};
use crate::widget::context::TextMeasure;
use crate::widget::{DirtyFlags, Element, ElementId, ElementTree, Widget};

use crate::core::sync::Mutex;
use std::any::Any;
use std::sync::Arc;
use tile_loader::{TileLoader, TileState};

type GeoCb = Arc<Mutex<dyn FnMut(f64, f64) + Send>>;

pub struct MapView {
    center_lat: f64,
    center_lng: f64,
    zoom: f64,
    provider: TileProvider,
    markers: Vec<MapMarker>,
    polylines: Vec<MapPolyline>,
    width: Option<f32>,
    height: Option<f32>,
    provider_source: Option<Arc<Mutex<TileProvider>>>,
    tile_cache: Option<Arc<TileCache>>,
    animate_target: Option<(f64, f64, u8)>,
    animate_duration_ms: u32,
    animate_easing: Easing,
    camera: Option<MapCamera>,
    smooth_zoom: bool,
    tile_detail: Option<f64>,
    overzoom: u8,
    show_attribution: bool,
    on_viewport_change: Option<Arc<Mutex<dyn FnMut(MapViewport) + Send>>>,
    on_marker_click: Option<Arc<Mutex<dyn FnMut(u64) + Send>>>,
    on_tap: Option<GeoCb>,
    on_long_press: Option<GeoCb>,
    on_interaction: Option<Arc<Mutex<dyn FnMut() + Send>>>,
}

const TAP_SLOP: f32 = 12.0;
const DOUBLE_TAP_MS: u128 = 350;
const MIN_MARKER_HIT_RADIUS: f32 = 10.0;
const MIN_ZOOM: f64 = 1.0;

impl MapView {
    pub fn new() -> Self {
        Self {
            center_lat: 55.7558,
            center_lng: 37.6173,
            zoom: 10.0,
            provider: TileProvider::osm(),
            markers: Vec::new(),
            polylines: Vec::new(),
            width: None,
            height: None,
            provider_source: None,
            tile_cache: None,
            animate_target: None,
            animate_duration_ms: 1000,
            animate_easing: Easing::EaseInOutCubic,
            camera: None,
            smooth_zoom: false,
            tile_detail: None,
            overzoom: 0,
            show_attribution: true,
            on_viewport_change: None,
            on_marker_click: None,
            on_tap: None,
            on_long_press: None,
            on_interaction: None,
        }
    }

    pub fn center(mut self, lat: f64, lng: f64) -> Self {
        self.center_lat = lat;
        self.center_lng = lng;
        self
    }

    pub fn zoom(mut self, z: u8) -> Self {
        self.zoom = z.clamp(1, 19) as f64;
        self
    }

    /// Начальный масштаб, дробный (с [`MapView::smooth_zoom`]).
    pub fn zoom_level(mut self, z: f64) -> Self {
        self.zoom = z.clamp(MIN_ZOOM, 22.0);
        self
    }

    /// Плавное масштабирование: щипок, колесо и двойное касание меняют масштаб непрерывно (иначе — шагами по
    /// уровню плиток, как раньше).
    pub fn smooth_zoom(mut self, on: bool) -> Self {
        self.smooth_zoom = on;
        self
    }

    /// Насколько детальнее масштаба брать плитки. По умолчанию — `log2(масштаб экрана)`: на экране с плотностью
    /// 2× плитка в 256 логических пикселей растянута вдвое и мылилась бы, поэтому берётся уровень на единицу глубже.
    pub fn tile_detail(mut self, levels: f64) -> Self {
        self.tile_detail = Some(levels.clamp(0.0, 3.0));
        self
    }

    /// Насколько можно приближать сверх последнего уровня плиток источника (плитки растягиваются).
    pub fn overzoom(mut self, levels: u8) -> Self {
        self.overzoom = levels.min(4);
        self
    }

    /// Подпись источника внизу карты (у OSM обязательна; можно показать своей разметкой и выключить здесь).
    pub fn attribution(mut self, show: bool) -> Self {
        self.show_attribution = show;
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

    /// Линии поверх плиток (под метками): маршрут, трек.
    pub fn polylines(mut self, lines: Vec<MapPolyline>) -> Self {
        self.polylines = lines;
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

    /// Перенести камеру, когда меняется `camera.seq` (см. [`MapCamera`]).
    pub fn camera(mut self, camera: MapCamera) -> Self {
        self.camera = Some(camera);
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

    pub fn on_marker_click(mut self, cb: impl FnMut(u64) + Send + 'static) -> Self {
        self.on_marker_click = Some(Arc::new(Mutex::new(cb)));
        self
    }

    /// Касание (щелчок) по карте мимо меток: широта, долгота точки.
    pub fn on_tap(mut self, cb: impl FnMut(f64, f64) + Send + 'static) -> Self {
        self.on_tap = Some(Arc::new(Mutex::new(cb)));
        self
    }

    /// Пользователь сам двигает или масштабирует карту (перетаскивание, щипок, колесо, двойное касание) — например,
    /// чтобы перестать следовать за местоположением.
    pub fn on_interaction(mut self, cb: impl FnMut() + Send + 'static) -> Self {
        self.on_interaction = Some(Arc::new(Mutex::new(cb)));
        self
    }

    /// Долгое нажатие на карту: широта, долгота точки (поставить метку, проложить маршрут сюда).
    pub fn on_long_press(mut self, cb: impl FnMut(f64, f64) + Send + 'static) -> Self {
        self.on_long_press = Some(Arc::new(Mutex::new(cb)));
        self
    }
}

impl Widget for MapView {
    fn create_element(&self) -> Box<dyn Element> {
        let mut e = MapViewElement {
            id: ElementId::new(),
            center_lat: self.center_lat,
            center_lng: self.center_lng,
            zoom: self.zoom,
            provider: self.provider.clone(),
            markers: self.markers.clone(),
            polylines: self.polylines.clone(),
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
            pinch: None,
            fly_animation: None,
            fly_from: (0.0, 0.0, 0.0),
            fly_to: (0.0, 0.0, 0.0),
            camera_seq: None,
            smooth_zoom: self.smooth_zoom,
            tile_detail: self.tile_detail,
            overzoom: self.overzoom,
            show_attribution: self.show_attribution,
            mss: MssFields::new(),
            text_measure: None,
            on_viewport_change: self.on_viewport_change.clone(),
            on_marker_click: self.on_marker_click.clone(),
            on_tap: self.on_tap.clone(),
            on_long_press: self.on_long_press.clone(),
            on_interaction: self.on_interaction.clone(),
            press_position: None,
            last_tap: None,
            long_pressed: false,
            last_viewport: None,
        };
        // камера, заданная при создании, — сразу без анимации
        if let Some(c) = self.camera {
            e.center_lat = c.lat;
            e.center_lng = c.lng;
            e.zoom = c.zoom;
            e.camera_seq = Some(c.seq);
        }
        Box::new(e)
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

/// Щипок: начальное расстояние и масштаб, точка карты под центром пальцев.
#[derive(Clone, Copy)]
struct Pinch {
    start_distance: f32,
    start_zoom: f64,
    anchor_lat: f64,
    anchor_lng: f64,
}

pub struct MapViewElement {
    id: ElementId,
    center_lat: f64,
    center_lng: f64,
    zoom: f64,
    provider: TileProvider,
    markers: Vec<MapMarker>,
    polylines: Vec<MapPolyline>,
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
    pinch: Option<Pinch>,
    fly_animation: Option<Animation>,
    fly_from: (f64, f64, f64),
    fly_to: (f64, f64, f64),
    camera_seq: Option<u64>,
    smooth_zoom: bool,
    tile_detail: Option<f64>,
    overzoom: u8,
    show_attribution: bool,
    mss: MssFields,
    text_measure: Option<Arc<dyn TextMeasure>>,
    on_viewport_change: Option<Arc<Mutex<dyn FnMut(MapViewport) + Send>>>,
    on_marker_click: Option<Arc<Mutex<dyn FnMut(u64) + Send>>>,
    on_tap: Option<GeoCb>,
    on_long_press: Option<GeoCb>,
    on_interaction: Option<Arc<Mutex<dyn FnMut() + Send>>>,
    press_position: Option<Point>,
    last_tap: Option<(Point, web_time::Instant)>,
    long_pressed: bool,
    last_viewport: Option<MapViewport>,
}

impl MapViewElement {
    fn active_filter(
        &self,
        target: &crate::animation::transition::AnimatedPropertyMap,
    ) -> Option<Vec<crate::effects::FilterEffect>> {
        if let Some(ref anim) = self.mss.keyframe_animation {
            if anim.is_running() {
                if let Some(filter) = anim.current_values().filter() {
                    return Some(filter);
                }
            }
        }
        if let Some(chain) = self.mss.transition.filter_chain() {
            if !chain.is_empty() {
                return Some(chain);
            }
            return None;
        }
        target.filter().or_else(|| self.mss.filter.clone())
    }

    fn has_filter_effects(
        &self,
        target: &crate::animation::transition::AnimatedPropertyMap,
    ) -> bool {
        self.active_filter(target).map_or(false, |f| !f.is_empty())
    }

    fn build_filter_effect(
        &self,
        target: &crate::animation::transition::AnimatedPropertyMap,
    ) -> crate::render::display_list::Effect {
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

    fn interacted(&self) {
        if let Some(cb) = self.on_interaction.clone() {
            if let Ok(mut f) = cb.lock() {
                f();
            }
        }
    }

    fn max_zoom(&self) -> f64 {
        (self.provider.max_zoom + self.overzoom) as f64
    }

    fn emit_viewport(&mut self) {
        let Some(cb) = self.on_viewport_change.clone() else {
            return;
        };
        let vp = MapViewport {
            center_lat: self.center_lat,
            center_lng: self.center_lng,
            zoom: self.zoom.round().clamp(1.0, 22.0) as u8,
            zoom_level: self.zoom,
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

    fn local(&self, p: Point) -> (f32, f32) {
        (p.x - self.bounds.origin.x, p.y - self.bounds.origin.y)
    }

    fn geo_at(&self, p: Point) -> (f64, f64) {
        let (x, y) = self.local(p);
        tile_math::pixel_to_geo_f(x, y, self.center_lat, self.center_lng, self.zoom, self.bounds.size.width, self.bounds.size.height)
    }

    /// Сдвиг карты так, чтобы точка (lat, lng) оказалась под экранной точкой `p`.
    fn put_geo_at(&mut self, lat: f64, lng: f64, p: Point) {
        let (x, y) = self.local(p);
        let (gx, gy) = tile_math::world_px(lat, lng, self.zoom);
        let cx = gx - (x - self.bounds.size.width / 2.0) as f64;
        let cy = gy - (y - self.bounds.size.height / 2.0) as f64;
        let (clat, clng) = tile_math::world_px_to_geo(cx, cy, self.zoom);
        self.center_lat = clat.clamp(-85.05, 85.05);
        self.center_lng = ((clng + 180.0).rem_euclid(360.0)) - 180.0;
    }

    /// Масштаб вокруг экранной точки (она остаётся на месте).
    fn zoom_around(&mut self, new_zoom: f64, p: Point) {
        let (lat, lng) = self.geo_at(p);
        self.zoom = new_zoom.clamp(MIN_ZOOM, self.max_zoom());
        self.put_geo_at(lat, lng, p);
        self.mark_dirty(DirtyFlags::RENDER);
    }

    fn drag_to(&mut self, position: Point) {
        // точка карты, взятая в начале перетаскивания, следует за пальцем
        let (sx, sy) = tile_math::world_px(self.drag_center_lat, self.drag_center_lng, self.zoom);
        let cx = sx - (position.x - self.drag_start.x) as f64;
        let cy = sy - (position.y - self.drag_start.y) as f64;
        let (lat, lng) = tile_math::world_px_to_geo(cx, cy, self.zoom);
        self.center_lat = lat.clamp(-85.05, 85.05);
        self.center_lng = ((lng + 180.0).rem_euclid(360.0)) - 180.0;
        self.mark_dirty(DirtyFlags::RENDER);
    }

    fn fly(&mut self, lat: f64, lng: f64, zoom: f64, duration_ms: u32, easing: Easing) {
        self.fly_from = (self.center_lat, self.center_lng, self.zoom);
        self.fly_to = (lat, lng, zoom.clamp(MIN_ZOOM, self.max_zoom()));
        self.fly_animation = Some(Animation::tween(easing).from(0.0).to(1.0).duration_ms(duration_ms).build());
    }

    fn clickable_marker_at(&self, position: Point) -> Option<u64> {
        self.on_marker_click.as_ref()?;
        let (x, y) = self.local(position);
        marker_at_f(&self.markers, Point::new(x, y), self.center_lat, self.center_lng, self.zoom, self.bounds.size, web_time::Instant::now())
    }

    fn track_press_movement(&mut self, position: Point) {
        if let Some(origin) = self.press_position {
            let dx = position.x - origin.x;
            let dy = position.y - origin.y;
            if dx * dx + dy * dy > TAP_SLOP * TAP_SLOP {
                self.press_position = None;
                self.interacted();
            }
        }
    }

    /// Конец нажатия без сдвига: метка, двойное касание (приблизить) или касание карты.
    fn finish_press(&mut self, touch: bool) {
        let Some(position) = self.press_position.take() else {
            return;
        };
        if std::mem::take(&mut self.long_pressed) {
            return;
        }
        if let Some(id) = self.clickable_marker_at(position) {
            if let Some(cb) = self.on_marker_click.clone() {
                if let Ok(mut f) = cb.lock() {
                    f(id);
                }
            }
            return;
        }
        // двойное касание пальцем (для мыши — Event::DoubleClick)
        let now = web_time::Instant::now();
        if touch {
            if let Some((p, t)) = self.last_tap {
                let (dx, dy) = (p.x - position.x, p.y - position.y);
                if now.duration_since(t).as_millis() < DOUBLE_TAP_MS && dx * dx + dy * dy < 40.0 * 40.0 {
                    self.last_tap = None;
                    self.double_tap(position);
                    return;
                }
            }
            self.last_tap = Some((position, now));
        }
        if let Some(cb) = self.on_tap.clone() {
            let (lat, lng) = self.geo_at(position);
            if let Ok(mut f) = cb.lock() {
                f(lat, lng);
            }
        }
    }

    fn double_tap(&mut self, position: Point) {
        self.interacted();
        let target = if self.smooth_zoom { self.zoom + 1.0 } else { self.zoom.round() + 1.0 };
        let (lat, lng) = self.geo_at(position);
        let old = self.zoom;
        self.zoom = target.clamp(MIN_ZOOM, self.max_zoom());
        // центр после приближения: точка под пальцем остаётся под пальцем
        self.put_geo_at(lat, lng, position);
        let (tlat, tlng, tz) = (self.center_lat, self.center_lng, self.zoom);
        self.zoom = old;
        self.put_geo_at(lat, lng, position);
        self.fly(tlat, tlng, tz, 250, Easing::EaseOutCubic);
        self.mark_dirty(DirtyFlags::RENDER);
    }

    fn start_drag(&mut self, position: Point) {
        self.dragging = true;
        self.drag_start = position;
        self.drag_center_lat = self.center_lat;
        self.drag_center_lng = self.center_lng;
    }

    fn two_touches(&self) -> Option<(Point, f32)> {
        if self.touches.len() < 2 {
            return None;
        }
        let pts: Vec<&Point> = self.touches.values().collect();
        let dx = pts[1].x - pts[0].x;
        let dy = pts[1].y - pts[0].y;
        Some((Point::new((pts[0].x + pts[1].x) / 2.0, (pts[0].y + pts[1].y) / 2.0), (dx * dx + dy * dy).sqrt().max(1.0)))
    }
}

pub fn marker_at(
    markers: &[MapMarker],
    position: Point,
    center_lat: f64,
    center_lng: f64,
    zoom: u8,
    viewport: Size,
    now: web_time::Instant,
) -> Option<u64> {
    marker_at_f(markers, position, center_lat, center_lng, zoom as f64, viewport, now)
}

/// [`marker_at`] при дробном масштабе.
pub fn marker_at_f(
    markers: &[MapMarker],
    position: Point,
    center_lat: f64,
    center_lng: f64,
    zoom: f64,
    viewport: Size,
    now: web_time::Instant,
) -> Option<u64> {
    markers.iter().rev().find_map(|marker| {
        let id = marker.id?;
        if marker.is_expired(now) || marker.current_opacity(now) <= 0.001 {
            return None;
        }
        let (px, py) = tile_math::geo_to_pixel_f(marker.lat, marker.lng, center_lat, center_lng, zoom, viewport.width, viewport.height);
        let radius = (marker.size * marker.current_scale(now) / 2.0).max(MIN_MARKER_HIT_RADIUS);
        let dx = position.x - px;
        let dy = position.y - py;
        (dx * dx + dy * dy <= radius * radius).then_some(id)
    })
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
            self.polylines = m.polylines.clone();
            self.preferred_width = m.width;
            self.preferred_height = m.height;
            self.smooth_zoom = m.smooth_zoom;
            self.tile_detail = m.tile_detail;
            self.overzoom = m.overzoom;
            self.show_attribution = m.show_attribution;
            self.on_viewport_change = m.on_viewport_change.clone();
            self.on_marker_click = m.on_marker_click.clone();
            self.on_tap = m.on_tap.clone();
            self.on_long_press = m.on_long_press.clone();
            self.on_interaction = m.on_interaction.clone();

            if let Some(c) = m.camera {
                if self.camera_seq != Some(c.seq) {
                    self.camera_seq = Some(c.seq);
                    if c.animate {
                        self.fly(c.lat, c.lng, c.zoom, m.animate_duration_ms.min(800), m.animate_easing);
                    } else {
                        self.fly_animation = None;
                        self.center_lat = c.lat;
                        self.center_lng = c.lng;
                        self.zoom = c.zoom.clamp(MIN_ZOOM, self.max_zoom());
                    }
                }
            }

            if let Some((target_lat, target_lng, target_zoom)) = m.animate_target {
                let needs_anim = (target_lat - self.fly_to.0).abs() > 1e-8
                    || (target_lng - self.fly_to.1).abs() > 1e-8
                    || (target_zoom as f64 - self.fly_to.2).abs() > 0.5
                    || self.fly_animation.is_none();

                if needs_anim {
                    self.fly(target_lat, target_lng, target_zoom as f64, m.animate_duration_ms, m.animate_easing);
                }
            }

            self.mark_dirty(DirtyFlags::RENDER);
        }
    }

    fn layout(&mut self, constraints: Constraints) -> Size {
        let w = self
            .preferred_width
            .unwrap_or(constraints.max_width)
            .min(constraints.max_width);
        let h = self
            .preferred_height
            .unwrap_or_else(|| {
                if constraints.max_height.is_finite() {
                    constraints.max_height
                } else {
                    400.0
                }
            })
            .min(constraints.max_height);
        self.bounds = Rect::new(Point::zero(), Size::new(w, h));
        // Размер области известен только после раскладки. Кадр `tick` мог
        // сообщить положение ещё с нулевым размером, а неподвижная карта
        // новых кадров не просит — оверлеи (облако, метки поверх него) так
        // и остались бы без размера до первого жеста.
        self.emit_viewport();
        Size::new(w, h)
    }

    fn build_display_list(&self, list: &mut DisplayList, _clip: Rect) {
        let bounds = self.bounds;

        list.push_clip(bounds);

        let target = self.mss.target_props(false, false, false, false);
        let filtered = self.has_filter_effects(&target);
        if filtered {
            list.push_effect_layer(self.build_filter_effect(&target), bounds);
        }

        list.push_rect(bounds, Color::new(0.678, 0.847, 0.902, 1.0), [0.0; 4]);

        if let Some(ref tile_atlas) = self.tile_atlas {
            self.render_tiles(list, tile_atlas);
        }

        if filtered {
            list.pop_effect_layer();
        }

        self.render_polylines(list);

        if self.show_attribution {
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
        }

        list.push_z_barrier();

        self.render_markers(list);

        list.pop_clip();
    }

    fn handle_event(
        &mut self,
        event: &Event,
        ctx: &mut crate::widget::context::EventContext,
    ) -> EventResult {
        match event {
            // правая кнопка мыши — то же, что долгое нажатие пальцем (долгого нажатия мышью syngui не выдаёт)
            Event::MouseDown { position, button: crate::input::MouseButton::Right } => {
                if self.bounds.contains(*position) && self.touches.is_empty() {
                    if let Some(cb) = self.on_long_press.clone() {
                        let (lat, lng) = self.geo_at(*position);
                        if let Ok(mut f) = cb.lock() {
                            f(lat, lng);
                        }
                        return EventResult::Handled;
                    }
                }
            }
            Event::MouseDown { position, .. } => {
                // палец, который syngui ещё и превращает в мышь, обрабатывается касаниями
                if self.bounds.contains(*position) && self.touches.is_empty() {
                    self.fly_animation = None;
                    self.start_drag(*position);
                    self.press_position = Some(*position);
                    self.long_pressed = false;
                    ctx.set_cursor(CursorIcon::Grabbing);
                    return EventResult::Handled;
                }
            }
            Event::MouseMove(position) => {
                if self.dragging && self.touches.is_empty() {
                    self.track_press_movement(*position);
                    if self.press_position.is_none() {
                        self.drag_to(*position);
                        self.emit_viewport();
                    }
                    ctx.set_cursor(CursorIcon::Grabbing);
                    return EventResult::Handled;
                } else if self.bounds.contains(*position) {
                    if self.clickable_marker_at(*position).is_some() {
                        ctx.set_cursor(CursorIcon::Pointer);
                    } else {
                        ctx.set_cursor(CursorIcon::Grab);
                    }
                }
            }
            Event::MouseUp { .. } => {
                if self.dragging && self.touches.is_empty() {
                    self.dragging = false;
                    ctx.set_cursor(CursorIcon::Default);
                    self.finish_press(false);
                    return EventResult::Handled;
                }
            }
            Event::DoubleClick { position, .. } => {
                if self.bounds.contains(*position) && self.touches.is_empty() {
                    self.double_tap(*position);
                    return EventResult::Handled;
                }
            }
            Event::LongPress { position } => {
                if self.bounds.contains(*position) && self.press_position.is_some() {
                    if let Some(cb) = self.on_long_press.clone() {
                        let (lat, lng) = self.geo_at(*position);
                        self.long_pressed = true;
                        if let Ok(mut f) = cb.lock() {
                            f(lat, lng);
                        }
                        return EventResult::Handled;
                    }
                }
            }
            Event::MouseWheel { position, delta, .. } => {
                if self.bounds.contains(*position) {
                    self.fly_animation = None;
                    self.interacted();
                    if self.smooth_zoom {
                        // одна «ступенька» колеса (~12 единиц) — половина уровня
                        let z = self.zoom + (*delta as f64) / 24.0;
                        self.zoom_around(z, *position);
                    } else {
                        const ZOOM_THRESHOLD: f32 = 12.0;
                        self.zoom_accumulator += *delta;
                        if self.zoom_accumulator.abs() >= ZOOM_THRESHOLD {
                            let step = if self.zoom_accumulator > 0.0 { 1.0 } else { -1.0 };
                            self.zoom_accumulator = 0.0;
                            let z = (self.zoom.round() + step).min(self.provider.max_zoom as f64);
                            self.zoom_around(z, *position);
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
                        self.start_drag(*position);
                        self.press_position = Some(*position);
                        self.long_pressed = false;
                    } else if let Some((center, dist)) = self.two_touches() {
                        self.dragging = false;
                        self.press_position = None;
                        self.last_tap = None;
                        let (lat, lng) = self.geo_at(center);
                        self.interacted();
                        self.pinch = Some(Pinch { start_distance: dist, start_zoom: self.zoom, anchor_lat: lat, anchor_lng: lng });
                    }
                    return EventResult::Handled;
                }
            }
            Event::TouchMove { id, position } => {
                if self.touches.contains_key(id) {
                    self.touches.insert(*id, *position);
                    if self.touches.len() == 1 && self.dragging {
                        self.track_press_movement(*position);
                        if self.press_position.is_none() {
                            self.drag_to(*position);
                            self.emit_viewport();
                        }
                        return EventResult::Handled;
                    } else if let (Some(p), Some((center, dist))) = (self.pinch, self.two_touches()) {
                        // точка карты, бывшая под пальцами, остаётся между ними: масштаб и сдвиг разом
                        let z = (p.start_zoom + (dist / p.start_distance).log2() as f64).clamp(MIN_ZOOM, self.max_zoom());
                        self.zoom = z;
                        self.put_geo_at(p.anchor_lat, p.anchor_lng, center);
                        self.mark_dirty(DirtyFlags::RENDER);
                        self.emit_viewport();
                        return EventResult::Handled;
                    }
                }
            }
            Event::TouchEnd { id, .. } => {
                if self.touches.remove(id).is_some() {
                    if self.touches.is_empty() {
                        self.dragging = false;
                        if self.pinch.take().is_some() && !self.smooth_zoom {
                            // без плавного масштаба — к ближайшему уровню плиток
                            let center = Point::new(self.bounds.origin.x + self.bounds.size.width / 2.0, self.bounds.origin.y + self.bounds.size.height / 2.0);
                            self.zoom_around(self.zoom.round(), center);
                        }
                        self.finish_press(true);
                        self.emit_viewport();
                    } else if self.touches.len() == 1 {
                        self.pinch = None;
                        let remaining = *self.touches.values().next().unwrap();
                        self.start_drag(remaining);
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
            let zoom = self.fly_from.2 + (self.fly_to.2 - self.fly_from.2) * t;
            self.zoom = if self.smooth_zoom { zoom } else { zoom.round() };
            self.center_lat = self.center_lat.clamp(-85.05, 85.05);
            self.mark_dirty(DirtyFlags::RENDER);

            if !still_running {
                self.center_lat = self.fly_to.0;
                self.center_lng = self.fly_to.1;
                self.zoom = if self.smooth_zoom { self.fly_to.2 } else { self.fly_to.2.round() };
                self.fly_animation = None;
            } else {
                needs_frame = true;
            }
        }

        if let Some(ref source) = self.provider_source {
            let new_provider = source.lock().unwrap_or_else(|e| e.into_inner()).clone();
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
        let keyframe_active = self
            .mss
            .keyframe_animation
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

        needs_frame
            || tiles_pending
            || tiles_deferred
            || self.provider_source.is_some()
            || transition_active
            || keyframe_active
            || markers_animating
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

    fn element_type_name(&self) -> &str {
        "MapView"
    }

    fn set_classes(&mut self, classes: Vec<String>) {
        self.classes = classes;
        self.mark_dirty(DirtyFlags::RENDER);
    }

    fn get_classes(&self) -> &[String] {
        &self.classes
    }

    fn mss(&self) -> Option<&crate::mss::MssFields> {
        Some(&self.mss)
    }

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
    /// Плитки уровня `round(zoom + tile_detail)` (не глубже источника), растянутые до текущего масштаба.
    fn render_tiles(&self, list: &mut DisplayList, tile_atlas: &Arc<Mutex<TileAtlas>>) {
        let bounds = self.bounds;
        let vw = bounds.size.width;
        let vh = bounds.size.height;
        let detail = self.tile_detail.unwrap_or_else(|| (list.scale_factor() as f64).log2().clamp(0.0, 2.0));
        let zt = (self.zoom + detail).round().clamp(0.0, self.provider.max_zoom as f64) as u8;
        let scale = 2f64.powf(self.zoom - zt as f64);

        // центр в мировых пикселях уровня плиток
        let ctx = tile_math::lng_to_tile_x(self.center_lng, zt) * 256.0;
        let cty = tile_math::lat_to_tile_y(self.center_lat, zt) * 256.0;
        let half_w = vw as f64 / 2.0 / scale;
        let half_h = vh as f64 / 2.0 / scale;
        let tx_min = ((ctx - half_w) / 256.0).floor() as i64;
        let tx_max = ((ctx + half_w) / 256.0).floor() as i64;
        let ty_min = ((cty - half_h) / 256.0).floor() as i64;
        let ty_max = ((cty + half_h) / 256.0).floor() as i64;
        let n = 1i64 << zt;
        // экранная координата мирового пикселя уровня zt (края округлены — без щелей между плитками)
        let sx = |wx: f64| (bounds.origin.x as f64 + vw as f64 / 2.0 + (wx - ctx) * scale).round() as f32;
        let sy = |wy: f64| (bounds.origin.y as f64 + vh as f64 / 2.0 + (wy - cty) * scale).round() as f32;

        let mut atlas = match tile_atlas.lock() {
            Ok(a) => a,
            Err(_) => return,
        };

        for ty in ty_min..=ty_max {
            if ty < 0 || ty >= n {
                continue;
            }
            for tx in tx_min..=tx_max {
                let wrapped_tx = tx.rem_euclid(n);
                let key = TileKey { x: wrapped_tx as u32, y: ty as u32, z: zt, provider_id: self.provider.id };
                let x0 = sx(tx as f64 * 256.0);
                let x1 = sx((tx + 1) as f64 * 256.0);
                let y0 = sy(ty as f64 * 256.0);
                let y1 = sy((ty + 1) as f64 * 256.0);
                let tile_rect = Rect::new(Point::new(x0, y0), Size::new(x1 - x0, y1 - y0));

                if let Some(slot) = atlas.get_tile(&key) {
                    let uv_rect = Rect::new(Point::new(slot.uv_x, slot.uv_y), Size::new(slot.uv_w, slot.uv_h));
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
                        let uv_rect = Rect::new(Point::new(slot.uv_x, slot.uv_y), Size::new(slot.uv_w, slot.uv_h));
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
            if pz == 0 {
                break;
            }
            px /= 2;
            py /= 2;
            pz -= 1;

            let parent_key = TileKey {
                x: px,
                y: py,
                z: pz,
                provider_id,
            };
            if let Some(slot) = atlas.get_tile(&parent_key) {
                let depth = orig_z - pz;
                let scale = 1.0 / (1u32 << depth) as f32;

                let sub_x = (orig_x % (1u32 << depth)) as f32 * scale;
                let sub_y = (orig_y % (1u32 << depth)) as f32 * scale;

                let uv_rect = Rect::new(
                    Point::new(slot.uv_x + sub_x * slot.uv_w, slot.uv_y + sub_y * slot.uv_h),
                    Size::new(slot.uv_w * scale, slot.uv_h * scale),
                );
                list.push_image(tile_rect, TextureId(0), uv_rect, Color::WHITE);
                return true;
            }
        }
        false
    }

    fn render_polylines(&self, list: &mut DisplayList) {
        let b = self.bounds;
        for line in &self.polylines {
            if line.points.len() < 2 {
                continue;
            }
            let pts: Vec<[f32; 2]> = line
                .points
                .iter()
                .map(|&(lat, lng)| {
                    let (x, y) = tile_math::geo_to_pixel_f(lat, lng, self.center_lat, self.center_lng, self.zoom, b.size.width, b.size.height);
                    [b.origin.x + x, b.origin.y + y]
                })
                .collect();
            if let Some((c, w)) = line.outline {
                list.push_line_strip(pts.clone(), c, w);
            }
            list.push_line_strip(pts, line.color, line.width);
        }
    }

    fn render_markers(&self, list: &mut DisplayList) {
        paint_markers(
            list,
            &self.markers,
            self.bounds,
            (self.center_lat, self.center_lng, self.zoom),
            self.text_measure.as_deref(),
        );
    }
}

/// Метки на области `bounds` при центре и масштабе `view` — общая отрисовка
/// для [`MapView`] и [`MarkerOverlay`].
pub(crate) fn paint_markers(
    list: &mut DisplayList,
    markers: &[MapMarker],
    bounds: Rect,
    view: (f64, f64, f64),
    text_measure: Option<&dyn TextMeasure>,
) {
    let (center_lat, center_lng, zoom) = view;
    let now = web_time::Instant::now();

    for marker in markers {
        if marker.is_expired(now) {
            continue;
        }

        let opacity = marker.current_opacity(now);
        if opacity <= 0.001 {
            continue;
        }
        let scale = marker.current_scale(now);
        let effective_size = marker.size * scale;

        let (px, py) = tile_math::geo_to_pixel_f(
            marker.lat,
            marker.lng,
            center_lat,
            center_lng,
            zoom,
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
            let label_w = text_measure
                .map(|tm| tm.measure_text_width(label, label_font, label.chars().count()))
                .unwrap_or_else(|| label.chars().count() as f32 * label_font * 0.6)
                + 8.0;
            let label_h = 18.0;
            let lx = screen_x - label_w / 2.0;
            let ly = screen_y - r - label_h - 4.0;

            let label_bg = Rect::new(Point::new(lx, ly), Size::new(label_w, label_h));
            list.push_rect(
                label_bg,
                Color::new(0.15, 0.15, 0.15, 0.85 * opacity),
                [4.0; 4],
            );

            let text_rect = Rect::new(
                Point::new(lx + 4.0, ly + 1.0),
                Size::new(label_w - 8.0, label_h - 2.0),
            );
            list.push_text_centered(label, text_rect, Color::new(1.0, 1.0, 1.0, opacity), 11.0);
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    const VIEWPORT: Size = Size::new(400.0, 300.0);
    const CENTER: (f64, f64) = (53.2144, 63.6246);
    const ZOOM: u8 = 15;

    fn hit(markers: &[MapMarker], x: f32, y: f32) -> Option<u64> {
        marker_at(
            markers,
            Point::new(x, y),
            CENTER.0,
            CENTER.1,
            ZOOM,
            VIEWPORT,
            web_time::Instant::now(),
        )
    }

    #[test]
    fn marker_at_finds_marker_under_cursor() {
        let markers = vec![MapMarker::new(CENTER.0, CENTER.1).id(7).size(16.0)];
        assert_eq!(hit(&markers, 200.0, 150.0), Some(7));
        assert_eq!(hit(&markers, 207.0, 150.0), Some(7));
        assert_eq!(hit(&markers, 230.0, 150.0), None);
    }

    #[test]
    fn marker_at_skips_markers_without_id_and_prefers_topmost() {
        let markers = vec![
            MapMarker::new(CENTER.0, CENTER.1).id(1),
            MapMarker::new(CENTER.0, CENTER.1).id(2),
            MapMarker::new(CENTER.0, CENTER.1),
        ];
        assert_eq!(hit(&markers, 200.0, 150.0), Some(2));
    }

    #[test]
    fn marker_at_keeps_small_markers_reachable() {
        let markers = vec![MapMarker::new(CENTER.0, CENTER.1).id(3).size(4.0)];
        assert_eq!(
            hit(&markers, 200.0 + MIN_MARKER_HIT_RADIUS - 1.0, 150.0),
            Some(3)
        );
    }
}
