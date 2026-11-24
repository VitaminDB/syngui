//! Отрисовка кадра композиции: слои снизу вверх, parenting матриц, маски,
//! track matte, прекомпозиции с time remap; фигуры собираются в контуры
//! (device space), стили (заливки/обводки) применяются к контурам выше по
//! списку — как в After Effects/lottie-web.

use std::f32::consts::{PI, TAU};

use super::geom::{
    dash_polyline, offset_polyline, pucker_bloat, stroke_polyline, trim_paths, twist_polyline,
    zigzag_path, zigzag_smooth, Hom, LineCap, LineJoin, Mat, Mat4, Polyline, ShapePath,
};
use super::model::{
    Composition, DashKind, Effect, Gradient, GradientKind, Layer, LayerKind, LayerMask, MaskMode,
    MatteMode, MergeMode, Shape, StrokeStyle, TextData, Transform,
};
use super::raster::{
    BlendMode, CoverageOp, FillRule, GradientStops, Mask, Paint, Pixmap, Rasterizer,
};

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
    /// Индекс merge-группы в `Collected::merges`.
    merge: Option<usize>,
}

/// Модификатор контура со значениями на текущем кадре.
#[derive(Clone, Copy)]
enum ModInst {
    Round(f32),
    Offset {
        amount: f32,
        join: LineJoin,
        miter: f32,
    },
    Zig {
        size: f32,
        ridges: f32,
        smooth: bool,
    },
    Pucker(f32),
    Twist {
        angle: f32,
        cx: f32,
        cy: f32,
    },
}

/// Контуры стиля `si`: без merge и по merge-группам.
fn style_paths(col: &Collected, si: usize) -> (Vec<usize>, Vec<(usize, Vec<usize>)>) {
    let mut plain = Vec::new();
    let mut groups: Vec<(usize, Vec<usize>)> = Vec::new();
    for (pi, p) in col.paths.iter().enumerate() {
        if !p.styles.contains(&si) {
            continue;
        }
        match p.merge {
            None => plain.push(pi),
            Some(g) => match groups.iter_mut().find(|(gid, _)| *gid == g) {
                Some((_, v)) => v.push(pi),
                None => groups.push((g, vec![pi])),
            },
        }
    }
    (plain, groups)
}

/// Камера списка слоёв: вид (мир → камера), зум и центр проекции.
struct Camera {
    view: Mat4,
    zoom: f32,
    center: [f32; 2],
}

/// Состояние отрисовки списка слоёв: мировые матрицы и камера.
struct Scene<'a> {
    world: Vec<Option<Mat4>>,
    camera: Option<Camera>,
    /// Композиция — для контекста выражений.
    comp: &'a Composition,
    has_expr: bool,
}

impl Scene<'_> {
    fn comp_ptr(&self) -> &Composition {
        self.comp
    }
}

struct Collected<'a> {
    styles: Vec<StyleInst<'a>>,
    trims: Vec<TrimInst>,
    paths: Vec<PathInst>,
    merges: Vec<MergeMode>,
}

#[inline]
fn two(v: &[f32]) -> [f32; 2] {
    [
        v.first().copied().unwrap_or(0.0),
        v.get(1).copied().unwrap_or(0.0),
    ]
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
        self.render_layers(
            &ctx,
            &comp.layers,
            frame,
            root,
            target,
            0,
            None,
            [comp.width, comp.height],
        );
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

    /// Мировая матрица слоя (4×4, с родителями) в координатах списка слоёв.
    fn layer_world(
        &self,
        layers: &[Layer],
        i: usize,
        frame: f32,
        scene: &mut Scene,
        depth: usize,
    ) -> Mat4 {
        if let Some(m) = scene.world[i] {
            return m;
        }
        let l = &layers[i];
        let _expr = scene
            .has_expr
            .then(|| super::expr::enter(scene.comp_ptr(), layers, i));
        // Ключевые кадры свойств заданы во времени содержащей композиции:
        // `st`/`sr` сдвигают только время дочерней прекомпозиции. Сверено с
        // lottie-web: PropertyFactory берёт `comp.renderedFrame − st` и
        // сравнивает с `t − st` (сдвиг сокращается), `sr` есть только в
        // CompElement.prepareFrame (`renderedFrame = num / sr`).
        let extra = if l.auto_orient {
            auto_orient_angle(&l.transform, frame)
        } else {
            0.0
        };
        let own = match &l.kind {
            LayerKind::Camera { .. } => camera_matrix(&l.transform, frame),
            _ if l.three_d => l.transform.matrix4(frame, extra),
            _ => Mat4::from_affine(&l.transform.matrix_with(frame, extra)),
        };
        let m = match l.parent {
            Some(p) if p != i && p < layers.len() && depth < 64 => self
                .layer_world(layers, p, frame, scene, depth + 1)
                .mul(&own),
            _ => own,
        };
        scene.world[i] = Some(m);
        m
    }

    /// Итоговая аффинная матрица слоя: 3D-слои при камере — перспективная
    /// проекция (линеаризованная в точке привязки), иначе — ортогональная.
    /// Точная гомография 3D-слоя при камере (локальные координаты →
    /// пиксели); `None` — слой рисуется аффинно.
    fn layer_homography(
        &self,
        layers: &[Layer],
        i: usize,
        frame: f32,
        root: &Mat,
        scene: &mut Scene,
    ) -> Option<Hom> {
        let cam = scene.camera.as_ref()?;
        let l = &layers[i];
        if !l.three_d {
            return None;
        }
        let (view, zoom, center) = (cam.view, cam.zoom, cam.center);
        let world = self.layer_world(layers, i, frame, scene, 0);
        let h = view.mul(&world).project_homography(zoom, center);
        Some(Hom::from_affine(root).mul(&h))
    }

    fn layer_matrix(
        &self,
        layers: &[Layer],
        i: usize,
        frame: f32,
        root: &Mat,
        scene: &mut Scene,
    ) -> Mat {
        let world = self.layer_world(layers, i, frame, scene, 0);
        let l = &layers[i];
        let local = match &scene.camera {
            Some(cam) if l.three_d => {
                let p0 = two(&l.transform.anchor.value(frame));
                cam.view
                    .mul(&world)
                    .project_affine(cam.zoom, cam.center, p0)
                    // За камерой — слой не виден.
                    .unwrap_or(Mat {
                        a: 0.0,
                        b: 0.0,
                        c: 0.0,
                        d: 0.0,
                        e: 0.0,
                        f: 0.0,
                    })
            }
            _ => world.ortho(),
        };
        root.mul(&local)
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
        size: [f32; 2],
    ) {
        if depth > MAX_DEPTH {
            return;
        }
        if clip.map(Mask::is_empty).unwrap_or(false) {
            return;
        }
        let mut scene = Scene {
            world: vec![None; layers.len()],
            camera: None,
            comp: ctx.comp,
            has_expr: ctx.comp.has_expressions,
        };
        // Активная камера списка — верхняя видимая (как в After Effects).
        if let Some(ci) = layers.iter().position(|l| {
            matches!(l.kind, LayerKind::Camera { .. }) && !l.hidden && l.in_range(frame)
        }) {
            if let LayerKind::Camera { zoom } = &layers[ci].kind {
                let world = self.layer_world(layers, ci, frame, &mut scene, 0);
                scene.camera = Some(Camera {
                    view: world.rigid_inverse(),
                    zoom: zoom.value(frame),
                    center: [size[0] / 2.0, size[1] / 2.0],
                });
            }
        }
        for i in (0..layers.len()).rev() {
            let l = &layers[i];
            if l.hidden || l.is_matte_source || !l.in_range(frame) {
                continue;
            }
            if matches!(
                l.kind,
                LayerKind::Null | LayerKind::Unsupported | LayerKind::Camera { .. }
            ) {
                continue;
            }
            let mat = self.layer_matrix(layers, i, frame, root, &mut scene);
            self.render_layer(
                ctx, layers, i, frame, mat, root, target, depth, &mut scene, clip,
            );
        }
    }

    /// Собственный клип слоя: внешний клип ∩ прямоугольник прекомпозиции ∩
    /// маски слоя. `None` — слой ничего не добавляет, действует внешний клип
    /// (без копирования); `Some(пустая)` — слой целиком невидим.
    fn layer_clip(
        &mut self,
        ctx: &Ctx,
        l: &Layer,
        frame: f32,
        mat: &Mat,
        outer: Option<&Mask>,
    ) -> Option<Mask> {
        let mut own: Option<Mask> = None;
        let region = |c: Option<&Mask>| -> [usize; 4] {
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
            let r = region(outer);
            if !rect_covers_region(mat, l.width, l.height, r) {
                let poly = rect_polyline(0.0, 0.0, l.width, l.height, mat);
                let rect = self
                    .rast
                    .fill(&[poly], FillRule::NonZero, ctx.w, ctx.h)
                    .unwrap_or_default();
                own = Some(match outer {
                    None => rect,
                    Some(c) => {
                        let m = self.rast.intersect(c, &rect);
                        self.rast.recycle(rect);
                        m
                    }
                });
            }
        }
        if !l.masks.is_empty() {
            let base = own.as_ref().or(outer);
            let r = region(base);
            let masks = self.layer_masks(ctx, &l.masks, frame, mat, r);
            let combined = match base {
                None => masks,
                Some(c) => {
                    let m = self.rast.intersect(c, &masks);
                    self.rast.recycle(masks);
                    m
                }
            };
            if let Some(old) = own.take() {
                self.rast.recycle(old);
            }
            own = Some(combined);
        }
        own
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
        scene: &mut Scene,
        outer_clip: Option<&Mask>,
    ) {
        let l = &layers[i];
        let _expr = ctx
            .comp
            .has_expressions
            .then(|| super::expr::enter(ctx.comp, layers, i));
        let opacity = l.transform.opacity_at(frame);
        if opacity <= 0.0 {
            return;
        }
        let own_clip = self.layer_clip(ctx, l, frame, &mat, outer_clip);
        if own_clip.as_ref().map(Mask::is_empty).unwrap_or(false) {
            if let Some(m) = own_clip {
                self.rast.recycle(m);
            }
            return;
        }
        let clip_ref = own_clip.as_ref().or(outer_clip);
        // Перспектива: слой рисуется аффинным приближением и деформируется
        // точной гомографией плоскости.
        let warp = self.layer_homography(layers, i, frame, root, scene);
        let needs_buffer = warp.is_some()
            || l.effects
                .iter()
                .any(|e| matches!(e, Effect::MotionBlur { .. }))
            || l.matte.is_some()
            || opacity < 1.0
            || l.blend != BlendMode::Normal
            || !l.effects.is_empty()
            || !l.styles.is_empty();
        if !needs_buffer {
            self.draw_content(ctx, l, frame, &mat, target, depth, clip_ref);
            if let Some(m) = own_clip {
                self.rast.recycle(m);
            }
            return;
        }
        let mut buf = self.take(ctx.w, ctx.h);
        let blur = l.effects.iter().find_map(|e| match e {
            Effect::MotionBlur { samples, shutter } => Some((
                samples.value(frame).round().clamp(1.0, 32.0) as usize,
                shutter.value(frame),
            )),
            _ => None,
        });
        match blur {
            Some((n, shutter)) if n > 1 && shutter > 0.0 => {
                // Затвор по центру кадра: моменты frame ± shutter/720.
                let span = shutter / 360.0;
                let mut tmp = self.take(ctx.w, ctx.h);
                for k in 0..n {
                    let f = frame + span * (k as f32 / (n - 1) as f32 - 0.5);
                    let m = self.layer_matrix(
                        layers,
                        i,
                        f,
                        root,
                        &mut Scene {
                            world: vec![None; layers.len()],
                            camera: None,
                            comp: ctx.comp,
                            has_expr: ctx.comp.has_expressions,
                        },
                    );
                    tmp.clear();
                    self.draw_content(ctx, l, f, &m, &mut tmp, depth, clip_ref);
                    buf.accumulate(&tmp, 1.0 / n as f32);
                }
                self.recycle(tmp);
            }
            _ => self.draw_content(ctx, l, frame, &mat, &mut buf, depth, clip_ref),
        }
        if !l.effects.is_empty() {
            self.apply_effects(ctx, l, frame, &mat, &mut buf);
        }
        if !l.styles.is_empty() && buf.dirty.is_some() {
            self.apply_styles(ctx, l, frame, &mat, &mut buf);
        }
        if let (Some(h), Some(ainv)) = (warp, mat.invert()) {
            // Пиксель буфера → локальная точка (обратная аффинной) → холст.
            let fwd = h.mul(&Hom::from_affine(&ainv));
            let mut wbuf = self.take(ctx.w, ctx.h);
            wbuf.warp_from(&buf, &fwd);
            std::mem::swap(&mut buf, &mut wbuf);
            self.recycle(wbuf);
        }
        if buf.dirty.is_some() {
            if let (Some(mode), Some(src)) = (l.matte, l.matte_source) {
                if src < layers.len() && src != i {
                    let ml = &layers[src];
                    let mut mbuf = self.take(ctx.w, ctx.h);
                    if !ml.hidden && ml.in_range(frame) {
                        let mm = self.layer_matrix(layers, src, frame, root, scene);
                        self.render_layer(
                            ctx, layers, src, frame, mm, root, &mut mbuf, depth, scene, outer_clip,
                        );
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
        if let Some(m) = own_clip {
            self.rast.recycle(m);
        }
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
                    self.rast.recycle(m);
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
                let (aw, ah) = if asset.width > 0.0 && asset.height > 0.0 {
                    (asset.width, asset.height)
                } else {
                    (iw, ih)
                };
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
                    self.rast.recycle(m);
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
                self.render_layers(
                    ctx,
                    children,
                    child_frame,
                    mat,
                    target,
                    depth + 1,
                    clip,
                    [l.width, l.height],
                );
            }
            LayerKind::Text(td) => self.draw_text(ctx, l, td, frame, mat, target, clip),
            LayerKind::Null | LayerKind::Unsupported | LayerKind::Camera { .. } => {}
        }
    }

    /// Совокупное покрытие масок слоя. Для прямых масок режимов
    /// add/intersect/lighten/darken область — объединение их охватов,
    /// иначе (subtract, inverted, difference) — вся `region`.
    fn layer_masks(
        &mut self,
        ctx: &Ctx,
        masks: &[LayerMask],
        frame: f32,
        mat: &Mat,
        region: [usize; 4],
    ) -> Mask {
        let mut covs: Vec<Option<Mask>> = Vec::with_capacity(masks.len());
        let bounded = masks.iter().all(|m| {
            !m.inverted
                && matches!(
                    m.mode,
                    MaskMode::Add | MaskMode::Intersect | MaskMode::Lighten | MaskMode::Darken
                )
        });
        let mut bb: Option<[usize; 4]> = None;
        for m in masks {
            let mut path = m.path.value(frame);
            path.closed = true;
            path.transform(mat);
            let mut poly = path.flatten(TOLERANCE);
            let x = m.expand.value(frame);
            if x.abs() > 1e-4 {
                poly = offset_polyline(&poly, x * mat.scale_factor(), LineJoin::Round, 4.0);
            }
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
                    Some(b) => [
                        b[0].min(r[0]),
                        b[1].min(r[1]),
                        b[2].max(r[2]),
                        b[3].max(r[3]),
                    ],
                });
            }
            covs.push(cov);
        }
        let mut r = region;
        if bounded {
            match bb {
                Some(b) => {
                    r = [
                        r[0].max(b[0]),
                        r[1].max(b[1]),
                        r[2].min(b[2]),
                        r[3].min(b[3]),
                    ];
                }
                None => r = [0, 0, 0, 0],
            }
        }
        let [rx0, ry0, rx1, ry1] = r;
        let (w, h) = (rx1.saturating_sub(rx0), ry1.saturating_sub(ry0));
        if w == 0 || h == 0 {
            for c in covs.into_iter().flatten() {
                self.rast.recycle(c);
            }
            return Mask {
                x0: rx0 as i32,
                y0: ry0 as i32,
                w,
                h,
                cov: Vec::new(),
            };
        }
        let first_full = matches!(masks[0].mode, MaskMode::Subtract | MaskMode::Intersect);
        let mut acc = self.rast.new_mask(
            rx0 as i32,
            ry0 as i32,
            w,
            h,
            if first_full { 1.0 } else { 0.0 },
        );
        let mut tmp = vec![0f32; w];
        for (m, cov) in masks.iter().zip(&covs) {
            let op = (m.opacity.value(frame) / 100.0).clamp(0.0, 1.0);
            for y in 0..h {
                // Строка покрытия маски на отрезке acc: вне охвата — ноль.
                tmp.fill(0.0);
                if let Some((sx, src)) = cov
                    .as_ref()
                    .and_then(|c| c.row_span((y + ry0) as i32, rx0 as i32, rx1 as i32))
                {
                    let off = sx as usize - rx0;
                    tmp[off..off + src.len()].copy_from_slice(src);
                }
                let row = &mut acc.cov[y * w..(y + 1) * w];
                for (r, &c0) in row.iter_mut().zip(tmp.iter()) {
                    let c = if m.inverted { 1.0 - c0 } else { c0 } * op;
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
        for c in covs.into_iter().flatten() {
            self.rast.recycle(c);
        }
        acc
    }

    fn draw_shapes(
        &mut self,
        ctx: &Ctx,
        items: &[Shape],
        frame: f32,
        mat: &Mat,
        target: &mut Pixmap,
        clip: Option<&Mask>,
    ) {
        let mut col = Collected {
            styles: Vec::new(),
            trims: Vec::new(),
            paths: Vec::new(),
            merges: Vec::new(),
        };
        collect_shapes(items, frame, *mat, 1.0, &CollectCtx::default(), &mut col, 0);
        apply_trims(&mut col);
        let mut polys: Vec<Polyline> = Vec::new();
        let mut stroke_polys: Vec<Polyline> = Vec::new();
        for si in 0..col.styles.len() {
            let style = &col.styles[si];
            // Обводки режут контуры как есть (merge не влияет на штрихи).
            polys.clear();
            for p in col.paths.iter().filter(|p| p.styles.contains(&si)) {
                polys.extend(p.polys.iter().cloned());
            }
            if polys.is_empty() {
                continue;
            }
            match style.shape {
                Shape::Fill {
                    color,
                    opacity,
                    rule,
                } => {
                    let c = color.value(frame);
                    let a = opacity.value(frame) / 100.0
                        * c.get(3).copied().unwrap_or(1.0)
                        * style.opacity;
                    if a <= 0.0 {
                        continue;
                    }
                    let paint = Paint::solid(
                        [
                            c[0],
                            c.get(1).copied().unwrap_or(0.0),
                            c.get(2).copied().unwrap_or(0.0),
                        ],
                        a,
                    );
                    if let Some(m) = self.style_fill_mask(ctx, &col, si, *rule) {
                        target.fill_mask(&m, &paint, clip);
                        self.rast.recycle(m);
                    }
                }
                Shape::GradientFill {
                    gradient,
                    opacity,
                    rule,
                } => {
                    let a = opacity.value(frame) / 100.0 * style.opacity;
                    let Some(paint) = gradient_paint(gradient, frame, &style.mat, a) else {
                        continue;
                    };
                    if let Some(m) = self.style_fill_mask(ctx, &col, si, *rule) {
                        target.fill_mask(&m, &paint, clip);
                        self.rast.recycle(m);
                    }
                }
                Shape::Stroke {
                    color,
                    opacity,
                    style: st,
                } => {
                    let c = color.value(frame);
                    let a = opacity.value(frame) / 100.0
                        * c.get(3).copied().unwrap_or(1.0)
                        * style.opacity;
                    if a <= 0.0 {
                        continue;
                    }
                    let paint = Paint::solid(
                        [
                            c[0],
                            c.get(1).copied().unwrap_or(0.0),
                            c.get(2).copied().unwrap_or(0.0),
                        ],
                        a,
                    );
                    stroke_polys.clear();
                    if col
                        .paths
                        .iter()
                        .any(|p| p.merge.is_some() && p.styles.contains(&si))
                    {
                        let mut src = Vec::new();
                        self.stroke_source(ctx, &col, si, &mut src);
                        build_stroke(&src, st, frame, &style.mat, &mut stroke_polys);
                    } else {
                        build_stroke(&polys, st, frame, &style.mat, &mut stroke_polys);
                    }
                    if let Some(m) = self
                        .rast
                        .fill(&stroke_polys, FillRule::NonZero, ctx.w, ctx.h)
                    {
                        target.fill_mask(&m, &paint, clip);
                        self.rast.recycle(m);
                    }
                }
                Shape::GradientStroke {
                    gradient,
                    opacity,
                    style: st,
                } => {
                    let a = opacity.value(frame) / 100.0 * style.opacity;
                    let Some(paint) = gradient_paint(gradient, frame, &style.mat, a) else {
                        continue;
                    };
                    stroke_polys.clear();
                    if col
                        .paths
                        .iter()
                        .any(|p| p.merge.is_some() && p.styles.contains(&si))
                    {
                        let mut src = Vec::new();
                        self.stroke_source(ctx, &col, si, &mut src);
                        build_stroke(&src, st, frame, &style.mat, &mut stroke_polys);
                    } else {
                        build_stroke(&polys, st, frame, &style.mat, &mut stroke_polys);
                    }
                    if let Some(m) = self
                        .rast
                        .fill(&stroke_polys, FillRule::NonZero, ctx.w, ctx.h)
                    {
                        target.fill_mask(&m, &paint, clip);
                        self.rast.recycle(m);
                    }
                }
                _ => {}
            }
        }
    }

    /// Маска заливки стиля: контуры без merge заливаются вместе, каждая
    /// merge-группа собирается булевыми операциями по покрытию (сверху вниз,
    /// как в skottie) и объединяется с остальным.
    fn style_fill_mask(
        &mut self,
        ctx: &Ctx,
        col: &Collected,
        si: usize,
        rule: FillRule,
    ) -> Option<Mask> {
        let (plain, groups) = style_paths(col, si);
        let plain: Vec<Polyline> = plain
            .iter()
            .flat_map(|&pi| col.paths[pi].polys.iter().cloned())
            .collect();
        let mut total = if plain.is_empty() {
            None
        } else {
            self.rast.fill(&plain, rule, ctx.w, ctx.h)
        };
        for (gid, idxs) in groups {
            let acc = self.merge_group_mask(ctx, col, gid, &idxs, rule);
            total = self.rast.combine(total, acc, CoverageOp::Union);
        }
        total
    }

    /// Покрытие merge-группы: булевы операции сверху вниз по списку
    /// (в обратном порядке добавления контуров), как в skottie.
    fn merge_group_mask(
        &mut self,
        ctx: &Ctx,
        col: &Collected,
        gid: usize,
        idxs: &[usize],
        rule: FillRule,
    ) -> Option<Mask> {
        let op = match col.merges[gid] {
            MergeMode::Add | MergeMode::Merge => CoverageOp::Union,
            MergeMode::Subtract => CoverageOp::Subtract,
            MergeMode::Intersect => CoverageOp::Intersect,
            MergeMode::Exclude => CoverageOp::Xor,
        };
        let mut acc: Option<Mask> = None;
        for (k, &pi) in idxs.iter().rev().enumerate() {
            let m = self.rast.fill(&col.paths[pi].polys, rule, ctx.w, ctx.h);
            acc = if k == 0 {
                m
            } else {
                self.rast.combine(acc, m, op)
            };
        }
        acc
    }

    /// Контуры для обводки стиля: обычные контуры как есть, merge-группы —
    /// граница результата булевой операции (изолиния покрытия).
    fn stroke_source(&mut self, ctx: &Ctx, col: &Collected, si: usize, out: &mut Vec<Polyline>) {
        out.clear();
        let (plain, groups) = style_paths(col, si);
        for pi in plain {
            out.extend(col.paths[pi].polys.iter().cloned());
        }
        for (gid, idxs) in groups {
            if let Some(m) = self.merge_group_mask(ctx, col, gid, &idxs, FillRule::NonZero) {
                out.extend(m.contours(0.5));
                self.rast.recycle(m);
            }
        }
    }

    /// Текстовый слой из встроенных глифов (`chars`, в том числе
    /// подставленных из системных шрифтов): раскладка строк, аниматоры
    /// (range-селекторы по буквам/словам/строкам) и текст по контуру маски.
    #[allow(clippy::too_many_arguments)]
    fn draw_text(
        &mut self,
        ctx: &Ctx,
        layer: &Layer,
        td: &TextData,
        frame: f32,
        mat: &Mat,
        target: &mut Pixmap,
        clip: Option<&Mask>,
    ) {
        let Some(doc) = td.doc_at(frame) else {
            return;
        };
        let source = text_source(td, doc, frame);
        if source.is_empty() {
            return;
        }
        let (family, style, ascent) = match ctx.comp.font(&doc.font) {
            Some(f) => (f.family.as_str(), f.style.as_str(), f.ascent),
            None => ("", "", 75.0),
        };
        let k = doc.size / 100.0;
        let (box_x, box_y) = match doc.box_pos {
            // Параграфный текст: первая базовая линия под верхом бокса.
            Some(ps) => (ps[0], ps[1] + ascent * k),
            None => (0.0, 0.0),
        };
        let text = source.replace("\r\n", "\r");
        let lines: Vec<&str> = text.split(['\r', '\n', '\u{3}']).collect();

        // Буквы и их индексы для селекторов: [буква, без пробелов, слово, строка].
        struct Letter<'g> {
            glyph: Option<&'g super::model::Glyph>,
            adv: f32,
            line: usize,
            idx: [usize; 4],
        }
        let mut letters: Vec<Letter> = Vec::new();
        let mut cnt = [0usize; 3];
        let mut missing = false;
        for (li, line) in lines.iter().enumerate() {
            let n = line.chars().count();
            for (ci, ch) in line.chars().enumerate() {
                let space = ch.is_whitespace();
                let mut buf = [0u8; 4];
                let g = ctx.comp.glyph(family, style, ch.encode_utf8(&mut buf));
                let adv = match g {
                    Some(g) => g.advance * k,
                    None => {
                        if !space {
                            missing = true;
                        }
                        doc.size * 0.33
                    }
                };
                letters.push(Letter {
                    glyph: g,
                    adv,
                    line: li,
                    idx: [cnt[0], cnt[1], cnt[2], li],
                });
                cnt[0] += 1;
                if !space {
                    cnt[1] += 1;
                }
                if space || ci + 1 == n {
                    cnt[2] += 1;
                }
            }
        }
        if missing {
            log::debug!("lottie: в `chars` нет части глифов текста `{}`", doc.text);
        }
        if letters.is_empty() {
            return;
        }
        let totals = [cnt[0], cnt[1], cnt[2], lines.len()];

        // Веса аниматоров по буквам.
        let na = td.animators.len();
        let mut mults = vec![1.0f32; letters.len() * na];
        for (ai, an) in td.animators.iter().enumerate() {
            let Some(sel) = &an.selector else {
                continue;
            };
            let unit = match sel.based_on {
                super::model::TextBasedOn::Characters => 0,
                super::model::TextBasedOn::CharactersExcludingSpaces => 1,
                super::model::TextBasedOn::Words => 2,
                super::model::TextBasedOn::Lines => 3,
            };
            let ev = SelectorEval::new(sel, frame, totals[unit]);
            for (li, l) in letters.iter().enumerate() {
                mults[li * na + ai] = ev.weight(l.idx[unit]);
            }
        }

        let base_tracking = doc.tracking / 1000.0 * doc.size;
        // Трекинг аниматоров — как у документа, в тысячных долях кегля.
        let extra: Vec<f32> = (0..letters.len())
            .map(|li| {
                td.animators
                    .iter()
                    .enumerate()
                    .filter_map(|(ai, an)| {
                        an.props
                            .tracking
                            .as_ref()
                            .map(|t| t.value(frame) / 1000.0 * doc.size * mults[li * na + ai])
                    })
                    .sum()
            })
            .collect();
        let mut line_width = vec![0.0f32; lines.len()];
        for (li, l) in letters.iter().enumerate() {
            line_width[l.line] += l.adv + base_tracking + extra[li];
        }

        // Контур для текста по маске (локальные координаты слоя).
        let path = td.path.as_ref().and_then(|p| {
            let sp = layer.mask_paths.get(p.mask)?.value(frame);
            let tol = TOLERANCE / mat.scale_factor().max(1e-6);
            let mut poly = sp.flatten(tol);
            if p.reversed.value(frame) != 0.0 {
                poly.pts.reverse();
            }
            let len = poly.length();
            (len > 0.0).then_some((p, poly, len))
        });

        let align = two(&td.anchor_align.value(frame));
        let mut pen_x = 0.0f32;
        let mut cur_line = usize::MAX;
        let mut spread = 0.0f32;
        // Пакеты букв с одинаковым стилем: заливка, обводка, ширина обводки.
        let mut batch_key: Option<[f32; 9]> = None;
        let mut batch_polys: Vec<Polyline> = Vec::new();
        for (li, l) in letters.iter().enumerate() {
            if l.line != cur_line {
                cur_line = l.line;
                let width = line_width[l.line];
                pen_x = match &path {
                    Some((p, _, len)) => {
                        let f = p.first_margin.value(frame);
                        let lm = p.last_margin.value(frame);
                        let free = len - f - lm - width;
                        spread = 0.0;
                        if p.force_align.value(frame) != 0.0 {
                            let n = letters.iter().filter(|x| x.line == l.line).count();
                            if n > 1 {
                                spread = free / (n - 1) as f32;
                            }
                            f
                        } else {
                            match doc.justify {
                                1 => f + free,
                                2 => f + free / 2.0,
                                _ => f,
                            }
                        }
                    }
                    None => {
                        box_x
                            + match doc.justify {
                                1 => -width,
                                2 => -width / 2.0,
                                _ => 0.0,
                            }
                    }
                };
            }
            // Свойства аниматоров для буквы.
            let mut anchor = [0.0f32; 2];
            let mut pos = [0.0f32; 2];
            let mut scale = [1.0f32; 2];
            let mut rot = 0.0f32;
            let mut skew = 0.0f32;
            let mut skew_axis = 0.0f32;
            let mut opacity = 100.0f32;
            let mut fill = doc.fill.clone();
            let mut stroke = doc.stroke.clone();
            let mut sw = doc.stroke_width;
            for (ai, an) in td.animators.iter().enumerate() {
                let m = mults[li * na + ai];
                if m == 0.0 {
                    continue;
                }
                let pr = &an.props;
                if let Some(a) = &pr.anchor {
                    let v = two(&a.value(frame));
                    anchor[0] += v[0] * m;
                    anchor[1] += v[1] * m;
                }
                if let Some(p) = &pr.position {
                    let v = two(&p.value(frame));
                    pos[0] += v[0] * m;
                    pos[1] += v[1] * m;
                }
                if let Some(sc) = &pr.scale {
                    let v = sc.value(frame);
                    let sx = v.first().copied().unwrap_or(100.0) / 100.0;
                    let sy = v.get(1).copied().unwrap_or(sx * 100.0) / 100.0;
                    scale[0] *= 1.0 + (sx - 1.0) * m;
                    scale[1] *= 1.0 + (sy - 1.0) * m;
                }
                if let Some(r) = &pr.rotation {
                    rot += r.value(frame) * m;
                }
                if let Some(sk) = &pr.skew {
                    skew += sk.value(frame) * m;
                }
                if let Some(sa) = &pr.skew_axis {
                    skew_axis += sa.value(frame) * m;
                }
                if let Some(o) = &pr.opacity {
                    opacity += (o.value(frame) * m - opacity) * m;
                }
                if let Some(c) = &pr.fill {
                    let c = c.value(frame);
                    let f = fill.get_or_insert_with(|| c.clone());
                    for (fi, cv) in f.iter_mut().zip(c.iter()).take(3) {
                        *fi += (cv - *fi) * m;
                    }
                }
                if let Some(c) = &pr.stroke {
                    let c = c.value(frame);
                    let f = stroke.get_or_insert_with(|| c.clone());
                    for (fi, cv) in f.iter_mut().zip(c.iter()).take(3) {
                        *fi += (cv - *fi) * m;
                    }
                }
                if let Some(w) = &pr.stroke_width {
                    sw += w.value(frame) * m;
                }
                if let Some(f) = fill.as_mut() {
                    let hue = pr.fill_hue.as_ref().map(|v| v.value(frame) * m);
                    let sat = pr
                        .fill_saturation
                        .as_ref()
                        .map(|v| v.value(frame) / 100.0 * m);
                    let bri = pr
                        .fill_brightness
                        .as_ref()
                        .map(|v| v.value(frame) / 100.0 * m);
                    if hue.is_some() || sat.is_some() || bri.is_some() {
                        adjust_hsv(
                            f,
                            hue.unwrap_or(0.0),
                            sat.unwrap_or(0.0),
                            bri.unwrap_or(0.0),
                        );
                    }
                }
            }
            let glyph_adv = l.adv;
            let half = glyph_adv / 2.0;
            let ax = align[0] / 100.0 * half;
            let ay = align[1] / 100.0 * doc.size;
            let line_y = l.line as f32 * doc.line_height - doc.baseline_shift;
            let base = match &path {
                Some((p, poly, _)) => {
                    let (pt, ang) = poly.point_at(pen_x + half).unwrap_or(([0.0; 2], 0.0));
                    let mut m = mat.mul(&Mat::translate(pt[0], pt[1]));
                    if p.perpendicular.value(frame) != 0.0 {
                        m = m.mul(&Mat::rotate(ang.to_degrees()));
                    }
                    m.mul(&Mat::translate(0.0, line_y))
                }
                None => mat.mul(&Mat::translate(pen_x + half, box_y + line_y)),
            };
            pen_x += glyph_adv + base_tracking + extra[li] + spread;
            let alpha = (opacity / 100.0).clamp(0.0, 1.0);
            let Some(g) = l.glyph else {
                continue;
            };
            if alpha <= 0.0 || (scale[0] == 0.0 && scale[1] == 0.0) {
                continue;
            }
            let anim = super::geom::compose_transform(anchor, pos, scale, rot, skew, skew_axis);
            let gm = base
                .mul(&Mat::translate(ax, ay))
                .mul(&anim)
                .mul(&Mat::translate(-ax - half, -ay))
                .mul(&Mat::scale(k, k));
            let rgba = |c: &Option<Vec<f32>>| -> [f32; 4] {
                match c {
                    Some(c) => [
                        c.first().copied().unwrap_or(0.0),
                        c.get(1).copied().unwrap_or(0.0),
                        c.get(2).copied().unwrap_or(0.0),
                        c.get(3).copied().unwrap_or(1.0) * alpha,
                    ],
                    None => [0.0; 4],
                }
            };
            let f = rgba(&fill);
            let st = rgba(&stroke);
            let key = [
                f[0],
                f[1],
                f[2],
                f[3],
                st[0],
                st[1],
                st[2],
                st[3],
                sw * gm.scale_factor() / k.max(1e-6),
            ];
            if batch_key.is_some_and(|b| b != key) {
                self.flush_text_batch(
                    ctx,
                    doc,
                    batch_key.take().unwrap(),
                    &mut batch_polys,
                    target,
                    clip,
                );
            }
            batch_key = Some(key);
            let mut col = Collected {
                styles: Vec::new(),
                trims: Vec::new(),
                paths: Vec::new(),
                merges: Vec::new(),
            };
            collect_shapes(&g.shapes, 0.0, gm, 1.0, &CollectCtx::default(), &mut col, 0);
            batch_polys.extend(col.paths.into_iter().flat_map(|p| p.polys));
        }
        if let Some(key) = batch_key {
            self.flush_text_batch(ctx, doc, key, &mut batch_polys, target, clip);
        }
    }

    /// Отрисовать накопленные буквы одного стиля: `key` — заливка RGBA,
    /// обводка RGBA и ширина обводки в пикселях.
    fn flush_text_batch(
        &mut self,
        ctx: &Ctx,
        doc: &super::model::TextDoc,
        key: [f32; 9],
        polys: &mut Vec<Polyline>,
        target: &mut Pixmap,
        clip: Option<&Mask>,
    ) {
        if polys.is_empty() {
            return;
        }
        let paint_fill = |r: &mut Self, target: &mut Pixmap, c: [f32; 4], ps: &[Polyline]| {
            if c[3] <= 0.0 || ps.is_empty() {
                return;
            }
            let paint = Paint::solid([c[0], c[1], c[2]], c[3]);
            if let Some(m) = r.rast.fill(ps, FillRule::NonZero, ctx.w, ctx.h) {
                target.fill_mask(&m, &paint, clip);
                r.rast.recycle(m);
            }
        };
        let fill = [key[0], key[1], key[2], key[3]];
        let stroke = [key[4], key[5], key[6], key[7]];
        let width = key[8];
        let stroke_polys: Vec<Polyline> = if stroke[3] > 0.0 && width > 0.0 {
            let mut out = Vec::new();
            for p in polys.iter() {
                stroke_polyline(
                    p,
                    width,
                    LineCap::Round,
                    LineJoin::Round,
                    4.0,
                    TOLERANCE,
                    &mut out,
                );
            }
            out
        } else {
            Vec::new()
        };
        if doc.stroke_over_fill {
            paint_fill(self, target, fill, polys);
            paint_fill(self, target, stroke, &stroke_polys);
        } else {
            paint_fill(self, target, stroke, &stroke_polys);
            paint_fill(self, target, fill, polys);
        }
        polys.clear();
    }

    /// Применить эффекты слоя к его буферу (до matte и смешивания).
    fn apply_effects(&mut self, ctx: &Ctx, l: &Layer, frame: f32, mat: &Mat, buf: &mut Pixmap) {
        let scale = mat.scale_factor().max(1e-6);
        for e in &l.effects {
            if buf.dirty.is_none() {
                return;
            }
            match e {
                Effect::Fill { color, opacity } => {
                    let c = color.value(frame);
                    let mut op = opacity.value(frame);
                    if op > 1.0 {
                        // Некоторые экспортёры пишут проценты.
                        op /= 100.0;
                    }
                    buf.effect_fill(
                        [
                            c.first().copied().unwrap_or(0.0),
                            c.get(1).copied().unwrap_or(0.0),
                            c.get(2).copied().unwrap_or(0.0),
                        ],
                        op.clamp(0.0, 1.0),
                    );
                }
                Effect::Tint {
                    black,
                    white,
                    amount,
                } => {
                    let b = black.value(frame);
                    let w = white.value(frame);
                    let three = |v: &[f32]| {
                        [
                            v.first().copied().unwrap_or(0.0),
                            v.get(1).copied().unwrap_or(0.0),
                            v.get(2).copied().unwrap_or(0.0),
                        ]
                    };
                    buf.effect_tint(three(&b), three(&w), amount.value(frame) / 100.0);
                }
                Effect::GaussianBlur {
                    blurriness,
                    dimensions,
                } => {
                    let sigma = blurriness.value(frame) * 0.25 * scale;
                    let (sx, sy) = match dimensions.value(frame).round() as i32 {
                        2 => (sigma, 0.0),
                        3 => (0.0, sigma),
                        _ => (sigma, sigma),
                    };
                    buf.blur(sx, sy);
                }
                Effect::DropShadow {
                    color,
                    opacity,
                    direction,
                    distance,
                    softness,
                    shadow_only,
                } => {
                    let c = color.value(frame);
                    let op = (opacity.value(frame) / 255.0).clamp(0.0, 1.0);
                    let rad = (direction.value(frame) - 90.0).to_radians();
                    let dist = distance.value(frame) * scale;
                    let (dx, dy) = (
                        (rad.cos() * dist).round() as i32,
                        (rad.sin() * dist).round() as i32,
                    );
                    let sigma = softness.value(frame) * 0.25 * scale;
                    let mut sbuf = self.take(ctx.w, ctx.h);
                    sbuf.shadow_of(
                        buf,
                        [
                            c.first().copied().unwrap_or(0.0),
                            c.get(1).copied().unwrap_or(0.0),
                            c.get(2).copied().unwrap_or(0.0),
                        ],
                        op,
                        dx,
                        dy,
                    );
                    sbuf.blur(sigma, sigma);
                    if !*shadow_only {
                        sbuf.composite(buf, 1.0, BlendMode::Normal);
                    }
                    std::mem::swap(buf, &mut sbuf);
                    self.recycle(sbuf);
                }
                Effect::Invert { blend } => buf.effect_invert(blend.value(frame) / 100.0),
                Effect::MotionBlur { .. } => {}
                Effect::Tritone {
                    highlights,
                    midtones,
                    shadows,
                    blend,
                } => {
                    buf.effect_tritone(
                        rgb3(&highlights.value(frame)),
                        rgb3(&midtones.value(frame)),
                        rgb3(&shadows.value(frame)),
                        blend.value(frame) / 100.0,
                    );
                }
                Effect::Levels { channels } => {
                    let mut ch = [[0.0, 1.0, 1.0, 0.0, 1.0]; 5];
                    for (dst, src) in ch.iter_mut().zip(channels) {
                        for (d, v) in dst.iter_mut().zip(src) {
                            *d = v.value(frame);
                        }
                    }
                    buf.effect_levels(&ch);
                }
                Effect::RadialWipe {
                    completion,
                    start_angle,
                    center,
                    direction,
                    feather,
                } => {
                    let c = mat.apply(two(&center.value(frame)));
                    buf.radial_wipe(
                        c,
                        start_angle.value(frame),
                        completion.value(frame) / 100.0,
                        *direction as i32,
                        feather.value(frame) * scale,
                    );
                }
                Effect::Stroke {
                    mask,
                    all_masks,
                    color,
                    brush_size,
                    opacity,
                    start,
                    end,
                    paint_style,
                } => {
                    let paths: Vec<&super::anim::Animated<ShapePath>> = if *all_masks {
                        l.mask_paths.iter().collect()
                    } else {
                        l.mask_paths.get(mask - 1).into_iter().collect()
                    };
                    let mut polys = Vec::new();
                    for p in paths {
                        let mut sp = p.value(frame);
                        sp.transform(mat);
                        polys.push(sp.flatten(TOLERANCE));
                    }
                    let (s0, s1) = (start.value(frame) / 100.0, end.value(frame) / 100.0);
                    if s0 > 0.0 || s1 < 1.0 {
                        polys = trim_paths(polys, s0, s1, 0.0, false);
                    }
                    let width = brush_size.value(frame) * scale;
                    let mut sp = Vec::new();
                    for p in &polys {
                        stroke_polyline(
                            p,
                            width,
                            LineCap::Round,
                            LineJoin::Round,
                            4.0,
                            TOLERANCE,
                            &mut sp,
                        );
                    }
                    let mut op = opacity.value(frame);
                    if op > 1.0 {
                        op /= 100.0;
                    }
                    let cov = self.rast.fill(&sp, FillRule::NonZero, ctx.w, ctx.h);
                    match paint_style {
                        3 => match &cov {
                            Some(m) => buf.multiply_coverage(m),
                            None => buf.clear(),
                        },
                        style => {
                            if *style == 2 {
                                buf.clear();
                            }
                            if let Some(m) = &cov {
                                buf.fill_mask(
                                    m,
                                    &Paint::solid(rgb3(&color.value(frame)), op.clamp(0.0, 1.0)),
                                    None,
                                );
                            }
                        }
                    }
                    if let Some(m) = cov {
                        self.rast.recycle(m);
                    }
                }
            }
        }
    }

    /// Стили слоя (layer styles): тени и свечения под слоем и внутри него,
    /// обводка снаружи, заливка цветом. Порядок — как в After Effects:
    /// снизу тень, внешнее свечение, обводка; поверх содержимого — цвет,
    /// внутреннее свечение, внутренняя тень.
    fn apply_styles(&mut self, ctx: &Ctx, l: &Layer, frame: f32, mat: &Mat, buf: &mut Pixmap) {
        use super::model::LayerStyle as St;
        let scale = mat.scale_factor().max(1e-6);
        let mut src = self.take(ctx.w, ctx.h);
        src.copy_from(buf);
        // Содержимое: перекраска и внутренние эффекты.
        for st in &l.styles {
            if let St::ColorOverlay { color, opacity } = st {
                buf.color_overlay(rgb3(&color.value(frame)), opacity.value(frame) / 100.0);
            }
        }
        for st in &l.styles {
            let mut s = self.take(ctx.w, ctx.h);
            match st {
                St::Glow {
                    inner: true,
                    color,
                    opacity,
                    size,
                    spread,
                    from_center,
                } => {
                    let sigma = size.value(frame) * scale * 0.5;
                    let margin = (sigma * 3.0).ceil() as usize + 2;
                    let c = rgb3(&color.value(frame));
                    let op = opacity.value(frame) / 100.0;
                    let sp = spread.value(frame) / 100.0;
                    if *from_center {
                        s.silhouette(&src, c, 1.0, 0, 0, sp, true, margin);
                        s.blur(sigma, sigma);
                        s.invert_alpha_colored(c, op);
                    } else {
                        s.silhouette(&src, c, op, 0, 0, sp, true, margin);
                        s.blur(sigma, sigma);
                    }
                    s.mask_by_alpha(&src);
                    buf.composite(&s, 1.0, BlendMode::Normal);
                }
                St::Shadow {
                    inner: true,
                    color,
                    opacity,
                    angle,
                    distance,
                    size,
                    spread,
                } => {
                    let (dx, dy) = style_offset(angle.value(frame), distance.value(frame) * scale);
                    let sigma = size.value(frame) * scale * 0.5;
                    let margin = (sigma * 3.0).ceil() as usize
                        + dx.unsigned_abs().max(dy.unsigned_abs()) as usize
                        + 2;
                    s.silhouette(
                        &src,
                        rgb3(&color.value(frame)),
                        opacity.value(frame) / 100.0,
                        dx,
                        dy,
                        spread.value(frame) / 100.0,
                        true,
                        margin,
                    );
                    s.blur(sigma, sigma);
                    s.mask_by_alpha(&src);
                    buf.composite(&s, 1.0, BlendMode::Normal);
                }
                _ => {}
            }
            self.recycle(s);
        }
        // Под слоем: тень, внешнее свечение, обводка.
        let mut under = self.take(ctx.w, ctx.h);
        let mut any_under = false;
        for pass in 0..3 {
            for st in &l.styles {
                let mut s = self.take(ctx.w, ctx.h);
                let drawn = match (pass, st) {
                    (
                        0,
                        St::Shadow {
                            inner: false,
                            color,
                            opacity,
                            angle,
                            distance,
                            size,
                            spread,
                        },
                    ) => {
                        let (dx, dy) =
                            style_offset(angle.value(frame), distance.value(frame) * scale);
                        let sigma = size.value(frame) * scale * 0.5;
                        s.silhouette(
                            &src,
                            rgb3(&color.value(frame)),
                            opacity.value(frame) / 100.0,
                            dx,
                            dy,
                            spread.value(frame) / 100.0,
                            false,
                            0,
                        );
                        s.blur(sigma, sigma);
                        true
                    }
                    (
                        1,
                        St::Glow {
                            inner: false,
                            color,
                            opacity,
                            size,
                            spread,
                            ..
                        },
                    ) => {
                        let sigma = size.value(frame) * scale * 0.5;
                        s.silhouette(
                            &src,
                            rgb3(&color.value(frame)),
                            opacity.value(frame) / 100.0,
                            0,
                            0,
                            spread.value(frame) / 100.0,
                            false,
                            0,
                        );
                        s.blur(sigma, sigma);
                        true
                    }
                    (
                        2,
                        St::Stroke {
                            color,
                            opacity,
                            size,
                        },
                    ) => {
                        s.dilated_of(
                            &src,
                            rgb3(&color.value(frame)),
                            opacity.value(frame) / 100.0,
                            size.value(frame) * scale,
                        );
                        true
                    }
                    _ => false,
                };
                if drawn {
                    under.composite(&s, 1.0, BlendMode::Normal);
                    any_under = true;
                }
                self.recycle(s);
            }
        }
        if any_under {
            under.composite(buf, 1.0, BlendMode::Normal);
            std::mem::swap(buf, &mut under);
        }
        self.recycle(under);
        self.recycle(src);
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

/// Range-селектор текста, вычисленный на кадре (как в lottie-web
/// `TextSelectorProperty`): границы в единицах индекса и easing.
struct SelectorEval<'a> {
    sel: &'a super::model::TextSelector,
    s: f32,
    e: f32,
    amount: f32,
    smooth: f32,
    ease: [f32; 4],
    perm: Option<Vec<usize>>,
}

impl<'a> SelectorEval<'a> {
    fn new(sel: &'a super::model::TextSelector, frame: f32, total: usize) -> Self {
        let divisor = if sel.index_units || total == 0 {
            1.0
        } else {
            100.0 / total as f32
        };
        let o = sel.offset.value(frame) / divisor;
        let mut s = sel.start.value(frame) / divisor + o;
        let mut e = sel.end.value(frame) / divisor + o;
        if s > e {
            std::mem::swap(&mut s, &mut e);
        }
        let ne = sel.ease_low.value(frame);
        let xe = sel.ease_high.value(frame);
        let (mut x1, mut y1, mut x2, mut y2) = (0.0, 0.0, 1.0, 1.0);
        if ne > 0.0 {
            x1 = ne / 100.0;
        } else {
            y1 = -ne / 100.0;
        }
        if xe > 0.0 {
            x2 = 1.0 - xe / 100.0;
        } else {
            y2 = 1.0 + xe / 100.0;
        }
        // Случайный порядок — детерминированная перестановка (LCG).
        let perm = sel.randomize.then(|| {
            let mut p: Vec<usize> = (0..total).collect();
            let mut st: u32 = 0x9E37_79B9 ^ total as u32;
            for i in (1..p.len()).rev() {
                st = st.wrapping_mul(1_664_525).wrapping_add(1_013_904_223);
                let j = (st >> 8) as usize % (i + 1);
                p.swap(i, j);
            }
            p
        });
        SelectorEval {
            sel,
            s,
            e,
            amount: sel.amount.value(frame) / 100.0,
            smooth: sel.smoothness.value(frame),
            ease: [x1, y1, x2, y2],
            perm,
        }
    }

    fn weight(&self, ind: usize) -> f32 {
        use super::model::SelectorShape as Sh;
        let ind = match &self.perm {
            Some(p) => p.get(ind).copied().unwrap_or(ind),
            None => ind,
        } as f32;
        let (s, e) = (self.s, self.e);
        let [x1, y1, x2, y2] = self.ease;
        let ease = |m: f32| super::anim::cubic_bezier_ease(x1, y1, x2, y2, m);
        let ramp = || {
            if e == s {
                if ind >= e {
                    1.0
                } else {
                    0.0
                }
            } else {
                (0.5 / (e - s) + (ind - s) / (e - s)).clamp(0.0, 1.0)
            }
        };
        let mut mult = match self.sel.shape {
            Sh::RampUp => ease(ramp()),
            Sh::RampDown => ease(if e == s {
                if ind >= e {
                    0.0
                } else {
                    1.0
                }
            } else {
                1.0 - ramp()
            }),
            Sh::Triangle => {
                if e == s {
                    0.0
                } else {
                    let m = ramp();
                    ease(if m < 0.5 {
                        m * 2.0
                    } else {
                        1.0 - 2.0 * (m - 0.5)
                    })
                }
            }
            Sh::Round => {
                if e == s {
                    0.0
                } else {
                    let tot = e - s;
                    let i = (ind + 0.5 - s).clamp(0.0, tot);
                    let x = -tot / 2.0 + i;
                    let a = tot / 2.0;
                    ease((1.0 - (x * x) / (a * a)).max(0.0).sqrt())
                }
            }
            Sh::Smooth => {
                if e == s {
                    0.0
                } else {
                    let i = (ind + 0.5 - s).clamp(0.0, e - s);
                    ease((1.0 + (PI + TAU * i / (e - s)).cos()) / 2.0)
                }
            }
            Sh::Square => {
                let mut m = 0.0;
                if ind >= s.floor() {
                    m = if ind - s < 0.0 {
                        (e.min(1.0) - (s - ind)).clamp(0.0, 1.0)
                    } else {
                        (e - ind).clamp(0.0, 1.0)
                    };
                }
                ease(m)
            }
        };
        if self.smooth != 100.0 {
            let sm = (self.smooth * 0.01).max(1e-8);
            let th = 0.5 - sm * 0.5;
            mult = if mult < th {
                0.0
            } else {
                ((mult - th) / sm).min(1.0)
            };
        }
        mult * self.amount
    }
}

/// Сдвиг тона (градусы), насыщенности и яркости (доли) цвета RGB на месте.
fn adjust_hsv(c: &mut [f32], hue: f32, sat: f32, bri: f32) {
    if c.len() < 3 {
        return;
    }
    let (r, g, b) = (c[0], c[1], c[2]);
    let max = r.max(g).max(b);
    let min = r.min(g).min(b);
    let d = max - min;
    let mut h = if d <= 0.0 {
        0.0
    } else if max == r {
        ((g - b) / d).rem_euclid(6.0)
    } else if max == g {
        (b - r) / d + 2.0
    } else {
        (r - g) / d + 4.0
    } / 6.0;
    let mut s = if max <= 0.0 { 0.0 } else { d / max };
    let mut v = max;
    h = (h + hue / 360.0).rem_euclid(1.0);
    s = (s + sat).clamp(0.0, 1.0);
    v = (v + bri).clamp(0.0, 1.0);
    let i = (h * 6.0).floor();
    let f = h * 6.0 - i;
    let p = v * (1.0 - s);
    let q = v * (1.0 - f * s);
    let t = v * (1.0 - (1.0 - f) * s);
    let (r, g, b) = match i as i32 % 6 {
        0 => (v, t, p),
        1 => (q, v, p),
        2 => (p, v, t),
        3 => (p, q, v),
        4 => (t, p, v),
        _ => (v, p, q),
    };
    c[0] = r;
    c[1] = g;
    c[2] = b;
}

#[inline]
fn rgb3(v: &[f32]) -> [f32; 3] {
    [
        v.first().copied().unwrap_or(0.0),
        v.get(1).copied().unwrap_or(0.0),
        v.get(2).copied().unwrap_or(0.0),
    ]
}

/// Смещение тени стиля: угол источника света (90° — сверху, тень вниз).
fn style_offset(angle_deg: f32, dist: f32) -> (i32, i32) {
    let r = angle_deg.to_radians();
    (
        (-r.cos() * dist).round() as i32,
        (r.sin() * dist).round() as i32,
    )
}

/// Мировая матрица камеры (из её пространства, где она смотрит вдоль +z):
/// позиция, наведение на точку интереса (`a`), ориентация и повороты.
fn camera_matrix(tr: &Transform, frame: f32) -> Mat4 {
    let p2 = tr.position_at(frame);
    let p = [p2[0], p2[1], tr.position_z(frame)];
    let a = tr.anchor.value(frame);
    let a = [
        a.first().copied().unwrap_or(p[0]),
        a.get(1).copied().unwrap_or(p[1]),
        a.get(2).copied().unwrap_or(p[2] + 1.0),
    ];
    let mut d = [a[0] - p[0], a[1] - p[1], a[2] - p[2]];
    let dl = (d[0] * d[0] + d[1] * d[1] + d[2] * d[2]).sqrt();
    if dl < 1e-6 {
        d = [0.0, 0.0, 1.0];
    } else {
        d = [d[0] / dl, d[1] / dl, d[2] / dl];
    }
    let cross = |u: [f32; 3], v: [f32; 3]| {
        [
            u[1] * v[2] - u[2] * v[1],
            u[2] * v[0] - u[0] * v[2],
            u[0] * v[1] - u[1] * v[0],
        ]
    };
    let norm = |v: [f32; 3]| {
        let l = (v[0] * v[0] + v[1] * v[1] + v[2] * v[2]).sqrt();
        if l < 1e-6 {
            [1.0, 0.0, 0.0]
        } else {
            [v[0] / l, v[1] / l, v[2] / l]
        }
    };
    // «Низ» мира — +y; базис камеры: x вправо, y вниз, z — взгляд.
    let x = norm(cross([0.0, 1.0, 0.0], d));
    let y = cross(d, x);
    let look = Mat4([
        [x[0], y[0], d[0], 0.0],
        [x[1], y[1], d[1], 0.0],
        [x[2], y[2], d[2], 0.0],
        [0.0, 0.0, 0.0, 1.0],
    ]);
    let mut m = Mat4::translate(p[0], p[1], p[2]).mul(&look);
    let g = |v: &[f32], i: usize| v.get(i).copied().unwrap_or(0.0);
    if let Some(o) = &tr.orientation {
        let o = o.value(frame);
        m = m
            .mul(&Mat4::rotate_x(g(&o, 0)))
            .mul(&Mat4::rotate_y(g(&o, 1)))
            .mul(&Mat4::rotate_z(g(&o, 2)));
    }
    if let Some(rx) = &tr.rotation_x {
        m = m.mul(&Mat4::rotate_x(rx.value(frame)));
    }
    if let Some(ry) = &tr.rotation_y {
        m = m.mul(&Mat4::rotate_y(ry.value(frame)));
    }
    m.mul(&Mat4::rotate_z(tr.rotation.value(frame)))
}

/// Текст документа с учётом выражения `sourceText`.
fn text_source<'a>(
    td: &TextData,
    doc: &'a super::model::TextDoc,
    frame: f32,
) -> std::borrow::Cow<'a, str> {
    use super::expr::{evaluate_value, interp::to_str, PropRef, Value};
    match &td.expr {
        Some(p) => match evaluate_value(
            p,
            frame,
            Value::Str(doc.text.as_str().into()),
            PropRef::None,
            0,
        ) {
            Some(v) => std::borrow::Cow::Owned(to_str(&v)),
            None => std::borrow::Cow::Borrowed(&doc.text),
        },
        None => std::borrow::Cow::Borrowed(&doc.text),
    }
}

/// Охват содержимого слоя в его локальных координатах без обводок —
/// `[x, y, ширина, высота]` (для `sourceRectAtTime` в выражениях).
pub fn layer_bounds(comp: &Composition, l: &Layer, frame: f32) -> Option<[f32; 4]> {
    match &l.kind {
        LayerKind::Shape(items) => {
            let mut col = Collected {
                styles: Vec::new(),
                trims: Vec::new(),
                paths: Vec::new(),
                merges: Vec::new(),
            };
            collect_shapes(
                items,
                frame,
                Mat::IDENTITY,
                1.0,
                &CollectCtx::default(),
                &mut col,
                0,
            );
            let mut bb: Option<[f32; 4]> = None;
            for p in col.paths.iter().flat_map(|p| p.polys.iter()) {
                if let Some(b) = p.bbox() {
                    bb = Some(match bb {
                        None => b,
                        Some(a) => [
                            a[0].min(b[0]),
                            a[1].min(b[1]),
                            a[2].max(b[2]),
                            a[3].max(b[3]),
                        ],
                    });
                }
            }
            bb.map(|b| [b[0], b[1], b[2] - b[0], b[3] - b[1]])
        }
        LayerKind::Solid(_) | LayerKind::Precomp(_) => Some([0.0, 0.0, l.width, l.height]),
        LayerKind::Image(id) => comp.images.get(id).map(|a| [0.0, 0.0, a.width, a.height]),
        LayerKind::Text(td) => {
            let doc = td.doc_at(frame)?;
            let text = text_source(td, doc, frame);
            let (family, style, ascent) = match comp.font(&doc.font) {
                Some(f) => (f.family.as_str(), f.style.as_str(), f.ascent),
                None => ("", "", 75.0),
            };
            let k = doc.size / 100.0;
            let lines: Vec<&str> = text.split(['\r', '\n']).collect();
            let width = lines
                .iter()
                .map(|line| {
                    line.chars()
                        .map(|ch| {
                            let mut b = [0u8; 4];
                            comp.glyph(family, style, ch.encode_utf8(&mut b))
                                .map(|g| g.advance * k)
                                .unwrap_or(doc.size * 0.33)
                                + doc.tracking / 1000.0 * doc.size
                        })
                        .sum::<f32>()
                })
                .fold(0.0f32, f32::max);
            let x = match doc.justify {
                1 => -width,
                2 => -width / 2.0,
                _ => 0.0,
            } + doc.box_pos.map(|p| p[0]).unwrap_or(0.0);
            let top = doc.box_pos.map(|p| p[1]).unwrap_or(-ascent * k);
            let h = ascent * k
                + (lines.len().saturating_sub(1)) as f32 * doc.line_height
                + doc.size * 0.25;
            Some([x, top, width, h])
        }
        _ => None,
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
    let s = [
        s.first().copied().unwrap_or(100.0) / 100.0,
        s.get(1).copied().unwrap_or(100.0) / 100.0,
    ];
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

/// Унаследованный контекст сбора фигур.
#[derive(Default, Clone)]
struct CollectCtx {
    styles: Vec<usize>,
    trims: Vec<usize>,
    mods: Vec<ModInst>,
    merge: Option<usize>,
}

fn collect_shapes<'a>(
    items: &'a [Shape],
    frame: f32,
    mat: Mat,
    opacity: f32,
    inherited: &CollectCtx,
    col: &mut Collected<'a>,
    depth: usize,
) {
    if depth > MAX_DEPTH {
        return;
    }
    let mut cx = inherited.clone();
    let mut idx = items.len();
    while idx > 0 {
        idx -= 1;
        let it = &items[idx];
        match it {
            Shape::Fill { .. }
            | Shape::Stroke { .. }
            | Shape::GradientFill { .. }
            | Shape::GradientStroke { .. } => {
                col.styles.push(StyleInst {
                    shape: it,
                    mat,
                    opacity,
                });
                cx.styles.push(col.styles.len() - 1);
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
                cx.trims.push(col.trims.len() - 1);
            }
            Shape::RoundCorners { radius } => cx.mods.push(ModInst::Round(radius.value(frame))),
            Shape::OffsetPath {
                amount,
                join,
                miter,
            } => cx.mods.push(ModInst::Offset {
                amount: amount.value(frame),
                join: *join,
                miter: *miter,
            }),
            Shape::ZigZag {
                size,
                ridges,
                smooth,
            } => cx.mods.push(ModInst::Zig {
                size: size.value(frame),
                ridges: ridges.value(frame),
                smooth: *smooth,
            }),
            Shape::PuckerBloat { amount } => cx.mods.push(ModInst::Pucker(amount.value(frame))),
            Shape::Twist { angle, center } => {
                let c = two(&center.value(frame));
                cx.mods.push(ModInst::Twist {
                    angle: angle.value(frame),
                    cx: c[0],
                    cy: c[1],
                });
            }
            Shape::Merge { mode } => {
                // Merge (простое объединение) эквивалентен раздельной отрисовке.
                cx.merge = if *mode == MergeMode::Merge {
                    None
                } else {
                    col.merges.push(*mode);
                    Some(col.merges.len() - 1)
                };
            }
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
                let order: Vec<usize> = if *above {
                    (0..n).collect()
                } else {
                    (0..n).rev().collect()
                };
                for k in order {
                    let rm = repeater_matrix(transform, frame, k as f32 + off);
                    let op = if n > 1 {
                        so + (eo - so) * k as f32 / (n - 1) as f32
                    } else {
                        so
                    };
                    collect_shapes(
                        before,
                        frame,
                        mat.mul(&rm),
                        opacity * op,
                        &cx,
                        col,
                        depth + 1,
                    );
                }
                break;
            }
            Shape::Group {
                items: sub,
                transform,
                ..
            } => {
                let gm = transform.matrix(frame);
                let gop = transform.opacity_at(frame);
                collect_shapes(sub, frame, mat.mul(&gm), opacity * gop, &cx, col, depth + 1);
            }
            Shape::Rect { .. }
            | Shape::Ellipse { .. }
            | Shape::Polystar { .. }
            | Shape::Path { .. } => {
                let Some(mut sp) = shape_to_path(it, frame) else {
                    continue;
                };
                // Модификаторы применяются в порядке списка (обратном обходу).
                // Round/Pucker работают по кривым; Offset/Zig/Twist переводят
                // контур в ломаную (в локальных координатах, допуск с учётом
                // масштаба) — последующие кривые-модификаторы пропускаются.
                let local_tol = TOLERANCE / mat.scale_factor().max(1e-6);
                let mut poly: Option<Polyline> = None;
                for m in cx.mods.iter().rev() {
                    match *m {
                        ModInst::Round(r) => {
                            if poly.is_none() {
                                sp = sp.round_corners(r);
                            }
                        }
                        ModInst::Pucker(a) => {
                            if poly.is_none() {
                                sp = pucker_bloat(&sp, a);
                            }
                        }
                        ModInst::Zig {
                            size,
                            ridges,
                            smooth,
                        } => {
                            if poly.is_none() {
                                if smooth {
                                    sp = zigzag_smooth(&sp, size, ridges);
                                } else {
                                    poly = Some(zigzag_path(&sp, size, ridges, local_tol));
                                }
                            }
                        }
                        ModInst::Offset {
                            amount,
                            join,
                            miter,
                        } => {
                            let base = poly.take().unwrap_or_else(|| sp.flatten(local_tol));
                            poly = Some(offset_polyline(&base, amount, join, miter));
                        }
                        ModInst::Twist {
                            angle,
                            cx: tx,
                            cy: ty,
                        } => {
                            let mut base = poly.take().unwrap_or_else(|| sp.flatten(local_tol));
                            twist_polyline(&mut base, angle, [tx, ty]);
                            poly = Some(base);
                        }
                    }
                }
                let poly = match poly {
                    Some(mut p) => {
                        p.transform(&mat);
                        p
                    }
                    None => {
                        sp.transform(&mat);
                        sp.flatten(TOLERANCE)
                    }
                };
                if poly.pts.len() >= 2 {
                    col.paths.push(PathInst {
                        polys: vec![poly],
                        styles: cx.styles.clone(),
                        trims: cx.trims.clone(),
                        merge: cx.merge,
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
        let idxs: Vec<usize> = (0..col.paths.len())
            .filter(|&p| col.paths[p].trims.contains(&ti))
            .collect();
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
            let total: f32 = idxs
                .iter()
                .flat_map(|&p| col.paths[p].polys.iter().map(Polyline::length))
                .sum();
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
        Shape::Ellipse {
            position,
            size,
            reversed,
        } => {
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
                    if long {
                        (or_, os)
                    } else {
                        (ir, is)
                    }
                } else {
                    (or_, os)
                };
                let perim = if *star {
                    TAU * rad / (num as f32 * 2.0)
                } else {
                    TAU * rad / (num as f32 * 4.0)
                };
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
            let ang =
                (e[1] - s[1]).atan2(e[0] - s[0]) + g.highlight_angle.value(frame).to_radians();
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
fn build_stroke(
    polys: &[Polyline],
    st: &StrokeStyle,
    frame: f32,
    mat: &Mat,
    out: &mut Vec<Polyline>,
) {
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
                stroke_polyline(
                    &piece,
                    width,
                    st.cap,
                    st.join,
                    st.miter_limit,
                    TOLERANCE,
                    out,
                );
            }
        } else {
            stroke_polyline(poly, width, st.cap, st.join, st.miter_limit, TOLERANCE, out);
        }
    }
}

#[cfg(test)]
mod tests {
    use super::super::anim::Animated;
    use super::super::model::{SelectorShape, TextBasedOn, TextSelector};
    use super::*;

    fn sel(shape: SelectorShape, s: f32, e: f32, o: f32) -> TextSelector {
        TextSelector {
            based_on: TextBasedOn::Characters,
            shape,
            index_units: false,
            start: Animated::Static(s),
            end: Animated::Static(e),
            offset: Animated::Static(o),
            amount: Animated::Static(100.0),
            ease_high: Animated::Static(0.0),
            ease_low: Animated::Static(0.0),
            smoothness: Animated::Static(100.0),
            randomize: false,
        }
    }

    #[test]
    fn square_selector_partial_letter() {
        // 4 буквы, 0..60% + смещение 0: буквы 0,1 целиком, 2 — на 0.4.
        let s = sel(SelectorShape::Square, 0.0, 60.0, 0.0);
        let ev = SelectorEval::new(&s, 0.0, 4);
        let w: Vec<f32> = (0..4).map(|i| ev.weight(i)).collect();
        assert!((w[0] - 1.0).abs() < 1e-5 && (w[1] - 1.0).abs() < 1e-5);
        assert!((w[2] - 0.4).abs() < 1e-5);
        assert_eq!(w[3], 0.0);
        // Смещение 50% сдвигает окно на две буквы.
        let s = sel(SelectorShape::Square, 0.0, 50.0, 50.0);
        let ev = SelectorEval::new(&s, 0.0, 4);
        assert_eq!(ev.weight(1), 0.0);
        assert!((ev.weight(2) - 1.0).abs() < 1e-5);
    }

    #[test]
    fn ramp_and_triangle_shapes() {
        let s = sel(SelectorShape::RampUp, 0.0, 100.0, 0.0);
        let ev = SelectorEval::new(&s, 0.0, 4);
        let w: Vec<f32> = (0..4).map(|i| ev.weight(i)).collect();
        assert!(w.windows(2).all(|p| p[0] < p[1]));
        let s = sel(SelectorShape::Triangle, 0.0, 100.0, 0.0);
        let ev = SelectorEval::new(&s, 0.0, 5);
        assert!(ev.weight(2) > ev.weight(0) && ev.weight(2) > ev.weight(4));
    }

    #[test]
    fn hsv_roundtrip_hue_shift() {
        let mut c = vec![1.0, 0.0, 0.0];
        adjust_hsv(&mut c, 120.0, 0.0, 0.0);
        assert!((c[1] - 1.0).abs() < 1e-5 && c[0].abs() < 1e-5);
    }

    #[test]
    fn polyline_point_at() {
        let p = Polyline {
            pts: vec![[0.0, 0.0], [10.0, 0.0], [10.0, 10.0]],
            closed: false,
        };
        let (pt, ang) = p.point_at(15.0).unwrap();
        assert!((pt[0] - 10.0).abs() < 1e-5 && (pt[1] - 5.0).abs() < 1e-5);
        assert!((ang - PI / 2.0).abs() < 1e-5);
    }
}

#[cfg(test)]
mod tests3d {
    use super::*;

    fn comp(cam_z: f32, layer_z: f32, ry: f32) -> Composition {
        let json = format!(
            r##"{{"fr":30,"ip":0,"op":10,"w":200,"h":200,"layers":[
            {{"ty":13,"ind":1,"pe":{{"a":0,"k":500}},"ks":{{"p":{{"a":0,"k":[100,100,{cam_z}]}},"a":{{"a":0,"k":[100,100,0]}}}},"ip":0,"op":10,"st":0}},
            {{"ty":1,"ind":2,"ddd":1,"sc":"#ff0000","sw":40,"sh":40,
              "ks":{{"p":{{"a":0,"k":[100,100,{layer_z}]}},"a":{{"a":0,"k":[20,20,0]}},"ry":{{"a":0,"k":{ry}}}}},"ip":0,"op":10,"st":0}}]}}"##
        );
        Composition::from_str(&json).unwrap()
    }

    fn width_of_red(c: &Composition) -> usize {
        let mut r = Renderer::new();
        let px = r.render_rgba8(c, 0.0, 200, 200);
        // Ширина красного по средней строке.
        (0..200)
            .filter(|x| px[(100 * 200 + x) * 4 + 3] > 128)
            .count()
    }

    #[test]
    fn default_camera_keeps_plane_size() {
        // Камера на расстоянии zoom: плоскость z=0 — в натуральную величину.
        assert!((width_of_red(&comp(-500.0, 0.0, 0.0)) as i32 - 40).abs() <= 1);
        // Ближе к камере — крупнее (вдвое при половине расстояния).
        assert!((width_of_red(&comp(-500.0, -250.0, 0.0)) as i32 - 80).abs() <= 2);
    }

    #[test]
    fn y_rotation_foreshortens() {
        // 60° по Y: ширина × cos 60° (линеаризация в центре слоя).
        let w = width_of_red(&comp(-500.0, 0.0, 60.0)) as i32;
        assert!((w - 20).abs() <= 2, "w={w}");
    }

    fn column_height(c: &Composition, x: usize) -> usize {
        let mut r = Renderer::new();
        let px = r.render_rgba8(c, 0.0, 200, 200);
        (0..200).filter(|y| px[(y * 200 + x) * 4 + 3] > 128).count()
    }

    #[test]
    fn perspective_keystone() {
        // 60° по Y: правый край уходит от камеры — он ниже левого.
        let c = comp(-500.0, 0.0, 60.0);
        let left = column_height(&c, 92);
        let right = column_height(&c, 108);
        assert!(left > right + 1, "left={left} right={right}");
        // Без поворота края равны.
        let c = comp(-500.0, 0.0, 0.0);
        assert_eq!(column_height(&c, 85), column_height(&c, 115));
    }

    #[test]
    fn mat4_rigid_inverse() {
        let m = Mat4::translate(3.0, -2.0, 5.0)
            .mul(&Mat4::rotate_y(30.0))
            .mul(&Mat4::rotate_x(-20.0));
        let p = m.rigid_inverse().apply(m.apply([1.0, 2.0, 3.0]));
        assert!(
            (p[0] - 1.0).abs() < 1e-4 && (p[1] - 2.0).abs() < 1e-4 && (p[2] - 3.0).abs() < 1e-4
        );
    }
}
