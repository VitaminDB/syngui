use crate::core::{Color, Point, Rect, Size};
use crate::render::DisplayList;
use super::paint::{Paint, LineCap, LineJoin};
use super::tessellator::*;

#[derive(Clone, Debug)]
pub struct RectCmd {
    pub rect: Rect,
    pub color: Color,
    pub corner_radius: [f32; 4],
}

#[derive(Clone, Debug)]
pub struct LineStripCmd {
    pub points: Vec<[f32; 2]>,
    pub color: Color,
    pub width: f32,
}

pub struct CanvasContext {
    paint: Paint,
    output: TessOutput,
    rect_commands: Vec<RectCmd>,
    line_strips: Vec<LineStripCmd>,
    origin: Point,
    size: Size,
    state_stack: Vec<Paint>,
    mss_color: Option<Color>,
    mss_background: Option<Color>,
    mss_accent: Option<Color>,
}

impl CanvasContext {
    pub fn new(origin: Point, size: Size) -> Self {
        Self {
            paint: Paint::default(),
            output: TessOutput::new(),
            rect_commands: Vec::new(),
            line_strips: Vec::new(),
            origin,
            size,
            state_stack: Vec::new(),
            mss_color: None,
            mss_background: None,
            mss_accent: None,
        }
    }

    pub fn set_mss_colors(
        &mut self,
        color: Option<Color>,
        background: Option<Color>,
        accent: Option<Color>,
    ) {
        self.mss_color = color;
        self.mss_background = background;
        self.mss_accent = accent;
    }

    pub fn mss_color(&self) -> Option<Color> { self.mss_color }

    pub fn mss_background(&self) -> Option<Color> { self.mss_background }

    pub fn mss_accent(&self) -> Option<Color> { self.mss_accent }

    pub fn width(&self) -> f32 {
        self.size.width
    }

    pub fn height(&self) -> f32 {
        self.size.height
    }

    pub fn size(&self) -> Size {
        self.size
    }

    pub fn save(&mut self) {
        self.state_stack.push(self.paint.clone());
    }

    pub fn restore(&mut self) {
        if let Some(state) = self.state_stack.pop() {
            self.paint = state;
        }
    }

    pub fn set_color(&mut self, color: Color) {
        self.paint.color = color;
    }

    pub fn set_stroke_width(&mut self, width: f32) {
        self.paint.stroke_width = width;
    }

    pub fn set_line_cap(&mut self, cap: LineCap) {
        self.paint.line_cap = cap;
    }

    pub fn set_line_join(&mut self, join: LineJoin) {
        self.paint.line_join = join;
    }

    pub fn set_anti_alias(&mut self, feather: f32) {
        self.paint.feather = feather;
    }

    pub fn draw_line(&mut self, x1: f32, y1: f32, x2: f32, y2: f32) {
        self.line_strips.push(LineStripCmd {
            points: vec![
                self.to_screen_arr(x1, y1),
                self.to_screen_arr(x2, y2),
            ],
            color: self.paint.color,
            width: self.paint.stroke_width,
        });
    }

    pub fn draw_polyline(&mut self, points: &[(f32, f32)]) {
        if points.len() < 2 {
            return;
        }
        self.line_strips.push(LineStripCmd {
            points: points.iter().map(|&(x, y)| self.to_screen_arr(x, y)).collect(),
            color: self.paint.color,
            width: self.paint.stroke_width,
        });
    }

    pub fn draw_rect(&mut self, x: f32, y: f32, w: f32, h: f32) {
        self.line_strips.push(LineStripCmd {
            points: vec![
                self.to_screen_arr(x, y),
                self.to_screen_arr(x + w, y),
                self.to_screen_arr(x + w, y + h),
                self.to_screen_arr(x, y + h),
                self.to_screen_arr(x, y),
            ],
            color: self.paint.color,
            width: self.paint.stroke_width,
        });
    }

    pub fn stroke_circle(&mut self, cx: f32, cy: f32, r: f32) {
        let segments = circle_segment_count(r);
        let center = Point::new(cx, cy);
        let pts = circle_points(center, r, segments);
        let mut screen_pts: Vec<[f32; 2]> = pts
            .iter()
            .map(|p| self.to_screen_arr(p.x, p.y))
            .collect();
        if let Some(&first) = screen_pts.first() {
            screen_pts.push(first);
        }
        self.line_strips.push(LineStripCmd {
            points: screen_pts,
            color: self.paint.color,
            width: self.paint.stroke_width,
        });
    }

    pub fn draw_arc(&mut self, cx: f32, cy: f32, r: f32, start_angle: f32, end_angle: f32) {
        let sweep = (end_angle - start_angle).abs();
        let segments = ((circle_segment_count(r) as f32) * sweep / std::f32::consts::TAU) as usize;
        let segments = segments.max(4);
        let center = Point::new(cx, cy);
        let pts = arc_points(center, r, start_angle, end_angle, segments);
        let screen_pts: Vec<[f32; 2]> = pts
            .iter()
            .map(|p| self.to_screen_arr(p.x, p.y))
            .collect();
        self.line_strips.push(LineStripCmd {
            points: screen_pts,
            color: self.paint.color,
            width: self.paint.stroke_width,
        });
    }

    pub fn draw_quad_bezier(
        &mut self,
        x0: f32, y0: f32,
        cpx: f32, cpy: f32,
        x1: f32, y1: f32,
    ) {
        let p0 = Point::new(x0, y0);
        let cp = Point::new(cpx, cpy);
        let p1 = Point::new(x1, y1);
        let pts = flatten_quad_bezier(p0, cp, p1, 0.5);
        let screen_pts: Vec<[f32; 2]> = pts
            .iter()
            .map(|p| self.to_screen_arr(p.x, p.y))
            .collect();
        self.line_strips.push(LineStripCmd {
            points: screen_pts,
            color: self.paint.color,
            width: self.paint.stroke_width,
        });
    }

    pub fn draw_cubic_bezier(
        &mut self,
        x0: f32, y0: f32,
        cp1x: f32, cp1y: f32,
        cp2x: f32, cp2y: f32,
        x1: f32, y1: f32,
    ) {
        let p0 = Point::new(x0, y0);
        let cp1 = Point::new(cp1x, cp1y);
        let cp2 = Point::new(cp2x, cp2y);
        let p1 = Point::new(x1, y1);
        let pts = flatten_cubic_bezier(p0, cp1, cp2, p1, 0.5);
        let screen_pts: Vec<[f32; 2]> = pts
            .iter()
            .map(|p| self.to_screen_arr(p.x, p.y))
            .collect();
        self.line_strips.push(LineStripCmd {
            points: screen_pts,
            color: self.paint.color,
            width: self.paint.stroke_width,
        });
    }

    pub fn fill_rect(&mut self, x: f32, y: f32, w: f32, h: f32) {
        self.rect_commands.push(RectCmd {
            rect: Rect::new(
                Point::new(self.origin.x + x, self.origin.y + y),
                Size::new(w, h),
            ),
            color: self.paint.color,
            corner_radius: [0.0; 4],
        });
    }

    pub fn fill_rounded_rect(&mut self, x: f32, y: f32, w: f32, h: f32, radius: f32) {
        self.rect_commands.push(RectCmd {
            rect: Rect::new(
                Point::new(self.origin.x + x, self.origin.y + y),
                Size::new(w, h),
            ),
            color: self.paint.color,
            corner_radius: [radius; 4],
        });
    }

    pub fn fill_circle(&mut self, cx: f32, cy: f32, r: f32) {
        self.rect_commands.push(RectCmd {
            rect: Rect::new(
                Point::new(self.origin.x + cx - r, self.origin.y + cy - r),
                Size::new(r * 2.0, r * 2.0),
            ),
            color: self.paint.color,
            corner_radius: [r; 4],
        });
    }

    pub fn fill_polygon(&mut self, points: &[(f32, f32)]) {
        if points.len() < 3 {
            return;
        }
        let pts: Vec<Point> = points.iter().map(|&(x, y)| self.to_screen(x, y)).collect();
        if self.paint.feather > 0.0 {
            tessellate_fill_polygon_aa(&pts, self.paint.color, self.paint.feather, &mut self.output);
        } else {
            tessellate_fill_polygon(&pts, self.paint.color, &mut self.output);
        }
    }

    /// Заливка простого полигона любой формы (в т.ч. невыпуклого) через
    /// ear-clipping — для контуров зданий Г-/П-образной формы.
    pub fn fill_polygon_concave(&mut self, points: &[(f32, f32)]) {
        if points.len() < 3 {
            return;
        }
        let pts: Vec<Point> = points.iter().map(|&(x, y)| self.to_screen(x, y)).collect();
        tessellate_fill_polygon_concave(&pts, self.paint.color, &mut self.output);
    }

    pub fn fill_area_strip(&mut self, curve_points: &[(f32, f32)], baseline_y: f32) {
        if curve_points.len() < 2 {
            return;
        }
        let screen_pts: Vec<(f32, f32)> = curve_points
            .iter()
            .map(|&(x, y)| {
                let p = self.to_screen(x, y);
                (p.x, p.y)
            })
            .collect();
        let screen_baseline = self.origin.y + baseline_y;
        tessellate_area_strip(&screen_pts, screen_baseline, self.paint.color, &mut self.output);
    }

    pub fn clear(&mut self, color: Color) {
        self.rect_commands.push(RectCmd {
            rect: Rect::new(self.origin, self.size),
            color,
            corner_radius: [0.0; 4],
        });
    }

    pub fn tessellated(&self) -> &TessOutput {
        &self.output
    }

    pub fn tessellated_mut(&mut self) -> &mut TessOutput {
        &mut self.output
    }

    pub fn into_tessellated(self) -> TessOutput {
        self.output
    }

    pub fn rect_commands(&self) -> &[RectCmd] {
        &self.rect_commands
    }

    pub fn line_strips(&self) -> &[LineStripCmd] {
        &self.line_strips
    }

    pub fn flush(self, list: &mut DisplayList) {
        for cmd in &self.rect_commands {
            list.push_rect(cmd.rect, cmd.color, cmd.corner_radius);
        }
        for cmd in self.line_strips {
            list.push_line_strip(cmd.points, cmd.color, cmd.width);
        }
        if !self.output.is_empty() {
            list.push_canvas(self.output.vertices, self.output.indices);
        }
    }

    fn to_screen(&self, x: f32, y: f32) -> Point {
        Point::new(self.origin.x + x, self.origin.y + y)
    }

    fn to_screen_arr(&self, x: f32, y: f32) -> [f32; 2] {
        [self.origin.x + x, self.origin.y + y]
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn make_ctx() -> CanvasContext {
        CanvasContext::new(Point::new(100.0, 200.0), Size::new(400.0, 300.0))
    }

    #[test]
    fn canvas_dimensions() {
        let ctx = make_ctx();
        assert_eq!(ctx.width(), 400.0);
        assert_eq!(ctx.height(), 300.0);
        assert_eq!(ctx.size(), Size::new(400.0, 300.0));
    }

    #[test]
    fn canvas_starts_empty() {
        let ctx = make_ctx();
        assert!(ctx.tessellated().is_empty());
        assert!(ctx.rect_commands().is_empty());
        assert!(ctx.line_strips().is_empty());
    }

    #[test]
    fn save_restore_color() {
        let mut ctx = make_ctx();
        ctx.set_color(Color::RED);
        ctx.save();
        ctx.set_color(Color::BLUE);
        ctx.restore();
        ctx.fill_rect(0.0, 0.0, 10.0, 10.0);
        assert_eq!(ctx.rect_commands()[0].color, Color::RED);
    }

    #[test]
    fn restore_without_save_is_noop() {
        let mut ctx = make_ctx();
        ctx.set_color(Color::GREEN);
        ctx.restore();
        ctx.fill_rect(0.0, 0.0, 10.0, 10.0);
        assert_eq!(ctx.rect_commands()[0].color, Color::GREEN);
    }

    #[test]
    fn nested_save_restore() {
        let mut ctx = make_ctx();
        ctx.set_color(Color::RED);
        ctx.save();
        ctx.set_color(Color::GREEN);
        ctx.save();
        ctx.set_color(Color::BLUE);
        ctx.restore();
        ctx.fill_rect(0.0, 0.0, 1.0, 1.0);
        assert_eq!(ctx.rect_commands()[0].color, Color::GREEN);
        ctx.restore();
        ctx.fill_rect(0.0, 0.0, 1.0, 1.0);
        assert_eq!(ctx.rect_commands()[1].color, Color::RED);
    }

    #[test]
    fn set_stroke_width() {
        let mut ctx = make_ctx();
        ctx.set_stroke_width(5.0);
        ctx.draw_line(0.0, 0.0, 10.0, 0.0);
        assert!(!ctx.line_strips().is_empty());
    }

    #[test]
    fn set_line_cap() {
        let mut ctx = make_ctx();
        ctx.set_line_cap(LineCap::Round);
        ctx.draw_line(0.0, 0.0, 10.0, 0.0);
    }

    #[test]
    fn set_line_join() {
        let mut ctx = make_ctx();
        ctx.set_line_join(LineJoin::Bevel);
        ctx.draw_rect(0.0, 0.0, 50.0, 50.0);
    }

    #[test]
    fn set_anti_alias() {
        let mut ctx = make_ctx();
        ctx.set_anti_alias(0.0);
        ctx.draw_line(0.0, 0.0, 10.0, 0.0);
        assert!(!ctx.line_strips().is_empty());
    }

    #[test]
    fn draw_line_produces_line_strip() {
        let mut ctx = make_ctx();
        ctx.draw_line(0.0, 0.0, 50.0, 50.0);
        assert_eq!(ctx.line_strips().len(), 1);
        assert_eq!(ctx.line_strips()[0].points.len(), 2);
    }

    #[test]
    fn draw_polyline_less_than_2_points_noop() {
        let mut ctx = make_ctx();
        ctx.draw_polyline(&[(0.0, 0.0)]);
        assert!(ctx.line_strips().is_empty());
    }

    #[test]
    fn draw_polyline_produces_line_strip() {
        let mut ctx = make_ctx();
        ctx.draw_polyline(&[(0.0, 0.0), (10.0, 0.0), (10.0, 10.0)]);
        assert_eq!(ctx.line_strips().len(), 1);
        assert_eq!(ctx.line_strips()[0].points.len(), 3);
    }

    #[test]
    fn draw_rect_produces_closed_line_strip() {
        let mut ctx = make_ctx();
        ctx.draw_rect(0.0, 0.0, 100.0, 50.0);
        assert_eq!(ctx.line_strips().len(), 1);
        assert_eq!(ctx.line_strips()[0].points.len(), 5);
    }

    #[test]
    fn stroke_circle_produces_line_strip() {
        let mut ctx = make_ctx();
        ctx.stroke_circle(50.0, 50.0, 25.0);
        assert_eq!(ctx.line_strips().len(), 1);
        assert!(ctx.line_strips()[0].points.len() > 16);
    }

    #[test]
    fn draw_arc_produces_line_strip() {
        let mut ctx = make_ctx();
        ctx.draw_arc(50.0, 50.0, 25.0, 0.0, std::f32::consts::PI);
        assert_eq!(ctx.line_strips().len(), 1);
    }

    #[test]
    fn draw_quad_bezier_produces_line_strip() {
        let mut ctx = make_ctx();
        ctx.draw_quad_bezier(0.0, 0.0, 50.0, 100.0, 100.0, 0.0);
        assert_eq!(ctx.line_strips().len(), 1);
    }

    #[test]
    fn draw_cubic_bezier_produces_line_strip() {
        let mut ctx = make_ctx();
        ctx.draw_cubic_bezier(0.0, 0.0, 30.0, 100.0, 70.0, 100.0, 100.0, 0.0);
        assert_eq!(ctx.line_strips().len(), 1);
    }

    #[test]
    fn fill_rect_adds_rect_cmd() {
        let mut ctx = make_ctx();
        ctx.set_color(Color::RED);
        ctx.fill_rect(10.0, 20.0, 30.0, 40.0);
        assert_eq!(ctx.rect_commands().len(), 1);
        let cmd = &ctx.rect_commands()[0];
        assert_eq!(cmd.color, Color::RED);
        assert_eq!(cmd.rect.origin.x, 110.0);
        assert_eq!(cmd.rect.origin.y, 220.0);
    }

    #[test]
    fn fill_polygon_produces_geometry() {
        let mut ctx = make_ctx();
        ctx.set_anti_alias(0.0);
        ctx.fill_polygon(&[(0.0, 0.0), (50.0, 0.0), (25.0, 50.0)]);
        let tess = ctx.tessellated();
        assert_eq!(tess.vertices.len(), 3);
        assert_eq!(tess.indices.len(), 3);
    }

    #[test]
    fn clear_fills_entire_canvas() {
        let mut ctx = make_ctx();
        ctx.clear(Color::WHITE);
        let cmd = &ctx.rect_commands()[0];
        assert_eq!(cmd.color, Color::WHITE);
    }

    #[test]
    fn to_screen_offset() {
        let ctx = CanvasContext::new(Point::new(50.0, 100.0), Size::new(200.0, 200.0));
        let screen = ctx.to_screen(10.0, 20.0);
        assert_eq!(screen.x, 60.0);
        assert_eq!(screen.y, 120.0);
    }
}
