//! Область обрезки (`overflow: hidden`) и её перевод в ножницы GPU.

/// Прямоугольник обрезки в логических пикселях.
///
/// Координаты хранятся точно (биты `f32`), а не округляются до целых: клип
/// приходит из раскладки, а раскладка при дробном DPI или `ui_scale` даёт
/// дробные границы. Округление тут складывалось с округлением при переводе в
/// физические пиксели и расширяло область наружу — по краю оставался пиксель,
/// который сами квады содержимого уже не закрашивают.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, Default)]
pub struct ClipRect {
    x_bits: u32,
    y_bits: u32,
    width_bits: u32,
    height_bits: u32,
    pub enabled: bool,
    corner_radius_bits: [u32; 4],
}

impl ClipRect {
    pub fn full_screen() -> Self {
        Self {
            x_bits: 0f32.to_bits(),
            y_bits: 0f32.to_bits(),
            width_bits: f32::INFINITY.to_bits(),
            height_bits: f32::INFINITY.to_bits(),
            enabled: false,
            corner_radius_bits: [0; 4],
        }
    }

    pub fn new(x: f32, y: f32, width: f32, height: f32) -> Self {
        Self {
            x_bits: x.to_bits(),
            y_bits: y.to_bits(),
            width_bits: width.to_bits(),
            height_bits: height.to_bits(),
            enabled: true,
            corner_radius_bits: [0; 4],
        }
    }

    pub fn from_rect(rect: crate::core::Rect) -> Self {
        Self::new(
            rect.origin.x,
            rect.origin.y,
            rect.size.width,
            rect.size.height,
        )
    }

    pub fn from_rect_rounded(rect: crate::core::Rect, corner_radius: [f32; 4]) -> Self {
        let mut clip = Self::from_rect(rect);
        clip.set_corner_radius(corner_radius);
        clip
    }

    #[inline]
    pub fn x(&self) -> f32 {
        f32::from_bits(self.x_bits)
    }

    #[inline]
    pub fn y(&self) -> f32 {
        f32::from_bits(self.y_bits)
    }

    #[inline]
    pub fn width(&self) -> f32 {
        f32::from_bits(self.width_bits)
    }

    #[inline]
    pub fn height(&self) -> f32 {
        f32::from_bits(self.height_bits)
    }

    /// Ножницы для этого клипа; `None` — область пуста, батч рисовать не нужно.
    pub fn scissor(&self, scale: f32, phys_w: u32, phys_h: u32) -> Option<(u32, u32, u32, u32)> {
        scissor_px(
            [self.x(), self.y(), self.width(), self.height()],
            scale,
            phys_w,
            phys_h,
        )
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

        let x1 = self.x().max(other.origin.x);
        let y1 = self.y().max(other.origin.y);
        let x2 = (self.x() + self.width()).min(other.origin.x + other.size.width);
        let y2 = (self.y() + self.height()).min(other.origin.y + other.size.height);

        if x2 <= x1 || y2 <= y1 {
            return Self::new(x1, y1, 0.0, 0.0);
        }

        let mut result = Self::new(x1, y1, x2 - x1, y2 - y1);
        result.corner_radius_bits = child_corner_bits_from_parent(
            self.x(),
            self.y(),
            self.width(),
            self.height(),
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

/// Перевод логического прямоугольника в ножницы GPU (физические пиксели).
///
/// Пиксель попадает внутрь, если внутри прямоугольника лежит его центр, — то
/// же правило, по которому растеризатор закрашивает квады содержимого.
/// Округление наружу открывало краевой пиксель, который содержимое покрывает
/// лишь на доли процента: на границе `overflow: hidden` оставалась полоска в
/// 1 px из того, что должно быть обрезано. Прямоугольник тоньше половины
/// пикселя сохраняет один пиксель, иначе содержимое исчезло бы совсем.
pub fn scissor_px(
    rect: [f32; 4],
    scale: f32,
    phys_w: u32,
    phys_h: u32,
) -> Option<(u32, u32, u32, u32)> {
    let [x, y, w, h] = rect;
    if !(w > 0.0) || !(h > 0.0) {
        return None;
    }
    let to_px = |v: f32, max: u32| (v * scale).round().clamp(0.0, max as f32) as u32;
    let sx = to_px(x, phys_w);
    let sy = to_px(y, phys_h);
    let mut sr = to_px(x + w, phys_w);
    let mut sb = to_px(y + h, phys_h);
    if sr == sx && sx < phys_w {
        sr = sx + 1;
    }
    if sb == sy && sy < phys_h {
        sb = sy + 1;
    }
    if sr <= sx || sb <= sy {
        return None;
    }
    Some((sx, sy, sr - sx, sb - sy))
}

fn child_corner_bits_from_parent(
    parent_x: f32,
    parent_y: f32,
    parent_w: f32,
    parent_h: f32,
    parent_radius_bits: [u32; 4],
    child_x1: f32,
    child_y1: f32,
    child_x2: f32,
    child_y2: f32,
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
        let parent = ClipRect::new(0.0, 0.0, 500.0, 400.0);
        let child = parent.intersect(r(0.0, 0.0, 500.0, 400.0));
        assert_eq!(child.corner_radius_f32(), [0.0; 4]);
    }

    #[test]
    fn scissor_rounds_to_pixel_centers() {
        // 480 лог. × 1.0417 = 499.99998: округление наружу открывало пиксель
        // 499, который квады содержимого уже не закрашивают, — по краю
        // области оставалась полоска шириной в 1 px.
        let clip = ClipRect::new(480.0, 0.0, 1440.0, 918.0);
        let (x, _y, w, _h) = clip.scissor(1.0416666, 2000, 1125).unwrap();
        assert_eq!(x, 500);
        assert_eq!(x + w, 2000);
    }

    #[test]
    fn scissor_keeps_one_pixel_for_thin_clip() {
        let clip = ClipRect::new(10.0, 10.0, 0.2, 0.2);
        let (_x, _y, w, h) = clip.scissor(1.0, 100, 100).unwrap();
        assert_eq!(
            (w, h),
            (1, 1),
            "содержимое тоньше пикселя не должно пропадать"
        );
    }

    #[test]
    fn scissor_is_none_for_empty_clip() {
        let clip = ClipRect::new(10.0, 10.0, 0.0, 5.0);
        assert!(clip.scissor(1.0, 100, 100).is_none());
    }

    #[test]
    fn intersect_rounded_override_child_radii() {
        let parent = ClipRect::from_rect_rounded(r(0.0, 0.0, 1000.0, 800.0), [20.0; 4]);
        let child = parent.intersect_rounded(r(100.0, 100.0, 300.0, 300.0), [8.0; 4]);
        assert_eq!(child.corner_radius_f32(), [8.0; 4]);
    }
}
