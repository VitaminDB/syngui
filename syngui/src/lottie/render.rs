//! Отрисовка кадра композиции: слои снизу вверх, parenting матриц, маски,
//! track matte, прекомпозиции с time remap; фигуры собираются в контуры
//! (device space), стили (заливки/обводки) применяются к контурам выше по
//! списку — как в After Effects/lottie-web.

use std::f32::consts::{PI, TAU};

use super::geom::{dash_polyline, stroke_polyline, trim_paths, Mat, Polyline, ShapePath};
use super::model::{
    Composition, DashKind, Gradient, GradientKind, Layer, LayerKind, LayerMask, MaskMode, MatteMode, Shape,
    StrokeStyle, Transform,
};
use super::raster::{BlendMode, FillRule, GradientStops, Mask, Paint, Pixmap, Rasterizer};

/// Допуск уплощения кривых в пикселях.
const TOLERANCE: f32 = 0.2;
const MAX_DEPTH: usize = 48;

/// Рендерер кадров: переиспользует растеризатор и буферы слоёв.
pub struct Renderer {
    rast: Rasterizer,
    pool: Vec<Pixmap>,
    target: Option<Pixmap>,
}

impl Default for Renderer {
    fn default() -> Self {
        Self::new()
    }
}

struct Ctx<'a> {
    comp: &'a Composition,
    w: usize,
    h: usize,
}

/// Экземпляр стиля (заливка/обводка) с матрицей и непрозрачностью своей группы.
struct StyleInst<'a> {
    shape: &'a Shape,
    mat: Mat,
    opacity: f32,
}

struct TrimInst {
    start: f32,
    end: f32,
    offset: f32,
    individually: bool,
}

struct PathInst {
    polys: Vec<Polyline>,
    styles: Vec<usize>,
    trims: Vec<usize>,
}

struct Collected<'a> {
    styles: Vec<StyleInst<'a>>,
    trims: Vec<TrimInst>,
    paths: Vec<PathInst>,
}

#[inline]
fn two(v: &[f32]) -> [f32; 2] {
    [v.first().copied().unwrap_or(0.0), v.get(1).copied().unwrap_or(0.0)]
}

impl Renderer {
    pub fn new() -> Self {
        Renderer {
            rast: Rasterizer::new(),
            pool: Vec::new(),
            target: None,
        }
    }

    /// Отрисовать кадр в RGBA8 (прямая альфа) размером `w×h`; композиция
    /// растягивается на весь буфер (пропорции подбирает вызывающий).
    pub fn render_rgba8(&mut self, comp: &Composition, frame: f32, w: u32, h: u32) -> Vec<u8> {
        let w = w.max(1) as usize;
        let h = h.max(1) as usize;
        let mut target = match self.target.take() {
            Some(t) if t.w == w && t.h == h => t,
            _ => Pixmap::new(w, h),
        };
        let root = Mat::scale(w as f32 / comp.width, h as f32 / comp.height);
        self.render(comp, frame, &root, &mut target);
        let out = target.to_rgba8();
        self.target = Some(target);
        out
    }

    /// Отрисовать кадр в `target` (очищается), `root` — из координат композиции в пиксели.
    pub fn render(&mut self, comp: &Composition, frame: f32, root: &Mat, target: &mut Pixmap) {
        target.clear();
        let ctx = Ctx {
            comp,
            w: target.w,
            h: target.h,
        };
        self.render_layers(&ctx, &comp.layers, frame, root, target, 0, None);
    }

    fn take(&mut self, w: usize, h: usize) -> Pixmap {
        if let Some(pos) = self.pool.iter().position(|p| p.w == w && p.h == h) {
            self.pool.swap_remove(pos)
        } else {
            Pixmap::new(w, h)
        }
    }

    fn recycle(&mut self, mut p: Pixmap) {
        p.clear();
        if self.pool.len() < 8 {
            self.pool.push(p);
        }
    }

    fn layer_matrix(
        &self,
        layers: &[Layer],
        i: usize,
        frame: f32,
        root: &Mat,
        memo: &mut Vec<Option<Mat>>,
        depth: usize,
    ) -> Mat {
        if let Some(m) = memo[i] {
            return m;
        }
        let l = &layers[i];
        // Ключевые кадры свойств заданы во времени композиции: `st`/`sr`
        // сдвигают только время дочерней прекомпозиции (как в lottie-web/rlottie).
        let extra = if l.auto_orient { auto_orient_angle(&l.transform, frame) } else { 0.0 };
        let own = l.transform.matrix_with(frame, extra);
        let parent = match l.parent {
            Some(p) if p != i && p < layers.len() && depth < 64 => {
                self.layer_matrix(layers, p, frame, root, memo, depth + 1)
            }
            _ => *root,
        };
        let m = parent.mul(&own);
        memo[i] = Some(m);
        m
    }

    #[allow(clippy::too_many_arguments)]
    fn render_layers(
        &mut self,
        ctx: &Ctx,
        layers: &[Layer],
        frame: f32,
        root: &Mat,
        target: &mut Pixmap,
        depth: usize,
        clip: Option<&Mask>,
    ) {
        if depth > MAX_DEPTH {
            return;
        }
        if clip.map(Mask::is_empty).unwrap_or(false) {
            return;
        }
        let mut memo = vec![None; layers.len()];
        for i in (0..layers.len()).rev() {
            let l = &layers[i];
            if l.hidden || l.is_matte_source || !l.in_range(frame) {
                continue;
            }
            if matches!(l.kind, LayerKind::Null | LayerKind::Unsupported) {
                continue;
            }
            let mat = self.layer_matrix(layers, i, frame, root, &mut memo, 0);
            self.render_layer(ctx, layers, i, frame, mat, root, target, depth, &mut memo, clip);
        }
    }

    /// Клип слоя: внешний клип ∩ прямоугольник прекомпозиции ∩ маски слоя.
    /// `None` — без ограничений, `Some(пустая)` — слой целиком невидим.
    fn layer_clip(&mut self, ctx: &Ctx, l: &Layer, frame: f32, mat: &Mat, outer: Option<&Mask>) -> Option<Mask> {
        let mut clip: Option<Mask> = outer.cloned();
        let region = |c: &Option<Mask>| -> [usize; 4] {
            match c {
                Some(m) => [
                    m.x0.max(0) as usize,
                    m.y0.max(0) as usize,
                    ((m.x0 as i64 + m.w as i64).max(0) as usize).min(ctx.w),
                    ((m.y0 as i64 + m.h as i64).max(0) as usize).min(ctx.h),
                ],
                None => [0, 0, ctx.w, ctx.h],
            }
        };
        if matches!(l.kind, LayerKind::Precomp(_)) && l.width > 0.0 && l.height > 0.0 {
            let r = region(&clip);
            if !rect_covers_region(mat, l.width, l.height, r) {
                let poly = rect_polyline(0.0, 0.0, l.width, l.height, mat);
                let rect = self
                    .rast
                    .fill(&[poly], FillRule::NonZero, ctx.w, ctx.h)
                    .unwrap_or_default();
                clip = Some(match clip {
                    None => rect,
                    Some(c) => Mask::intersection(&c, &rect),
                });
            }
        }
        if !l.masks.is_empty() {
            let r = region(&clip);
            let masks = self.layer_masks(ctx, &l.masks, frame, mat, r);
            clip = Some(match clip {
                None => masks,
                Some(c) => Mask::intersection(&c, &masks),
            });
        }
        clip
    }

    #[allow(clippy::too_many_arguments)]
    fn render_layer(
        &mut self,
        ctx: &Ctx,
        layers: &[Layer],
        i: usize,
        frame: f32,
        mat: Mat,
        root: &Mat,
        target: &mut Pixmap,
        depth: usize,
        memo: &mut Vec<Option<Mat>>,
        outer_clip: Option<&Mask>,
    ) {
        let l = &layers[i];
        let opacity = l.transform.opacity_at(frame);
        if opacity <= 0.0 {
            return;
        }
        let clip = self.layer_clip(ctx, l, frame, &mat, outer_clip);
        if clip.as_ref().map(Mask::is_empty).unwrap_or(false) {
            return;
        }
        let clip_ref = clip.as_ref();
        let needs_buffer = l.matte.is_some() || opacity < 1.0 || l.blend != BlendMode::Normal;
        if !needs_buffer {
            self.draw_content(ctx, l, frame, &mat, target, depth, clip_ref);
            return;
        }
        let mut buf = self.take(ctx.w, ctx.h);
        self.draw_content(ctx, l, frame, &mat, &mut buf, depth, clip_ref);
        if buf.dirty.is_some() {
            if let (Some(mode), Some(src)) = (l.matte, l.matte_source) {
                if src < layers.len() && src != i {
                    let ml = &layers[src];
                    let mut mbuf = self.take(ctx.w, ctx.h);
                    if !ml.hidden && ml.in_range(frame) {
                        let mm = self.layer_matrix(layers, src, frame, root, memo, 0);
                        self.render_layer(ctx, layers, src, frame, mm, root, &mut mbuf, depth, memo, outer_clip);
                    }
                    let (luma, inverted) = match mode {
                        MatteMode::Alpha => (false, false),
                        MatteMode::AlphaInverted => (false, true),
                        MatteMode::Luma => (true, false),
                        MatteMode::LumaInverted => (true, true),
                    };
                    buf.apply_matte(&mbuf, luma, inverted);
                    self.recycle(mbuf);
                }
            }
            target.composite(&buf, opacity, l.blend);
        }
        self.recycle(buf);
    }

    #[allow(clippy::too_many_arguments)]
    fn draw_content(
        &mut self,
        ctx: &Ctx,
        l: &Layer,
        frame: f32,
        mat: &Mat,
        target: &mut Pixmap,
        depth: usize,
        clip: Option<&Mask>,
    ) {
        match &l.kind {
            LayerKind::Shape(items) => self.draw_shapes(ctx, items, frame, mat, target, clip),
            LayerKind::Solid(color) => {
                let poly = rect_polyline(0.0, 0.0, l.width, l.height, mat);
                if let Some(m) = self.rast.fill(&[poly], FillRule::NonZero, ctx.w, ctx.h) {
                    target.fill_mask(&m, &Paint::solid(*color, 1.0), clip);
                }
            }
            LayerKind::Image(id) => {
                let Some(asset) = ctx.comp.images.get(id) else {
                    return;
                };
                let Some(image) = asset.pixels.clone() else {
                    return;
                };
                let (iw, ih) = (image.width.max(1) as f32, image.height.max(1) as f32);
                let (aw, ah) = if asset.width > 0.0 && asset.height > 0.0 { (asset.width, asset.height) } else { (iw, ih) };
                let img_mat = mat.mul(&Mat::scale(aw / iw, ah / ih));
                let Some(inv) = img_mat.invert() else {
                    return;
                };
                let poly = rect_polyline(0.0, 0.0, aw, ah, mat);
                if let Some(m) = self.rast.fill(&[poly], FillRule::NonZero, ctx.w, ctx.h) {
                    target.fill_mask(
                        &m,
                        &Paint::Image {
                            inv,
                            image,
                            opacity: 1.0,
                        },
                        clip,
                    );
                }
            }
            LayerKind::Precomp(id) => {
                let Some(children) = ctx.comp.precomps.get(id) else {
                    return;
                };
                let child_frame = match &l.time_remap {
                    Some(tm) => tm.value(frame) * ctx.comp.frame_rate,
                    None => l.local_frame(frame),
                };
                self.render_layers(ctx, children, child_frame, mat, target, depth + 1, clip);
            }
            LayerKind::Null | LayerKind::Unsupported => {}
        }
    }

    /// Совокупное покрытие масок слоя. Для прямых масок режимов
    /// add/intersect/lighten/darken область — объединение их охватов,
    /// иначе (subtract, inverted, difference) — вся `region`.
    fn layer_masks(&mut self, ctx: &Ctx, masks: &[LayerMask], frame: f32, mat: &Mat, region: [usize; 4]) -> Mask {
        let mut covs: Vec<Option<Mask>> = Vec::with_capacity(masks.len());
        let bounded = masks
            .iter()
            .all(|m| !m.inverted && matches!(m.mode, MaskMode::Add | MaskMode::Intersect | MaskMode::Lighten | MaskMode::Darken));
        let mut bb: Option<[usize; 4]> = None;
        for m in masks {
            let mut path = m.path.value(frame);
            path.closed = true;
            path.transform(mat);
            let poly = path.flatten(TOLERANCE);
            let cov = self.rast.fill(&[poly], FillRule::NonZero, ctx.w, ctx.h);
            if let Some(c) = &cov {
                let r = [
                    c.x0.max(0) as usize,
                    c.y0.max(0) as usize,
                    (c.x0 as usize + c.w).min(ctx.w),
                    (c.y0 as usize + c.h).min(ctx.h),
                ];
                bb = Some(match bb {
                    None => r,
                    Some(b) => [b[0].min(r[0]), b[1].min(r[1]), b[2].max(r[2]), b[3].max(r[3])],
                });
            }
            covs.push(cov);
        }
        let mut r = region;
        if bounded {
            match bb {
                Some(b) => {
                    r = [r[0].max(b[0]), r[1].max(b[1]), r[2].min(b[2]), r[3].min(b[3])];
                }
                None => r = [0, 0, 0, 0],
            }
        }
        let [rx0, ry0, rx1, ry1] = r;
        let (w, h) = (rx1.saturating_sub(rx0), ry1.saturating_sub(ry0));
        let mut acc = Mask {
            x0: rx0 as i32,
            y0: ry0 as i32,
            w,
            h,
            cov: Vec::new(),
        };
        if w == 0 || h == 0 {
            return acc;
        }
        let first_full = matches!(masks[0].mode, MaskMode::Subtract | MaskMode::Intersect);
        acc.cov = vec![if first_full { 1.0 } else { 0.0 }; w * h];
        for (m, cov) in masks.iter().zip(&covs) {
            let op = (m.opacity.value(frame) / 100.0).clamp(0.0, 1.0);
            for y in 0..h {
                for x in 0..w {
                    let mut c = cov
                        .as_ref()
                        .map(|c| c.get((x + rx0) as i32, (y + ry0) as i32))
                        .unwrap_or(0.0);
                    if m.inverted {
                        c = 1.0 - c;
                    }
                    c *= op;
                    let r = &mut acc.cov[y * w + x];
                    *r = match m.mode {
                        MaskMode::Add => *r + c - *r * c,
                        MaskMode::Subtract => *r * (1.0 - c),
                        MaskMode::Intersect => *r * c,
                        MaskMode::Lighten => r.max(c),
                        MaskMode::Darken => r.min(c),
                        MaskMode::Difference => *r + c - 2.0 * *r * c,
                        MaskMode::None => *r,
                    };
                }
            }
        }
        acc
    }

    fn draw_shapes(&mut self, ctx: &Ctx, items: &[Shape], frame: f32, mat: &Mat, target: &mut Pixmap, clip: Option<&Mask>) {
        let mut col = Collected {
            styles: Vec::new(),
            trims: Vec::new(),
            paths: Vec::new(),
        };
        collect_shapes(items, frame, *mat, 1.0, &[], &[], &[], &mut col, 0);
        apply_trims(&mut col);
        let mut polys: Vec<Polyline> = Vec::new();
        let mut stroke_polys: Vec<Polyline> = Vec::new();
        for (si, style) in col.styles.iter().enumerate() {
            polys.clear();
            for p in col.paths.iter().filter(|p| p.styles.contains(&si)) {
                polys.extend(p.polys.iter().cloned());
            }
            if polys.is_empty() {
                continue;
            }
            match style.shape {
                Shape::Fill { color, opacity, rule } => {
                    let c = color.value(frame);
                    let a = opacity.value(frame) / 100.0 * c.get(3).copied().unwrap_or(1.0) * style.opacity;
                    if a <= 0.0 {
                        continue;
                    }
                    let paint = Paint::solid([c[0], c.get(1).copied().unwrap_or(0.0), c.get(2).copied().unwrap_or(0.0)], a);
                    if let Some(m) = self.rast.fill(&polys, *rule, ctx.w, ctx.h) {
                        target.fill_mask(&m, &paint, clip);
                    }
                }
                Shape::GradientFill { gradient, opacity, rule } => {
                    let a = opacity.value(frame) / 100.0 * style.opacity;
                    let Some(paint) = gradient_paint(gradient, frame, &style.mat, a) else {
                        continue;
                    };
                    if let Some(m) = self.rast.fill(&polys, *rule, ctx.w, ctx.h) {
                        target.fill_mask(&m, &paint, clip);
                    }
                }
                Shape::Stroke { color, opacity, style: st } => {
                    let c = color.value(frame);
                    let a = opacity.value(frame) / 100.0 * c.get(3).copied().unwrap_or(1.0) * style.opacity;
                    if a <= 0.0 {
                        continue;
                    }
                    let paint = Paint::solid([c[0], c.get(1).copied().unwrap_or(0.0), c.get(2).copied().unwrap_or(0.0)], a);
                    stroke_polys.clear();
                    build_stroke(&polys, st, frame, &style.mat, &mut stroke_polys);
                    if let Some(m) = self.rast.fill(&stroke_polys, FillRule::NonZero, ctx.w, ctx.h) {
                        target.fill_mask(&m, &paint, clip);
                    }
                }
                Shape::GradientStroke { gradient, opacity, style: st } => {
                    let a = opacity.value(frame) / 100.0 * style.opacity;
                    let Some(paint) = gradient_paint(gradient, frame, &style.mat, a) else {
                        continue;
                    };
                    stroke_polys.clear();
                    build_stroke(&polys, st, frame, &style.mat, &mut stroke_polys);
                    if let Some(m) = self.rast.fill(&stroke_polys, FillRule::NonZero, ctx.w, ctx.h) {
                        target.fill_mask(&m, &paint, clip);
                    }
                }
                _ => {}
            }
        }
    }
}

impl Layer {
    /// Время дочерней прекомпозиции: сдвиг `st` и растяжение `sr`.
    #[inline]
    pub fn local_frame(&self, frame: f32) -> f32 {
        (frame - self.start_time) / self.stretch
    }

    #[inline]
    pub fn in_range(&self, frame: f32) -> bool {
        frame >= self.in_point && frame < self.out_point
    }
}

fn auto_orient_angle(tr: &Transform, local: f32) -> f32 {
    let p0 = tr.position_at(local - 0.5);
    let p1 = tr.position_at(local + 0.5);
    let d = [p1[0] - p0[0], p1[1] - p0[1]];
    if d[0].abs() < 1e-4 && d[1].abs() < 1e-4 {
        0.0
    } else {
        d[1].atan2(d[0]).to_degrees()
    }
}

fn rect_polyline(x: f32, y: f32, w: f32, h: f32, mat: &Mat) -> Polyline {
    Polyline {
        pts: vec![
            mat.apply([x, y]),
            mat.apply([x + w, y]),
            mat.apply([x + w, y + h]),
            mat.apply([x, y + h]),
        ],
        closed: true,
    }
}

/// Покрывает ли прямоугольник `w×h` (в локальных координатах `mat`) область
/// холста `[x0,y0,x1,y1)` в пикселях.
fn rect_covers_region(mat: &Mat, w: f32, h: f32, r: [usize; 4]) -> bool {
    let Some(inv) = mat.invert() else {
        return false;
    };
    let eps = 1e-3;
    let (x0, y0, x1, y1) = (r[0] as f32, r[1] as f32, r[2] as f32, r[3] as f32);
    [[x0, y0], [x1, y0], [x1, y1], [x0, y1]].iter().all(|c| {
        let p = inv.apply(*c);
        p[0] >= -eps && p[1] >= -eps && p[0] <= w + eps && p[1] <= h + eps
    })
}

/// Матрица k-й копии repeater'а: `T(k·p)·T(a)·R(k·r)·S(s^k)·T(-a)`.
fn repeater_matrix(tr: &Transform, frame: f32, k: f32) -> Mat {
    let a = two(&tr.anchor.value(frame));
    let p = tr.position_at(frame);
    let s = tr.scale.value(frame);
    let s = [s.first().copied().unwrap_or(100.0) / 100.0, s.get(1).copied().unwrap_or(100.0) / 100.0];
    let r = tr.rotation.value(frame);
    let pw = |v: f32| -> f32 {
        if (k - k.round()).abs() < 1e-6 {
            v.powi(k.round() as i32)
        } else {
            v.signum() * v.abs().powf(k)
        }
    };
    Mat::translate(p[0] * k, p[1] * k)
        .mul(&Mat::translate(a[0], a[1]))
        .mul(&Mat::rotate(r * k))
        .mul(&Mat::scale(pw(s[0]), pw(s[1])))
        .mul(&Mat::translate(-a[0], -a[1]))
}

#[allow(clippy::too_many_arguments)]
fn collect_shapes<'a>(
    items: &'a [Shape],
    frame: f32,
    mat: Mat,
    opacity: f32,
    inherited_styles: &[usize],
    inherited_trims: &[usize],
    inherited_rounds: &[f32],
    col: &mut Collected<'a>,
    depth: usize,
) {
    if depth > MAX_DEPTH {
        return;
    }
    let mut active_styles = inherited_styles.to_vec();
    let mut active_trims = inherited_trims.to_vec();
    let mut rounds = inherited_rounds.to_vec();
    let mut idx = items.len();
    while idx > 0 {
        idx -= 1;
        let it = &items[idx];
        match it {
            Shape::Fill { .. } | Shape::Stroke { .. } | Shape::GradientFill { .. } | Shape::GradientStroke { .. } => {
                col.styles.push(StyleInst {
                    shape: it,
                    mat,
                    opacity,
                });
                active_styles.push(col.styles.len() - 1);
            }
            Shape::Trim {
                start,
                end,
                offset,
                individually,
            } => {
                col.trims.push(TrimInst {
                    start: start.value(frame) / 100.0,
                    end: end.value(frame) / 100.0,
                    offset: offset.value(frame),
                    individually: *individually,
                });
                active_trims.push(col.trims.len() - 1);
            }
            Shape::RoundCorners { radius } => rounds.push(radius.value(frame)),
            Shape::Repeater {
                copies,
                offset,
                transform,
                start_opacity,
                end_opacity,
                above,
            } => {
                let n = copies.value(frame).ceil().max(0.0) as usize;
                let off = offset.value(frame);
                let so = (start_opacity.value(frame) / 100.0).clamp(0.0, 1.0);
                let eo = (end_opacity.value(frame) / 100.0).clamp(0.0, 1.0);
                let before = &items[..idx];
                let order: Vec<usize> = if *above { (0..n).collect() } else { (0..n).rev().collect() };
                for k in order {
                    let rm = repeater_matrix(transform, frame, k as f32 + off);
                    let op = if n > 1 { so + (eo - so) * k as f32 / (n - 1) as f32 } else { so };
                    collect_shapes(
                        before,
                        frame,
                        mat.mul(&rm),
                        opacity * op,
                        &active_styles,
                        &active_trims,
                        &rounds,
                        col,
                        depth + 1,
                    );
                }
                break;
            }
            Shape::Group { items: sub, transform, .. } => {
                let gm = transform.matrix(frame);
                let gop = transform.opacity_at(frame);
                collect_shapes(
                    sub,
                    frame,
                    mat.mul(&gm),
                    opacity * gop,
                    &active_styles,
                    &active_trims,
                    &rounds,
                    col,
                    depth + 1,
                );
            }
            Shape::Rect { .. } | Shape::Ellipse { .. } | Shape::Polystar { .. } | Shape::Path { .. } => {
                let Some(mut sp) = shape_to_path(it, frame) else {
                    continue;
                };
                // Скругления применяются в порядке списка: последнее встреченное — первым.
                for r in rounds.iter().rev() {
                    sp = sp.round_corners(*r);
                }
                sp.transform(&mat);
                let poly = sp.flatten(TOLERANCE);
                if poly.pts.len() >= 2 {
                    col.paths.push(PathInst {
                        polys: vec![poly],
                        styles: active_styles.clone(),
                        trims: active_trims.clone(),
                    });
                }
            }
            Shape::Ignored => {}
        }
    }
}

fn apply_trims(col: &mut Collected) {
    if col.trims.is_empty() {
        return;
    }
    // Порядок применения — порядок списка (обратный порядку встречи).
    for ti in (0..col.trims.len()).rev() {
        let t = &col.trims[ti];
        let idxs: Vec<usize> = (0..col.paths.len()).filter(|&p| col.paths[p].trims.contains(&ti)).collect();
        if idxs.is_empty() {
            continue;
        }
        if !t.individually {
            for p in idxs {
                let polys = std::mem::take(&mut col.paths[p].polys);
                col.paths[p].polys = trim_paths(polys, t.start, t.end, t.offset, false);
            }
        } else {
            // Все контуры как одна последовательность: общая длина, затем куски по контурам.
            let total: f32 = idxs.iter().flat_map(|&p| col.paths[p].polys.iter().map(Polyline::length)).sum();
            if total <= 0.0 {
                continue;
            }
            let mut o = (t.offset % 360.0) / 360.0;
            if o < 0.0 {
                o += 1.0;
            }
            let mut s = t.start.clamp(0.0, 1.0) + o;
            let mut e = t.end.clamp(0.0, 1.0) + o;
            if s > e {
                std::mem::swap(&mut s, &mut e);
            }
            let mut ranges: Vec<(f32, f32)> = Vec::new();
            if (e - s).abs() >= 1e-6 {
                if s <= 0.0 && e >= 1.0 {
                    continue;
                }
                if e <= 1.0 {
                    ranges.push((s * total, e * total));
                } else if s >= 1.0 {
                    ranges.push(((s - 1.0) * total, (e - 1.0) * total));
                } else {
                    ranges.push((s * total, total));
                    ranges.push((0.0, (e - 1.0) * total));
                }
            }
            let mut off = 0.0f32;
            for &p in &idxs {
                let polys = std::mem::take(&mut col.paths[p].polys);
                let mut out = Vec::new();
                for poly in &polys {
                    let l = poly.length();
                    for &(r0, r1) in &ranges {
                        let a = r0.max(off) - off;
                        let b = r1.min(off + l) - off;
                        if b > a {
                            out.extend(poly.slice(a, b));
                        }
                    }
                    off += l;
                }
                col.paths[p].polys = out;
            }
        }
    }
}

/// Локальный контур фигуры на кадре.
fn shape_to_path(shape: &Shape, frame: f32) -> Option<ShapePath> {
    const K: f32 = 0.5519;
    match shape {
        Shape::Path { path, reversed } => {
            let mut p = path.value(frame);
            if *reversed {
                p.reverse();
            }
            Some(p)
        }
        Shape::Ellipse { position, size, reversed } => {
            let c = position.position(frame);
            let s = two(&size.value(frame));
            let (rx, ry) = (s[0] / 2.0, s[1] / 2.0);
            let (kx, ky) = (rx * K, ry * K);
            let mut p = ShapePath::with_capacity(4, true);
            if !*reversed {
                p.push([c[0], c[1] - ry], [-kx, 0.0], [kx, 0.0]);
                p.push([c[0] + rx, c[1]], [0.0, -ky], [0.0, ky]);
                p.push([c[0], c[1] + ry], [kx, 0.0], [-kx, 0.0]);
                p.push([c[0] - rx, c[1]], [0.0, ky], [0.0, -ky]);
            } else {
                p.push([c[0], c[1] - ry], [kx, 0.0], [-kx, 0.0]);
                p.push([c[0] - rx, c[1]], [0.0, -ky], [0.0, ky]);
                p.push([c[0], c[1] + ry], [-kx, 0.0], [kx, 0.0]);
                p.push([c[0] + rx, c[1]], [0.0, ky], [0.0, -ky]);
            }
            Some(p)
        }
        Shape::Rect {
            position,
            size,
            roundness,
            reversed,
        } => {
            let c = position.position(frame);
            let s = two(&size.value(frame));
            let (hw, hh) = (s[0] / 2.0, s[1] / 2.0);
            let r = roundness.value(frame).min(hw.abs()).min(hh.abs()).max(0.0);
            let mut p;
            if r <= 1e-6 {
                p = ShapePath::with_capacity(4, true);
                p.push([c[0] + hw, c[1] - hh], [0.0; 2], [0.0; 2]);
                p.push([c[0] + hw, c[1] + hh], [0.0; 2], [0.0; 2]);
                p.push([c[0] - hw, c[1] + hh], [0.0; 2], [0.0; 2]);
                p.push([c[0] - hw, c[1] - hh], [0.0; 2], [0.0; 2]);
            } else {
                let k = r * 0.5523;
                p = ShapePath::with_capacity(8, true);
                p.push([c[0] + hw, c[1] - hh + r], [0.0, -k], [0.0; 2]);
                p.push([c[0] + hw, c[1] + hh - r], [0.0; 2], [0.0, k]);
                p.push([c[0] + hw - r, c[1] + hh], [k, 0.0], [0.0; 2]);
                p.push([c[0] - hw + r, c[1] + hh], [0.0; 2], [-k, 0.0]);
                p.push([c[0] - hw, c[1] + hh - r], [0.0, k], [0.0; 2]);
                p.push([c[0] - hw, c[1] - hh + r], [0.0; 2], [0.0, -k]);
                p.push([c[0] - hw + r, c[1] - hh], [-k, 0.0], [0.0; 2]);
                p.push([c[0] + hw - r, c[1] - hh], [0.0; 2], [k, 0.0]);
            }
            if *reversed {
                p.reverse();
            }
            Some(p)
        }
        Shape::Polystar {
            position,
            rotation,
            points,
            outer_radius,
            inner_radius,
            outer_roundness,
            inner_roundness,
            star,
            reversed,
        } => {
            let c = position.position(frame);
            let dir = if *reversed { -1.0 } else { 1.0 };
            let pts = points.value(frame).floor().max(1.0) as usize;
            let or_ = outer_radius.value(frame);
            let ir = inner_radius.value(frame);
            let os = outer_roundness.value(frame) / 100.0;
            let is = inner_roundness.value(frame) / 100.0;
            let num = if *star { pts * 2 } else { pts };
            let step = TAU / num as f32;
            let mut ang = -PI / 2.0 + rotation.value(frame).to_radians();
            let mut p = ShapePath::with_capacity(num, true);
            let mut long = true;
            for _ in 0..num {
                let (rad, roundness) = if *star {
                    if long { (or_, os) } else { (ir, is) }
                } else {
                    (or_, os)
                };
                let perim = if *star { TAU * rad / (num as f32 * 2.0) } else { TAU * rad / (num as f32 * 4.0) };
                let x = rad * ang.cos();
                let y = rad * ang.sin();
                let l = (x * x + y * y).sqrt();
                let (ox, oy) = if l > 0.0 { (y / l, -x / l) } else { (0.0, 0.0) };
                let f = perim * roundness * dir;
                p.push([c[0] + x, c[1] + y], [ox * f, oy * f], [-ox * f, -oy * f]);
                long = !long;
                ang += step * dir;
            }
            Some(p)
        }
        _ => None,
    }
}

fn gradient_paint(g: &Gradient, frame: f32, mat: &Mat, opacity: f32) -> Option<Paint> {
    if opacity <= 0.0 {
        return None;
    }
    let inv = mat.invert()?;
    let raw = g.stops.value(frame);
    let stops = GradientStops::from_raw(&raw, g.color_count);
    if stops.colors.is_empty() {
        return None;
    }
    let s = two(&g.start.value(frame));
    let e = two(&g.end.value(frame));
    Some(match g.kind {
        GradientKind::Linear => Paint::Linear {
            inv,
            start: s,
            end: e,
            stops,
            opacity,
        },
        GradientKind::Radial => {
            let rad = ((e[0] - s[0]).powi(2) + (e[1] - s[1]).powi(2)).sqrt();
            let mut percent = g.highlight_length.value(frame) / 100.0;
            if percent >= 1.0 {
                percent = 0.99;
            } else if percent <= -1.0 {
                percent = -0.99;
            }
            let dist = rad * percent;
            let ang = (e[1] - s[1]).atan2(e[0] - s[0]) + g.highlight_angle.value(frame).to_radians();
            let focal = [s[0] + ang.cos() * dist, s[1] + ang.sin() * dist];
            Paint::Radial {
                inv,
                center: s,
                radius: rad,
                focal,
                stops,
                opacity,
            }
        }
    })
}

/// Контуры обводки (в device space) для набора ломаных.
fn build_stroke(polys: &[Polyline], st: &StrokeStyle, frame: f32, mat: &Mat, out: &mut Vec<Polyline>) {
    let scale = mat.scale_factor();
    let width = st.width.value(frame) * scale;
    if width <= 0.0 {
        return;
    }
    let mut pattern: Vec<f32> = Vec::new();
    let mut dash_offset = 0.0f32;
    for (kind, v) in &st.dashes {
        let val = v.value(frame) * scale;
        match kind {
            DashKind::Offset => dash_offset = val,
            _ => pattern.push(val),
        }
    }
    let dashed = pattern.iter().any(|v| *v > 0.0);
    for poly in polys {
        if dashed {
            for piece in dash_polyline(poly, &pattern, dash_offset) {
                stroke_polyline(&piece, width, st.cap, st.join, st.miter_limit, TOLERANCE, out);
            }
        } else {
            stroke_polyline(poly, width, st.cap, st.join, st.miter_limit, TOLERANCE, out);
        }
    }
}
