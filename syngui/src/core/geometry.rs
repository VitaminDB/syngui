use crate::core::Point;

#[derive(Clone, Debug, Default)]
pub struct Path {
    pub commands: Vec<PathCommand>,
}

#[derive(Clone, Copy, Debug)]
pub enum PathCommand {
    MoveTo(Point),
    LineTo(Point),
    QuadTo(Point, Point),
    CubicTo(Point, Point, Point),
    Close,
}

impl Path {
    pub fn new() -> Self {
        Self { commands: Vec::new() }
    }

    pub fn move_to(mut self, p: Point) -> Self {
        self.commands.push(PathCommand::MoveTo(p));
        self
    }

    pub fn line_to(mut self, p: Point) -> Self {
        self.commands.push(PathCommand::LineTo(p));
        self
    }

    pub fn close(mut self) -> Self {
        self.commands.push(PathCommand::Close);
        self
    }
}

pub struct Bezier;

impl Bezier {
    pub fn quad(p0: Point, p1: Point, p2: Point, t: f32) -> Point {
        let t2 = t * t;
        let mt = 1.0 - t;
        let mt2 = mt * mt;
        
        Point::new(
            mt2 * p0.x + 2.0 * mt * t * p1.x + t2 * p2.x,
            mt2 * p0.y + 2.0 * mt * t * p1.y + t2 * p2.y,
        )
    }

    pub fn cubic(p0: Point, p1: Point, p2: Point, p3: Point, t: f32) -> Point {
        let t2 = t * t;
        let t3 = t2 * t;
        let mt = 1.0 - t;
        let mt2 = mt * mt;
        let mt3 = mt2 * mt;

        Point::new(
            mt3 * p0.x + 3.0 * mt2 * t * p1.x + 3.0 * mt * t2 * p2.x + t3 * p3.x,
            mt3 * p0.y + 3.0 * mt2 * t * p1.y + 3.0 * mt * t2 * p2.y + t3 * p3.y,
        )
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn path_new_is_empty() {
        let p = Path::new();
        assert!(p.commands.is_empty());
    }

    #[test]
    fn path_builder_chain() {
        let p = Path::new()
            .move_to(Point::new(0.0, 0.0))
            .line_to(Point::new(10.0, 0.0))
            .line_to(Point::new(10.0, 10.0))
            .close();
        assert_eq!(p.commands.len(), 4);
    }

    #[test]
    fn path_move_to() {
        let p = Path::new().move_to(Point::new(5.0, 10.0));
        assert_eq!(p.commands.len(), 1);
        match p.commands[0] {
            PathCommand::MoveTo(pt) => {
                assert_eq!(pt.x, 5.0);
                assert_eq!(pt.y, 10.0);
            }
            _ => panic!("expected MoveTo"),
        }
    }

    #[test]
    fn path_line_to() {
        let p = Path::new().line_to(Point::new(3.0, 4.0));
        match p.commands[0] {
            PathCommand::LineTo(pt) => {
                assert_eq!(pt.x, 3.0);
                assert_eq!(pt.y, 4.0);
            }
            _ => panic!("expected LineTo"),
        }
    }

    #[test]
    fn path_close() {
        let p = Path::new().close();
        assert!(matches!(p.commands[0], PathCommand::Close));
    }

    #[test]
    fn path_default_is_empty() {
        let p = Path::default();
        assert!(p.commands.is_empty());
    }

    #[test]
    fn path_clone() {
        let p = Path::new().move_to(Point::new(1.0, 2.0));
        let p2 = p.clone();
        assert_eq!(p2.commands.len(), 1);
    }

    #[test]
    fn bezier_quad_at_zero() {
        let p0 = Point::new(0.0, 0.0);
        let p1 = Point::new(5.0, 10.0);
        let p2 = Point::new(10.0, 0.0);
        let r = Bezier::quad(p0, p1, p2, 0.0);
        assert!((r.x - p0.x).abs() < 1e-5);
        assert!((r.y - p0.y).abs() < 1e-5);
    }

    #[test]
    fn bezier_quad_at_one() {
        let p0 = Point::new(0.0, 0.0);
        let p1 = Point::new(5.0, 10.0);
        let p2 = Point::new(10.0, 0.0);
        let r = Bezier::quad(p0, p1, p2, 1.0);
        assert!((r.x - p2.x).abs() < 1e-5);
        assert!((r.y - p2.y).abs() < 1e-5);
    }

    #[test]
    fn bezier_quad_at_half() {
        let p0 = Point::new(0.0, 0.0);
        let p1 = Point::new(5.0, 10.0);
        let p2 = Point::new(10.0, 0.0);
        let r = Bezier::quad(p0, p1, p2, 0.5);
        assert!((r.x - 5.0).abs() < 1e-5);
        assert!((r.y - 5.0).abs() < 1e-5);
    }

    #[test]
    fn bezier_quad_straight_line() {
        let p0 = Point::new(0.0, 0.0);
        let p1 = Point::new(5.0, 5.0);
        let p2 = Point::new(10.0, 10.0);
        let r = Bezier::quad(p0, p1, p2, 0.5);
        assert!((r.x - 5.0).abs() < 1e-5);
        assert!((r.y - 5.0).abs() < 1e-5);
    }

    #[test]
    fn bezier_cubic_at_zero() {
        let p0 = Point::new(0.0, 0.0);
        let p1 = Point::new(3.0, 10.0);
        let p2 = Point::new(7.0, 10.0);
        let p3 = Point::new(10.0, 0.0);
        let r = Bezier::cubic(p0, p1, p2, p3, 0.0);
        assert!((r.x - p0.x).abs() < 1e-5);
        assert!((r.y - p0.y).abs() < 1e-5);
    }

    #[test]
    fn bezier_cubic_at_one() {
        let p0 = Point::new(0.0, 0.0);
        let p1 = Point::new(3.0, 10.0);
        let p2 = Point::new(7.0, 10.0);
        let p3 = Point::new(10.0, 0.0);
        let r = Bezier::cubic(p0, p1, p2, p3, 1.0);
        assert!((r.x - p3.x).abs() < 1e-5);
        assert!((r.y - p3.y).abs() < 1e-5);
    }

    #[test]
    fn bezier_cubic_straight_line() {
        let p0 = Point::new(0.0, 0.0);
        let p1 = Point::new(10.0 / 3.0, 10.0 / 3.0);
        let p2 = Point::new(20.0 / 3.0, 20.0 / 3.0);
        let p3 = Point::new(10.0, 10.0);
        let r = Bezier::cubic(p0, p1, p2, p3, 0.5);
        assert!((r.x - 5.0).abs() < 1e-4);
        assert!((r.y - 5.0).abs() < 1e-4);
    }

    #[test]
    fn bezier_cubic_symmetry() {
        let p0 = Point::new(0.0, 0.0);
        let p1 = Point::new(0.0, 10.0);
        let p2 = Point::new(10.0, 10.0);
        let p3 = Point::new(10.0, 0.0);
        let r = Bezier::cubic(p0, p1, p2, p3, 0.5);
        assert!((r.x - 5.0).abs() < 1e-5);
    }
}
