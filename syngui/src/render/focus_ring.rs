use crate::core::{Color, Rect};
use crate::render::DisplayList;

pub const FOCUS_COLOR: Color = Color::new(0.0955, 0.3005, 0.9130, 1.0);

const RING_WIDTH: f32 = 2.0;

const RING_OFFSET: f32 = 2.0;

pub fn draw_focus_ring(list: &mut DisplayList, bounds: Rect, corner_radius: f32) {
    list.push_outline(
        bounds,
        FOCUS_COLOR,
        RING_WIDTH,
        RING_OFFSET,
        [corner_radius; 4],
    );
}
