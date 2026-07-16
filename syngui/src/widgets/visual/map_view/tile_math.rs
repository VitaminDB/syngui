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
