//! Модель данных Lottie (bodymovin JSON) и её разбор. Поддерживаются слои
//! shape/solid/null/precomp/image, маски, track matte, time remap, parenting,
//! фигуры (rect/ellipse/polystar/path), заливки и обводки (цвет и градиенты),
//! trim paths, repeater, скругление углов, текстовые слои с аниматорами
//! и текстом по контуру.

use std::collections::HashMap;
use std::sync::Arc;

use super::anim::Animated;
use super::geom::{compose_transform, LineCap, LineJoin, Mat, Mat4, ShapePath};
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
    pub fonts: Vec<FontDef>,
    /// Встроенные глифы: ключ — (семейство, начертание, символ).
    pub glyphs: HashMap<(String, String, String), Glyph>,
    /// Сколько слоёв неподдерживаемого типа (аудио, камера и т. п.) пропущено.
    pub unsupported_layers: usize,
    /// Параметры воспроизведения из манифеста dotLottie (если анимация
    /// загружена из .lottie и манифест их задаёт).
    pub playback: Option<DotLottieAnimation>,
    /// Имя прекомпозиции (`nm` ассета) → её id — для `comp('…')`.
    pub precomp_names: HashMap<String, String>,
    /// В анимации есть выражения (рендер ставит контекст слоя).
    pub has_expressions: bool,
}

/// Анимация из манифеста dotLottie и её параметры воспроизведения.
#[derive(Debug, Clone, Default)]
pub struct DotLottieAnimation {
    pub id: String,
    pub speed: Option<f32>,
    pub looping: Option<bool>,
    pub autoplay: Option<bool>,
    /// 1 — вперёд, −1 — назад.
    pub direction: Option<f32>,
    /// Режим `bounce`: туда-обратно.
    pub bounce: bool,
    pub theme_color: Option<[f32; 3]>,
}

/// Манифест dotLottie: список анимаций и анимация по умолчанию.
#[derive(Debug, Clone, Default)]
pub struct DotLottieManifest {
    /// Версия формата (`"1"`/`"1.0"` или `"2"`).
    pub version: String,
    pub animations: Vec<DotLottieAnimation>,
    /// `activeAnimationId` (v1) или `initial.animation` (v2).
    pub active: Option<String>,
}

impl DotLottieManifest {
    fn parse(v: &Json) -> DotLottieManifest {
        let animations = v
            .get("animations")
            .and_then(Json::as_array)
            .map(|arr| {
                arr.iter()
                    .map(|a| DotLottieAnimation {
                        id: a.str_or("id", "").to_string(),
                        speed: a.get("speed").and_then(Json::as_f32),
                        looping: a.get("loop").and_then(|l| match l {
                            Json::Bool(b) => Some(*b),
                            Json::Num(n) => Some(*n != 0.0),
                            _ => None,
                        }),
                        autoplay: a.get("autoplay").and_then(|l| match l {
                            Json::Bool(b) => Some(*b),
                            _ => None,
                        }),
                        direction: a.get("direction").and_then(Json::as_f32),
                        bounce: a.str_or("mode", "normal") == "bounce",
                        theme_color: a.get("themeColor").and_then(|c| match c {
                            Json::Str(s) => Some(parse_hex_color(s)),
                            _ => None,
                        }),
                    })
                    .collect()
            })
            .unwrap_or_default();
        let active = v
            .get("activeAnimationId")
            .and_then(|a| match a {
                Json::Str(s) => Some(s.clone()),
                _ => None,
            })
            .or_else(|| {
                v.get("initial")
                    .and_then(|i| i.get("animation"))
                    .and_then(|a| match a {
                        Json::Str(s) => Some(s.clone()),
                        _ => None,
                    })
            })
            .filter(|s| !s.is_empty());
        DotLottieManifest {
            version: v.str_or("version", "").to_string(),
            animations,
            active,
        }
    }
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
    /// Расширение (+) / сужение (−) маски в единицах слоя (`x`).
    pub expand: Animated<f32>,
}

#[derive(Debug, Clone)]
pub struct FontDef {
    /// Имя для ссылок из текста (`fName`).
    pub name: String,
    pub family: String,
    pub style: String,
    /// Подъём базовой линии в единицах на 100 px кегля.
    pub ascent: f32,
}

/// Встроенный глиф (`chars[]`): контуры на кегль 100 px.
#[derive(Debug)]
pub struct Glyph {
    pub shapes: Vec<Shape>,
    /// Ширина (advance) на 100 px кегля.
    pub advance: f32,
}

/// Текстовый документ — состояние текста слоя на отрезке кадров.
#[derive(Debug, Clone)]
pub struct TextDoc {
    pub text: String,
    pub font: String,
    pub size: f32,
    pub fill: Option<Vec<f32>>,
    pub stroke: Option<Vec<f32>>,
    pub stroke_width: f32,
    /// Обводка поверх заливки.
    pub stroke_over_fill: bool,
    pub line_height: f32,
    pub baseline_shift: f32,
    /// Межбуквенный интервал (единицы на 1000 кегля).
    pub tracking: f32,
    /// 0 — влево, 1 — вправо, 2 — по центру.
    pub justify: i64,
    /// Левый верх текстового бокса (параграфный текст).
    pub box_pos: Option<[f32; 2]>,
    pub box_size: Option<[f32; 2]>,
}

/// Единицы, по которым считается индекс буквы в селекторе (`b`).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum TextBasedOn {
    Characters,
    CharactersExcludingSpaces,
    Words,
    Lines,
}

/// Форма селектора (`sh`).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum SelectorShape {
    Square,
    RampUp,
    RampDown,
    Triangle,
    Round,
    Smooth,
}

/// Range-селектор аниматора текста (`t.a[].s`).
#[derive(Debug)]
pub struct TextSelector {
    pub based_on: TextBasedOn,
    pub shape: SelectorShape,
    /// Единицы start/end/offset: `true` — индексы, `false` — проценты.
    pub index_units: bool,
    pub start: Animated<f32>,
    pub end: Animated<f32>,
    pub offset: Animated<f32>,
    /// Проценты 0..100.
    pub amount: Animated<f32>,
    /// Ease High / Ease Low (`xe`/`ne`), проценты −100..100.
    pub ease_high: Animated<f32>,
    pub ease_low: Animated<f32>,
    /// Smoothness (`sm`, только для формы Square), проценты.
    pub smoothness: Animated<f32>,
    /// Порядок букв перемешивается (`rn`).
    pub randomize: bool,
}

/// Анимируемые свойства аниматора текста (`t.a[].a`): значение действует
/// на букву с весом селектора.
#[derive(Debug, Default)]
pub struct TextAnimProps {
    pub anchor: Option<Animated<Vec<f32>>>,
    pub position: Option<Animated<Vec<f32>>>,
    /// Проценты.
    pub scale: Option<Animated<Vec<f32>>>,
    pub rotation: Option<Animated<f32>>,
    pub skew: Option<Animated<f32>>,
    pub skew_axis: Option<Animated<f32>>,
    /// Проценты 0..100.
    pub opacity: Option<Animated<f32>>,
    pub fill: Option<Animated<Vec<f32>>>,
    pub stroke: Option<Animated<Vec<f32>>>,
    pub stroke_width: Option<Animated<f32>>,
    /// Трекинг (единицы на 1000 кегля).
    pub tracking: Option<Animated<f32>>,
    /// Сдвиг тона заливки (градусы), насыщенности и яркости (проценты).
    pub fill_hue: Option<Animated<f32>>,
    pub fill_saturation: Option<Animated<f32>>,
    pub fill_brightness: Option<Animated<f32>>,
}

#[derive(Debug)]
pub struct TextAnimator {
    pub selector: Option<TextSelector>,
    pub props: TextAnimProps,
}

/// Текст по контуру маски (`t.p`).
#[derive(Debug)]
pub struct TextPathOpts {
    /// Индекс маски слоя (среди всех `masksProperties`).
    pub mask: usize,
    pub first_margin: Animated<f32>,
    pub last_margin: Animated<f32>,
    pub reversed: Animated<f32>,
    pub perpendicular: Animated<f32>,
    pub force_align: Animated<f32>,
}

#[derive(Debug)]
pub struct TextData {
    /// Документы по кадрам (hold-ключи): действует последний с `t <= frame`.
    pub docs: Vec<(f32, TextDoc)>,
    pub animators: Vec<TextAnimator>,
    /// Выравнивание точки привязки букв (`m.a`, проценты).
    pub anchor_align: Animated<Vec<f32>>,
    pub path: Option<TextPathOpts>,
    /// Выражение `sourceText` (`t.d.x`): `value` — текст документа,
    /// результат — новый текст.
    pub expr: Option<std::sync::Arc<super::expr::Program>>,
}

impl TextData {
    pub fn doc_at(&self, frame: f32) -> Option<&TextDoc> {
        self.docs
            .iter()
            .rev()
            .find(|(t, _)| *t <= frame)
            .or(self.docs.first())
            .map(|(_, d)| d)
    }
}

/// Эффект слоя (поддерживаемое подмножество).
#[derive(Debug)]
pub enum Effect {
    /// Tint (20): карта чёрного/белого по яркости.
    Tint {
        black: Animated<Vec<f32>>,
        white: Animated<Vec<f32>>,
        /// Проценты 0..100.
        amount: Animated<f32>,
    },
    /// Fill (21): перекраска непрозрачных пикселей.
    Fill {
        color: Animated<Vec<f32>>,
        /// 0..1.
        opacity: Animated<f32>,
    },
    /// Drop Shadow (25).
    DropShadow {
        color: Animated<Vec<f32>>,
        /// 0..255.
        opacity: Animated<f32>,
        /// Градусы, 0 — вверх, по часовой.
        direction: Animated<f32>,
        distance: Animated<f32>,
        softness: Animated<f32>,
        shadow_only: bool,
    },
    /// Gaussian Blur (29).
    GaussianBlur {
        blurriness: Animated<f32>,
        /// 1 — оба направления, 2 — горизонталь, 3 — вертикаль.
        dimensions: Animated<f32>,
    },
    /// Tritone (23): света/полутона/тени по яркости.
    Tritone {
        highlights: Animated<Vec<f32>>,
        midtones: Animated<Vec<f32>>,
        shadows: Animated<Vec<f32>>,
        /// Смешивание с исходным, проценты.
        blend: Animated<f32>,
    },
    /// Pro Levels (24): по каналам RGB, R, G, B, A — вход чёрное/белое,
    /// гамма, выход чёрное/белое.
    Levels { channels: Vec<[Animated<f32>; 5]> },
    /// Radial Wipe (26).
    RadialWipe {
        /// Проценты.
        completion: Animated<f32>,
        start_angle: Animated<f32>,
        center: Animated<Vec<f32>>,
        /// 1 — по часовой, 2 — против, 3 — в обе стороны.
        direction: i64,
        feather: Animated<f32>,
    },
    /// Stroke (22): обводка масок слоя.
    Stroke {
        /// Номер маски (с 1), если не `all_masks`.
        mask: usize,
        all_masks: bool,
        color: Animated<Vec<f32>>,
        brush_size: Animated<f32>,
        /// 0..1 (в файлах — проценты или доли).
        opacity: Animated<f32>,
        /// Проценты.
        start: Animated<f32>,
        end: Animated<f32>,
        /// 1 — поверх слоя, 2 — на прозрачном, 3 — проявить слой.
        paint_style: i64,
    },
}

/// Стиль слоя (`sy`, layer styles Photoshop-образца).
#[derive(Debug)]
pub enum LayerStyle {
    /// Drop Shadow (1) и Inner Shadow (2).
    Shadow {
        inner: bool,
        color: Animated<Vec<f32>>,
        /// Проценты.
        opacity: Animated<f32>,
        /// Угол источника света, градусы (90 — свет сверху, тень вниз).
        angle: Animated<f32>,
        distance: Animated<f32>,
        size: Animated<f32>,
        /// Spread / Choke, проценты.
        spread: Animated<f32>,
    },
    /// Outer Glow (3) и Inner Glow (4).
    Glow {
        inner: bool,
        color: Animated<Vec<f32>>,
        opacity: Animated<f32>,
        size: Animated<f32>,
        spread: Animated<f32>,
        /// Inner Glow: источник от центра (`sr: 1`) или от края.
        from_center: bool,
    },
    /// Stroke (0): обводка снаружи.
    Stroke {
        color: Animated<Vec<f32>>,
        opacity: Animated<f32>,
        size: Animated<f32>,
    },
    /// Color Overlay (7).
    ColorOverlay {
        color: Animated<Vec<f32>>,
        opacity: Animated<f32>,
    },
}

#[derive(Debug)]
pub enum LayerKind {
    Precomp(String),
    Solid([f32; 3]),
    Image(String),
    Null,
    Shape(Vec<Shape>),
    Text(TextData),
    /// Камера (ty 13): `zoom` — расстояние до плоскости проекции (`pe`).
    /// Точка интереса — `transform.anchor`.
    Camera {
        zoom: Animated<f32>,
    },
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
    /// Контуры всех масок по порядку `masksProperties` (включая режим None) —
    /// для текста по контуру и эффекта Stroke.
    pub mask_paths: Vec<Animated<ShapePath>>,
    /// Имена масок параллельно `mask_paths` — для `mask('…')`.
    pub mask_names: Vec<String>,
    pub matte: Option<MatteMode>,
    /// Индекс слоя-источника matte в том же списке.
    pub matte_source: Option<usize>,
    pub is_matte_source: bool,
    pub blend: BlendMode,
    pub effects: Vec<Effect>,
    /// Все эффекты как параметры (для выражений).
    pub effect_controls: Vec<EffectCtl>,
    /// Имена фигур верхнего уровня shape-слоя (для выражений).
    pub shape_names: Vec<String>,
    pub styles: Vec<LayerStyle>,
    pub hidden: bool,
    /// 3D-слой (`ddd: 1`): учитываются z, повороты X/Y и ориентация.
    pub three_d: bool,
    pub auto_orient: bool,
    /// Размер прекомпозиции / сплошного слоя.
    pub width: f32,
    pub height: f32,
}

#[derive(Debug, Clone)]
pub enum Position {
    Joint(Animated<Vec<f32>>),
    Split {
        x: Animated<f32>,
        y: Animated<f32>,
        z: Option<Animated<f32>>,
    },
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
    /// 3D: повороты X/Y и ориентация (градусы).
    pub rotation_x: Option<Animated<f32>>,
    pub rotation_y: Option<Animated<f32>>,
    pub orientation: Option<Animated<Vec<f32>>>,
}

fn two(v: &[f32]) -> [f32; 2] {
    [
        v.first().copied().unwrap_or(0.0),
        v.get(1).copied().unwrap_or(0.0),
    ]
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
            rotation_x: None,
            rotation_y: None,
            orientation: None,
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
                z: Animated::parse(p.get("z")),
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
            rotation_x: Animated::parse(v.get("rx")),
            rotation_y: Animated::parse(v.get("ry")),
            orientation: Animated::parse(v.get("or")),
        }
    }

    /// Координата z позиции (3D-слои).
    pub fn position_z(&self, frame: f32) -> f32 {
        match &self.position {
            Position::Joint(p) => p.value(frame).get(2).copied().unwrap_or(0.0),
            Position::Split { z, .. } => z.as_ref().map(|z| z.value(frame)).unwrap_or(0.0),
        }
    }

    /// Матрица 3D-слоя: `T(p)·O·Rx·Ry·Rz·Skew·S·T(−a)` с z-компонентами
    /// позиции, привязки и масштаба; `extra_rotation` добавляется к Z.
    pub fn matrix4(&self, frame: f32, extra_rotation: f32) -> Mat4 {
        let a = self.anchor.value(frame);
        let s = self.scale.value(frame);
        let p = self.position_at(frame);
        let g = |v: &[f32], i: usize, d: f32| v.get(i).copied().unwrap_or(d);
        let mut m = Mat4::translate(p[0], p[1], self.position_z(frame));
        if let Some(o) = &self.orientation {
            let o = o.value(frame);
            m = m
                .mul(&Mat4::rotate_x(g(&o, 0, 0.0)))
                .mul(&Mat4::rotate_y(g(&o, 1, 0.0)))
                .mul(&Mat4::rotate_z(g(&o, 2, 0.0)));
        }
        if let Some(rx) = &self.rotation_x {
            m = m.mul(&Mat4::rotate_x(rx.value(frame)));
        }
        if let Some(ry) = &self.rotation_y {
            m = m.mul(&Mat4::rotate_y(ry.value(frame)));
        }
        // Поворот Z, наклон и масштаб XY — общей 2D-частью.
        let flat = compose_transform(
            [0.0, 0.0],
            [0.0, 0.0],
            [g(&s, 0, 100.0) / 100.0, g(&s, 1, 100.0) / 100.0],
            self.rotation.value(frame) + extra_rotation,
            self.skew.value(frame),
            self.skew_axis.value(frame),
        );
        m.mul(&Mat4::from_affine(&flat))
            .mul(&Mat4::scale(1.0, 1.0, g(&s, 2, 100.0) / 100.0))
            .mul(&Mat4::translate(
                -g(&a, 0, 0.0),
                -g(&a, 1, 0.0),
                -g(&a, 2, 0.0),
            ))
    }

    pub fn position_at(&self, frame: f32) -> [f32; 2] {
        match &self.position {
            Position::Joint(p) => p.position(frame),
            Position::Split { x, y, .. } => [x.value(frame), y.value(frame)],
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
            [
                s.first().copied().unwrap_or(100.0) / 100.0,
                s.get(1).copied().unwrap_or(100.0) / 100.0,
            ],
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
            Position::Split { x, y, .. } => !x.is_animated() && !y.is_animated(),
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

/// Режим merge paths (`mm`). `Merge` — простое объединение контуров в один
/// (как раздельная отрисовка), остальные — булевы операции по покрытию.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum MergeMode {
    Merge,
    Add,
    Subtract,
    Intersect,
    Exclude,
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
        /// Имена элементов (`nm`) параллельно `items` — для `content('…')`
        /// в выражениях.
        item_names: Vec<String>,
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
    /// Merge paths: булева операция над контурами выше по группе.
    Merge {
        mode: MergeMode,
    },
    /// Offset paths: параллельный сдвиг контура.
    OffsetPath {
        amount: Animated<f32>,
        join: LineJoin,
        miter: f32,
    },
    /// Zig Zag: зубцы/волны вдоль контура.
    ZigZag {
        size: Animated<f32>,
        ridges: Animated<f32>,
        smooth: bool,
    },
    /// Pucker & Bloat: вершины к центру, касательные от центра (и наоборот).
    PuckerBloat {
        amount: Animated<f32>,
    },
    /// Twist: закручивание вокруг центра (сильнее у центра).
    Twist {
        angle: Animated<f32>,
        center: Animated<Vec<f32>>,
    },
    /// Неподдерживаемый элемент фигуры.
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
        kind: if v.i64_or("t", 1) == 2 {
            GradientKind::Radial
        } else {
            GradientKind::Linear
        },
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
            let (items, item_names) = parse_shape_list(v.get("it"));
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
                item_names,
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
        "mm" => Shape::Merge {
            mode: match v.i64_or("mm", 1) {
                2 => MergeMode::Add,
                3 => MergeMode::Subtract,
                4 => MergeMode::Intersect,
                5 => MergeMode::Exclude,
                _ => MergeMode::Merge,
            },
        },
        "op" => Shape::OffsetPath {
            amount: Animated::parse_or(v.get("a"), 0.0),
            join: match v.i64_or("lj", 1) {
                2 => LineJoin::Round,
                3 => LineJoin::Bevel,
                _ => LineJoin::Miter,
            },
            miter: v.f32_or("ml", 4.0),
        },
        "zz" => Shape::ZigZag {
            size: Animated::parse_or(v.get("s"), 0.0),
            ridges: Animated::parse_or(v.get("r"), 1.0),
            // `pt` — число или анимируемое свойство: 1 — уголки, 2 — волна.
            smooth: v
                .get("pt")
                .and_then(|p| {
                    p.as_f64()
                        .map(|x| x as f32)
                        .or_else(|| Animated::<f32>::parse(Some(p)).map(|a| *a.first_value()))
                })
                .map(|p| p >= 1.5)
                .unwrap_or(false),
        },
        "pb" => Shape::PuckerBloat {
            amount: Animated::parse_or(v.get("a"), 0.0),
        },
        "tw" => Shape::Twist {
            angle: Animated::parse_or(v.get("a"), 0.0),
            center: Animated::parse_or(v.get("c"), vec![0.0, 0.0]),
        },
        other => {
            log::debug!("lottie: неизвестный элемент фигуры `{other}` пропущен");
            Shape::Ignored
        }
    };
    Some(shape)
}

/// Список фигур с именами элементов (`nm`).
fn parse_shape_list(v: Option<&Json>) -> (Vec<Shape>, Vec<String>) {
    let mut items = Vec::new();
    let mut names = Vec::new();
    for it in v.and_then(Json::as_array).unwrap_or(&[]) {
        if let Some(s) = parse_shape(it) {
            items.push(s);
            names.push(it.str_or("nm", "").to_string());
        }
    }
    (items, names)
}

/// Эффект слоя как набор параметров — для `effect('…')('…')` в выражениях
/// (в том числе Slider/Color/Checkbox Control и псевдо-эффекты).
#[derive(Debug)]
pub struct EffectCtl {
    pub name: String,
    pub match_name: String,
    pub params: Vec<EffectParam>,
}

#[derive(Debug)]
pub struct EffectParam {
    pub name: String,
    pub match_name: String,
    pub value: Animated<Vec<f32>>,
}

fn parse_effect_controls(v: &Json) -> Vec<EffectCtl> {
    fn params(list: &[Json], out: &mut Vec<EffectParam>) {
        for p in list {
            // Группы параметров (ty 5) — разворачиваем.
            if let Some(sub) = p.get("ef").and_then(Json::as_array) {
                params(sub, out);
                continue;
            }
            out.push(EffectParam {
                name: p.str_or("nm", "").to_string(),
                match_name: p.str_or("mn", "").to_string(),
                value: Animated::parse_or(p.get("v"), vec![0.0]),
            });
        }
    }
    v.get("ef")
        .and_then(Json::as_array)
        .map(|list| {
            list.iter()
                .map(|e| {
                    let mut ps = Vec::new();
                    params(e.get("ef").and_then(Json::as_array).unwrap_or(&[]), &mut ps);
                    EffectCtl {
                        name: e.str_or("nm", "").to_string(),
                        match_name: e.str_or("mn", "").to_string(),
                        params: ps,
                    }
                })
                .collect()
        })
        .unwrap_or_default()
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
        expand: Animated::parse_or(v.get("x"), 0.0),
    })
}

fn parse_text_doc(v: &Json) -> TextDoc {
    let color = |key: &str| -> Option<Vec<f32>> {
        v.get(key)
            .map(Json::num_array)
            .filter(|c| c.len() >= 3)
            .map(|c| normalize_color(&c))
    };
    let pair = |key: &str| -> Option<[f32; 2]> {
        let n = v.get(key)?.num_array();
        if n.len() >= 2 {
            Some([n[0], n[1]])
        } else {
            None
        }
    };
    let size = v.f32_or("s", 36.0);
    TextDoc {
        text: v.str_or("t", "").to_string(),
        font: v.str_or("f", "").to_string(),
        size,
        fill: color("fc"),
        stroke: color("sc"),
        stroke_width: v.f32_or("sw", 0.0),
        stroke_over_fill: v.bool_or("of", true),
        line_height: {
            let lh = v.f32_or("lh", 0.0);
            if lh > 0.0 {
                lh
            } else {
                size * 1.2
            }
        },
        baseline_shift: v.f32_or("ls", 0.0),
        tracking: v.f32_or("tr", 0.0),
        justify: v.i64_or("j", 0),
        box_pos: pair("ps"),
        box_size: pair("sz"),
    }
}

fn parse_selector(v: &Json) -> TextSelector {
    let num = |key: &str, d: f32| Animated::parse_or(v.get(key), d);
    if v.i64_or("t", 0) != 0 {
        log::debug!("lottie: селектор-выражение текста — действует на все буквы");
    }
    TextSelector {
        based_on: match v.i64_or("b", 1) {
            2 => TextBasedOn::CharactersExcludingSpaces,
            3 => TextBasedOn::Words,
            4 => TextBasedOn::Lines,
            _ => TextBasedOn::Characters,
        },
        shape: match v.i64_or("sh", 1) {
            2 => SelectorShape::RampUp,
            3 => SelectorShape::RampDown,
            4 => SelectorShape::Triangle,
            5 => SelectorShape::Round,
            6 => SelectorShape::Smooth,
            _ => SelectorShape::Square,
        },
        index_units: v.i64_or("r", 1) == 2,
        // Селектор-выражение (`t: 1`) — весь текст.
        start: if v.i64_or("t", 0) != 0 {
            Animated::Static(0.0)
        } else {
            num("s", 0.0)
        },
        end: if v.i64_or("t", 0) != 0 {
            Animated::Static(100.0)
        } else {
            num("e", 100.0)
        },
        offset: if v.i64_or("t", 0) != 0 {
            Animated::Static(0.0)
        } else {
            num("o", 0.0)
        },
        amount: num("a", 100.0),
        ease_high: num("xe", 0.0),
        ease_low: num("ne", 0.0),
        smoothness: num("sm", 100.0),
        randomize: v.i64_or("rn", 0) == 1,
    }
}

fn parse_anim_props(v: &Json) -> TextAnimProps {
    let vec = |key: &str| Animated::<Vec<f32>>::parse(v.get(key));
    let num = |key: &str| Animated::<f32>::parse(v.get(key));
    let color = |key: &str| vec(key).map(|a| a.map(|c| normalize_color(c)));
    TextAnimProps {
        anchor: vec("a"),
        position: vec("p"),
        scale: vec("s"),
        rotation: num("r").or_else(|| num("rz")),
        skew: num("sk"),
        skew_axis: num("sa"),
        opacity: num("o"),
        fill: color("fc"),
        stroke: color("sc"),
        stroke_width: num("sw"),
        tracking: num("t"),
        fill_hue: num("fh"),
        fill_saturation: num("fs"),
        fill_brightness: num("fb"),
    }
}

fn parse_text(v: &Json) -> TextData {
    let t = v.get("t");
    let docs = t
        .and_then(|t| t.get("d"))
        .and_then(|d| d.get("k"))
        .and_then(Json::as_array)
        .map(|arr| {
            arr.iter()
                .filter_map(|k| Some((k.f32_or("t", 0.0), parse_text_doc(k.get("s")?))))
                .collect()
        })
        .unwrap_or_default();
    let expr = t
        .and_then(|t| t.get("d"))
        .and_then(|d| d.get("x"))
        .and_then(Json::as_str)
        .and_then(|src| match super::expr::Program::compile(src) {
            Ok(p) => {
                super::anim::note_compiled();
                Some(std::sync::Arc::new(p))
            }
            Err(e) => {
                log::debug!("lottie: выражение текста не разобрано ({e})");
                None
            }
        });
    let animators = t
        .and_then(|t| t.get("a"))
        .and_then(Json::as_array)
        .map(|arr| {
            arr.iter()
                .map(|a| TextAnimator {
                    selector: a.get("s").filter(|s| s.is_object()).map(parse_selector),
                    props: a.get("a").map(parse_anim_props).unwrap_or_default(),
                })
                .collect()
        })
        .unwrap_or_default();
    let anchor_align = Animated::parse_or(
        t.and_then(|t| t.get("m")).and_then(|m| m.get("a")),
        vec![0.0, 0.0],
    );
    let path = t.and_then(|t| t.get("p")).and_then(|p| {
        let mask = p.get("m").and_then(Json::as_i64)?;
        let num = |key: &str| Animated::parse_or(p.get(key), 0.0);
        Some(TextPathOpts {
            mask: mask.max(0) as usize,
            first_margin: num("f"),
            last_margin: num("l"),
            reversed: num("r"),
            perpendicular: Animated::parse_or(p.get("p"), 1.0),
            force_align: num("a"),
        })
    });
    TextData {
        docs,
        animators,
        anchor_align,
        path,
        expr,
    }
}

fn parse_effects(v: &Json) -> Vec<Effect> {
    let Some(list) = v.get("ef").and_then(Json::as_array) else {
        return Vec::new();
    };
    let mut out = Vec::new();
    for e in list {
        if e.i64_or("en", 1) == 0 {
            continue;
        }
        let vals = e.get("ef").and_then(Json::as_array).unwrap_or(&[]);
        let val = |i: usize| vals.get(i).and_then(|x| x.get("v"));
        let color = |i: usize| {
            Animated::parse_or(val(i), vec![0.0, 0.0, 0.0, 1.0]).map(|c| normalize_color(c))
        };
        let num = |i: usize, d: f32| Animated::parse_or(val(i), d);
        match e.i64_or("ty", -1) {
            20 => out.push(Effect::Tint {
                black: color(0),
                white: color(1),
                amount: num(2, 100.0),
            }),
            21 => out.push(Effect::Fill {
                color: color(2),
                opacity: num(6, 1.0),
            }),
            25 => out.push(Effect::DropShadow {
                color: color(0),
                opacity: num(1, 127.0),
                direction: num(2, 135.0),
                distance: num(3, 10.0),
                softness: num(4, 7.0),
                shadow_only: val(5)
                    .and_then(|v| v.get("k"))
                    .and_then(Json::as_f64)
                    .map(|v| v != 0.0)
                    .unwrap_or(false),
            }),
            29 => out.push(Effect::GaussianBlur {
                blurriness: num(0, 0.0),
                dimensions: num(1, 1.0),
            }),
            23 => out.push(Effect::Tritone {
                highlights: color(0),
                midtones: color(1),
                shadows: color(2),
                blend: num(3, 0.0),
            }),
            24 => {
                // Блоки по 7 значений, начиная со 2-го: группа, вход
                // чёрное/белое, гамма, выход чёрное/белое, конец группы.
                let channels = [2usize, 9, 16, 23, 30]
                    .iter()
                    .map(|&b| {
                        [
                            num(b + 1, 0.0),
                            num(b + 2, 1.0),
                            num(b + 3, 1.0),
                            num(b + 4, 0.0),
                            num(b + 5, 1.0),
                        ]
                    })
                    .collect();
                out.push(Effect::Levels { channels });
            }
            26 => out.push(Effect::RadialWipe {
                completion: num(0, 0.0),
                start_angle: num(1, 0.0),
                center: Animated::parse_or(val(2), vec![0.0, 0.0]),
                direction: num(3, 1.0).first_value().round() as i64,
                feather: num(4, 0.0),
            }),
            22 => out.push(Effect::Stroke {
                mask: num(0, 1.0).first_value().round().max(1.0) as usize,
                all_masks: *num(1, 0.0).first_value() != 0.0,
                color: color(3),
                brush_size: num(4, 2.0),
                opacity: num(6, 1.0),
                start: num(7, 0.0),
                end: num(8, 100.0),
                paint_style: num(10, 1.0).first_value().round() as i64,
            }),
            other => {
                log::debug!(
                    "lottie: эффект {other} (`{}`) не поддерживается",
                    e.str_or("nm", "")
                );
            }
        }
    }
    out
}

fn parse_styles(v: &Json) -> Vec<LayerStyle> {
    let Some(list) = v.get("sy").and_then(Json::as_array) else {
        return Vec::new();
    };
    let mut out = Vec::new();
    for st in list {
        let num = |key: &str, d: f32| Animated::parse_or(st.get(key), d);
        let color = || parse_color(st.get("c"));
        match st.i64_or("ty", -1) {
            t @ (1 | 2) => out.push(LayerStyle::Shadow {
                inner: t == 2,
                color: color(),
                opacity: num("o", 75.0),
                angle: num("a", 120.0),
                distance: num("d", 5.0),
                size: num("s", 5.0),
                spread: num("ch", 0.0),
            }),
            t @ (3 | 4) => out.push(LayerStyle::Glow {
                inner: t == 4,
                color: color(),
                opacity: num("o", 75.0),
                size: num("s", 5.0),
                spread: num("ch", 0.0),
                from_center: *num("sr", 2.0).first_value() < 1.5,
            }),
            0 => out.push(LayerStyle::Stroke {
                color: color(),
                opacity: num("o", 100.0),
                size: num("s", 3.0),
            }),
            7 => out.push(LayerStyle::ColorOverlay {
                color: color(),
                opacity: num("so", 100.0),
            }),
            other => log::debug!(
                "lottie: стиль слоя {other} (`{}`) не поддерживается",
                st.str_or("nm", "")
            ),
        }
    }
    out
}

fn read_manifest(zip: &super::zip::Zip) -> Option<DotLottieManifest> {
    let m = zip.read("manifest.json")?;
    let text = String::from_utf8(m).ok()?;
    let v = json::parse(text.trim_start_matches('\u{feff}')).ok()?;
    Some(DotLottieManifest::parse(&v))
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
        5 => LayerKind::Text(parse_text(v)),
        13 => LayerKind::Camera {
            zoom: Animated::parse_or(v.get("pe"), 1000.0),
        },
        4 => LayerKind::Shape(parse_shape_list(v.get("shapes")).0),
        other => {
            *unsupported += 1;
            log::warn!(
                "lottie: слой `{}` типа {other} не поддерживается (аудио/камера/data) и пропущен",
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
            if sr.abs() < 1e-6 {
                1.0
            } else {
                sr
            }
        },
        time_remap: Animated::parse(v.get("tm")),
        masks: v
            .get("masksProperties")
            .and_then(Json::as_array)
            .map(|a| a.iter().filter_map(parse_mask).collect())
            .unwrap_or_default(),
        mask_names: v
            .get("masksProperties")
            .and_then(Json::as_array)
            .map(|a| a.iter().map(|m| m.str_or("nm", "").to_string()).collect())
            .unwrap_or_default(),
        mask_paths: v
            .get("masksProperties")
            .and_then(Json::as_array)
            .map(|a| {
                a.iter()
                    .map(|m| Animated::parse_or(m.get("pt"), ShapePath::default()))
                    .collect()
            })
            .unwrap_or_default(),
        matte,
        matte_source: None,
        is_matte_source: v.i64_or("td", 0) == 1,
        blend: BlendMode::from_lottie(v.i64_or("bm", 0)),
        effects: parse_effects(v),
        effect_controls: if v.get("ef").is_some() {
            parse_effect_controls(v)
        } else {
            Vec::new()
        },
        shape_names: if ty == 4 {
            parse_shape_list(v.get("shapes")).1
        } else {
            Vec::new()
        },
        styles: parse_styles(v),
        hidden: v.bool_or("hd", false),
        three_d: v.i64_or("ddd", 0) == 1,
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
        if let Some(p) = raw
            .get(i)
            .and_then(|l| l.get("parent"))
            .and_then(Json::as_i64)
        {
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

    /// Из байтов: обычный JSON или контейнер dotLottie (.lottie, ZIP).
    pub fn from_bytes(bytes: &[u8]) -> Result<Composition, LottieError> {
        if super::zip::Zip::is_zip(bytes) {
            return Composition::from_dotlottie(bytes);
        }
        let text =
            std::str::from_utf8(bytes).map_err(|_| LottieError::Invalid("файл не в UTF-8"))?;
        Composition::from_str(text.trim_start_matches('\u{feff}'))
    }

    /// Контейнер dotLottie: анимация по умолчанию (`activeAnimationId` /
    /// `initial.animation`, иначе первая из манифеста).
    pub fn from_dotlottie(bytes: &[u8]) -> Result<Composition, LottieError> {
        Composition::from_dotlottie_animation(bytes, None)
    }

    /// Манифест dotLottie без разбора анимаций (список id и параметры).
    pub fn dotlottie_manifest(bytes: &[u8]) -> Result<DotLottieManifest, LottieError> {
        let zip = super::zip::Zip::parse(bytes).map_err(LottieError::Invalid)?;
        Ok(read_manifest(&zip).unwrap_or_default())
    }

    /// Контейнер dotLottie (v1: `animations/`, `images/`; v2: `a/`, `i/`):
    /// анимация с заданным `id` (или по умолчанию). Растровые ассеты
    /// берутся из архива, параметры воспроизведения манифеста — в
    /// [`Composition::playback`].
    pub fn from_dotlottie_animation(
        bytes: &[u8],
        id: Option<&str>,
    ) -> Result<Composition, LottieError> {
        let zip = super::zip::Zip::parse(bytes).map_err(LottieError::Invalid)?;
        let manifest = read_manifest(&zip).unwrap_or_default();
        let entry = match id {
            Some(id) => Some(
                manifest
                    .animations
                    .iter()
                    .find(|a| a.id == id)
                    .cloned()
                    .unwrap_or(DotLottieAnimation {
                        id: id.to_string(),
                        ..Default::default()
                    }),
            ),
            None => manifest
                .active
                .as_ref()
                .and_then(|act| manifest.animations.iter().find(|a| &a.id == act))
                .or(manifest.animations.first())
                .cloned(),
        };
        let is_anim = |n: &str| {
            (n.starts_with("animations/") || n.starts_with("a/"))
                && n.ends_with(".json")
                && !n.contains("__MACOSX")
        };
        let anim_path = match &entry {
            Some(e) if !e.id.is_empty() => [
                format!("animations/{}.json", e.id),
                format!("a/{}.json", e.id),
            ]
            .into_iter()
            .find(|p| zip.entries.iter().any(|z| &z.name == p)),
            _ => None,
        }
        .or_else(|| {
            if id.is_some() {
                None
            } else {
                zip.entries
                    .iter()
                    .map(|e| e.name.clone())
                    .find(|n| is_anim(n))
            }
        })
        .ok_or(match id {
            Some(_) => LottieError::Invalid("dotLottie: анимация с таким id не найдена"),
            None => LottieError::Invalid("dotLottie: не найдена анимация"),
        })?;
        let data = zip.read(&anim_path).ok_or(LottieError::Invalid(
            "dotLottie: не удалось распаковать анимацию",
        ))?;
        let text = String::from_utf8(data)
            .map_err(|_| LottieError::Invalid("dotLottie: анимация не в UTF-8"))?;
        let mut comp = Composition::from_str(text.trim_start_matches('\u{feff}'))?;
        comp.playback = entry;
        // Внешние картинки внутри архива: кладём байты как встроенные.
        for asset in comp.images.values_mut() {
            if asset.embedded.is_some() || asset.path.is_empty() {
                continue;
            }
            let p = asset.path.trim_start_matches('/');
            let file = p.rsplit('/').next().unwrap_or(p);
            asset.embedded = zip
                .read(p)
                .or_else(|| zip.read(&format!("images/{file}")))
                .or_else(|| zip.read(&format!("i/{file}")))
                .or_else(|| zip.read(file));
            if asset.embedded.is_none() {
                log::warn!("dotLottie: картинка `{}` не найдена в архиве", asset.path);
            }
        }
        Ok(comp)
    }

    pub fn from_json(root: &Json) -> Result<Composition, LottieError> {
        if !root.is_object() {
            return Err(LottieError::Invalid("корень документа не объект"));
        }
        if root.get("layers").is_none() {
            return Err(LottieError::Invalid("нет поля `layers` — это не Lottie"));
        }
        let mut unsupported = 0usize;
        let compiled_before = super::anim::compiled_expressions();
        let layers = parse_layers(root.get("layers"), &mut unsupported);
        let mut precomp_names = HashMap::new();
        let mut precomps = HashMap::new();
        let mut images = HashMap::new();
        if let Some(assets) = root.get("assets").and_then(Json::as_array) {
            for a in assets {
                let id = a.str_or("id", "").to_string();
                if a.get("layers").is_some() {
                    if let Some(nm) = a.get("nm").and_then(Json::as_str) {
                        precomp_names.insert(nm.to_string(), id.clone());
                    }
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
        let fonts: Vec<FontDef> = root
            .get("fonts")
            .and_then(|f| f.get("list"))
            .and_then(Json::as_array)
            .map(|arr| {
                arr.iter()
                    .map(|f| FontDef {
                        name: f.str_or("fName", "").to_string(),
                        family: f.str_or("fFamily", "").to_string(),
                        style: f.str_or("fStyle", "").to_string(),
                        ascent: f.f32_or("ascent", 75.0),
                    })
                    .collect()
            })
            .unwrap_or_default();
        let mut glyphs = HashMap::new();
        if let Some(chars) = root.get("chars").and_then(Json::as_array) {
            for c in chars {
                let shapes = c
                    .get("data")
                    .and_then(|d| d.get("shapes"))
                    .and_then(Json::as_array)
                    .map(|a| a.iter().filter_map(parse_shape).collect())
                    .unwrap_or_default();
                glyphs.insert(
                    (
                        c.str_or("fFamily", "").to_string(),
                        c.str_or("style", "").to_string(),
                        c.str_or("ch", "").to_string(),
                    ),
                    Glyph {
                        shapes,
                        advance: c.f32_or("w", 0.0),
                    },
                );
            }
        }
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
            fonts,
            glyphs,
            unsupported_layers: unsupported,
            playback: None,
            precomp_names,
            has_expressions: super::anim::compiled_expressions() != compiled_before,
        })
    }

    /// Шрифт по имени (`fName`); иначе первый из списка.
    pub fn font(&self, name: &str) -> Option<&FontDef> {
        self.fonts
            .iter()
            .find(|f| f.name == name)
            .or_else(|| self.fonts.first())
    }

    /// Встроенный глиф: точное совпадение семейства и начертания,
    /// иначе любой с тем же символом.
    pub fn glyph(&self, family: &str, style: &str, ch: &str) -> Option<&Glyph> {
        self.glyphs
            .get(&(family.to_string(), style.to_string(), ch.to_string()))
            .or_else(|| {
                self.glyphs
                    .iter()
                    .find(|((_, _, c), _)| c == ch)
                    .map(|(_, g)| g)
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
