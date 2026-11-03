//! Глифы системных шрифтов для текстовых слоёв Lottie без встроенных
//! контуров (`chars`). Контуры достаются текстовой подсистемой фреймворка:
//! поиск файла шрифта — font-kit, контур глифа — swash (те же библиотеки,
//! которыми syngui рисует весь текст). Глифы приводятся к формату `chars`:
//! кегль 100 px, базовая линия в y=0, y вниз.

use std::collections::{HashMap, HashSet};

use swash::scale::ScaleContext;
use swash::zeno::Verb;

use super::anim::Animated;
use super::geom::ShapePath;
use super::model::{Composition, Glyph, Layer, LayerKind, Shape};

/// Вес из строки начертания («Medium», «Bold Italic», …).
fn weight_of(style: &str) -> u16 {
    let s = style.to_ascii_lowercase();
    for (needle, w) in [
        ("thin", 100),
        ("hairline", 100),
        ("extralight", 200),
        ("ultralight", 200),
        ("semibold", 600),
        ("demibold", 600),
        ("demi", 600),
        ("extrabold", 800),
        ("ultrabold", 800),
        ("light", 300),
        ("medium", 500),
        ("black", 900),
        ("heavy", 900),
        ("bold", 700),
    ] {
        if s.contains(needle) {
            return w;
        }
    }
    400
}

fn is_italic(style: &str) -> bool {
    let s = style.to_ascii_lowercase();
    s.contains("italic") || s.contains("oblique")
}

/// Контур глифа swash (y вверх) → фигуры Lottie (y вниз, кегль 100).
fn outline_to_shapes(outline: &swash::scale::outline::Outline) -> Vec<Shape> {
    let pts = outline.points();
    let verbs = outline.verbs();
    let p = |i: usize| -> [f32; 2] { [pts[i].x, -pts[i].y] };
    let mut shapes = Vec::new();
    let mut cur: Option<ShapePath> = None;
    let mut pi = 0usize;
    let mut flush = |cur: &mut Option<ShapePath>, shapes: &mut Vec<Shape>| {
        if let Some(sp) = cur.take() {
            if sp.verts.len() >= 2 {
                shapes.push(Shape::Path {
                    path: Animated::Static(sp),
                    reversed: false,
                });
            }
        }
    };
    for v in verbs {
        match v {
            Verb::MoveTo => {
                flush(&mut cur, &mut shapes);
                let mut sp = ShapePath::with_capacity(8, true);
                sp.push(p(pi), [0.0; 2], [0.0; 2]);
                pi += 1;
                cur = Some(sp);
            }
            Verb::LineTo => {
                if let Some(sp) = cur.as_mut() {
                    sp.push(p(pi), [0.0; 2], [0.0; 2]);
                }
                pi += 1;
            }
            Verb::QuadTo => {
                if let Some(sp) = cur.as_mut() {
                    let q = p(pi);
                    let e = p(pi + 1);
                    let last = *sp.verts.last().unwrap_or(&[0.0, 0.0]);
                    // Квадратичная → кубическая.
                    let c1 = [last[0] + 2.0 / 3.0 * (q[0] - last[0]), last[1] + 2.0 / 3.0 * (q[1] - last[1])];
                    let c2 = [e[0] + 2.0 / 3.0 * (q[0] - e[0]), e[1] + 2.0 / 3.0 * (q[1] - e[1])];
                    if let Some(out) = sp.outs.last_mut() {
                        *out = [c1[0] - last[0], c1[1] - last[1]];
                    }
                    sp.push(e, [c2[0] - e[0], c2[1] - e[1]], [0.0; 2]);
                }
                pi += 2;
            }
            Verb::CurveTo => {
                if let Some(sp) = cur.as_mut() {
                    let c1 = p(pi);
                    let c2 = p(pi + 1);
                    let e = p(pi + 2);
                    let last = *sp.verts.last().unwrap_or(&[0.0, 0.0]);
                    if let Some(out) = sp.outs.last_mut() {
                        *out = [c1[0] - last[0], c1[1] - last[1]];
                    }
                    sp.push(e, [c2[0] - e[0], c2[1] - e[1]], [0.0; 2]);
                }
                pi += 3;
            }
            Verb::Close => {
                if let Some(sp) = cur.as_mut() {
                    sp.closed = true;
                }
            }
        }
    }
    flush(&mut cur, &mut shapes);
    shapes
}

fn collect_needed(layers: &[Layer], comp: &Composition, needed: &mut HashSet<(String, String, char)>) {
    for l in layers {
        let LayerKind::Text(td) = &l.kind else {
            continue;
        };
        for (_, doc) in &td.docs {
            let (family, style) = match comp.font(&doc.font) {
                Some(f) => (f.family.clone(), f.style.clone()),
                None => (doc.font.clone(), String::new()),
            };
            for ch in doc.text.chars() {
                if ch == '\r' || ch == '\n' || ch == ' ' {
                    continue;
                }
                if comp.glyph(&family, &style, ch.to_string().as_str()).is_none() {
                    needed.insert((family.clone(), style.clone(), ch));
                }
            }
        }
    }
}

/// Дозагрузить в композицию глифы системных шрифтов для символов текстовых
/// слоёв, которых нет во встроенных `chars`. Возвращает число добавленных.
pub fn load_missing_glyphs(comp: &mut Composition) -> usize {
    let mut needed: HashSet<(String, String, char)> = HashSet::new();
    collect_needed(&comp.layers, comp, &mut needed);
    let precomps: Vec<&Vec<Layer>> = comp.precomps.values().collect();
    for layers in precomps {
        collect_needed(layers, comp, &mut needed);
    }
    if needed.is_empty() {
        return 0;
    }
    // Группировка по шрифту, чтобы искать и парсить файл один раз.
    let mut by_font: HashMap<(String, String), Vec<char>> = HashMap::new();
    for (family, style, ch) in needed {
        by_font.entry((family, style)).or_default().push(ch);
    }
    let mut added = 0usize;
    let mut ctx = ScaleContext::new();
    for ((family, style), chars) in by_font {
        let weight = weight_of(&style);
        let found = crate::text::font_discovery::discover_styled(&family, weight, is_italic(&style)).or_else(|| {
            // Семейства нет в системе — универсальный шрифт того же веса.
            log::info!("lottie: шрифт `{family}` ({style}) не найден, берётся системный по весу {weight}");
            Some(crate::text::font_discovery::discover_weight_font(None, weight)).filter(|(d, _)| !d.is_empty())
        });
        let Some((data, index)) = found else {
            log::warn!("lottie: системный шрифт для `{family}` не найден — текст будет пропущен");
            continue;
        };
        let Some(font) = swash::FontRef::from_index(&data, index as usize) else {
            continue;
        };
        let charmap = font.charmap();
        let metrics = font.glyph_metrics(&[]).scale(100.0);
        let mut scaler = ctx.builder(font).size(100.0).hint(false).build();
        for ch in chars {
            let gid = charmap.map(ch);
            if gid == 0 {
                log::debug!("lottie: в шрифте `{family}` нет символа {ch:?}");
                continue;
            }
            let Some(outline) = scaler.scale_outline(gid) else {
                continue;
            };
            comp.glyphs.insert(
                (family.clone(), style.clone(), ch.to_string()),
                Glyph {
                    shapes: outline_to_shapes(&outline),
                    advance: metrics.advance_width(gid),
                },
            );
            added += 1;
        }
    }
    added
}
