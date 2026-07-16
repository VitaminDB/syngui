#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, Default)]
pub struct ClipRect {
    pub x: u32,
    pub y: u32,
    pub width: u32,
    pub height: u32,
    pub enabled: bool,
    corner_radius_bits: [u32; 4],
}

impl ClipRect {
    pub fn full_screen() -> Self {
        Self {
            x: 0,
            y: 0,
            width: u32::MAX,
            height: u32::MAX,
            enabled: false,
            corner_radius_bits: [0; 4],
        }
    }

    pub fn new(x: u32, y: u32, width: u32, height: u32) -> Self {
        Self {
            x,
            y,
            width,
            height,
            enabled: true,
            corner_radius_bits: [0; 4],
        }
    }

    pub fn from_rect(rect: crate::core::Rect) -> Self {
        let x = rect.origin.x.floor() as u32;
        let y = rect.origin.y.floor() as u32;
        let right = (rect.origin.x + rect.size.width).ceil() as u32;
        let bottom = (rect.origin.y + rect.size.height).ceil() as u32;
        Self {
            x,
            y,
            width: right - x,
            height: bottom - y,
            enabled: true,
            corner_radius_bits: [0; 4],
        }
    }

    pub fn from_rect_rounded(rect: crate::core::Rect, corner_radius: [f32; 4]) -> Self {
        let mut clip = Self::from_rect(rect);
        clip.set_corner_radius(corner_radius);
        clip
    }

    pub fn corner_radius_f32(&self) -> [f32; 4] {
        self.corner_radius_bits.map(f32::from_bits)
    }

    pub fn set_corner_radius(&mut self, radii: [f32; 4]) {
        self.corner_radius_bits = radii.map(f32::to_bits);
    }

    pub fn has_corner_radius(&self) -> bool {
        self.corner_radius_bits != [0; 4]
    }

    pub fn intersect(&self, other: crate::core::Rect) -> Self {
        if !self.enabled {
            return Self::from_rect(other);
        }

        let x1 = self.x.max(other.origin.x.floor() as u32);
        let y1 = self.y.max(other.origin.y.floor() as u32);
        let x2 = (self.x + self.width).min((other.origin.x + other.size.width).ceil() as u32);
        let y2 = (self.y + self.height).min((other.origin.y + other.size.height).ceil() as u32);

        if x2 <= x1 || y2 <= y1 {
            return Self::new(x1, y1, 0, 0);
        }

        let mut result = Self::new(x1, y1, x2 - x1, y2 - y1);
        result.corner_radius_bits = child_corner_bits_from_parent(
            self.x,
            self.y,
            self.width,
            self.height,
            self.corner_radius_bits,
            x1,
            y1,
            x2,
            y2,
        );
        result
    }

    pub fn intersect_rounded(&self, other: crate::core::Rect, corner_radius: [f32; 4]) -> Self {
        let mut result = self.intersect(other);
        let has_new_radius = corner_radius.iter().any(|r| *r > 0.0);
        if has_new_radius {
            result.set_corner_radius(corner_radius);
        }
        result
    }
}

fn child_corner_bits_from_parent(
    parent_x: u32,
    parent_y: u32,
    parent_w: u32,
    parent_h: u32,
    parent_radius_bits: [u32; 4],
    child_x1: u32,
    child_y1: u32,
    child_x2: u32,
    child_y2: u32,
) -> [u32; 4] {
    if parent_radius_bits == [0; 4] {
        return [0; 4];
    }

    let parent_x2 = parent_x + parent_w;
    let parent_y2 = parent_y + parent_h;

    let touches_left = child_x1 <= parent_x;
    let touches_top = child_y1 <= parent_y;
    let touches_right = child_x2 >= parent_x2;
    let touches_bottom = child_y2 >= parent_y2;

    let mut out = [0u32; 4];
    if touches_left && touches_top {
        out[0] = parent_radius_bits[0];
    }
    if touches_right && touches_top {
        out[1] = parent_radius_bits[1];
    }
    if touches_right && touches_bottom {
        out[2] = parent_radius_bits[2];
    }
    if touches_left && touches_bottom {
        out[3] = parent_radius_bits[3];
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::core::{Point, Rect, Size};

    fn r(x: f32, y: f32, w: f32, h: f32) -> Rect {
        Rect::new(Point::new(x, y), Size::new(w, h))
    }

    #[test]
    fn child_fully_inside_parent_has_no_rounded_corners() {
        let parent = ClipRect::from_rect_rounded(r(0.0, 0.0, 1000.0, 800.0), [20.0; 4]);
        assert!(parent.has_corner_radius());

        let child = parent.intersect(r(300.0, 300.0, 100.0, 100.0));
        assert_eq!(
            child.corner_radius_f32(),
            [0.0; 4],
            "child внутри parent не должен наследовать rounded углы"
        );
    }

    #[test]
    fn child_covering_parent_inherits_all_corners() {
        let parent = ClipRect::from_rect_rounded(r(0.0, 0.0, 500.0, 400.0), [16.0; 4]);
        let child = parent.intersect(r(0.0, 0.0, 500.0, 400.0));
        assert_eq!(child.corner_radius_f32(), [16.0; 4]);
    }

    #[test]
    fn child_at_top_left_corner_inherits_only_top_left() {
        let parent = ClipRect::from_rect_rounded(r(0.0, 0.0, 500.0, 400.0), [10.0; 4]);
        let child = parent.intersect(r(0.0, 0.0, 100.0, 80.0));
        let radii = child.corner_radius_f32();
        assert_eq!(radii[0], 10.0, "TL должен наследоваться");
        assert_eq!(radii[1], 0.0, "TR нет — правый край не достигнут");
        assert_eq!(radii[2], 0.0, "BR нет");
        assert_eq!(radii[3], 0.0, "BL нет — нижний край не достигнут");
    }

    #[test]
    fn parent_without_radius_never_propagates() {
        let parent = ClipRect::new(0, 0, 500, 400);
        let child = parent.intersect(r(0.0, 0.0, 500.0, 400.0));
        assert_eq!(child.corner_radius_f32(), [0.0; 4]);
    }

    #[test]
    fn intersect_rounded_override_child_radii() {
        let parent = ClipRect::from_rect_rounded(r(0.0, 0.0, 1000.0, 800.0), [20.0; 4]);
        let child = parent.intersect_rounded(r(100.0, 100.0, 300.0, 300.0), [8.0; 4]);
        assert_eq!(child.corner_radius_f32(), [8.0; 4]);
    }
}
