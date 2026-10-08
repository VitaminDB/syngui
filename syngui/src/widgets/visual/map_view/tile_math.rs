use std::f64::consts::PI;

pub fn lng_to_tile_x(lng: f64, zoom: u8) -> f64 {
    let n = (1u64 << zoom) as f64;
    (lng + 180.0) / 360.0 * n
}

pub fn lat_to_tile_y(lat: f64, zoom: u8) -> f64 {
    let n = (1u64 << zoom) as f64;
    let lat_rad = lat.to_radians();
    (1.0 - lat_rad.tan().asinh() / PI) / 2.0 * n
}

pub fn tile_x_to_lng(x: f64, zoom: u8) -> f64 {
    let n = (1u64 << zoom) as f64;
    x / n * 360.0 - 180.0
}

pub fn tile_y_to_lat(y: f64, zoom: u8) -> f64 {
    let n = (1u64 << zoom) as f64;
    let lat_rad = (PI * (1.0 - 2.0 * y / n)).sinh().atan();
    lat_rad.to_degrees()
}

pub fn geo_to_pixel(
    lat: f64,
    lng: f64,
    center_lat: f64,
    center_lng: f64,
    zoom: u8,
    viewport_w: f32,
    viewport_h: f32,
) -> (f32, f32) {
    let tile_size = 256.0_f64;
    let cx = lng_to_tile_x(center_lng, zoom) * tile_size;
    let cy = lat_to_tile_y(center_lat, zoom) * tile_size;
    let px = lng_to_tile_x(lng, zoom) * tile_size;
    let py = lat_to_tile_y(lat, zoom) * tile_size;
    let x = (px - cx) as f32 + viewport_w / 2.0;
    let y = (py - cy) as f32 + viewport_h / 2.0;
    (x, y)
}

pub fn pixel_to_geo(
    px: f32,
    py: f32,
    center_lat: f64,
    center_lng: f64,
    zoom: u8,
    viewport_w: f32,
    viewport_h: f32,
) -> (f64, f64) {
    let tile_size = 256.0_f64;
    let cx = lng_to_tile_x(center_lng, zoom) * tile_size;
    let cy = lat_to_tile_y(center_lat, zoom) * tile_size;
    let world_x = cx + (px - viewport_w / 2.0) as f64;
    let world_y = cy + (py - viewport_h / 2.0) as f64;
    let lng = tile_x_to_lng(world_x / tile_size, zoom);
    let lat = tile_y_to_lat(world_y / tile_size, zoom);
    (lat, lng)
}

/// Мировые пиксели точки при дробном масштабе `zoom` (мир — 256·2^zoom пикселей).
pub fn world_px(lat: f64, lng: f64, zoom: f64) -> (f64, f64) {
    let size = 256.0 * 2f64.powf(zoom);
    let lat = lat.clamp(-85.05112878, 85.05112878);
    let x = (lng + 180.0) / 360.0 * size;
    let y = (1.0 - lat.to_radians().tan().asinh() / PI) / 2.0 * size;
    (x, y)
}

/// Обратное к [`world_px`].
pub fn world_px_to_geo(x: f64, y: f64, zoom: f64) -> (f64, f64) {
    let size = 256.0 * 2f64.powf(zoom);
    let lng = x / size * 360.0 - 180.0;
    let lat = (PI * (1.0 - 2.0 * y / size)).sinh().atan().to_degrees();
    (lat, lng)
}

/// [`geo_to_pixel`] при дробном масштабе.
pub fn geo_to_pixel_f(
    lat: f64,
    lng: f64,
    center_lat: f64,
    center_lng: f64,
    zoom: f64,
    viewport_w: f32,
    viewport_h: f32,
) -> (f32, f32) {
    let (cx, cy) = world_px(center_lat, center_lng, zoom);
    let (px, py) = world_px(lat, lng, zoom);
    // по долготе — кратчайший путь через 180-й меридиан
    let size = 256.0 * 2f64.powf(zoom);
    let mut dx = px - cx;
    if dx > size / 2.0 {
        dx -= size;
    } else if dx < -size / 2.0 {
        dx += size;
    }
    ((dx) as f32 + viewport_w / 2.0, (py - cy) as f32 + viewport_h / 2.0)
}

/// [`pixel_to_geo`] при дробном масштабе.
pub fn pixel_to_geo_f(
    px: f32,
    py: f32,
    center_lat: f64,
    center_lng: f64,
    zoom: f64,
    viewport_w: f32,
    viewport_h: f32,
) -> (f64, f64) {
    let (cx, cy) = world_px(center_lat, center_lng, zoom);
    world_px_to_geo(cx + (px - viewport_w / 2.0) as f64, cy + (py - viewport_h / 2.0) as f64, zoom)
}

#[cfg(test)]
mod frac_tests {
    use super::*;

    #[test]
    fn fractional_matches_integer() {
        let (x, y) = geo_to_pixel(55.0, 37.0, 55.1, 37.2, 12, 400.0, 300.0);
        let (fx, fy) = geo_to_pixel_f(55.0, 37.0, 55.1, 37.2, 12.0, 400.0, 300.0);
        assert!((x - fx).abs() < 0.01 && (y - fy).abs() < 0.01);
        let (lat, lng) = pixel_to_geo_f(fx, fy, 55.1, 37.2, 12.0, 400.0, 300.0);
        assert!((lat - 55.0).abs() < 1e-6 && (lng - 37.0).abs() < 1e-6);
    }
}
