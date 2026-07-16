use crate::core::{Color, Gradient, Point, Rect, Size};
use crate::gpu::image_store::{ImageHandle, ImageStore};
use crate::input::{Event, EventResult};
use crate::layout::Constraints;
use crate::mss::{ComputedStyle, MssFields};
use crate::render::{DisplayList, TextureId};
use crate::widget::context::EventContext;
use crate::widget::{DirtyFlags, Element, ElementId, ElementTree, StyledElement, UpdateContext, Widget};
use std::any::Any;
use std::sync::Arc;
use crate::core::sync::Mutex;

use super::{tile_math, MapViewport};

#[derive(Clone, Copy, Debug, PartialEq)]
pub struct HeatPoint {
    pub lat: f64,
    pub lng: f64,
    pub value: f32,
}

impl HeatPoint {
    pub fn new(lat: f64, lng: f64, value: f32) -> Self {
        Self { lat, lng, value }
    }
}

fn default_gradient() -> Gradient {
    use crate::core::gradient::ColorStop;
    Gradient::Linear {
        angle_deg: 0.0,
        stops: vec![
            ColorStop::new(Color::from_hex("#2563EB"), 0.0),
            ColorStop::new(Color::from_hex("#22D3EE"), 0.25),
            ColorStop::new(Color::from_hex("#34D399"), 0.45),
            ColorStop::new(Color::from_hex("#FACC15"), 0.7),
            ColorStop::new(Color::from_hex("#F97316"), 0.85),
            ColorStop::new(Color::from_hex("#EF4444"), 1.0),
        ],
    }
}

pub struct HeatOverlay {
    points: Vec<HeatPoint>,
    viewport: MapViewport,
    color_min: f32,
    color_max: f32,
    gradient: Gradient,
    idw_power: f32,
    resolution: u32,
    opacity: f32,
    cache_key: String,
}

impl HeatOverlay {
    pub fn new() -> Self {
        Self {
            points: Vec::new(),
            viewport: MapViewport {
                center_lat: 0.0,
                center_lng: 0.0,
                zoom: 1,
                viewport_w: 0.0,
                viewport_h: 0.0,
            },
            color_min: 0.0,
            color_max: 1.0,
            gradient: default_gradient(),
            idw_power: 2.0,
            resolution: 160,
            opacity: 0.72,
            cache_key: "syngui-map-heat".to_string(),
        }
    }

    pub fn points(mut self, points: Vec<HeatPoint>) -> Self {
        self.points = points;
        self
    }

    pub fn viewport(mut self, viewport: MapViewport) -> Self {
        self.viewport = viewport;
        self
    }

    pub fn color_range(mut self, min: f32, max: f32) -> Self {
        self.color_min = min;
        self.color_max = max;
        self
    }

    pub fn gradient(mut self, gradient: Gradient) -> Self {
        self.gradient = gradient;
        self
    }

    pub fn idw_power(mut self, power: f32) -> Self {
        self.idw_power = power.max(0.1);
        self
    }

    pub fn resolution(mut self, resolution: u32) -> Self {
        self.resolution = resolution.clamp(8, 320);
        self
    }

    pub fn opacity(mut self, opacity: f32) -> Self {
        self.opacity = opacity.clamp(0.0, 1.0);
        self
    }

    pub fn cache_key(mut self, key: impl Into<String>) -> Self {
        self.cache_key = key.into();
        self
    }
}

impl Default for HeatOverlay {
    fn default() -> Self {
        Self::new()
    }
}

impl Widget for HeatOverlay {
    fn create_element(&self) -> Box<dyn Element> {
        Box::new(HeatOverlayElement {
            id: ElementId::new(),
            points: self.points.clone(),
            viewport: self.viewport,
            color_min: self.color_min,
            color_max: self.color_max,
            gradient: self.gradient.clone(),
            idw_power: self.idw_power,
            resolution: self.resolution,
            opacity: self.opacity,
            cache_key: self.cache_key.clone(),
            bounds: Rect::zero(),
            image_store: None,
            image_handle: None,
            buf_dims: (0, 0),
            last_gen: None,
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

#[derive(PartialEq)]
struct GenKey {
    vp: MapViewport,
    points_sig: u64,
    bw: u32,
    bh: u32,
    color_min_bits: u32,
    color_max_bits: u32,
    opacity_bits: u32,
}

pub struct HeatOverlayElement {
    id: ElementId,
    points: Vec<HeatPoint>,
    viewport: MapViewport,
    color_min: f32,
    color_max: f32,
    gradient: Gradient,
    idw_power: f32,
    resolution: u32,
    opacity: f32,
    cache_key: String,
    bounds: Rect,
    image_store: Option<Arc<Mutex<ImageStore>>>,
    image_handle: Option<ImageHandle>,
    buf_dims: (u32, u32),
    last_gen: Option<GenKey>,
    classes: Vec<String>,
    dirty_flags: DirtyFlags,
    mss: MssFields,
}

fn points_signature(points: &[HeatPoint]) -> u64 {
    let mut h: u64 = 0xcbf29ce484222325;
    for p in points {
        for bits in [p.lat.to_bits(), p.lng.to_bits(), (p.value as f64).to_bits()] {
            h ^= bits;
            h = h.wrapping_mul(0x100000001b3);
        }
    }
    h
}

impl HeatOverlayElement {
    fn gen_key(&self, bw: u32, bh: u32) -> GenKey {
        GenKey {
            vp: self.viewport,
            points_sig: points_signature(&self.points),
            bw,
            bh,
            color_min_bits: self.color_min.to_bits(),
            color_max_bits: self.color_max.to_bits(),
            opacity_bits: self.opacity.to_bits(),
        }
    }

    fn maybe_generate(&mut self) {
        let vp = self.viewport;
        if vp.viewport_w <= 1.0 || vp.viewport_h <= 1.0 || self.points.is_empty() {
            return;
        }

        let bw = self.resolution.max(8);
        let aspect = (vp.viewport_h / vp.viewport_w).max(0.05);
        let bh = ((bw as f32) * aspect).round().clamp(8.0, 320.0) as u32;

        let key = self.gen_key(bw, bh);
        if self.last_gen.as_ref() == Some(&key) {
            return;
        }

        let store = match &self.image_store {
            Some(s) => s.clone(),
            None => {
                eprintln!("HeatOverlay: image_store недоступен, слой не рисуется");
                return;
            }
        };

        let rgba = self.render_idw(bw, bh, vp);

        let mut store = match store.lock() {
            Ok(s) => s,
            Err(_) => {
                eprintln!("HeatOverlay: не удалось заблокировать image_store");
                return;
            }
        };
        let handle = match self.image_handle {
            Some(h) => h,
            None => {
                let (h, _) = store.request_rgba(&self.cache_key, bw, bh, rgba.clone());
                self.image_handle = Some(h);
                h
            }
        };
        store.update_rgba(handle, bw, bh, rgba);

        self.buf_dims = (bw, bh);
        self.last_gen = Some(key);
    }

    fn render_idw(&self, bw: u32, bh: u32, vp: MapViewport) -> Vec<u8> {
        let lut = self.gradient.rasterize(256);
        let alpha = (self.opacity * 255.0).round().clamp(0.0, 255.0) as u8;

        let screen_pts: Vec<(f32, f32, f32)> = self
            .points
            .iter()
            .map(|p| {
                let (px, py) = tile_math::geo_to_pixel(
                    p.lat,
                    p.lng,
                    vp.center_lat,
                    vp.center_lng,
                    vp.zoom,
                    vp.viewport_w,
                    vp.viewport_h,
                );
                (px, py, p.value)
            })
            .collect();

        let range = self.color_max - self.color_min;
        let half_power = 0.5 * self.idw_power;

        let mut rgba = vec![0u8; (bw * bh * 4) as usize];
        for j in 0..bh {
            let sy = (j as f32 + 0.5) / bh as f32 * vp.viewport_h;
            for i in 0..bw {
                let sx = (i as f32 + 0.5) / bw as f32 * vp.viewport_w;

                let mut num = 0.0f32;
                let mut den = 0.0f32;
                let mut exact: Option<f32> = None;
                for &(px, py, val) in &screen_pts {
                    let dx = sx - px;
                    let dy = sy - py;
                    let d2 = dx * dx + dy * dy;
                    if d2 < 1.0 {
                        exact = Some(val);
                        break;
                    }
                    let w = d2.powf(-half_power);
                    num += val * w;
                    den += w;
                }

                let value = exact.unwrap_or_else(|| if den > 0.0 { num / den } else { self.color_min });

                let t = if range.abs() < 1e-6 {
                    0.5
                } else {
                    ((value - self.color_min) / range).clamp(0.0, 1.0)
                };
                let idx = (t * 255.0).round().clamp(0.0, 255.0) as usize;
                let o = idx * 4;

                let p = ((j * bw + i) * 4) as usize;
                rgba[p] = lut[o];
                rgba[p + 1] = lut[o + 1];
                rgba[p + 2] = lut[o + 2];
                rgba[p + 3] = alpha;
            }
        }
        rgba
    }
}

impl Element for HeatOverlayElement {
    fn update(&mut self, widget: &dyn Widget, _ctx: &mut UpdateContext) {
        if let Some(w) = widget.as_any().downcast_ref::<HeatOverlay>() {
            self.points = w.points.clone();
            self.viewport = w.viewport;
            self.color_min = w.color_min;
            self.color_max = w.color_max;
            self.gradient = w.gradient.clone();
            self.idw_power = w.idw_power;
            self.resolution = w.resolution;
            self.opacity = w.opacity;
            self.cache_key = w.cache_key.clone();
            self.mark_dirty(DirtyFlags::LAYOUT | DirtyFlags::RENDER);
        }
    }

    fn layout(&mut self, constraints: Constraints) -> Size {
        let w = if constraints.max_width.is_finite() { constraints.max_width } else { 400.0 };
        let h = if constraints.max_height.is_finite() { constraints.max_height } else { 400.0 };
        self.bounds = Rect::new(self.bounds.origin, Size::new(w, h));
        self.maybe_generate();
        Size::new(w, h)
    }

    fn build_display_list(&self, list: &mut DisplayList, _clip: Rect) {
        if let Some(handle) = self.image_handle {
            if self.buf_dims.0 == 0 {
                return;
            }
            let uv = Rect::new(Point::new(0.0, 0.0), Size::new(1.0, 1.0));
            list.push_image(self.bounds, TextureId(handle.0), uv, Color::WHITE);
        }
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
        self.image_store = tree.image_store.clone();
    }

    fn element_type_name(&self) -> &str {
        "HeatOverlay"
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

impl StyledElement for HeatOverlayElement {
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

#[cfg(test)]
mod tests {
    use super::*;

    fn element(points: Vec<HeatPoint>, min: f32, max: f32) -> HeatOverlayElement {
        HeatOverlayElement {
            id: ElementId::new(),
            points,
            viewport: MapViewport {
                center_lat: 53.2144,
                center_lng: 63.6246,
                zoom: 13,
                viewport_w: 400.0,
                viewport_h: 300.0,
            },
            color_min: min,
            color_max: max,
            gradient: default_gradient(),
            idw_power: 2.0,
            resolution: 64,
            opacity: 0.7,
            cache_key: "test".to_string(),
            bounds: Rect::new(Point::new(0.0, 0.0), Size::new(400.0, 300.0)),
            image_store: None,
            image_handle: None,
            buf_dims: (0, 0),
            last_gen: None,
            classes: Vec::new(),
            dirty_flags: DirtyFlags::empty(),
            mss: MssFields::new(),
        }
    }

    #[test]
    fn render_idw_buffer_has_expected_size_and_alpha() {
        let pts = vec![
            HeatPoint::new(53.22, 63.62, 80.0),
            HeatPoint::new(53.21, 63.63, 10.0),
        ];
        let e = element(pts, 10.0, 80.0);
        let bw = 64;
        let bh = (bw as f32 * (e.viewport.viewport_h / e.viewport.viewport_w)).round() as u32;
        let rgba = e.render_idw(bw, bh, e.viewport);
        assert_eq!(rgba.len(), (bw * bh * 4) as usize);
        let expected_alpha = (0.7f32 * 255.0).round() as u8;
        assert!(rgba.chunks_exact(4).all(|p| p[3] == expected_alpha));
    }

    #[test]
    fn idw_pixel_near_hot_point_is_warmer() {
        let hot = HeatPoint::new(53.225, 63.62, 90.0);
        let cold = HeatPoint::new(53.205, 63.63, 5.0);
        let e = element(vec![hot, cold], 5.0, 90.0);

        let (hx, hy) = tile_math::geo_to_pixel(
            hot.lat, hot.lng, e.viewport.center_lat, e.viewport.center_lng,
            e.viewport.zoom, e.viewport.viewport_w, e.viewport.viewport_h,
        );
        let (cx, cy) = tile_math::geo_to_pixel(
            cold.lat, cold.lng, e.viewport.center_lat, e.viewport.center_lng,
            e.viewport.zoom, e.viewport.viewport_w, e.viewport.viewport_h,
        );

        let bw = 64u32;
        let bh = (bw as f32 * (e.viewport.viewport_h / e.viewport.viewport_w)).round() as u32;
        let rgba = e.render_idw(bw, bh, e.viewport);

        let sample = |sx: f32, sy: f32| -> [u8; 3] {
            let i = ((sx / e.viewport.viewport_w * bw as f32) as u32).min(bw - 1);
            let j = ((sy / e.viewport.viewport_h * bh as f32) as u32).min(bh - 1);
            let o = ((j * bw + i) * 4) as usize;
            [rgba[o], rgba[o + 1], rgba[o + 2]]
        };

        let hot_px = sample(hx, hy);
        let cold_px = sample(cx, cy);
        assert!(
            hot_px[0] as i32 - hot_px[2] as i32 > cold_px[0] as i32 - cold_px[2] as i32,
            "у горячей точки красный должен преобладать над синим сильнее, чем у холодной"
        );
    }

    #[test]
    fn points_signature_is_deterministic() {
        let a = vec![HeatPoint::new(1.0, 2.0, 3.0), HeatPoint::new(4.0, 5.0, 6.0)];
        let b = a.clone();
        let mut c = a.clone();
        c[0].value = 9.0;
        assert_eq!(points_signature(&a), points_signature(&b));
        assert_ne!(points_signature(&a), points_signature(&c));
    }
}
