#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum UISpace {}

pub type Point = euclid::Point2D<f32, UISpace>;
pub type Size = euclid::Size2D<f32, UISpace>;
pub type Rect = euclid::Rect<f32, UISpace>;
pub type Vector = euclid::Vector2D<f32, UISpace>;
pub type Transform = euclid::Transform2D<f32, UISpace, UISpace>;

#[derive(Clone, Copy, Debug, PartialEq, Default)]
pub struct EdgeInsets {
    pub left: f32,
    pub top: f32,
    pub right: f32,
    pub bottom: f32,
}

impl EdgeInsets {
    pub const fn zero() -> Self {
        Self { left: 0.0, top: 0.0, right: 0.0, bottom: 0.0 }
    }

    pub const fn new(left: f32, top: f32, right: f32, bottom: f32) -> Self {
        Self { left, top, right, bottom }
    }

    pub const fn all(value: f32) -> Self {
        Self::new(value, value, value, value)
    }

    pub const fn symmetric(horizontal: f32, vertical: f32) -> Self {
        Self::new(horizontal, vertical, horizontal, vertical)
    }

    pub fn horizontal(&self) -> f32 {
        self.left + self.right
    }

    pub fn vertical(&self) -> f32 {
        self.top + self.bottom
    }
}

pub trait RectExt {
    fn x(&self) -> f32;
    fn y(&self) -> f32;
    fn right(&self) -> f32;
    fn bottom(&self) -> f32;
    fn center(&self) -> Point;
    fn zero() -> Self;
    fn from_size(width: f32, height: f32) -> Self;
}

impl RectExt for Rect {
    fn x(&self) -> f32 { self.origin.x }
    fn y(&self) -> f32 { self.origin.y }
    fn right(&self) -> f32 { self.origin.x + self.size.width }
    fn bottom(&self) -> f32 { self.origin.y + self.size.height }
    fn center(&self) -> Point {
        Point::new(
            self.origin.x + self.size.width / 2.0,
            self.origin.y + self.size.height / 2.0,
        )
    }
    fn zero() -> Self { Self::new(Point::zero(), Size::zero()) }
    fn from_size(width: f32, height: f32) -> Self {
        Self::new(Point::zero(), Size::new(width, height))
    }
}

#[cfg(test)]
mod tests {
    use crate::core::*;

    #[test]
    fn srgb_to_linear_zero() {
        assert_eq!(srgb_to_linear(0.0), 0.0);
    }

    #[test]
    fn srgb_to_linear_one() {
        assert!((srgb_to_linear(1.0) - 1.0).abs() < 1e-5);
    }

    #[test]
    fn srgb_to_linear_low_values() {
        let v = 0.04045;
        assert!((srgb_to_linear(v) - v / 12.92).abs() < 1e-6);
    }

    #[test]
    fn srgb_to_linear_mid_value() {
        let result = srgb_to_linear(0.5);
        assert!(result > 0.2 && result < 0.23);
    }

    #[test]
    fn color_new() {
        let c = Color::new(0.1, 0.2, 0.3, 0.4);
        assert_eq!(c.r, 0.1);
        assert_eq!(c.g, 0.2);
        assert_eq!(c.b, 0.3);
        assert_eq!(c.a, 0.4);
    }

    #[test]
    fn color_rgb_alpha_is_one() {
        let c = Color::rgb(0.5, 0.5, 0.5);
        assert_eq!(c.a, 1.0);
    }

    #[test]
    fn color_rgba() {
        let c = Color::rgba(0.1, 0.2, 0.3, 0.4);
        assert_eq!(c, Color::new(0.1, 0.2, 0.3, 0.4));
    }

    #[test]
    fn color_default_is_transparent_black() {
        let c = Color::default();
        assert_eq!(c, Color::new(0.0, 0.0, 0.0, 0.0));
    }

    #[test]
    fn color_from_srgb() {
        let c = Color::from_srgb(255, 255, 255, 1.0);
        assert!((c.r - 1.0).abs() < 1e-4);
        assert!((c.g - 1.0).abs() < 1e-4);
        assert!((c.b - 1.0).abs() < 1e-4);
        assert_eq!(c.a, 1.0);
    }

    #[test]
    fn color_from_srgb_black() {
        let c = Color::from_srgb(0, 0, 0, 1.0);
        assert_eq!(c.r, 0.0);
        assert_eq!(c.g, 0.0);
        assert_eq!(c.b, 0.0);
    }

    #[test]
    fn color_from_hex_6_digit() {
        let c = Color::from_hex("#FFFFFF");
        assert!((c.r - 1.0).abs() < 1e-4);
        assert!((c.g - 1.0).abs() < 1e-4);
        assert!((c.b - 1.0).abs() < 1e-4);
        assert_eq!(c.a, 1.0);
    }

    #[test]
    fn color_from_hex_8_digit() {
        let c = Color::from_hex("#FF000080");
        assert!((c.r - 1.0).abs() < 1e-4);
        assert!(c.g < 0.01);
        assert!(c.b < 0.01);
        assert!((c.a - 128.0 / 255.0).abs() < 0.01);
    }

    #[test]
    fn color_from_hex_without_hash() {
        let c = Color::from_hex("000000");
        assert_eq!(c.r, 0.0);
        assert_eq!(c.g, 0.0);
        assert_eq!(c.b, 0.0);
    }

    #[test]
    fn color_constants() {
        assert_eq!(Color::WHITE, Color::rgb(1.0, 1.0, 1.0));
        assert_eq!(Color::BLACK, Color::rgb(0.0, 0.0, 0.0));
        assert_eq!(Color::RED, Color::rgb(1.0, 0.0, 0.0));
        assert_eq!(Color::GREEN, Color::rgb(0.0, 1.0, 0.0));
        assert_eq!(Color::BLUE, Color::rgb(0.0, 0.0, 1.0));
        assert_eq!(Color::TRANSPARENT, Color::rgba(0.0, 0.0, 0.0, 0.0));
    }

    #[test]
    fn color_to_array() {
        let c = Color::new(0.1, 0.2, 0.3, 0.4);
        assert_eq!(c.to_array(), [0.1, 0.2, 0.3, 0.4]);
    }

    #[test]
    fn color_to_premultiplied_array() {
        let c = Color::new(1.0, 0.5, 0.0, 0.5);
        let pm = c.to_premultiplied_array();
        assert_eq!(pm, [0.5, 0.25, 0.0, 0.5]);
    }

    #[test]
    fn color_with_alpha() {
        let c = Color::rgb(1.0, 0.0, 0.0);
        let c2 = c.with_alpha(0.5);
        assert_eq!(c2.r, 1.0);
        assert_eq!(c2.a, 0.5);
    }

    #[test]
    fn color_multiply_alpha() {
        let c = Color::rgba(1.0, 1.0, 1.0, 0.8);
        let c2 = c.multiply_alpha(0.5);
        assert!((c2.a - 0.4).abs() < 1e-6);
        assert_eq!(c2.r, 1.0);
    }

    #[test]
    fn color_darken() {
        let c = Color::rgb(1.0, 1.0, 1.0);
        let d = c.darken(0.5);
        assert!((d.r - 0.5).abs() < 1e-6);
        assert_eq!(d.a, 1.0);
    }

    #[test]
    fn color_darken_zero_unchanged() {
        let c = Color::rgb(0.5, 0.5, 0.5);
        let d = c.darken(0.0);
        assert_eq!(d, c);
    }

    #[test]
    fn color_lighten() {
        let c = Color::rgb(0.0, 0.0, 0.0);
        let l = c.lighten(1.0);
        assert!((l.r - 1.0).abs() < 1e-6);
        assert!((l.g - 1.0).abs() < 1e-6);
    }

    #[test]
    fn color_lighten_zero_unchanged() {
        let c = Color::rgb(0.5, 0.5, 0.5);
        let l = c.lighten(0.0);
        assert_eq!(l, c);
    }

    #[test]
    fn color_lerp_endpoints() {
        let a = Color::RED;
        let b = Color::BLUE;
        assert_eq!(a.lerp(&b, 0.0), a);
        assert_eq!(a.lerp(&b, 1.0), b);
    }

    #[test]
    fn color_lerp_midpoint() {
        let a = Color::new(0.0, 0.0, 0.0, 1.0);
        let b = Color::new(1.0, 1.0, 1.0, 1.0);
        let m = a.lerp(&b, 0.5);
        assert!((m.r - 0.5).abs() < 1e-6);
        assert!((m.g - 0.5).abs() < 1e-6);
        assert!((m.b - 0.5).abs() < 1e-6);
    }

    #[test]
    fn color_lerp_clamps_t() {
        let a = Color::BLACK;
        let b = Color::WHITE;
        assert_eq!(a.lerp(&b, -1.0), a);
        assert_eq!(a.lerp(&b, 2.0), b);
    }

    #[test]
    fn edge_insets_zero() {
        let e = EdgeInsets::zero();
        assert_eq!(e.left, 0.0);
        assert_eq!(e.top, 0.0);
        assert_eq!(e.right, 0.0);
        assert_eq!(e.bottom, 0.0);
    }

    #[test]
    fn edge_insets_all() {
        let e = EdgeInsets::all(10.0);
        assert_eq!(e.left, 10.0);
        assert_eq!(e.top, 10.0);
        assert_eq!(e.right, 10.0);
        assert_eq!(e.bottom, 10.0);
    }

    #[test]
    fn edge_insets_symmetric() {
        let e = EdgeInsets::symmetric(5.0, 10.0);
        assert_eq!(e.left, 5.0);
        assert_eq!(e.right, 5.0);
        assert_eq!(e.top, 10.0);
        assert_eq!(e.bottom, 10.0);
    }

    #[test]
    fn edge_insets_horizontal_vertical() {
        let e = EdgeInsets::new(1.0, 2.0, 3.0, 4.0);
        assert_eq!(e.horizontal(), 4.0);
        assert_eq!(e.vertical(), 6.0);
    }

    #[test]
    fn edge_insets_default_is_zero() {
        assert_eq!(EdgeInsets::default(), EdgeInsets::zero());
    }

    #[test]
    fn rect_ext_accessors() {
        let r = Rect::new(Point::new(10.0, 20.0), Size::new(100.0, 50.0));
        assert_eq!(r.x(), 10.0);
        assert_eq!(r.y(), 20.0);
        assert_eq!(r.right(), 110.0);
        assert_eq!(r.bottom(), 70.0);
    }

    #[test]
    fn rect_ext_center() {
        let r = Rect::new(Point::new(0.0, 0.0), Size::new(100.0, 200.0));
        let c = r.center();
        assert_eq!(c.x, 50.0);
        assert_eq!(c.y, 100.0);
    }

    #[test]
    fn rect_ext_zero() {
        let r = Rect::zero();
        assert_eq!(r.origin.x, 0.0);
        assert_eq!(r.origin.y, 0.0);
        assert_eq!(r.size.width, 0.0);
        assert_eq!(r.size.height, 0.0);
    }

    #[test]
    fn rect_ext_from_size() {
        let r = <Rect as RectExt>::from_size(100.0, 50.0);
        assert_eq!(r.origin, Point::zero());
        assert_eq!(r.size.width, 100.0);
        assert_eq!(r.size.height, 50.0);
    }

    #[test]
    fn shadow_new() {
        let s = Shadow::new(Color::BLACK, 1.0, 2.0, 3.0);
        assert_eq!(s.offset_x, 1.0);
        assert_eq!(s.offset_y, 2.0);
        assert_eq!(s.blur_radius, 3.0);
        assert_eq!(s.spread, 0.0);
    }

    #[test]
    fn shadow_with_spread() {
        let s = Shadow::new(Color::BLACK, 0.0, 0.0, 5.0).with_spread(3.0);
        assert_eq!(s.spread, 3.0);
    }

    #[test]
    fn shadow_default() {
        let s = Shadow::default();
        assert_eq!(s.offset_x, 0.0);
        assert_eq!(s.offset_y, 0.0);
        assert_eq!(s.blur_radius, 0.0);
        assert_eq!(s.spread, 0.0);
    }

    #[test]
    fn shadow_parse_rgba_first() {
        let s = Shadow::parse("rgba(0, 0, 0, 0.3) 0px 19px 38px").unwrap();
        assert_eq!(s.offset_x, 0.0);
        assert_eq!(s.offset_y, 19.0);
        assert_eq!(s.blur_radius, 38.0);
        assert!(s.color.a > 0.29 && s.color.a < 0.31);
    }

    #[test]
    fn shadow_parse_hex_color() {
        let s = Shadow::parse("#000000 2px 4px 6px").unwrap();
        assert_eq!(s.offset_x, 2.0);
        assert_eq!(s.offset_y, 4.0);
        assert_eq!(s.blur_radius, 6.0);
    }

    #[test]
    fn shadow_parse_with_spread() {
        let s = Shadow::parse("rgba(0,0,0,1) 1px 2px 3px 4px").unwrap();
        assert_eq!(s.spread, 4.0);
    }

    #[test]
    fn shadow_parse_no_px_suffix() {
        let s = Shadow::parse("rgba(0,0,0,1) 1 2 3").unwrap();
        assert_eq!(s.offset_x, 1.0);
        assert_eq!(s.offset_y, 2.0);
        assert_eq!(s.blur_radius, 3.0);
    }

    #[test]
    fn shadow_parse_too_few_tokens() {
        assert!(Shadow::parse("rgba(0,0,0,1) 1px").is_none());
    }

    #[test]
    fn shadow_parse_no_color() {
        assert!(Shadow::parse("1px 2px 3px").is_none());
    }

    #[test]
    fn shadow_parse_rgb_color() {
        let s = Shadow::parse("rgb(255, 0, 0) 0 5 10").unwrap();
        assert!((s.color.r - 1.0).abs() < 1e-3);
        assert!(s.color.g < 0.01);
    }

    #[test]
    fn shadows_new_is_empty() {
        let s = Shadows::new();
        assert!(s.is_empty());
        assert_eq!(s.as_slice().len(), 0);
    }

    #[test]
    fn shadows_push() {
        let mut s = Shadows::new();
        s.push(Shadow::default());
        assert!(!s.is_empty());
        assert_eq!(s.as_slice().len(), 1);
    }

    #[test]
    fn shadows_parse_single() {
        let s = Shadows::parse("rgba(0,0,0,0.5) 0px 4px 8px").unwrap();
        assert_eq!(s.as_slice().len(), 1);
    }

    #[test]
    fn shadows_parse_multiple() {
        let s = Shadows::parse(
            "rgba(0,0,0,0.3) 0px 19px 38px, rgba(0,0,0,0.22) 0px 15px 12px"
        ).unwrap();
        assert_eq!(s.as_slice().len(), 2);
        assert_eq!(s.as_slice()[0].offset_y, 19.0);
        assert_eq!(s.as_slice()[1].offset_y, 15.0);
    }

    #[test]
    fn shadows_parse_invalid() {
        assert!(Shadows::parse("nonsense").is_none());
    }

    #[test]
    fn shadows_into_iter() {
        let mut s = Shadows::new();
        s.push(Shadow::new(Color::RED, 1.0, 2.0, 3.0));
        s.push(Shadow::new(Color::BLUE, 4.0, 5.0, 6.0));
        let collected: Vec<Shadow> = s.into_iter().collect();
        assert_eq!(collected.len(), 2);
    }

    #[test]
    fn shadows_iter_ref() {
        let mut s = Shadows::new();
        s.push(Shadow::default());
        let count = (&s).into_iter().count();
        assert_eq!(count, 1);
    }

    #[test]
    fn gradient_resolve_stops_two() {
        let stops = vec![
            ColorStop::auto(Color::RED),
            ColorStop::auto(Color::BLUE),
        ];
        let resolved = Gradient::resolve_stops(&stops);
        assert_eq!(resolved.len(), 2);
        assert_eq!(resolved[0].1, 0.0);
        assert_eq!(resolved[1].1, 1.0);
    }

    #[test]
    fn gradient_resolve_stops_three_auto() {
        let stops = vec![
            ColorStop::auto(Color::RED),
            ColorStop::auto(Color::GREEN),
            ColorStop::auto(Color::BLUE),
        ];
        let resolved = Gradient::resolve_stops(&stops);
        assert_eq!(resolved.len(), 3);
        assert_eq!(resolved[0].1, 0.0);
        assert!((resolved[1].1 - 0.5).abs() < 1e-6);
        assert_eq!(resolved[2].1, 1.0);
    }

    #[test]
    fn gradient_resolve_stops_explicit() {
        let stops = vec![
            ColorStop::new(Color::RED, 0.0),
            ColorStop::new(Color::GREEN, 0.3),
            ColorStop::new(Color::BLUE, 1.0),
        ];
        let resolved = Gradient::resolve_stops(&stops);
        assert_eq!(resolved[1].1, 0.3);
    }

    #[test]
    fn gradient_sample_endpoints() {
        let g = Gradient::Linear {
            angle_deg: 90.0,
            stops: vec![
                ColorStop::new(Color::RED, 0.0),
                ColorStop::new(Color::BLUE, 1.0),
            ],
        };
        let c0 = g.sample(0.0);
        assert_eq!(c0, Color::RED);
        let c1 = g.sample(1.0);
        assert_eq!(c1, Color::BLUE);
    }

    #[test]
    fn gradient_sample_midpoint() {
        let g = Gradient::Linear {
            angle_deg: 90.0,
            stops: vec![
                ColorStop::new(Color::new(0.0, 0.0, 0.0, 1.0), 0.0),
                ColorStop::new(Color::new(1.0, 1.0, 1.0, 1.0), 1.0),
            ],
        };
        let mid = g.sample(0.5);
        assert!((mid.r - 0.5).abs() < 1e-6);
        assert!((mid.g - 0.5).abs() < 1e-6);
    }

    #[test]
    fn gradient_rasterize() {
        let g = Gradient::Linear {
            angle_deg: 90.0,
            stops: vec![
                ColorStop::new(Color::BLACK, 0.0),
                ColorStop::new(Color::WHITE, 1.0),
            ],
        };
        let data = g.rasterize(256);
        assert_eq!(data.len(), 256 * 4);
        assert!(data[0] < 10);
        assert!(data[255 * 4] > 245);
    }

    #[test]
    fn linear_to_srgb_roundtrip() {
        let srgb_val = 0.5f32;
        let linear = srgb_to_linear(srgb_val);
        let back = linear_to_srgb(linear);
        assert!((back - srgb_val).abs() < 1e-4);
    }
}
