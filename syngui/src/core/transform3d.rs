//! 3D-трансформации плоских элементов: поворот вокруг осей X/Y/Z, сдвиг по Z,
//! перспектива и зеркальное отражение.
//!
//! Элемент остаётся плоским (как `transform: rotateY()` в CSS): его
//! содержимое рисуется в текстуру слоя, а слой выводится четырёхугольником,
//! углы которого спроецированы с перспективой. Углы передаются в однородных
//! координатах `[X, Y, W]` (экранная точка — `(X/W, Y/W)`), поэтому GPU
//! интерполирует текстуру перспективно-корректно, без «излома» по диагонали.
//!
//! Вращение применяется после 2D-трансформаций (`translate`, `rotate`,
//! `scale`) вокруг точки `transform-origin`; точка схода перспективы — там же.

use super::{Point, Rect, Transform};

/// Параметры 3D-трансформации элемента. Углы — в градусах.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Transform3D {
    pub rotate_x: f32,
    pub rotate_y: f32,
    /// Поворот в плоскости экрана внутри 3D-преобразования (после X и Y).
    pub rotate_z: f32,
    /// Сдвиг к зрителю (положительный — ближе, крупнее).
    pub translate_z: f32,
    /// Расстояние до зрителя, px. `None` — 4 × больший размер элемента;
    /// `Some(0)` — без перспективы (ортогональная проекция).
    pub perspective: Option<f32>,
    /// Точка вращения относительно левого верхнего угла элемента.
    pub origin: Point,
    /// Показывать обратную сторону (элемент, повёрнутый «спиной» к зрителю).
    pub backface_visible: bool,
}

impl Default for Transform3D {
    fn default() -> Self {
        Self {
            rotate_x: 0.0,
            rotate_y: 0.0,
            rotate_z: 0.0,
            translate_z: 0.0,
            perspective: None,
            origin: Point::zero(),
            backface_visible: true,
        }
    }
}

impl Transform3D {
    /// Трансформация что-то меняет.
    pub fn is_active(&self) -> bool {
        self.rotate_x.abs() > 1e-3
            || self.rotate_y.abs() > 1e-3
            || self.rotate_z.abs() > 1e-3
            || self.translate_z.abs() > 1e-3
    }

    /// Матрица поворота 3×3 (строки): `Rx · Ry · Rz`, как
    /// `transform: rotateX() rotateY() rotateZ()` в CSS (ось Y — вниз,
    /// Z — к зрителю).
    pub fn rotation(&self) -> [[f32; 3]; 3] {
        let (sx, cx) = self.rotate_x.to_radians().sin_cos();
        let (sy, cy) = self.rotate_y.to_radians().sin_cos();
        let (sz, cz) = self.rotate_z.to_radians().sin_cos();
        let rx = [[1.0, 0.0, 0.0], [0.0, cx, -sx], [0.0, sx, cx]];
        let ry = [[cy, 0.0, sy], [0.0, 1.0, 0.0], [-sy, 0.0, cy]];
        let rz = [[cz, -sz, 0.0], [sz, cz, 0.0], [0.0, 0.0, 1.0]];
        mat_mul(&mat_mul(&rx, &ry), &rz)
    }

    /// Элемент повёрнут к зрителю обратной стороной.
    pub fn shows_back(&self) -> bool {
        self.rotation()[2][2] < -1e-4
    }
}

fn mat_mul(a: &[[f32; 3]; 3], b: &[[f32; 3]; 3]) -> [[f32; 3]; 3] {
    let mut out = [[0.0; 3]; 3];
    for (i, row) in out.iter_mut().enumerate() {
        for (j, cell) in row.iter_mut().enumerate() {
            *cell = a[i][0] * b[0][j] + a[i][1] * b[1][j] + a[i][2] * b[2][j];
        }
    }
    out
}

/// С какой стороны от элемента его отражение.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Default)]
pub enum ReflectSide {
    #[default]
    Below,
    Above,
    Left,
    Right,
}

/// Зеркальное отражение (`box-reflect: below 2px 0.35 50%`).
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Reflection {
    pub side: ReflectSide,
    /// Зазор между элементом и отражением, px.
    pub gap: f32,
    /// Непрозрачность у края элемента; дальше — затухание до нуля.
    pub opacity: f32,
    /// Длина отражения — доля размера элемента (0..1].
    pub length: f32,
}

impl Default for Reflection {
    fn default() -> Self {
        Self { side: ReflectSide::Below, gap: 0.0, opacity: 0.35, length: 0.5 }
    }
}

impl Reflection {
    /// `below|above|left|right [зазор] [непрозрачность] [длина%]`.
    pub fn parse(s: &str) -> Option<Self> {
        let mut r = Reflection::default();
        let mut numbers = 0;
        for tok in s.split_whitespace() {
            match tok.to_ascii_lowercase().as_str() {
                "none" => return None,
                "below" | "bottom" => r.side = ReflectSide::Below,
                "above" | "top" => r.side = ReflectSide::Above,
                "left" => r.side = ReflectSide::Left,
                "right" => r.side = ReflectSide::Right,
                t => {
                    if let Some(p) = t.strip_suffix('%') {
                        r.length = (p.parse::<f32>().ok()? / 100.0).clamp(0.01, 1.0);
                    } else if let Some(px) = t.strip_suffix("px") {
                        r.gap = px.parse().ok()?;
                    } else {
                        let v: f32 = t.parse().ok()?;
                        // Первое голое число — зазор, второе — непрозрачность;
                        // одно число не больше 1 — непрозрачность.
                        if numbers == 0 && v > 1.0 {
                            r.gap = v;
                        } else {
                            r.opacity = v.clamp(0.0, 1.0);
                        }
                        numbers += 1;
                    }
                }
            }
        }
        Some(r)
    }
}

/// Четырёхугольник, которым выводится текстура слоя.
#[derive(Clone, Debug, PartialEq)]
pub struct ProjectedQuad {
    /// Углы (левый верхний, правый верхний, правый нижний, левый нижний) —
    /// логические px в однородной форме `[X, Y, W]`.
    pub pos: [[f32; 3]; 4],
    /// Точка слоя (логические px экрана) для каждого угла.
    pub src: [[f32; 2]; 4],
    /// Непрозрачность в углах (затухание отражения).
    pub alpha: [f32; 4],
}

/// Ближе этого (доля расстояния до зрителя) точка не подходит — иначе
/// W уходит в ноль и четырёхугольник «выворачивается».
const MIN_W: f32 = 0.05;

/// Четырёхугольники слоя элемента: отражение (если есть), затем сам элемент.
///
/// `bounds` — границы элемента в координатах раскладки, `screen` — уже
/// действующая 2D-трансформация (предков и самого элемента). `None` у
/// `t3d` — плоский элемент (только отражение). Пустой список — элемент
/// не виден (повёрнут спиной при `backface-visibility: hidden`).
pub fn project_layer(
    bounds: Rect,
    screen: &Transform,
    t3d: Option<&Transform3D>,
    reflection: Option<&Reflection>,
) -> Vec<ProjectedQuad> {
    let mut quads = Vec::with_capacity(2);
    let t3d = t3d.filter(|t| t.is_active());
    if let Some(t) = t3d {
        if !t.backface_visible && t.shows_back() {
            return quads;
        }
    }
    let size = bounds.size.width.max(bounds.size.height).max(1.0);
    let map2 = |p: (f32, f32)| {
        let q = screen.transform_point(euclid::Point2D::new(p.0, p.1));
        (q.x, q.y)
    };
    let project: Box<dyn Fn((f32, f32)) -> [f32; 3]> = match t3d {
        None => Box::new(move |p| {
            let s = map2(p);
            [s.0, s.1, 1.0]
        }),
        Some(t) => {
            let o = map2((bounds.origin.x + t.origin.x, bounds.origin.y + t.origin.y));
            let r = t.rotation();
            let d = match t.perspective {
                Some(d) if d <= 0.0 => 0.0,
                Some(d) => d,
                None => size * 4.0,
            };
            let tz = t.translate_z;
            Box::new(move |p| {
                let s = map2(p);
                let v = [s.0 - o.0, s.1 - o.1, 0.0];
                let x = r[0][0] * v[0] + r[0][1] * v[1];
                let y = r[1][0] * v[0] + r[1][1] * v[1];
                let z = r[2][0] * v[0] + r[2][1] * v[1] + tz;
                let w = if d > 0.0 { (1.0 - z / d).max(MIN_W) } else { 1.0 };
                [o.0 * w + x, o.1 * w + y, w]
            })
        }
    };

    if let Some(refl) = reflection {
        let (x0, y0) = (bounds.origin.x, bounds.origin.y);
        let (x1, y1) = (x0 + bounds.size.width, y0 + bounds.size.height);
        let len_v = bounds.size.height * refl.length;
        let len_h = bounds.size.width * refl.length;
        let g = refl.gap;
        let a = refl.opacity;
        // Углы отражения (TL, TR, BR, BL), соответствующие им точки
        // элемента и непрозрачность.
        let (dst, src, alpha): ([(f32, f32); 4], [(f32, f32); 4], [f32; 4]) = match refl.side {
            ReflectSide::Below => (
                [(x0, y1 + g), (x1, y1 + g), (x1, y1 + g + len_v), (x0, y1 + g + len_v)],
                [(x0, y1), (x1, y1), (x1, y1 - len_v), (x0, y1 - len_v)],
                [a, a, 0.0, 0.0],
            ),
            ReflectSide::Above => (
                [(x0, y0 - g - len_v), (x1, y0 - g - len_v), (x1, y0 - g), (x0, y0 - g)],
                [(x0, y0 + len_v), (x1, y0 + len_v), (x1, y0), (x0, y0)],
                [0.0, 0.0, a, a],
            ),
            ReflectSide::Right => (
                [(x1 + g, y0), (x1 + g + len_h, y0), (x1 + g + len_h, y1), (x1 + g, y1)],
                [(x1, y0), (x1 - len_h, y0), (x1 - len_h, y1), (x1, y1)],
                [a, 0.0, 0.0, a],
            ),
            ReflectSide::Left => (
                [(x0 - g - len_h, y0), (x0 - g, y0), (x0 - g, y1), (x0 - g - len_h, y1)],
                [(x0 + len_h, y0), (x0, y0), (x0, y1), (x0 + len_h, y1)],
                [0.0, a, a, 0.0],
            ),
        };
        quads.push(ProjectedQuad {
            pos: dst.map(|p| project(p)),
            src: src.map(|p| {
                let s = map2(p);
                [s.0, s.1]
            }),
            alpha,
        });
    }

    // Сам элемент — с запасом под тени и свечение, вылезающие за границы.
    let m = (size * 0.25).max(16.0);
    let r = bounds.inflate(m, m);
    let corners = [
        (r.origin.x, r.origin.y),
        (r.origin.x + r.size.width, r.origin.y),
        (r.origin.x + r.size.width, r.origin.y + r.size.height),
        (r.origin.x, r.origin.y + r.size.height),
    ];
    quads.push(ProjectedQuad {
        pos: corners.map(|p| project(p)),
        src: corners.map(|p| {
            let s = map2(p);
            [s.0, s.1]
        }),
        alpha: [1.0; 4],
    });
    quads
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::core::Size;

    fn screen(q: &[f32; 3]) -> (f32, f32) {
        (q[0] / q[2], q[1] / q[2])
    }

    #[test]
    fn flat_layer_maps_to_itself() {
        let b = Rect::new(Point::new(10.0, 20.0), Size::new(100.0, 50.0));
        let q = project_layer(b, &Transform::identity(), None, None);
        assert_eq!(q.len(), 1);
        for (p, s) in q[0].pos.iter().zip(q[0].src.iter()) {
            assert_eq!(screen(p), (s[0], s[1]));
        }
    }

    #[test]
    fn rotate_y_narrows_and_keeps_center() {
        let b = Rect::new(Point::new(0.0, 0.0), Size::new(100.0, 100.0));
        let t = Transform3D { rotate_y: 60.0, origin: Point::new(50.0, 50.0), perspective: Some(0.0), ..Default::default() };
        let q = project_layer(b, &Transform::identity(), Some(&t), None);
        let q = &q[0];
        // Ортогонально: ширина × cos 60° = половина (с запасом m = 25 px).
        let l = screen(&q.pos[0]).0;
        let r = screen(&q.pos[1]).0;
        assert!(((r - l) - 150.0 * 0.5).abs() < 0.01, "{l} {r}");
        assert!(((l + r) / 2.0 - 50.0).abs() < 0.01);
    }

    #[test]
    fn perspective_makes_near_edge_bigger() {
        let b = Rect::new(Point::new(0.0, 0.0), Size::new(100.0, 100.0));
        // Нижний край (y > 0) при rotateX > 0 идёт к зрителю — шире верхнего.
        let t = Transform3D { rotate_x: 50.0, origin: Point::new(50.0, 50.0), perspective: Some(300.0), ..Default::default() };
        let q = &project_layer(b, &Transform::identity(), Some(&t), None)[0];
        let top = screen(&q.pos[1]).0 - screen(&q.pos[0]).0;
        let bottom = screen(&q.pos[2]).0 - screen(&q.pos[3]).0;
        assert!(bottom > top * 1.2, "top {top} bottom {bottom}");
    }

    #[test]
    fn hidden_backface_is_culled() {
        let b = Rect::new(Point::new(0.0, 0.0), Size::new(10.0, 10.0));
        let t = Transform3D { rotate_y: 180.0, backface_visible: false, ..Default::default() };
        assert!(project_layer(b, &Transform::identity(), Some(&t), None).is_empty());
        let t = Transform3D { rotate_y: 180.0, ..Default::default() };
        assert_eq!(project_layer(b, &Transform::identity(), Some(&t), None).len(), 1);
    }

    #[test]
    fn reflection_below_mirrors_bottom_edge() {
        let b = Rect::new(Point::new(0.0, 0.0), Size::new(40.0, 40.0));
        let r = Reflection { gap: 2.0, opacity: 0.5, length: 0.5, ..Default::default() };
        let q = project_layer(b, &Transform::identity(), None, Some(&r));
        assert_eq!(q.len(), 2);
        let refl = &q[0];
        assert_eq!(screen(&refl.pos[0]), (0.0, 42.0));
        assert_eq!(refl.src[0], [0.0, 40.0]);
        assert_eq!(refl.src[3], [0.0, 20.0]);
        assert_eq!(refl.alpha, [0.5, 0.5, 0.0, 0.0]);
    }

    #[test]
    fn reflection_parse() {
        let r = Reflection::parse("below 4px 0.4 60%").unwrap();
        assert_eq!(r.side, ReflectSide::Below);
        assert_eq!(r.gap, 4.0);
        assert!((r.opacity - 0.4).abs() < 1e-6);
        assert!((r.length - 0.6).abs() < 1e-6);
        assert!(Reflection::parse("none").is_none());
        assert_eq!(Reflection::parse("right").unwrap().side, ReflectSide::Right);
    }
}
