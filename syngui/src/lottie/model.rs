//! Модель данных Lottie (bodymovin JSON) и её разбор. Поддерживаются слои
//! shape/solid/null/precomp/image, маски, track matte, time remap, parenting,
//! фигуры (rect/ellipse/polystar/path), заливки и обводки (цвет и градиенты),
//! trim paths, repeater, скругление углов. Текстовые слои и выражения —
//! вне объёма (пропускаются с предупреждением в лог).

use std::collections::HashMap;
use std::sync::Arc;

use super::anim::Animated;
use super::geom::{compose_transform, LineCap, LineJoin, Mat, ShapePath};
use super::json::{self, Json};
use super::raster::{BlendMode, FillRule, ImageRgba};

#[derive(Debug)]
pub enum LottieError {
    Json(json::JsonError),
    Invalid(&'static str),
}

impl std::fmt::Display for LottieError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            LottieError::Json(e) => write!(f, "Lottie: {e}"),
            LottieError::Invalid(m) => write!(f, "Lottie: {m}"),
        }
    }
}

impl std::error::Error for LottieError {}

impl From<json::JsonError> for LottieError {
    fn from(e: json::JsonError) -> Self {
        LottieError::Json(e)
    }
}

/// Анимация целиком: главная композиция, прекомпозиции и ассеты.
#[derive(Debug)]
pub struct Composition {
    pub name: String,
    pub width: f32,
    pub height: f32,
    pub frame_rate: f32,
    pub in_point: f32,
    pub out_point: f32,
    pub layers: Vec<Layer>,
    pub precomps: HashMap<String, Vec<Layer>>,
    pub images: HashMap<String, ImageAsset>,
    pub markers: Vec<Marker>,
    /// Сколько слоёв неподдерживаемого типа (текст и т. п.) пропущено.
    pub unsupported_layers: usize,
}

#[derive(Debug, Clone)]
pub struct Marker {
    pub name: String,
    pub frame: f32,
    pub duration: f32,
}

#[derive(Debug)]
pub struct ImageAsset {
    pub id: String,
    pub width: f32,
    pub height: f32,
    /// Путь (каталог `u` + имя `p`), если картинка внешняя.
    pub path: String,
    /// Байты файла из data-URI (`e: 1`), ещё не декодированные.
    pub embedded: Option<Vec<u8>>,
    /// Декодированные пиксели — заполняются [`Composition::load_images`].
    pub pixels: Option<Arc<ImageRgba>>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum MatteMode {
    Alpha,
    AlphaInverted,
    Luma,
    LumaInverted,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum MaskMode {
    None,
    Add,
    Subtract,
    Intersect,
    Lighten,
    Darken,
    Difference,
}

#[derive(Debug, Clone)]
pub struct LayerMask {
    pub mode: MaskMode,
    pub inverted: bool,
    pub opacity: Animated<f32>,
    pub path: Animated<ShapePath>,
}

#[derive(Debug)]
pub enum LayerKind {
    Precomp(String),
    Solid([f32; 3]),
    Image(String),
    Null,
    Shape(Vec<Shape>),
    Unsupported,
}

#[derive(Debug)]
pub struct Layer {
    pub kind: LayerKind,
    pub name: String,
    pub index: i64,
    /// Индекс родителя в том же списке слоёв (разрешён при разборе).
    pub parent: Option<usize>,
    pub transform: Transform,
    pub in_point: f32,
    pub out_point: f32,
    pub start_time: f32,
    pub stretch: f32,
    /// Time remap прекомпозиции: значение в секундах.
    pub time_remap: Option<Animated<f32>>,
    pub masks: Vec<LayerMask>,
    pub matte: Option<MatteMode>,
    /// Индекс слоя-источника matte в том же списке.
    pub matte_source: Option<usize>,
    pub is_matte_source: bool,
    pub blend: BlendMode,
    pub hidden: bool,
    pub auto_orient: bool,
    /// Размер прекомпозиции / сплошного слоя.
    pub width: f32,
    pub height: f32,
}

#[derive(Debug, Clone)]
pub enum Position {
    Joint(Animated<Vec<f32>>),
    Split { x: Animated<f32>, y: Animated<f32> },
}

#[derive(Debug, Clone)]
pub struct Transform {
    pub anchor: Animated<Vec<f32>>,
    pub position: Position,
    pub scale: Animated<Vec<f32>>,
    pub rotation: Animated<f32>,
    /// Проценты 0..100.
    pub opacity: Animated<f32>,
    pub skew: Animated<f32>,
    pub skew_axis: Animated<f32>,
}

fn two(v: &[f32]) -> [f32; 2] {
    [v.first().copied().unwrap_or(0.0), v.get(1).copied().unwrap_or(0.0)]
}

impl Transform {
    pub fn identity() -> Transform {
        Transform {
            anchor: Animated::Static(vec![0.0, 0.0]),
            position: Position::Joint(Animated::Static(vec![0.0, 0.0])),
            scale: Animated::Static(vec![100.0, 100.0]),
            rotation: Animated::Static(0.0),
            opacity: Animated::Static(100.0),
            skew: Animated::Static(0.0),
            skew_axis: Animated::Static(0.0),
        }
    }

    pub fn parse(v: Option<&Json>) -> Transform {
        let Some(v) = v else {
            return Transform::identity();
        };
        let position = match v.get("p") {
            Some(p) if p.bool_or("s", false) => Position::Split {
                x: Animated::parse_or(p.get("x"), 0.0),
                y: Animated::parse_or(p.get("y"), 0.0),
            },
            p => Position::Joint(Animated::parse_or(p, vec![0.0, 0.0])),
        };
        // Поворот: `r`, у 3D-слоёв — `rz`.
        let rotation = Animated::parse(v.get("r"))
            .or_else(|| Animated::parse(v.get("rz")))
            .unwrap_or(Animated::Static(0.0));
        Transform {
            anchor: Animated::parse_or(v.get("a"), vec![0.0, 0.0]),
            position,
            scale: Animated::parse_or(v.get("s"), vec![100.0, 100.0]),
            rotation,
            opacity: Animated::parse_or(v.get("o"), 100.0),
            skew: Animated::parse_or(v.get("sk"), 0.0),
            skew_axis: Animated::parse_or(v.get("sa"), 0.0),
        }
    }

    pub fn position_at(&self, frame: f32) -> [f32; 2] {
        match &self.position {
            Position::Joint(p) => p.position(frame),
            Position::Split { x, y } => [x.value(frame), y.value(frame)],
        }
    }

    pub fn matrix(&self, frame: f32) -> Mat {
        self.matrix_with(frame, 0.0)
    }

    /// Матрица с дополнительным поворотом (auto-orient).
    pub fn matrix_with(&self, frame: f32, extra_rotation: f32) -> Mat {
        let s = self.scale.value(frame);
        compose_transform(
            two(&self.anchor.value(frame)),
            self.position_at(frame),
            [s.first().copied().unwrap_or(100.0) / 100.0, s.get(1).copied().unwrap_or(100.0) / 100.0],
            self.rotation.value(frame) + extra_rotation,
            self.skew.value(frame),
            self.skew_axis.value(frame),
        )
    }

    /// Непрозрачность 0..1.
    pub fn opacity_at(&self, frame: f32) -> f32 {
        (self.opacity.value(frame) / 100.0).clamp(0.0, 1.0)
    }

    pub fn is_static(&self) -> bool {
        let pos_static = match &self.position {
            Position::Joint(p) => !p.is_animated(),
            Position::Split { x, y } => !x.is_animated() && !y.is_animated(),
        };
        pos_static
            && !self.anchor.is_animated()
            && !self.scale.is_animated()
            && !self.rotation.is_animated()
            && !self.opacity.is_animated()
            && !self.skew.is_animated()
            && !self.skew_axis.is_animated()
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum GradientKind {
    Linear,
    Radial,
}

#[derive(Debug, Clone)]
pub struct Gradient {
    pub kind: GradientKind,
    pub start: Animated<Vec<f32>>,
    pub end: Animated<Vec<f32>>,
    /// Длина смещения фокуса (проценты радиуса) и его угол.
    pub highlight_length: Animated<f32>,
    pub highlight_angle: Animated<f32>,
    pub stops: Animated<Vec<f32>>,
    pub color_count: usize,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum DashKind {
    Dash,
    Gap,
    Offset,
}

#[derive(Debug, Clone)]
pub struct StrokeStyle {
    pub width: Animated<f32>,
    pub cap: LineCap,
    pub join: LineJoin,
    pub miter_limit: f32,
    pub dashes: Vec<(DashKind, Animated<f32>)>,
}

#[derive(Debug)]
pub enum Shape {
    Group {
        name: String,
        items: Vec<Shape>,
        transform: Transform,
    },
    Rect {
        position: Animated<Vec<f32>>,
        size: Animated<Vec<f32>>,
        roundness: Animated<f32>,
        reversed: bool,
    },
    Ellipse {
        position: Animated<Vec<f32>>,
        size: Animated<Vec<f32>>,
        reversed: bool,
    },
    Polystar {
        position: Animated<Vec<f32>>,
        rotation: Animated<f32>,
        points: Animated<f32>,
        outer_radius: Animated<f32>,
        inner_radius: Animated<f32>,
        outer_roundness: Animated<f32>,
        inner_roundness: Animated<f32>,
        star: bool,
        reversed: bool,
    },
    Path {
        path: Animated<ShapePath>,
        reversed: bool,
    },
    Fill {
        color: Animated<Vec<f32>>,
        opacity: Animated<f32>,
        rule: FillRule,
    },
    Stroke {
        color: Animated<Vec<f32>>,
        opacity: Animated<f32>,
        style: StrokeStyle,
    },
    GradientFill {
        gradient: Gradient,
        opacity: Animated<f32>,
        rule: FillRule,
    },
    GradientStroke {
        gradient: Gradient,
        opacity: Animated<f32>,
        style: StrokeStyle,
    },
    Trim {
        start: Animated<f32>,
        end: Animated<f32>,
        offset: Animated<f32>,
        individually: bool,
    },
    Repeater {
        copies: Animated<f32>,
        offset: Animated<f32>,
        transform: Transform,
        start_opacity: Animated<f32>,
        end_opacity: Animated<f32>,
        /// Копии накладываются сверху (`m: 1`) или снизу.
        above: bool,
    },
    RoundCorners {
        radius: Animated<f32>,
    },
    /// Merge paths / twist / pucker / zigzag / offset: контуры рисуются как есть.
    Ignored,
}

/// Цвет `[r,g,b(,a)]`: старые экспортёры пишут 0..255.
fn normalize_color(v: &[f32]) -> Vec<f32> {
    if v.iter().take(3).any(|c| *c > 1.0) {
        v.iter().map(|c| c / 255.0).collect()
    } else {
        v.to_vec()
    }
}

fn parse_color(v: Option<&Json>) -> Animated<Vec<f32>> {
    Animated::parse_or(v, vec![0.0, 0.0, 0.0]).map(|c| normalize_color(c))
}

fn parse_fill_rule(v: &Json) -> FillRule {
    if v.i64_or("r", 1) == 2 {
        FillRule::EvenOdd
    } else {
        FillRule::NonZero
    }
}

fn parse_stroke_style(v: &Json) -> StrokeStyle {
    let dashes = v
        .get("d")
        .and_then(Json::as_array)
        .map(|arr| {
            arr.iter()
                .filter_map(|d| {
                    let kind = match d.str_or("n", "d") {
                        "o" => DashKind::Offset,
                        "g" => DashKind::Gap,
                        _ => DashKind::Dash,
                    };
                    Some((kind, Animated::parse(d.get("v"))?))
                })
                .collect()
        })
        .unwrap_or_default();
    StrokeStyle {
        width: Animated::parse_or(v.get("w"), 1.0),
        cap: match v.i64_or("lc", 2) {
            1 => LineCap::Butt,
            3 => LineCap::Square,
            _ => LineCap::Round,
        },
        join: match v.i64_or("lj", 2) {
            1 => LineJoin::Miter,
            3 => LineJoin::Bevel,
            _ => LineJoin::Round,
        },
        miter_limit: v.f32_or("ml", 4.0),
        dashes,
    }
}

fn parse_gradient(v: &Json) -> Gradient {
    let g = v.get("g");
    let color_count = g.map(|g| g.i64_or("p", 0)).unwrap_or(0).max(0) as usize;
    let stops = Animated::parse_or(g.and_then(|g| g.get("k")), Vec::new());
    // Если `p` отсутствует, число цветов — по длине массива (по 4 на стоп).
    let color_count = if color_count == 0 {
        stops.first_value().len() / 4
    } else {
        color_count
    };
    Gradient {
        kind: if v.i64_or("t", 1) == 2 { GradientKind::Radial } else { GradientKind::Linear },
        start: Animated::parse_or(v.get("s"), vec![0.0, 0.0]),
        end: Animated::parse_or(v.get("e"), vec![100.0, 0.0]),
        highlight_length: Animated::parse_or(v.get("h"), 0.0),
        highlight_angle: Animated::parse_or(v.get("a"), 0.0),
        stops,
        color_count,
    }
}

fn parse_shape(v: &Json) -> Option<Shape> {
    if v.bool_or("hd", false) {
        return None;
    }
    let reversed = v.i64_or("d", 1) == 3;
    let shape = match v.str_or("ty", "") {
        "gr" => {
            let items = v
                .get("it")
                .and_then(Json::as_array)
                .map(|a| a.iter().filter_map(parse_shape).collect::<Vec<_>>())
                .unwrap_or_default();
            // Трансформация группы лежит внутри `it` элементом `tr`.
            let transform = v
                .get("it")
                .and_then(Json::as_array)
                .and_then(|a| a.iter().find(|i| i.str_or("ty", "") == "tr"))
                .map(|tr| Transform::parse(Some(tr)))
                .unwrap_or_else(Transform::identity);
            Shape::Group {
                name: v.str_or("nm", "").to_string(),
                items,
                transform,
            }
        }
        "tr" => return None,
        "rc" => Shape::Rect {
            position: Animated::parse_or(v.get("p"), vec![0.0, 0.0]),
            size: Animated::parse_or(v.get("s"), vec![0.0, 0.0]),
            roundness: Animated::parse_or(v.get("r"), 0.0),
            reversed,
        },
        "el" => Shape::Ellipse {
            position: Animated::parse_or(v.get("p"), vec![0.0, 0.0]),
            size: Animated::parse_or(v.get("s"), vec![0.0, 0.0]),
            reversed,
        },
        "sr" => Shape::Polystar {
            position: Animated::parse_or(v.get("p"), vec![0.0, 0.0]),
            rotation: Animated::parse_or(v.get("r"), 0.0),
            points: Animated::parse_or(v.get("pt"), 5.0),
            outer_radius: Animated::parse_or(v.get("or"), 0.0),
            inner_radius: Animated::parse_or(v.get("ir"), 0.0),
            outer_roundness: Animated::parse_or(v.get("os"), 0.0),
            inner_roundness: Animated::parse_or(v.get("is"), 0.0),
            star: v.i64_or("sy", 1) == 1,
            reversed,
        },
        "sh" => Shape::Path {
            path: Animated::parse_or(v.get("ks"), ShapePath::default()),
            reversed,
        },
        "fl" => Shape::Fill {
            color: parse_color(v.get("c")),
            opacity: Animated::parse_or(v.get("o"), 100.0),
            rule: parse_fill_rule(v),
        },
        "st" => Shape::Stroke {
            color: parse_color(v.get("c")),
            opacity: Animated::parse_or(v.get("o"), 100.0),
            style: parse_stroke_style(v),
        },
        "gf" => Shape::GradientFill {
            gradient: parse_gradient(v),
            opacity: Animated::parse_or(v.get("o"), 100.0),
            rule: parse_fill_rule(v),
        },
        "gs" => Shape::GradientStroke {
            gradient: parse_gradient(v),
            opacity: Animated::parse_or(v.get("o"), 100.0),
            style: parse_stroke_style(v),
        },
        "tm" => Shape::Trim {
            start: Animated::parse_or(v.get("s"), 0.0),
            end: Animated::parse_or(v.get("e"), 100.0),
            offset: Animated::parse_or(v.get("o"), 0.0),
            individually: v.i64_or("m", 1) == 2,
        },
        "rp" => {
            let tr = v.get("tr");
            Shape::Repeater {
                copies: Animated::parse_or(v.get("c"), 1.0),
                offset: Animated::parse_or(v.get("o"), 0.0),
                transform: Transform::parse(tr),
                start_opacity: Animated::parse_or(tr.and_then(|t| t.get("so")), 100.0),
                end_opacity: Animated::parse_or(tr.and_then(|t| t.get("eo")), 100.0),
                above: v.i64_or("m", 1) == 1,
            }
        }
        "rd" => Shape::RoundCorners {
            radius: Animated::parse_or(v.get("r"), 0.0),
        },
        "mm" | "tw" | "pb" | "zz" | "op" => Shape::Ignored,
        other => {
            log::debug!("lottie: неизвестный элемент фигуры `{other}` пропущен");
            Shape::Ignored
        }
    };
    Some(shape)
}

fn parse_mask(v: &Json) -> Option<LayerMask> {
    let mode = match v.str_or("mode", "a") {
        "n" => MaskMode::None,
        "a" => MaskMode::Add,
        "s" => MaskMode::Subtract,
        "i" => MaskMode::Intersect,
        "l" => MaskMode::Lighten,
        "d" => MaskMode::Darken,
        "f" => MaskMode::Difference,
        _ => MaskMode::Add,
    };
    if mode == MaskMode::None {
        return None;
    }
    Some(LayerMask {
        mode,
        inverted: v.bool_or("inv", false),
        opacity: Animated::parse_or(v.get("o"), 100.0),
        path: Animated::parse_or(v.get("pt"), ShapePath::default()),
    })
}

fn parse_hex_color(s: &str) -> [f32; 3] {
    let h = s.trim_start_matches('#');
    let c = |i: usize| -> f32 {
        h.get(i..i + 2)
            .and_then(|x| u8::from_str_radix(x, 16).ok())
            .map(|v| v as f32 / 255.0)
            .unwrap_or(0.0)
    };
    if h.len() >= 6 {
        [c(0), c(2), c(4)]
    } else {
        [0.0, 0.0, 0.0]
    }
}

fn parse_layer(v: &Json, unsupported: &mut usize) -> Layer {
    let ty = v.i64_or("ty", -1);
    let kind = match ty {
        0 => LayerKind::Precomp(v.str_or("refId", "").to_string()),
        1 => LayerKind::Solid(parse_hex_color(v.str_or("sc", "#000000"))),
        2 => LayerKind::Image(v.str_or("refId", "").to_string()),
        3 => LayerKind::Null,
        4 => LayerKind::Shape(
            v.get("shapes")
                .and_then(Json::as_array)
                .map(|a| a.iter().filter_map(parse_shape).collect())
                .unwrap_or_default(),
        ),
        other => {
            *unsupported += 1;
            log::warn!(
                "lottie: слой `{}` типа {other} не поддерживается (текст/аудио/камера) и пропущен",
                v.str_or("nm", "")
            );
            LayerKind::Unsupported
        }
    };
    let matte = match v.i64_or("tt", 0) {
        1 => Some(MatteMode::Alpha),
        2 => Some(MatteMode::AlphaInverted),
        3 => Some(MatteMode::Luma),
        4 => Some(MatteMode::LumaInverted),
        _ => None,
    };
    let (width, height) = match kind {
        LayerKind::Solid(_) => (v.f32_or("sw", 0.0), v.f32_or("sh", 0.0)),
        _ => (v.f32_or("w", 0.0), v.f32_or("h", 0.0)),
    };
    Layer {
        kind,
        name: v.str_or("nm", "").to_string(),
        index: v.i64_or("ind", -1),
        parent: None,
        transform: Transform::parse(v.get("ks")),
        in_point: v.f32_or("ip", 0.0),
        out_point: v.f32_or("op", f32::MAX),
        start_time: v.f32_or("st", 0.0),
        stretch: {
            let sr = v.f32_or("sr", 1.0);
            if sr.abs() < 1e-6 { 1.0 } else { sr }
        },
        time_remap: Animated::parse(v.get("tm")),
        masks: v
            .get("masksProperties")
            .and_then(Json::as_array)
            .map(|a| a.iter().filter_map(parse_mask).collect())
            .unwrap_or_default(),
        matte,
        matte_source: None,
        is_matte_source: v.i64_or("td", 0) == 1,
        blend: BlendMode::from_lottie(v.i64_or("bm", 0)),
        hidden: v.bool_or("hd", false),
        auto_orient: v.i64_or("ao", 0) == 1,
        width,
        height,
    }
}

/// Разобрать список слоёв и связать parent/matte по индексам `ind`.
fn parse_layers(v: Option<&Json>, unsupported: &mut usize) -> Vec<Layer> {
    let mut layers: Vec<Layer> = v
        .and_then(Json::as_array)
        .map(|a| a.iter().map(|l| parse_layer(l, unsupported)).collect())
        .unwrap_or_default();
    let by_ind: HashMap<i64, usize> = layers
        .iter()
        .enumerate()
        .filter(|(_, l)| l.index >= 0)
        .map(|(i, l)| (l.index, i))
        .collect();
    let raw = v.and_then(Json::as_array).unwrap_or(&[]);
    for i in 0..layers.len() {
        if let Some(p) = raw.get(i).and_then(|l| l.get("parent")).and_then(Json::as_i64) {
            let idx = by_ind.get(&p).copied();
            layers[i].parent = idx.filter(|&pi| pi != i);
        }
        if layers[i].matte.is_some() {
            let tp = raw.get(i).and_then(|l| l.get("tp")).and_then(Json::as_i64);
            layers[i].matte_source = match tp.and_then(|t| by_ind.get(&t).copied()) {
                Some(src) => Some(src),
                None if i > 0 => Some(i - 1),
                None => None,
            };
            if let Some(src) = layers[i].matte_source {
                layers[src].is_matte_source = true;
            }
        }
    }
    layers
}

/// Декодер base64 (стандартный и URL-safe алфавит, с/без `=`).
pub fn base64_decode(s: &str) -> Option<Vec<u8>> {
    let mut out = Vec::with_capacity(s.len() * 3 / 4);
    let mut buf = 0u32;
    let mut bits = 0u32;
    for b in s.bytes() {
        let v = match b {
            b'A'..=b'Z' => b - b'A',
            b'a'..=b'z' => b - b'a' + 26,
            b'0'..=b'9' => b - b'0' + 52,
            b'+' | b'-' => 62,
            b'/' | b'_' => 63,
            b'=' | b'\n' | b'\r' | b' ' | b'\t' => continue,
            _ => return None,
        } as u32;
        buf = (buf << 6) | v;
        bits += 6;
        if bits >= 8 {
            bits -= 8;
            out.push((buf >> bits) as u8);
            buf &= (1 << bits) - 1;
        }
    }
    Some(out)
}

impl Composition {
    #[allow(clippy::should_implement_trait)]
    pub fn from_str(text: &str) -> Result<Composition, LottieError> {
        let root = json::parse(text)?;
        Composition::from_json(&root)
    }

    pub fn from_bytes(bytes: &[u8]) -> Result<Composition, LottieError> {
        let text = std::str::from_utf8(bytes).map_err(|_| LottieError::Invalid("файл не в UTF-8"))?;
        Composition::from_str(text.trim_start_matches('\u{feff}'))
    }

    pub fn from_json(root: &Json) -> Result<Composition, LottieError> {
        if !root.is_object() {
            return Err(LottieError::Invalid("корень документа не объект"));
        }
        if root.get("layers").is_none() {
            return Err(LottieError::Invalid("нет поля `layers` — это не Lottie"));
        }
        let mut unsupported = 0usize;
        let layers = parse_layers(root.get("layers"), &mut unsupported);
        let mut precomps = HashMap::new();
        let mut images = HashMap::new();
        if let Some(assets) = root.get("assets").and_then(Json::as_array) {
            for a in assets {
                let id = a.str_or("id", "").to_string();
                if a.get("layers").is_some() {
                    precomps.insert(id, parse_layers(a.get("layers"), &mut unsupported));
                } else if a.get("p").is_some() {
                    let p = a.str_or("p", "");
                    let embedded = if p.starts_with("data:") {
                        p.find(',').and_then(|i| base64_decode(&p[i + 1..]))
                    } else {
                        None
                    };
                    let path = if embedded.is_some() {
                        String::new()
                    } else {
                        format!("{}{}", a.str_or("u", ""), p)
                    };
                    images.insert(
                        id.clone(),
                        ImageAsset {
                            id,
                            width: a.f32_or("w", 0.0),
                            height: a.f32_or("h", 0.0),
                            path,
                            embedded,
                            pixels: None,
                        },
                    );
                }
            }
        }
        let markers = root
            .get("markers")
            .and_then(Json::as_array)
            .map(|a| {
                a.iter()
                    .map(|m| Marker {
                        name: m.str_or("cm", "").to_string(),
                        frame: m.f32_or("tm", 0.0),
                        duration: m.f32_or("dr", 0.0),
                    })
                    .collect()
            })
            .unwrap_or_default();
        let frame_rate = root.f32_or("fr", 30.0).max(1.0);
        let in_point = root.f32_or("ip", 0.0);
        let out_point = root.f32_or("op", in_point + frame_rate);
        Ok(Composition {
            name: root.str_or("nm", "").to_string(),
            width: root.f32_or("w", 0.0).max(1.0),
            height: root.f32_or("h", 0.0).max(1.0),
            frame_rate,
            in_point,
            out_point,
            layers,
            precomps,
            images,
            markers,
            unsupported_layers: unsupported,
        })
    }

    /// Длительность в кадрах.
    pub fn frame_count(&self) -> f32 {
        (self.out_point - self.in_point).max(0.0)
    }

    pub fn duration_secs(&self) -> f32 {
        self.frame_count() / self.frame_rate
    }

    /// Загрузить растровые ассеты: `decode` получает байты файла (из data-URI
    /// или прочитанного `path`) и возвращает RGBA8 с прямой альфой.
    pub fn load_images(&mut self, mut decode: impl FnMut(&ImageAsset) -> Option<ImageRgba>) {
        for asset in self.images.values_mut() {
            if asset.pixels.is_none() {
                asset.pixels = decode(asset).map(Arc::new);
            }
        }
    }

    pub fn has_images(&self) -> bool {
        !self.images.is_empty()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn base64_roundtrip() {
        assert_eq!(base64_decode("aGVsbG8=").unwrap(), b"hello");
        assert_eq!(base64_decode("aGVsbG8").unwrap(), b"hello");
        assert_eq!(base64_decode("AAEC").unwrap(), vec![0, 1, 2]);
    }

    #[test]
    fn parses_minimal_composition() {
        let c = Composition::from_str(
            r#"{"v":"5.5.7","fr":30,"ip":0,"op":60,"w":100,"h":50,"layers":[
              {"ty":4,"ind":1,"nm":"a","ks":{"o":{"a":0,"k":50},"p":{"a":0,"k":[10,20]}},"ip":0,"op":60,"st":0,
               "shapes":[{"ty":"rc","p":{"a":0,"k":[0,0]},"s":{"a":0,"k":[10,10]},"r":{"a":0,"k":0}},
                         {"ty":"fl","c":{"a":0,"k":[255,0,0,1]},"o":{"a":0,"k":100}}]},
              {"ty":3,"ind":2,"parent":1,"ks":{}}
            ]}"#,
        )
        .unwrap();
        assert_eq!(c.layers.len(), 2);
        assert_eq!(c.layers[1].parent, Some(0));
        assert!((c.layers[0].transform.opacity_at(0.0) - 0.5).abs() < 1e-6);
        match &c.layers[0].kind {
            LayerKind::Shape(items) => match &items[1] {
                Shape::Fill { color, .. } => assert_eq!(color.first_value()[0], 1.0),
                _ => panic!("ожидалась заливка"),
            },
            _ => panic!("ожидался shape-слой"),
        }
    }
}
