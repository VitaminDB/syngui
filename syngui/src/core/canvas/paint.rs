use crate::core::Color;

#[derive(Clone, Copy, Debug, PartialEq, Default)]
pub enum LineCap {
    #[default]
    Butt,
    Round,
    Square,
}

#[derive(Clone, Copy, Debug, PartialEq, Default)]
pub enum LineJoin {
    #[default]
    Miter,
    Round,
    Bevel,
}

#[derive(Clone, Debug)]
pub struct Paint {
    pub color: Color,
    pub stroke_width: f32,
    pub line_cap: LineCap,
    pub line_join: LineJoin,
    pub feather: f32,
}

impl Default for Paint {
    fn default() -> Self {
        Self {
            color: Color::BLACK,
            stroke_width: 1.0,
            line_cap: LineCap::Butt,
            line_join: LineJoin::Miter,
            feather: 1.0,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn line_cap_default_is_butt() {
        assert_eq!(LineCap::default(), LineCap::Butt);
    }

    #[test]
    fn line_join_default_is_miter() {
        assert_eq!(LineJoin::default(), LineJoin::Miter);
    }

    #[test]
    fn paint_default_values() {
        let p = Paint::default();
        assert_eq!(p.color, Color::BLACK);
        assert_eq!(p.stroke_width, 1.0);
        assert_eq!(p.line_cap, LineCap::Butt);
        assert_eq!(p.line_join, LineJoin::Miter);
        assert_eq!(p.feather, 1.0);
    }

    #[test]
    fn paint_clone() {
        let mut p = Paint::default();
        p.color = Color::RED;
        p.stroke_width = 3.0;
        let p2 = p.clone();
        assert_eq!(p2.color, Color::RED);
        assert_eq!(p2.stroke_width, 3.0);
    }

    #[test]
    fn line_cap_variants() {
        let caps = [LineCap::Butt, LineCap::Round, LineCap::Square];
        assert_eq!(caps.len(), 3);
        assert_ne!(caps[0], caps[1]);
        assert_ne!(caps[1], caps[2]);
    }

    #[test]
    fn line_join_variants() {
        let joins = [LineJoin::Miter, LineJoin::Round, LineJoin::Bevel];
        assert_eq!(joins.len(), 3);
        assert_ne!(joins[0], joins[1]);
        assert_ne!(joins[1], joins[2]);
    }

    #[test]
    fn line_cap_copy() {
        let a = LineCap::Round;
        let b = a;
        assert_eq!(a, b);
    }

    #[test]
    fn line_join_copy() {
        let a = LineJoin::Bevel;
        let b = a;
        assert_eq!(a, b);
    }
}
