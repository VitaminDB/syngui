//! CPU-растеризация текста в альфа-маску поверх реестра шрифтов [`super::registry`]
//! и системного поиска шрифтов (`swash`). Используется в программных композиторах
//! и исполнителях графов (например, `TextPlus` шаблонов Fusion).

use std::collections::HashMap;
use std::sync::{Arc, Mutex, OnceLock};

use swash::scale::{Render, ScaleContext, Source};
use swash::zeno::Format;
use swash::FontRef;

use super::registry;

/// Параметры растеризации многострочного текста.
#[derive(Clone, Copy, Debug)]
pub struct TextRasterOpts<'a> {
    pub family: &'a str,
    /// Насыщенность (100..900, 400 — обычный, 700 — жирный, 800..900 — тяжёлый).
    pub weight: u16,
    pub italic: bool,
    pub size_px: f32,
    /// Множитель межбуквенного интервала (1.0 — нормальный, >1.0 — разреженный).
    pub tracking: f32,
    /// Множитель межстрочного интервала (1.0 — нормальный).
    pub line_spacing: f32,
    /// Горизонтальное выравнивание: `-1.0` влево, `0.0` по центру, `1.0` вправо.
    pub h_align: f32,
    /// Вертикальное выравнивание: `-1.0` по низу, `0.0` по центру, `1.0` по верху.
    pub v_align: f32,
}

impl Default for TextRasterOpts<'_> {
    fn default() -> Self {
        Self {
            family: "",
            weight: 400,
            italic: false,
            size_px: 48.0,
            tracking: 1.0,
            line_spacing: 1.0,
            h_align: 0.0,
            v_align: 0.0,
        }
    }
}

/// Результат растеризации: одноканальная маска покрытия (0..255) и смещение
/// левого верхнего угла относительно точки якоря (0, 0).
#[derive(Clone, Debug, Default)]
pub struct RasterizedText {
    pub width: u32,
    pub height: u32,
    pub alpha: Vec<u8>,
    pub offset_x: f32,
    pub offset_y: f32,
}

#[derive(Clone)]
struct FontBlob {
    data: Arc<[u8]>,
    index: u32,
}

impl FontBlob {
    fn font_ref(&self) -> Option<FontRef<'_>> {
        FontRef::from_index(&self.data, self.index as usize)
    }
}

fn cache() -> &'static Mutex<HashMap<(String, u16, bool, u64), Option<FontBlob>>> {
    static C: OnceLock<Mutex<HashMap<(String, u16, bool, u64), Option<FontBlob>>>> = OnceLock::new();
    C.get_or_init(|| Mutex::new(HashMap::new()))
}

fn pick_registered(family: &str, weight: u16, italic: bool) -> Option<FontBlob> {
    let files = registry::family_fonts(family);
    if files.is_empty() {
        return None;
    }
    let mut best: Option<(i32, FontBlob)> = None;
    for data in files {
        let Some(f) = FontRef::from_index(&data, 0) else { continue };
        let (fw, var) = registry::face_weight(&data);
        let is_it = matches!(
            f.attributes().style(),
            swash::Style::Italic | swash::Style::Oblique(_)
        );
        let mut score = if var { 10 } else { (fw as i32 - weight as i32).abs() };
        if is_it != italic {
            score += 1000;
        }
        if best.as_ref().is_none_or(|(s, _)| score < *s) {
            best = Some((score, FontBlob { data, index: 0 }));
        }
    }
    best.map(|(_, b)| b)
}

#[cfg(all(not(target_arch = "wasm32"), not(target_os = "android")))]
fn discover_system(family: Option<&str>, weight: u16) -> Option<FontBlob> {
    let (bytes, idx) = super::font_discovery::discover_weight_font(family, weight);
    if bytes.is_empty() {
        return None;
    }
    Some(FontBlob {
        data: Arc::from(bytes),
        index: idx,
    })
}

#[cfg(any(target_arch = "wasm32", target_os = "android"))]
fn discover_system(_family: Option<&str>, _weight: u16) -> Option<FontBlob> {
    None
}

fn resolve_font(family: &str, weight: u16, italic: bool) -> Option<FontBlob> {
    let norm = family.trim().to_lowercase();
    let gen = registry::generation();
    let key = (norm.clone(), weight, italic, gen);
    if let Some(hit) = cache().lock().unwrap_or_else(|e| e.into_inner()).get(&key) {
        return hit.clone();
    }
    let found = if !norm.is_empty() {
        pick_registered(family, weight, italic)
            .or_else(|| discover_system(Some(family), weight))
            .or_else(|| discover_system(None, weight))
    } else {
        discover_system(None, weight)
    };
    cache()
        .lock()
        .unwrap_or_else(|e| e.into_inner())
        .insert(key, found.clone());
    found
}

fn resolve_fallback(weight: u16, italic: bool) -> Option<FontBlob> {
    resolve_font("", weight, italic)
}

fn normalized_wght(font: FontRef<'_>, weight: u16) -> Vec<i16> {
    let wght = swash::tag_from_bytes(b"wght");
    if font.variations().any(|v| v.tag() == wght) {
        font.variations()
            .normalized_coords([(wght, weight as f32)].iter().copied())
            .collect()
    } else {
        Vec::new()
    }
}

struct PlacedGlyph {
    x: f32,
    y: f32,
    width: u32,
    height: u32,
    data: Vec<u8>,
}

/// Растеризовать многострочный текст в альфа-маску с антиалиасингом.
pub fn rasterize_text(text: &str, opts: &TextRasterOpts<'_>) -> RasterizedText {
    if text.is_empty() || opts.size_px <= 0.5 {
        return RasterizedText::default();
    }
    let primary = resolve_font(opts.family, opts.weight, opts.italic);
    let fallback = resolve_fallback(opts.weight, opts.italic);
    let Some(main_blob) = primary.as_ref().or(fallback.as_ref()) else {
        return RasterizedText::default();
    };
    let Some(main_font) = main_blob.font_ref() else {
        return RasterizedText::default();
    };

    let main_coords = normalized_wght(main_font, opts.weight);
    let fb_font = fallback.as_ref().and_then(FontBlob::font_ref);
    let fb_coords = fb_font
        .map(|f| normalized_wght(f, opts.weight))
        .unwrap_or_default();

    let metrics = main_font.metrics(&main_coords).scale(opts.size_px);
    let ascent = metrics.ascent.max(opts.size_px * 0.72);
    let descent = metrics.descent.max(opts.size_px * 0.2);
    let line_h = (ascent + descent + metrics.leading.max(0.0)).max(opts.size_px * 1.1)
        * opts.line_spacing.clamp(0.4, 4.0);
    let extra_track = (opts.tracking - 1.0) * opts.size_px * 0.5;

    let mut ctx = ScaleContext::new();
    let lines: Vec<&str> = text.split('\n').collect();
    let total_h = (lines.len().saturating_sub(1) as f32) * line_h + (ascent + descent);
    // y=0 — точка якоря; v_align: 1 = верх в якоре, 0 = центр в якоре, -1 = низ в якоре
    let top_y = match opts.v_align {
        v if v > 0.3 => 0.0,
        v if v < -0.3 => -total_h,
        _ => -total_h * 0.5,
    };

    let mut placed: Vec<PlacedGlyph> = Vec::new();
    let mut min_x = f32::INFINITY;
    let mut min_y = f32::INFINITY;
    let mut max_x = f32::NEG_INFINITY;
    let mut max_y = f32::NEG_INFINITY;

    for (li, line) in lines.iter().enumerate() {
        let baseline_y = top_y + ascent + li as f32 * line_h;
        let chars: Vec<char> = line.chars().collect();
        let mut advances = Vec::with_capacity(chars.len());
        let mut chosen = Vec::with_capacity(chars.len());
        let mut line_w = 0.0f32;

        for (ci, &ch) in chars.iter().enumerate() {
            let gid_main = main_font.charmap().map(ch);
            let (use_fb, gid) = if gid_main != 0 {
                (false, gid_main)
            } else if let Some(fb) = fb_font {
                let g = fb.charmap().map(ch);
                (g != 0, if g != 0 { g } else { gid_main })
            } else {
                (false, gid_main)
            };
            let (font, coords) = if use_fb {
                (fb_font.unwrap(), fb_coords.as_slice())
            } else {
                (main_font, main_coords.as_slice())
            };
            let gm = font.glyph_metrics(coords).scale(opts.size_px);
            let adv = gm.advance_width(gid);
            let step = if ci + 1 < chars.len() {
                (adv + extra_track).max(0.0)
            } else {
                adv
            };
            line_w += step;
            advances.push(step);
            chosen.push((use_fb, gid));
        }

        // h_align: -1 = левый край в якоре, 0 = центр в якоре, 1 = правый край в якоре
        let mut pen_x = if opts.h_align < -0.3 {
            0.0
        } else if opts.h_align > 0.3 {
            -line_w
        } else {
            -line_w * 0.5
        };

        for ((use_fb, gid), step) in chosen.into_iter().zip(advances.into_iter()) {
            let (font, coords) = if use_fb {
                (fb_font.unwrap(), fb_coords.as_slice())
            } else {
                (main_font, main_coords.as_slice())
            };
            if gid != 0 {
                let mut scaler = ctx
                    .builder(font)
                    .size(opts.size_px)
                    .hint(true)
                    .normalized_coords(coords)
                    .build();
                if let Some(img) = Render::new(&[Source::Outline])
                    .format(Format::Alpha)
                    .render(&mut scaler, gid)
                {
                    let gw = img.placement.width;
                    let gh = img.placement.height;
                    if gw > 0 && gh > 0 {
                        let gx = pen_x + img.placement.left as f32;
                        let gy = baseline_y - img.placement.top as f32;
                        min_x = min_x.min(gx);
                        min_y = min_y.min(gy);
                        max_x = max_x.max(gx + gw as f32);
                        max_y = max_y.max(gy + gh as f32);
                        placed.push(PlacedGlyph {
                            x: gx,
                            y: gy,
                            width: gw,
                            height: gh,
                            data: img.data,
                        });
                    }
                }
            }
            pen_x += step;
        }
    }

    if placed.is_empty() || !min_x.is_finite() {
        return RasterizedText::default();
    }

    let ox = min_x.floor();
    let oy = min_y.floor();
    let w = (max_x.ceil() - ox).max(1.0) as u32;
    let h = (max_y.ceil() - oy).max(1.0) as u32;
    let mut alpha = vec![0u8; (w * h) as usize];

    for g in placed {
        let ix = (g.x - ox).round() as i32;
        let iy = (g.y - oy).round() as i32;
        for row in 0..g.height as i32 {
            let dy = iy + row;
            if dy < 0 || dy >= h as i32 {
                continue;
            }
            let src_off = (row as u32 * g.width) as usize;
            let dst_off = (dy as u32 * w) as usize;
            for col in 0..g.width as i32 {
                let dx = ix + col;
                if dx < 0 || dx >= w as i32 {
                    continue;
                }
                let sa = g.data[src_off + col as usize];
                let d = &mut alpha[dst_off + dx as usize];
                if sa > *d {
                    *d = sa;
                }
            }
        }
    }

    RasterizedText {
        width: w,
        height: h,
        alpha,
        offset_x: ox,
        offset_y: oy,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn rasterizes_latin_and_cyrillic_with_tracking() {
        let normal = rasterize_text(
            "DAVINCI\nПривет",
            &TextRasterOpts {
                size_px: 48.0,
                ..Default::default()
            },
        );
        assert!(normal.width > 40 && normal.height > 40);
        assert!(normal.alpha.iter().any(|&a| a > 200));

        let wide = rasterize_text(
            "DAVINCI",
            &TextRasterOpts {
                size_px: 48.0,
                tracking: 1.8,
                ..Default::default()
            },
        );
        let tight = rasterize_text(
            "DAVINCI",
            &TextRasterOpts {
                size_px: 48.0,
                tracking: 1.0,
                ..Default::default()
            },
        );
        assert!(wide.width > tight.width, "wide={} tight={}", wide.width, tight.width);
        assert_eq!(rasterize_text("", &TextRasterOpts::default()).width, 0);
    }
}
