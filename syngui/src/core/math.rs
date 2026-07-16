pub fn lerp(a: f32, b: f32, t: f32) -> f32 {
    a + (b - a) * t
}

pub fn clamp(value: f32, min: f32, max: f32) -> f32 {
    value.max(min).min(max)
}

pub fn smoothstep(edge0: f32, edge1: f32, x: f32) -> f32 {
    let t = clamp((x - edge0) / (edge1 - edge0), 0.0, 1.0);
    t * t * (3.0 - 2.0 * t)
}

pub fn map_range(value: f32, from_min: f32, from_max: f32, to_min: f32, to_max: f32) -> f32 {
    to_min + (value - from_min) * (to_max - to_min) / (from_max - from_min)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn lerp_boundaries() {
        assert_eq!(lerp(0.0, 10.0, 0.0), 0.0);
        assert_eq!(lerp(0.0, 10.0, 1.0), 10.0);
    }

    #[test]
    fn lerp_midpoint() {
        assert_eq!(lerp(0.0, 10.0, 0.5), 5.0);
        assert_eq!(lerp(-10.0, 10.0, 0.5), 0.0);
    }

    #[test]
    fn lerp_extrapolation() {
        assert_eq!(lerp(0.0, 10.0, 2.0), 20.0);
        assert_eq!(lerp(0.0, 10.0, -1.0), -10.0);
    }

    #[test]
    fn clamp_within_range() {
        assert_eq!(clamp(5.0, 0.0, 10.0), 5.0);
    }

    #[test]
    fn clamp_below_min() {
        assert_eq!(clamp(-5.0, 0.0, 10.0), 0.0);
    }

    #[test]
    fn clamp_above_max() {
        assert_eq!(clamp(15.0, 0.0, 10.0), 10.0);
    }

    #[test]
    fn clamp_at_boundaries() {
        assert_eq!(clamp(0.0, 0.0, 10.0), 0.0);
        assert_eq!(clamp(10.0, 0.0, 10.0), 10.0);
    }

    #[test]
    fn smoothstep_boundaries() {
        assert_eq!(smoothstep(0.0, 1.0, 0.0), 0.0);
        assert_eq!(smoothstep(0.0, 1.0, 1.0), 1.0);
    }

    #[test]
    fn smoothstep_midpoint() {
        assert_eq!(smoothstep(0.0, 1.0, 0.5), 0.5);
    }

    #[test]
    fn smoothstep_clamped_outside() {
        assert_eq!(smoothstep(0.0, 1.0, -1.0), 0.0);
        assert_eq!(smoothstep(0.0, 1.0, 2.0), 1.0);
    }

    #[test]
    fn smoothstep_custom_edges() {
        assert_eq!(smoothstep(10.0, 20.0, 10.0), 0.0);
        assert_eq!(smoothstep(10.0, 20.0, 20.0), 1.0);
        assert_eq!(smoothstep(10.0, 20.0, 15.0), 0.5);
    }

    #[test]
    fn map_range_identity() {
        assert_eq!(map_range(5.0, 0.0, 10.0, 0.0, 10.0), 5.0);
    }

    #[test]
    fn map_range_scale() {
        assert_eq!(map_range(5.0, 0.0, 10.0, 0.0, 100.0), 50.0);
    }

    #[test]
    fn map_range_offset() {
        assert_eq!(map_range(0.0, 0.0, 10.0, 10.0, 20.0), 10.0);
        assert_eq!(map_range(10.0, 0.0, 10.0, 10.0, 20.0), 20.0);
    }

    #[test]
    fn map_range_inverted() {
        assert_eq!(map_range(0.0, 0.0, 10.0, 10.0, 0.0), 10.0);
        assert_eq!(map_range(10.0, 0.0, 10.0, 10.0, 0.0), 0.0);
    }
}
