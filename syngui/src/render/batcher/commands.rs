use crate::render::{DrawCommand, RenderOp, ShaderType};
use crate::text::FontAtlas;

use super::Batcher;

impl Batcher {
    pub(super) fn process_command(&mut self, cmd: &DrawCommand, font_atlas: &mut FontAtlas) {
        match cmd {
            DrawCommand::Rect {
                rect,
                color,
                corner_radius,
                border,
                per_side_border,
                clip_rect,
                ..
            } => {
                // Невидимая заливка (прозрачный фон `#00000000`, opacity 0) без рамки ничего не рисует,
                // но на тайловых GPU (Adreno 5xx) за неё платят полной заливкой со смешиванием:
                // прозрачные фоны страниц стола перекрашивали экран по нескольку раз за кадр листания.
                let visible_border = border
                    .as_ref()
                    .is_some_and(|b| b.width > 0.0 && b.color.a > 0.0)
                    || per_side_border
                        .as_ref()
                        .is_some_and(|b| b.color.a > 0.0 && b.widths.iter().any(|w| *w > 0.0));
                if color.a * self.current_opacity < 1.0 / 512.0 && !visible_border {
                    return;
                }
                let shader = fill_shader(corner_radius, border.as_ref(), per_side_border.is_some());
                // Лёгкие конвейеры заливки получают цвет уже в sRGB (см. `Batcher::srgb_vertex`).
                let fill_color = if self.srgb_vertex && shader != ShaderType::Rect {
                    let [r, g, b, a] = super::encode_srgb(color.to_array());
                    crate::core::Color::new(r, g, b, a)
                } else {
                    *color
                };
                let color = &fill_color;
                self.ensure_batch_rect(shader, None, *clip_rect, rect.inflate(1.0, 1.0));
                self.set_clip_expand(clip_rect);
                if let Some(psb) = per_side_border {
                    let bc = psb.color;
                    self.add_rect_per_side_border(
                        *rect,
                        *color,
                        *corner_radius,
                        psb.widths,
                        bc,
                        border.as_ref(),
                    );
                } else if let Some(border) = border {
                    self.add_rect_with_border(
                        *rect,
                        *color,
                        *corner_radius,
                        border.width,
                        border.color,
                    );
                } else {
                    self.add_rect(*rect, *color, *corner_radius);
                }
            }
            DrawCommand::Outline {
                rect,
                color,
                ring_width,
                corner_radius,
                clip_rect,
                ..
            } => {
                let pad = ring_width + 2.0;
                self.ensure_batch_rect(ShaderType::Rect, None, *clip_rect, rect.inflate(pad, pad));
                self.add_outline(*rect, *color, *corner_radius, *ring_width);
            }
            DrawCommand::GradientRect {
                rect,
                gradient,
                corner_radius,
                border,
                per_side_border,
                clip_rect,
                ..
            } => {
                let shader = fill_shader(corner_radius, border.as_ref(), per_side_border.is_some());
                self.ensure_batch_rect(shader, None, *clip_rect, rect.inflate(1.0, 1.0));
                self.set_clip_expand(clip_rect);
                let encode = self.srgb_vertex && shader != ShaderType::Rect;
                let start = if encode {
                    self.current_batch_mut().vertices.len()
                } else {
                    0
                };
                self.add_linear_gradient_rect(
                    *rect,
                    gradient,
                    *corner_radius,
                    border.as_ref(),
                    per_side_border.as_ref(),
                );
                // Лёгкий конвейер: цвета вершин градиента — в sRGB (см. `Batcher::srgb_vertex`).
                if encode {
                    for v in &mut self.current_batch_mut().vertices[start..] {
                        v.color = super::encode_srgb(v.color);
                    }
                }
            }
            DrawCommand::Text {
                text,
                rect,
                color,
                font_size,
                font_weight,
                text_align,
                decoration,
                font_family,
                letter_spacing,
                text_shadow,
                bbox_sample,
                clip_rect,
                no_wrap,
                italic,
                line_height,
                text_stroke,
                text_fx,
                ..
            } => {
                // Наклон курсива: верх глифа сдвигается вправо на 0.21·h
                // (~12°), низ на месте.
                let shear = if *italic { 0.21_f32 } else { 0.0 };
                if text.is_empty() {
                    return;
                }
                let sf = self.scale_factor;
                let phys_font_size = ((*font_size * sf).round() as u16).max(1);
                let phys_max_width = if *no_wrap { 0.0 } else { rect.size.width * sf };
                let weight = *font_weight;
                let phys_letter_spacing = *letter_spacing * sf;
                let glyphs = self.shape_text_cached_spacing(
                    font_atlas,
                    text,
                    phys_font_size,
                    phys_max_width,
                    weight,
                    font_family.as_deref(),
                    phys_letter_spacing,
                );
                if glyphs.is_empty() {
                    return;
                }
                // `line-height`: раскладка ставит строки с шагом 1.3 размера, а виджет
                // меряет блок по заданному интервалу — без этого текст стоял посреди своего
                // прямоугольника с пустотой сверху и снизу. Строки раздвигаются до шага;
                // половина разницы сверху получается центровкой, как в CSS.
                let base_step = phys_font_size as f32 * 1.3;
                let glyphs = match line_height.map(|lh| lh * sf) {
                    Some(step)
                        if (step - base_step).abs() > 0.5
                            && glyphs.iter().any(|g| g.line_top > 0.0) =>
                    {
                        let extra = step - base_step;
                        let v: Vec<_> = glyphs
                            .iter()
                            .map(|g| {
                                let line = (g.line_top / base_step).round();
                                let mut g = g.clone();
                                g.y += line * extra;
                                g.line_top += line * extra;
                                g
                            })
                            .collect();
                        std::sync::Arc::new(v)
                    }
                    _ => glyphs,
                };

                // Вертикаль — по метрикам шрифта (образец «H»), если в тексте есть буквы или
                // цифры: иначе соседние подписи в одном ряду стояли на разной базовой линии —
                // «720p» или «Second clip» (с нижним выносным) центрировались по своему контуру
                // и уезжали на 1–2 px вверх относительно «NVDEC» и «test». Строки из одних
                // значков, эмодзи и знаков («×», «•», «_») центрируются по контуру, как раньше.
                // Пустой образец — явный запрос центровки по контуру (`text-box-edge: ink`).
                let sample: Option<&str> = match bbox_sample.as_deref() {
                    Some("") => None,
                    Some(s) => Some(s),
                    None => text_has_letters(text).then_some(DEFAULT_BBOX_SAMPLE),
                };
                let mut glyph_min_y = f32::MAX;
                let mut glyph_max_y = f32::MIN;
                let mut glyph_min_x = f32::MAX;
                let mut glyph_max_x = f32::MIN;
                if let Some(sample) = sample {
                    let sample_glyphs = self.shape_text_cached_spacing(
                        font_atlas,
                        sample,
                        phys_font_size,
                        f32::INFINITY,
                        400,
                        font_family.as_deref(),
                        phys_letter_spacing,
                    );
                    for glyph in sample_glyphs.iter() {
                        if glyph.glyph.width == 0 || glyph.glyph.height == 0 {
                            continue;
                        }
                        let gy = glyph.y / sf;
                        let gh = glyph.glyph.height as f32 / sf;
                        glyph_min_y = glyph_min_y.min(gy);
                        glyph_max_y = glyph_max_y.max(gy + gh);
                    }
                    // строки переноса: образец — одна строка, низ блока — на последней строке
                    let last_line = glyphs.iter().map(|g| g.line_top).fold(0.0f32, f32::max);
                    if glyph_max_y > f32::MIN {
                        glyph_max_y += last_line / sf;
                    }
                    for glyph in glyphs.iter() {
                        if glyph.glyph.width == 0 || glyph.glyph.height == 0 {
                            continue;
                        }
                        let gx = glyph.x / sf;
                        let gw = glyph.glyph.width as f32 / sf;
                        glyph_min_x = glyph_min_x.min(gx);
                        glyph_max_x = glyph_max_x.max(gx + gw);
                    }
                } else {
                    for glyph in glyphs.iter() {
                        if glyph.glyph.width == 0 || glyph.glyph.height == 0 {
                            continue;
                        }
                        let gy = glyph.y / sf;
                        let gh = glyph.glyph.height as f32 / sf;
                        glyph_min_y = glyph_min_y.min(gy);
                        glyph_max_y = glyph_max_y.max(gy + gh);
                        let gx = glyph.x / sf;
                        let gw = glyph.glyph.width as f32 / sf;
                        glyph_min_x = glyph_min_x.min(gx);
                        glyph_max_x = glyph_max_x.max(gx + gw);
                    }
                }
                let text_height = glyph_max_y - glyph_min_y;
                let text_width = glyph_max_x - glyph_min_x;

                let mut origin_y = if rect.size.height > 0.0 && text_height > 0.0 {
                    if text_align.is_top() {
                        rect.origin.y - glyph_min_y
                    } else if text_align.is_bottom() {
                        rect.origin.y + rect.size.height - text_height - glyph_min_y
                    } else {
                        rect.origin.y + (rect.size.height - text_height) / 2.0 - glyph_min_y
                    }
                } else {
                    rect.origin.y
                };

                if sample.is_some() && sf > 0.0 {
                    origin_y = (origin_y * sf).round() / sf;
                }

                // Доля свободного места слева: 0 — влево, ½ — по центру, 1 — вправо.
                let h_frac = if text_align.is_hcenter()
                    && rect.size.width > text_width
                    && text_width > 0.0
                {
                    0.5
                } else if text_align.is_right() && rect.size.width > text_width && text_width > 0.0
                {
                    1.0
                } else {
                    0.0
                };
                let mut origin_x = if h_frac > 0.0 {
                    rect.origin.x + (rect.size.width - text_width) * h_frac - glyph_min_x
                } else {
                    rect.origin.x
                };
                // Строки переноса выравниваются каждая сама: блок стоит по самой широкой, а
                // короткие сдвигаются внутри него (иначе вторая строка центрированного текста
                // прижата к левому краю первой).
                let mut lines: Vec<(f32, f32, f32)> = Vec::new();
                if h_frac > 0.0 {
                    for glyph in glyphs.iter() {
                        if glyph.glyph.width == 0 || glyph.glyph.height == 0 {
                            continue;
                        }
                        let (x0, x1) = (glyph.x / sf, (glyph.x + glyph.glyph.width as f32) / sf);
                        match lines.iter_mut().find(|l| l.0 == glyph.line_top) {
                            Some(l) => {
                                l.1 = l.1.min(x0);
                                l.2 = l.2.max(x1);
                            }
                            None => lines.push((glyph.line_top, x0, x1)),
                        }
                    }
                    if lines.len() < 2 {
                        lines.clear();
                    }
                }

                if sample.is_some() && sf > 0.0 {
                    origin_x = (origin_x * sf).round() / sf;
                }

                // Снап к физической пиксельной сетке: дробное положение квада при
                // Linear-сэмплере атласа размывает глиф. Работает и под сдвигом
                // прокрутки — он целочисленный в физических пикселях и сетку не
                // ломает; см. `transform_keeps_pixel_grid`.
                let snap = sf > 0.0 && self.transform_keeps_pixel_grid();

                // Bbox текста: от origin на ширину строк; высота — по rect и
                // фактической высоте строк (текст может выходить за rect).
                let text_bbox_rect = crate::core::Rect::new(
                    crate::core::Point::new(
                        origin_x.min(rect.origin.x),
                        origin_y.min(rect.origin.y),
                    ),
                    crate::core::Size::new(
                        text_width.max(rect.size.width) + (origin_x - rect.origin.x).abs(),
                        text_height.max(rect.size.height) + (origin_y - rect.origin.y).abs(),
                    ),
                );
                // Эффекты: слои тени (`text-shadow` списком), свечение (`text-glow`),
                // заливка градиентом (`text-gradient`), анимация (`text-animation`).
                let fx = text_fx.as_deref();
                let anim = fx.and_then(|f| f.animation);
                let phase = fx.map_or(0.0, |f| f.phase);
                use crate::mss::fields::TextAnimationKind as Anim;
                // «Дыхание» 0..1 для `pulse`.
                let pulse = if anim == Some(Anim::Pulse) {
                    0.5 - 0.5 * (std::f32::consts::TAU * phase).cos()
                } else {
                    1.0
                };
                let place = GlyphPlace {
                    origin_x,
                    origin_y,
                    lines: &lines,
                    text_width,
                    h_frac,
                    glyph_min_x,
                    snap,
                    sf,
                };
                let paint = TextPaint::new(
                    *color,
                    fx.and_then(|f| f.gradient.as_ref()),
                    anim,
                    phase,
                    (
                        origin_x + glyph_min_x,
                        origin_y + glyph_min_y,
                        text_width.max(1.0),
                        text_height.max(1.0),
                    ),
                );
                let opacity = self.current_opacity;
                let srgb_vertex = self.srgb_vertex;
                // Цвет вершины: прозрачность слоя и (для лёгкого шейдера) sRGB.
                let vcol = move |c: crate::core::Color, mono: bool| -> [f32; 4] {
                    let mut a = c.to_array();
                    a[3] *= opacity;
                    if mono && srgb_vertex {
                        a = super::encode_srgb(a);
                    }
                    a
                };

                // Слои размытия снизу вверх: (dx, dy, размытие, цвет | None — цвет заливки, множитель альфы).
                let mut blur_layers: Vec<(f32, f32, f32, Option<crate::core::Color>, f32, bool)> =
                    Vec::new();
                let shadow_alpha = if anim == Some(Anim::Pulse) {
                    0.3 + 0.7 * pulse
                } else {
                    1.0
                };
                match fx.filter(|f| !f.shadows.is_empty()) {
                    // CSS: первый слой — верхний, рисуется последним.
                    Some(f) => {
                        for sh in f.shadows.iter().rev() {
                            blur_layers.push((
                                sh.offset_x,
                                sh.offset_y,
                                sh.blur_radius.max(0.0),
                                Some(sh.color),
                                shadow_alpha,
                                false,
                            ));
                        }
                    }
                    None => {
                        if let Some(sh) = text_shadow {
                            blur_layers.push((
                                sh.offset_x,
                                sh.offset_y,
                                sh.blur_radius.max(0.0),
                                Some(sh.color),
                                shadow_alpha,
                                false,
                            ));
                        }
                    }
                }
                if let Some(g) = fx.and_then(|f| f.glow) {
                    // Несколько слоёв размытия: широкий ореол, средний и плотный у самых букв —
                    // заметно ярче одной тени. `pulse` сильнее всего дышит внешним слоем.
                    // Размытие атласа берёт σ = blur/3; свечению нужно σ ≈ радиус/2 — ×1.5.
                    let r = g.radius * 1.5;
                    let layers = [
                        (r, 1.0, 0.2 + 0.8 * pulse),
                        (r * 0.45, 1.0, 0.45 + 0.55 * pulse),
                        (r * 0.15, 0.9, 0.7 + 0.3 * pulse),
                    ];
                    for (blur, a, p) in layers {
                        if blur > 0.25 {
                            blur_layers.push((
                                0.0,
                                0.0,
                                blur,
                                g.color,
                                a * if anim == Some(Anim::Pulse) { p } else { 1.0 },
                                true,
                            ));
                        }
                    }
                }
                let any_color_glyph = glyphs.iter().any(|g| g.glyph.is_color);

                for &(dx, dy, blur, layer_color, alpha_mul, is_glow) in &blur_layers {
                    // Глифы, заранее размытые в атласе (`FontAtlas::blurred_glyph`): лёгкий
                    // шейдер и одна выборка; цикл размытия в шейдере — только если их нет.
                    let blur_tx = blur * sf;
                    let baked: Option<Vec<Option<(crate::text::font_atlas::CachedGlyph, u32)>>> =
                        (blur > 0.0 && !shader_text_shadow() && !any_color_glyph)
                            .then(|| {
                                glyphs
                                    .iter()
                                    .map(|g| {
                                        if g.glyph.width == 0 || g.glyph.height == 0 {
                                            None
                                        } else {
                                            if is_glow {
                                                font_atlas.glow_glyph(&g.glyph, blur_tx)
                                            } else {
                                                font_atlas.blurred_glyph(&g.glyph, blur_tx)
                                            }
                                        }
                                    })
                                    .collect::<Vec<_>>()
                            })
                            .filter(|v| {
                                glyphs.iter().zip(v).all(|(g, b)| {
                                    b.is_some() || g.glyph.width == 0 || g.glyph.height == 0
                                })
                            });
                    let layer_paint = |x: f32, y: f32| -> crate::core::Color {
                        let c = layer_color.unwrap_or_else(|| paint.at(x, y).with_alpha(1.0));
                        c.with_alpha(c.a * alpha_mul)
                    };
                    let uniform = layer_color.is_some();
                    if let Some(baked) = baked {
                        let pad = blur.ceil() * 2.0 + dx.abs() + dy.abs() + 6.0;
                        self.ensure_batch_rect(
                            ShaderType::TextMono,
                            None,
                            *clip_rect,
                            text_bbox_rect.inflate(pad, pad),
                        );
                        for (glyph, b) in glyphs.iter().zip(baked) {
                            let Some((bg, k)) = b else { continue };
                            let (gx, gy) = place.glyph_pos(glyph, dx, dy);
                            let kp = k as f32 / sf;
                            let (x, y) = (gx - kp, gy - kp);
                            let (w, h) = (bg.width as f32 / sf, bg.height as f32 / sf);
                            let (nx, ny) = if uniform { (1, 1) } else { paint.subdiv(w, h) };
                            self.emit_glyph_quad(
                                [x, y, w, h],
                                shear * h,
                                [bg.uv_x, bg.uv_y, bg.uv_x + bg.uv_w, bg.uv_y + bg.uv_h],
                                (nx, ny),
                                [0.0, -1.0, 0.0, 0.0],
                                [0.0; 4],
                                &|px, py| vcol(layer_paint(px, py), true),
                            );
                        }
                    } else if blur > 0.0 {
                        // Прежний путь: размытие циклом в шейдере (радиус выборки до 6 px).
                        let pad_r = blur.ceil();
                        self.ensure_batch_rect(
                            ShaderType::Text,
                            None,
                            *clip_rect,
                            text_bbox_rect
                                .inflate(pad_r + dx.abs() + dy.abs(), pad_r + dx.abs() + dy.abs()),
                        );
                        for glyph in glyphs.iter() {
                            if glyph.glyph.width == 0 || glyph.glyph.height == 0 {
                                continue;
                            }
                            let (gx, gy) = place.glyph_pos(glyph, dx, dy);
                            let gw = glyph.glyph.width as f32 / sf;
                            let gh = glyph.glyph.height as f32 / sf;
                            let (x, y, w, h) =
                                (gx - pad_r, gy - pad_r, gw + 2.0 * pad_r, gh + 2.0 * pad_r);
                            let g = &glyph.glyph;
                            let upx = if gw > 0.0 { pad_r * (g.uv_w / gw) } else { 0.0 };
                            let upy = if gh > 0.0 { pad_r * (g.uv_h / gh) } else { 0.0 };
                            let is_color_flag = if g.is_color { 1.0 } else { 0.0 };
                            let (nx, ny) = if uniform { (1, 1) } else { paint.subdiv(w, h) };
                            self.emit_glyph_quad(
                                [x, y, w, h],
                                shear * h,
                                [
                                    g.uv_x - upx,
                                    g.uv_y - upy,
                                    g.uv_x + g.uv_w + upx,
                                    g.uv_y + g.uv_h + upy,
                                ],
                                (nx, ny),
                                [is_color_flag, blur, 0.0, 0.0],
                                [g.uv_x, g.uv_y, g.uv_x + g.uv_w, g.uv_y + g.uv_h],
                                &|px, py| vcol(layer_paint(px, py), false),
                            );
                        }
                    } else {
                        // Тень без размытия — копия букв со сдвигом.
                        let shader = if any_color_glyph {
                            ShaderType::Text
                        } else {
                            ShaderType::TextMono
                        };
                        let mono = shader == ShaderType::TextMono;
                        self.ensure_batch_rect(
                            shader,
                            None,
                            *clip_rect,
                            text_bbox_rect.inflate(dx.abs() + 1.0, dy.abs() + 1.0),
                        );
                        for glyph in glyphs.iter() {
                            if glyph.glyph.width == 0
                                || glyph.glyph.height == 0
                                || glyph.glyph.is_color
                            {
                                continue;
                            }
                            let (x, y) = place.glyph_pos(glyph, dx, dy);
                            let g = &glyph.glyph;
                            let (w, h) = (g.width as f32 / sf, g.height as f32 / sf);
                            let (nx, ny) = if uniform { (1, 1) } else { paint.subdiv(w, h) };
                            self.emit_glyph_quad(
                                [x, y, w, h],
                                shear * h,
                                [g.uv_x, g.uv_y, g.uv_x + g.uv_w, g.uv_y + g.uv_h],
                                (nx, ny),
                                [0.0; 4],
                                [0.0; 4],
                                &|px, py| vcol(layer_paint(px, py), mono),
                            );
                        }
                    }
                }

                // Контур (`text-stroke`): кольцо вокруг букв, заранее посчитанное в атласе
                // (`FontAtlas::outline_glyph`) — внутри буквы контура нет, поэтому прозрачная
                // заливка даёт полые буквы. Нет места в атласе — копии глифов по кругу.
                let stroke_pad = text_stroke.map_or(0.0, |st| st.width.ceil()) + 1.0;
                let mut stroke_copies: Vec<(f32, f32, [f32; 4])> = Vec::new();
                if let Some(st) = text_stroke {
                    let ring: Option<Vec<Option<(crate::text::font_atlas::CachedGlyph, u32)>>> =
                        (!any_color_glyph)
                            .then(|| {
                                glyphs
                                    .iter()
                                    .map(|g| {
                                        if g.glyph.width == 0 || g.glyph.height == 0 {
                                            None
                                        } else {
                                            font_atlas.outline_glyph(&g.glyph, st.width * sf)
                                        }
                                    })
                                    .collect::<Vec<_>>()
                            })
                            .filter(|v| {
                                glyphs.iter().zip(v).all(|(g, b)| {
                                    b.is_some() || g.glyph.width == 0 || g.glyph.height == 0
                                })
                            });
                    if let Some(ring) = ring {
                        self.ensure_batch_rect(
                            ShaderType::TextMono,
                            None,
                            *clip_rect,
                            text_bbox_rect.inflate(stroke_pad + 2.0, stroke_pad + 2.0),
                        );
                        let sc = vcol(st.color, true);
                        for (glyph, b) in glyphs.iter().zip(ring) {
                            let Some((bg, k)) = b else { continue };
                            let (gx, gy) = place.glyph_pos(glyph, 0.0, 0.0);
                            let kp = k as f32 / sf;
                            let (w, h) = (bg.width as f32 / sf, bg.height as f32 / sf);
                            self.emit_glyph_quad(
                                [gx - kp, gy - kp, w, h],
                                shear * h,
                                [bg.uv_x, bg.uv_y, bg.uv_x + bg.uv_w, bg.uv_y + bg.uv_h],
                                (1, 1),
                                [0.0, -1.0, 0.0, 0.0],
                                [0.0; 4],
                                &|_, _| sc,
                            );
                        }
                    } else {
                        let mono = !any_color_glyph;
                        let mut stroke_color = vcol(st.color, mono);
                        let w = st.width;
                        let rings: &[(f32, usize)] = if w > 2.5 {
                            &[(1.0, 16), (0.5, 8)]
                        } else if w > 1.0 {
                            &[(1.0, 12)]
                        } else {
                            &[(1.0, 8)]
                        };
                        // Копии перекрываются: полупрозрачный контур иначе
                        // копится до непрозрачного. Альфа копии — такая, чтобы
                        // половина копий (типичная точка контура) дала заданную.
                        let copies: usize = rings.iter().map(|r| r.1).sum();
                        let a = stroke_color[3].clamp(0.0, 1.0);
                        if a < 1.0 {
                            stroke_color[3] = 1.0 - (1.0 - a).powf(2.0 / copies as f32);
                        }
                        for &(k, n) in rings {
                            for i in 0..n {
                                let a = std::f32::consts::TAU * i as f32 / n as f32;
                                stroke_copies.push((
                                    w * k * a.cos(),
                                    w * k * a.sin(),
                                    stroke_color,
                                ));
                            }
                        }
                    }
                }

                // Лёгкий шейдер обычных глифов; эмодзи (цветные глифы) — общий.
                let main_shader = if any_color_glyph {
                    ShaderType::Text
                } else {
                    ShaderType::TextMono
                };
                let mono = main_shader == ShaderType::TextMono;
                self.ensure_batch_rect(
                    main_shader,
                    None,
                    *clip_rect,
                    text_bbox_rect.inflate(stroke_pad, stroke_pad),
                );
                // `pulse` без тени и свечения — дышит сама заливка.
                let fill_alpha = if anim == Some(Anim::Pulse) && blur_layers.is_empty() {
                    0.55 + 0.45 * pulse
                } else {
                    1.0
                };
                let fill_uniform = paint.is_uniform();
                let base_fill = paint.at(origin_x, origin_y);
                let color_array = vcol(base_fill.with_alpha(base_fill.a * fill_alpha), mono);

                for (dx, dy, stroke_color) in stroke_copies {
                    for glyph in glyphs.iter() {
                        if glyph.glyph.width == 0 || glyph.glyph.height == 0 || glyph.glyph.is_color
                        {
                            continue;
                        }
                        let (x, y) = place.glyph_pos(glyph, dx, dy);
                        let g = &glyph.glyph;
                        let h = g.height as f32 / sf;
                        self.emit_glyph_quad(
                            [x, y, g.width as f32 / sf, h],
                            shear * h,
                            [g.uv_x, g.uv_y, g.uv_x + g.uv_w, g.uv_y + g.uv_h],
                            (1, 1),
                            [0.0; 4],
                            [0.0; 4],
                            &|_, _| stroke_color,
                        );
                    }
                }
                for glyph in glyphs.iter() {
                    if glyph.glyph.width == 0 || glyph.glyph.height == 0 {
                        continue;
                    }
                    let (x, y) = place.glyph_pos(glyph, 0.0, 0.0);
                    let g = &glyph.glyph;
                    let (w, h) = (g.width as f32 / sf, g.height as f32 / sf);
                    let is_color_flag = if g.is_color { 1.0 } else { 0.0 };
                    let (nx, ny) = if fill_uniform || g.is_color {
                        (1, 1)
                    } else {
                        paint.subdiv(w, h)
                    };
                    self.emit_glyph_quad(
                        [x, y, w, h],
                        shear * h,
                        [g.uv_x, g.uv_y, g.uv_x + g.uv_w, g.uv_y + g.uv_h],
                        (nx, ny),
                        [is_color_flag, 0.0, 0.0, 0.0],
                        [0.0; 4],
                        &|px, py| {
                            if fill_uniform {
                                color_array
                            } else {
                                let c = paint.at(px, py);
                                vcol(c.with_alpha(c.a * fill_alpha), mono)
                            }
                        },
                    );
                }

                if *decoration != crate::mss::TextDecoration::None && text_width > 0.0 {
                    let mut line_thickness = (*font_size * 0.07).max(1.0);
                    let mut line_y = match decoration {
                        crate::mss::TextDecoration::Underline => origin_y + text_height + 2.0,
                        crate::mss::TextDecoration::LineThrough => origin_y + text_height * 0.5,
                        _ => origin_y,
                    };
                    if snap {
                        line_thickness = (line_thickness * sf).round().max(1.0) / sf;
                        line_y = (line_y * sf).round() / sf;
                    }
                    let line_rect = crate::core::Rect::new(
                        crate::core::Point::new(origin_x, line_y),
                        crate::core::Size::new(text_width, line_thickness),
                    );
                    let line_color = crate::core::Color::new(
                        color_array[0],
                        color_array[1],
                        color_array[2],
                        color_array[3],
                    );
                    self.ensure_batch_rect(ShaderType::Rect, None, *clip_rect, line_rect);
                    self.add_rect(line_rect, line_color, [0.0; 4]);
                }
            }
            DrawCommand::Shadow {
                rect,
                color,
                blur_radius,
                offset,
                corner_radius,
                inset,
                clip_rect,
                ..
            } => {
                let pad = blur_radius.max(0.0) * 1.5 + offset.0.abs().max(offset.1.abs());
                let shadow_rect = rect.inflate(pad, pad);
                if *inset {
                    self.ensure_batch_rect(ShaderType::InnerShadow, None, *clip_rect, shadow_rect);
                    self.add_inner_shadow(*rect, *color, *blur_radius, *offset, *corner_radius);
                } else {
                    self.ensure_batch_rect(ShaderType::Shadow, None, *clip_rect, shadow_rect);
                    self.add_shadow(*rect, *color, *blur_radius, *offset, *corner_radius, true);
                }
            }
            DrawCommand::GlowShadow {
                rect,
                color,
                blur_radius,
                offset,
                corner_radius,
                clip_rect,
                ..
            } => {
                let pad = blur_radius.max(0.0) * 1.5 + offset.0.abs().max(offset.1.abs());
                self.ensure_batch_rect(
                    ShaderType::GlowShadow,
                    None,
                    *clip_rect,
                    rect.inflate(pad, pad),
                );
                self.add_shadow(*rect, *color, *blur_radius, *offset, *corner_radius, false);
            }
            DrawCommand::TextSelection {
                text,
                sel_start,
                sel_end,
                base_x,
                y,
                height,
                font_size,
                font_weight,
                color,
                font_family,
                clip_rect,
                ..
            } => {
                let sf = self.scale_factor;
                let phys_font_size = ((*font_size * sf).round() as u16).max(1);
                let ff = font_family.as_deref();
                // Меряем тем же начертанием, каким рисуется текст: у жирных
                // глифов advance шире, и подсветка обрывалась до конца слова.
                let bold = *font_weight;
                let sel_start = snap_boundary(text, *sel_start);
                let sel_end = snap_boundary(text, *sel_end);
                let start_char_count = text[..sel_start].chars().count();
                let text_before_start = &text[..sel_start];
                let start_x_offset = font_atlas.measure_text_width_styled(
                    text_before_start,
                    phys_font_size,
                    start_char_count,
                    bold,
                    ff,
                );
                let start_x = *base_x + start_x_offset / sf;
                let end_char_count = text[..sel_end].chars().count();
                let text_before_end = &text[..sel_end];
                let end_x_offset = font_atlas.measure_text_width_styled(
                    text_before_end,
                    phys_font_size,
                    end_char_count,
                    bold,
                    ff,
                );
                let end_x = *base_x + end_x_offset / sf;
                let sel_width = (end_x - start_x).max(1.0);
                let sel_rect = crate::core::Rect::new(
                    crate::core::Point::new(start_x, *y),
                    crate::core::Size::new(sel_width, *height),
                );
                self.ensure_batch_rect(
                    ShaderType::Rect,
                    None,
                    *clip_rect,
                    sel_rect.inflate(1.0, 1.0),
                );
                self.add_rect(sel_rect, *color, [2.0; 4]);
            }
            DrawCommand::TextCursor {
                text,
                cursor_pos,
                base_x,
                y,
                height,
                font_size,
                font_weight,
                color,
                font_family,
                clip_rect,
                ..
            } => {
                let sf = self.scale_factor;
                let phys_font_size = ((*font_size * sf).round() as u16).max(1);
                let bold = *font_weight;
                let byte_pos = snap_boundary(text, *cursor_pos);
                let text_before_cursor = &text[..byte_pos];
                let char_count = text_before_cursor.chars().count();
                let cursor_x_offset = font_atlas.measure_text_width_styled(
                    text_before_cursor,
                    phys_font_size,
                    char_count,
                    bold,
                    font_family.as_deref(),
                );
                let cursor_x = *base_x + cursor_x_offset / sf;
                let cursor_rect = crate::core::Rect::new(
                    crate::core::Point::new(cursor_x, *y),
                    crate::core::Size::new(1.5, *height),
                );
                self.ensure_batch_rect(ShaderType::Rect, None, *clip_rect, cursor_rect);
                self.add_rect(cursor_rect, *color, [0.0; 4]);
            }
            // Клип входит в ключ батча; порядок между батчами разных клипов
            // хранит bbox-упорядочивание — рвать группу не нужно.
            DrawCommand::PushClip { .. } | DrawCommand::PopClip => {}
            // Порядок слоёв Stack гарантирует bbox-упорядочивание батчей
            // (пересекающиеся примитивы не переставляются) — барьер не нужен.
            DrawCommand::ZBarrier => {}
            DrawCommand::PushTransform(transform) => {
                self.transform_stack.push(self.current_transform);
                self.current_transform = transform.then(&self.current_transform);
            }
            DrawCommand::PopTransform => {
                if let Some(prev) = self.transform_stack.pop() {
                    self.current_transform = prev;
                }
            }
            DrawCommand::PushOpacity(opacity) => {
                self.opacity_stack.push(self.current_opacity);
                self.current_opacity *= opacity;
            }
            DrawCommand::PopOpacity => {
                if let Some(prev) = self.opacity_stack.pop() {
                    self.current_opacity = prev;
                }
            }
            DrawCommand::BeginEffectLayer { effect, bounds } => {
                self.flush_all_buckets();
                self.ops.push(RenderOp::BeginEffect {
                    effect: effect.clone(),
                    bounds: *bounds,
                });
            }
            DrawCommand::EndEffectLayer { .. } => {
                self.flush_all_buckets();
                self.ops.push(RenderOp::EndEffect);
            }
            DrawCommand::Canvas {
                vertices,
                indices,
                clip_rect,
                ..
            } => {
                let opacity = self.current_opacity;
                let transform = self.current_transform;
                let is_identity = transform == crate::core::Transform::identity();
                let mut bbox = [f32::MAX, f32::MAX, f32::MIN, f32::MIN];
                for v in vertices.iter() {
                    let p = if is_identity {
                        v.position
                    } else {
                        let p = transform
                            .transform_point(euclid::Point2D::new(v.position[0], v.position[1]));
                        [p.x, p.y]
                    };
                    bbox[0] = bbox[0].min(p[0]);
                    bbox[1] = bbox[1].min(p[1]);
                    bbox[2] = bbox[2].max(p[0]);
                    bbox[3] = bbox[3].max(p[1]);
                }
                let bbox = if vertices.is_empty() {
                    None
                } else {
                    Some(bbox)
                };
                self.ensure_batch_bbox(ShaderType::Rect, None, *clip_rect, bbox);
                let state = self.current_batch_mut();
                let base = state.vertices.len() as u32;
                use crate::render::Vertex;
                state.vertices.reserve(vertices.len());
                state.indices.reserve(indices.len());
                state.vertices.extend(vertices.iter().map(|v| {
                    let pos = if is_identity {
                        v.position
                    } else {
                        let p = transform
                            .transform_point(euclid::Point2D::new(v.position[0], v.position[1]));
                        [p.x, p.y]
                    };
                    let mut color = v.color;
                    color[3] *= opacity;
                    Vertex {
                        position: pos,
                        uv: v.uv,
                        color,
                        data: v.data,
                        data2: v.data2,
                    }
                }));
                state.indices.extend(indices.iter().map(|idx| base + idx));
            }
            DrawCommand::LineStrip {
                points,
                color,
                width,
                clip_rect,
                ..
            } => {
                let opacity = self.current_opacity;
                let transform = self.current_transform;
                let is_identity = transform == crate::core::Transform::identity();
                let color_array = {
                    let mut c = color.to_array();
                    c[3] *= opacity;
                    c
                };
                let feather = 1.0_f32;
                let half_w = *width * 0.5 + feather;
                let mut bbox = [f32::MAX, f32::MAX, f32::MIN, f32::MIN];
                for pt in points.iter() {
                    let p = if is_identity {
                        *pt
                    } else {
                        let p = transform.transform_point(euclid::Point2D::new(pt[0], pt[1]));
                        [p.x, p.y]
                    };
                    bbox[0] = bbox[0].min(p[0] - half_w);
                    bbox[1] = bbox[1].min(p[1] - half_w);
                    bbox[2] = bbox[2].max(p[0] + half_w);
                    bbox[3] = bbox[3].max(p[1] + half_w);
                }
                let bbox = if points.is_empty() { None } else { Some(bbox) };
                self.ensure_batch_bbox(ShaderType::Line, None, *clip_rect, bbox);
                let state = self.current_batch_mut();
                use crate::render::Vertex;
                let seg_count = points.len().saturating_sub(1);
                state.vertices.reserve(seg_count * 4);
                state.indices.reserve(seg_count * 6);
                for i in 0..seg_count {
                    let a = points[i];
                    let b = points[i + 1];
                    let dx = b[0] - a[0];
                    let dy = b[1] - a[1];
                    let len = (dx * dx + dy * dy).sqrt();
                    if len < 0.001 {
                        continue;
                    }
                    let inv_len = 1.0 / len;
                    let dir = [dx * inv_len, dy * inv_len];
                    let norm = [-dir[1], dir[0]];
                    let ext = [dir[0] * feather, dir[1] * feather];
                    let a_ext = [a[0] - ext[0], a[1] - ext[1]];
                    let b_ext = [b[0] + ext[0], b[1] + ext[1]];
                    let n_off = [norm[0] * half_w, norm[1] * half_w];
                    let corners: [[f32; 2]; 4] = [
                        [a_ext[0] + n_off[0], a_ext[1] + n_off[1]],
                        [b_ext[0] + n_off[0], b_ext[1] + n_off[1]],
                        [b_ext[0] - n_off[0], b_ext[1] - n_off[1]],
                        [a_ext[0] - n_off[0], a_ext[1] - n_off[1]],
                    ];
                    let base = state.vertices.len() as u32;
                    let data = [*width, feather, 0.0, 0.0];
                    let (transformed_corners, data2) = if is_identity {
                        (corners, [a[0], a[1], b[0], b[1]])
                    } else {
                        use wide::f32x4;
                        let xs = f32x4::new([
                            corners[0][0],
                            corners[1][0],
                            corners[2][0],
                            corners[3][0],
                        ]);
                        let ys = f32x4::new([
                            corners[0][1],
                            corners[1][1],
                            corners[2][1],
                            corners[3][1],
                        ]);
                        let xs_out = f32x4::splat(transform.m11) * xs
                            + f32x4::splat(transform.m21) * ys
                            + f32x4::splat(transform.m31);
                        let ys_out = f32x4::splat(transform.m12) * xs
                            + f32x4::splat(transform.m22) * ys
                            + f32x4::splat(transform.m32);
                        let xo: [f32; 4] = xs_out.into();
                        let yo: [f32; 4] = ys_out.into();
                        let pa = transform.transform_point(euclid::Point2D::new(a[0], a[1]));
                        let pb = transform.transform_point(euclid::Point2D::new(b[0], b[1]));
                        (
                            [
                                [xo[0], yo[0]],
                                [xo[1], yo[1]],
                                [xo[2], yo[2]],
                                [xo[3], yo[3]],
                            ],
                            [pa.x, pa.y, pb.x, pb.y],
                        )
                    };
                    state.vertices.extend_from_slice(&[
                        Vertex {
                            position: transformed_corners[0],
                            uv: [0.0, 0.0],
                            color: color_array,
                            data,
                            data2,
                        },
                        Vertex {
                            position: transformed_corners[1],
                            uv: [0.0, 0.0],
                            color: color_array,
                            data,
                            data2,
                        },
                        Vertex {
                            position: transformed_corners[2],
                            uv: [0.0, 0.0],
                            color: color_array,
                            data,
                            data2,
                        },
                        Vertex {
                            position: transformed_corners[3],
                            uv: [0.0, 0.0],
                            color: color_array,
                            data,
                            data2,
                        },
                    ]);
                    state.indices.extend_from_slice(&[
                        base,
                        base + 1,
                        base + 2,
                        base,
                        base + 2,
                        base + 3,
                    ]);
                }
            }
            DrawCommand::Image {
                rect,
                texture_id,
                uv_rect,
                color,
                clip_rect,
                ..
            } => {
                self.ensure_batch_rect(ShaderType::Image, Some(*texture_id), *clip_rect, *rect);
                let color_array = self.apply_opacity(color.to_array());
                let mut quad = self.transform_quad([
                    [rect.origin.x, rect.origin.y],
                    [rect.origin.x + rect.size.width, rect.origin.y],
                    [
                        rect.origin.x + rect.size.width,
                        rect.origin.y + rect.size.height,
                    ],
                    [rect.origin.x, rect.origin.y + rect.size.height],
                ]);
                // Без поворота края картинки — по пикселям устройства: на дробной
                // позиции (текст выше дал высоту 17,3 px) выборка смешивает соседние
                // texel'ы, и картинка 1:1 (узор в пиксель, мелкий текст) мылится.
                let t = &self.current_transform;
                if t.m12 == 0.0 && t.m21 == 0.0 && self.scale_factor > 0.0 {
                    let sf = self.scale_factor;
                    for p in &mut quad {
                        p[0] = (p[0] * sf).round() / sf;
                        p[1] = (p[1] * sf).round() / sf;
                    }
                }
                let [p0, p1, p2, p3] = quad;
                let data = [2.0, 0.0, 0.0, 0.0];
                use crate::render::Vertex;
                let state = self.current_batch_mut();
                let base = state.vertices.len() as u32;
                state.vertices.extend_from_slice(&[
                    Vertex {
                        position: p0,
                        uv: [uv_rect.origin.x, uv_rect.origin.y],
                        color: color_array,
                        data,
                        data2: [0.0; 4],
                    },
                    Vertex {
                        position: p1,
                        uv: [uv_rect.origin.x + uv_rect.size.width, uv_rect.origin.y],
                        color: color_array,
                        data,
                        data2: [0.0; 4],
                    },
                    Vertex {
                        position: p2,
                        uv: [
                            uv_rect.origin.x + uv_rect.size.width,
                            uv_rect.origin.y + uv_rect.size.height,
                        ],
                        color: color_array,
                        data,
                        data2: [0.0; 4],
                    },
                    Vertex {
                        position: p3,
                        uv: [uv_rect.origin.x, uv_rect.origin.y + uv_rect.size.height],
                        color: color_array,
                        data,
                        data2: [0.0; 4],
                    },
                ]);
                state.indices.extend_from_slice(&[
                    base,
                    base + 1,
                    base + 2,
                    base,
                    base + 2,
                    base + 3,
                ]);
            }
            DrawCommand::Cached(_) | DrawCommand::Custom { .. } => {}
        }
    }
}

/// Байтовый индекс, безопасный для среза: не дальше конца и на границе
/// символа (виджет мог подменить текст, оставив старую каретку).
fn snap_boundary(text: &str, i: usize) -> usize {
    let mut i = i.min(text.len());
    while i > 0 && !text.is_char_boundary(i) {
        i -= 1;
    }
    i
}

/// Шейдер заливки: без рамки — лёгкие `fs_flat` (без скруглений) и `fs_rounded`, общий `Rect` — только для рамок.
/// На тайловых GPU (Adreno 5xx) общий шейдер исполнялся с регистрами самой тяжёлой ветки: фон стоил как рамка.
fn fill_shader(
    corner_radius: &[f32; 4],
    border: Option<&crate::render::Border>,
    per_side_border: bool,
) -> ShaderType {
    if per_side_border || border.is_some_and(|b| b.width > 0.0) {
        ShaderType::Rect
    } else if corner_radius.iter().any(|r| *r > 0.5) {
        ShaderType::RectRounded
    } else {
        ShaderType::RectFlat
    }
}

/// Образец вертикальных границ текста по умолчанию — прописная «H»: по центру ставится высота
/// прописных, от верха до базовой линии. Базовая линия у всех строк одного размера одна, а слово
/// без выносных («Fit», «Filters») стоит по центру кнопки так же, как при центровке по контуру;
/// нижние выносные свисают под неё, как в наборе.
const DEFAULT_BBOX_SAMPLE: &str = "H";

/// Есть ли в тексте буквы или цифры обычного шрифта (не значки из областей частного
/// использования, где живут иконочные шрифты).
fn text_has_letters(text: &str) -> bool {
    text.chars().any(|c| {
        c.is_alphanumeric()
            && !matches!(c as u32, 0xE000..=0xF8FF | 0xF0000..=0xFFFFD | 0x100000..=0x10FFFD)
    })
}

#[cfg(test)]
mod text_sample_tests {
    use super::text_has_letters;

    #[test]
    fn letters_digits_and_symbols() {
        assert!(text_has_letters("Second clip"));
        assert!(text_has_letters("−1"));
        assert!(text_has_letters("目次"));
        assert!(!text_has_letters("×"));
        assert!(!text_has_letters("_"));
        assert!(!text_has_letters("\u{e5cd}"));
        assert!(!text_has_letters("🙂"));
    }
}

/// Отладка: `SYNGUI_SHADER_TEXT_SHADOW=1` — тень текста прежним циклом размытия в шейдере
/// (сверка с глифами, размытыми в атласе).
fn shader_text_shadow() -> bool {
    static V: std::sync::OnceLock<bool> = std::sync::OnceLock::new();
    *V.get_or_init(|| std::env::var_os("SYNGUI_SHADER_TEXT_SHADOW").is_some())
}

/// Где стоят глифы одной текстовой команды: начало, выравнивание строк
/// переноса, снап к пиксельной сетке, наклон курсива.
struct GlyphPlace<'a> {
    origin_x: f32,
    origin_y: f32,
    /// Строки переноса (line_top, x0, x1) — для `text-align` по каждой строке.
    lines: &'a [(f32, f32, f32)],
    text_width: f32,
    h_frac: f32,
    glyph_min_x: f32,
    snap: bool,
    sf: f32,
}

impl GlyphPlace<'_> {
    fn line_shift(&self, line_top: f32) -> f32 {
        self.lines
            .iter()
            .find(|l| l.0 == line_top)
            .map(|l| (self.text_width - (l.2 - l.1)) * self.h_frac - (l.1 - self.glyph_min_x))
            .unwrap_or(0.0)
    }

    /// Левый верхний угол глифа со сдвигом (`dx`, `dy`), логические px.
    fn glyph_pos(&self, g: &crate::text::font_atlas::ShapedGlyph, dx: f32, dy: f32) -> (f32, f32) {
        let ox = self.origin_x + self.line_shift(g.line_top) + dx;
        let oy = self.origin_y + dy;
        let sf = self.sf;
        if self.snap {
            ((ox * sf + g.x).round() / sf, (oy * sf + g.y).round() / sf)
        } else {
            (ox + g.x / sf, oy + g.y / sf)
        }
    }
}

/// Цвет букв в точке: `color` или `text-gradient`, с анимацией
/// (`gradient` — сдвиг, `rainbow` — круг оттенков, `shimmer` — блик).
struct TextPaint {
    base: crate::core::Color,
    /// Узлы градиента: (цвет в sRGB 0..1, позиция).
    stops: Vec<([f32; 4], f32)>,
    /// Ось градиента: единичное направление, центр и длина (CSS: 0deg — вверх, 90deg — вправо).
    dir: (f32, f32),
    center: (f32, f32),
    len: f32,
    anim: Option<crate::mss::fields::TextAnimationKind>,
    phase: f32,
    /// Контур текста: x, y, ширина, высота.
    ink: (f32, f32, f32, f32),
}

/// Полуширина блика `shimmer` в долях ширины текста.
const SHIMMER_HALF: f32 = 0.16;

impl TextPaint {
    fn new(
        base: crate::core::Color,
        gradient: Option<&crate::core::Gradient>,
        anim: Option<crate::mss::fields::TextAnimationKind>,
        phase: f32,
        ink: (f32, f32, f32, f32),
    ) -> Self {
        use crate::core::color::linear_to_srgb;
        let (mut stops, mut angle) = (Vec::new(), 90.0f32);
        if let Some(g @ crate::core::Gradient::Linear { angle_deg, .. }) = gradient {
            angle = *angle_deg;
            stops = g
                .resolved_stops()
                .into_iter()
                .map(|(c, p)| {
                    (
                        [
                            linear_to_srgb(c.r),
                            linear_to_srgb(c.g),
                            linear_to_srgb(c.b),
                            c.a,
                        ],
                        p,
                    )
                })
                .collect();
        }
        let a = angle.to_radians();
        let dir = (a.sin(), -a.cos());
        let len = (ink.2 * dir.0.abs() + ink.3 * dir.1.abs()).max(1.0);
        Self {
            base,
            stops,
            dir,
            center: (ink.0 + ink.2 * 0.5, ink.1 + ink.3 * 0.5),
            len,
            anim,
            phase,
            ink,
        }
    }

    /// Один цвет на весь текст — квады не делятся.
    fn is_uniform(&self) -> bool {
        self.stops.len() < 2 && self.anim != Some(crate::mss::fields::TextAnimationKind::Shimmer)
    }

    /// На сколько частей делить квад `w`×`h`, чтобы цвет вершин (интерполяция линейная)
    /// повторял градиент с изломами в узлах и узкий блик.
    fn subdiv(&self, w: f32, h: f32) -> (u32, u32) {
        use crate::mss::fields::TextAnimationKind as A;
        let (mut nx, mut ny) = (1.0f32, 1.0f32);
        if self.stops.len() >= 2 {
            let mut segs = (self.stops.len() - 1) as f32;
            if self.anim == Some(A::Gradient) {
                segs = segs * 2.0 + 2.0;
            }
            if self.anim == Some(A::Rainbow) {
                segs += 4.0;
            }
            let extent = w * self.dir.0.abs() + h * self.dir.1.abs();
            let n = (extent / (self.len / segs) * 1.5).ceil();
            nx = nx.max((n * self.dir.0.abs()).ceil());
            ny = ny.max((n * self.dir.1.abs()).ceil());
        }
        if self.anim == Some(A::Shimmer) {
            let cell = (self.ink.2 * SHIMMER_HALF / 3.0).max(1.0);
            nx = nx.max((w / cell).ceil());
        }
        (nx.clamp(1.0, 24.0) as u32, ny.clamp(1.0, 24.0) as u32)
    }

    fn at(&self, x: f32, y: f32) -> crate::core::Color {
        use crate::core::color::{linear_to_srgb, srgb_to_linear};
        use crate::mss::fields::TextAnimationKind as A;
        // Работаем в sRGB 0..1, как CSS-градиенты.
        let mut c = if self.stops.len() >= 2 {
            let mut t = ((x - self.center.0) * self.dir.0 + (y - self.center.1) * self.dir.1)
                / self.len
                + 0.5;
            if self.anim == Some(A::Gradient) {
                // Градиент «туда и обратно» с периодом в две длины плывёт вдоль оси: без шва.
                let s = (t - 2.0 * self.phase).rem_euclid(2.0);
                t = if s <= 1.0 { s } else { 2.0 - s };
            }
            sample_stops(&self.stops, t)
        } else {
            let b = self.base;
            [
                linear_to_srgb(b.r),
                linear_to_srgb(b.g),
                linear_to_srgb(b.b),
                b.a,
            ]
        };
        match self.anim {
            Some(A::Rainbow) => {
                let (h, s, l) = rgb_to_hsl(c[0], c[1], c[2]);
                let shift = self.phase * 360.0;
                let (r, g, b) = if s < 0.2 {
                    // Серый/белый текст оттенка не имеет — берём насыщенный цвет его яркости.
                    hsl_to_rgb(shift, 0.85, l.clamp(0.45, 0.68))
                } else {
                    hsl_to_rgb(h + shift, s, l)
                };
                c = [r, g, b, c[3]];
            }
            Some(A::Shimmer) => {
                let u = (x - self.ink.0) / self.ink.2.max(1.0);
                let center = -SHIMMER_HALF * 2.0 + (1.0 + SHIMMER_HALF * 4.0) * self.phase;
                let k = (1.0 - (u - center).abs() / SHIMMER_HALF).clamp(0.0, 1.0);
                let hl = k * k * (3.0 - 2.0 * k);
                let luma = 0.2126 * c[0] + 0.7152 * c[1] + 0.0722 * c[2];
                if luma > 0.6 {
                    // Светлый текст ярче не сделать: остальное приглушено, блик — во всю силу.
                    c[3] *= 0.55 + 0.45 * hl;
                    for i in 0..3 {
                        c[i] += (1.0 - c[i]) * 0.6 * hl;
                    }
                } else {
                    for i in 0..3 {
                        c[i] += (1.0 - c[i]) * 0.7 * hl;
                    }
                }
            }
            _ => {}
        }
        crate::core::Color::new(
            srgb_to_linear(c[0]),
            srgb_to_linear(c[1]),
            srgb_to_linear(c[2]),
            c[3],
        )
    }
}

fn sample_stops(stops: &[([f32; 4], f32)], t: f32) -> [f32; 4] {
    let first = stops[0];
    let last = stops[stops.len() - 1];
    if t <= first.1 {
        return first.0;
    }
    if t >= last.1 {
        return last.0;
    }
    for w in stops.windows(2) {
        let ((c0, p0), (c1, p1)) = (w[0], w[1]);
        if t >= p0 && t <= p1 {
            let k = if p1 - p0 < 1e-6 {
                0.0
            } else {
                (t - p0) / (p1 - p0)
            };
            return [0, 1, 2, 3].map(|i| c0[i] + (c1[i] - c0[i]) * k);
        }
    }
    last.0
}

fn rgb_to_hsl(r: f32, g: f32, b: f32) -> (f32, f32, f32) {
    let max = r.max(g).max(b);
    let min = r.min(g).min(b);
    let l = (max + min) * 0.5;
    let d = max - min;
    if d < 1e-6 {
        return (0.0, 0.0, l);
    }
    let s = d / (1.0 - (2.0 * l - 1.0).abs()).max(1e-6);
    let h = if max == r {
        60.0 * ((g - b) / d).rem_euclid(6.0)
    } else if max == g {
        60.0 * ((b - r) / d + 2.0)
    } else {
        60.0 * ((r - g) / d + 4.0)
    };
    (h, s.min(1.0), l)
}

fn hsl_to_rgb(h: f32, s: f32, l: f32) -> (f32, f32, f32) {
    let h = h.rem_euclid(360.0);
    let c = (1.0 - (2.0 * l - 1.0).abs()) * s;
    let x = c * (1.0 - ((h / 60.0).rem_euclid(2.0) - 1.0).abs());
    let m = l - c * 0.5;
    let (r, g, b) = match (h / 60.0) as u32 {
        0 => (c, x, 0.0),
        1 => (x, c, 0.0),
        2 => (0.0, c, x),
        3 => (0.0, x, c),
        4 => (x, 0.0, c),
        _ => (c, 0.0, x),
    };
    (r + m, g + m, b + m)
}

impl Batcher {
    /// Квад глифа `[x, y, w, h]` (верх сдвинут на `s` — курсив) с UV `[u0, v0, u1, v1]`,
    /// поделённый на `n.0`×`n.1` клеток: цвет вершины — `color(x, y)` в точке до
    /// преобразования (градиент и блик привязаны к тексту).
    #[allow(clippy::too_many_arguments)]
    fn emit_glyph_quad(
        &mut self,
        rect: [f32; 4],
        s: f32,
        uv: [f32; 4],
        n: (u32, u32),
        data: [f32; 4],
        data2: [f32; 4],
        color: &dyn Fn(f32, f32) -> [f32; 4],
    ) {
        use crate::render::Vertex;
        let [x, y, w, h] = rect;
        let (nx, ny) = (n.0.max(1), n.1.max(1));
        let pos = |u: f32, v: f32| [x + u * w + s * (1.0 - v), y + v * h];
        let tex = |u: f32, v: f32| [uv[0] + (uv[2] - uv[0]) * u, uv[1] + (uv[3] - uv[1]) * v];
        for j in 0..ny {
            let (v0, v1) = (j as f32 / ny as f32, (j + 1) as f32 / ny as f32);
            for i in 0..nx {
                let (u0, u1) = (i as f32 / nx as f32, (i + 1) as f32 / nx as f32);
                let corners = [(u0, v0), (u1, v0), (u1, v1), (u0, v1)];
                let p = corners.map(|(u, v)| pos(u, v));
                let cols = p.map(|q| color(q[0], q[1]));
                let tp = self.transform_quad(p);
                let state = self.current_batch_mut();
                let base = state.vertices.len() as u32;
                for k in 0..4 {
                    let (u, v) = corners[k];
                    state.vertices.push(Vertex {
                        position: tp[k],
                        uv: tex(u, v),
                        color: cols[k],
                        data,
                        data2,
                    });
                }
                state.indices.extend_from_slice(&[
                    base,
                    base + 1,
                    base + 2,
                    base,
                    base + 2,
                    base + 3,
                ]);
            }
        }
    }
}
