//! Эмиттер частиц: непрерывный поток, всплески, частицы при наведении и
//! «след» за указателем — с пресетами и настройкой из MSS.
//!
//! [`ParticleEmitter`] — обёртка вокруг содержимого (значок, кнопка) или
//! самостоятельный слой. Обёртка видит указатель над собой: `particle-hover-rate`
//! включает поток только при наведении, `particle-emitter: pointer` пускает
//! частицы из-под курсора. Всплеск — изменением `burst_token` (как у
//! [`super::ParticleSystem`]).
//!
//! ```css
//! .dock-icon-fx {
//!     particle-preset: sparkle;     /* sparkle magic confetti fireworks sparks snow rain fire
//!                                      smoke bubbles hearts embers poof dust stars */
//!     particle-rate: 0;             /* частиц в секунду всегда */
//!     particle-hover-rate: 18;      /* … пока указатель над элементом */
//!     particle-burst: 40;           /* частиц во всплеске */
//!     particle-lifetime: 0.6s 1.2s;
//!     particle-speed: 20 60;        /* px/с */
//!     particle-direction: -90deg;   /* 0 — вправо, -90 — вверх */
//!     particle-spread: 60deg;
//!     particle-gravity: 0 -20;      /* px/с² (или одно число — по вертикали) */
//!     particle-drag: 1;
//!     particle-size: 2px 6px;
//!     particle-size-end: 0.2;       /* множитель размера к концу жизни */
//!     particle-color: var(--accent);
//!     particle-colors: "#fff #ffe9a8 #a8d8ff";
//!     particle-color-end: #ff000000;
//!     particle-shape: star;         /* circle square triangle star spark ring glow heart mixed */
//!     particle-spin: 180deg;        /* до ± градусов в секунду */
//!     particle-glow: 6px;           /* мягкое свечение вокруг частиц */
//!     particle-wobble: 6px;         /* боковое покачивание */
//!     particle-twinkle: 0.5;        /* мерцание 0..1 */
//!     particle-emitter: rect;       /* point [x% y%] | line top|bottom|left|right | rect | ring | pointer */
//!     particle-layer: front;        /* front | back — над или под содержимым */
//!     particle-max: 300;
//! }
//! ```

use super::super::containers::IntoWidget;
use crate::animation::transition::mss_color_to_core;
use crate::core::canvas::CanvasContext;
use crate::core::{Color, Point, Rect, Size};
use crate::input::{Event, EventResult};
use crate::layout::Constraints;
use crate::mss::{ComputedStyle, MssColor, MssFields, StyleValue};
use crate::render::DisplayList;
use crate::widget::context::EventContext;
use crate::widget::{DirtyFlags, Element, ElementId, ElementTree, LayoutHint, UpdateContext, Widget};
use std::any::Any;
use std::f32::consts::{PI, TAU};
use std::time::Duration;

/// Форма частицы.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum ParticleShape {
    Circle,
    Square,
    Triangle,
    Star,
    /// Чёрточка вдоль скорости (искры, дождь).
    Spark,
    Ring,
    /// Мягкое размытое пятно (огонь, дым, магия).
    Glow,
    Heart,
    /// Квадраты, треугольники и круги вперемешку (конфетти).
    Mixed,
}

impl ParticleShape {
    pub fn parse(s: &str) -> Option<Self> {
        Some(match s.trim() {
            "circle" | "dot" => Self::Circle,
            "square" | "rect" => Self::Square,
            "triangle" => Self::Triangle,
            "star" => Self::Star,
            "spark" | "line" => Self::Spark,
            "ring" | "bubble" => Self::Ring,
            "glow" | "soft" => Self::Glow,
            "heart" => Self::Heart,
            "mixed" | "confetti" => Self::Mixed,
            _ => return None,
        })
    }
}

/// Откуда вылетают частицы (координаты — доли размера элемента).
#[derive(Clone, Copy, Debug, PartialEq)]
pub enum EmitterShape {
    Point(f32, f32),
    /// Вдоль края: 0 — верх, 1 — право, 2 — низ, 3 — лево.
    Line(u8),
    /// Равномерно по площади.
    Rect,
    /// По контуру вписанного эллипса.
    Ring,
    /// Из-под указателя (пока он над элементом).
    Pointer,
}

impl EmitterShape {
    pub fn parse(s: &str) -> Option<Self> {
        let toks: Vec<&str> = s.split_whitespace().collect();
        let pct = |t: &str| t.trim_end_matches('%').parse::<f32>().ok().map(|v| v / 100.0);
        Some(match toks.first().copied()? {
            "point" | "center" => match (toks.get(1).and_then(|t| pct(t)), toks.get(2).and_then(|t| pct(t))) {
                (Some(x), Some(y)) => Self::Point(x, y),
                _ => Self::Point(0.5, 0.5),
            },
            "line" => Self::Line(match toks.get(1).copied().unwrap_or("bottom") {
                "top" => 0,
                "right" => 1,
                "left" => 3,
                _ => 2,
            }),
            "rect" | "area" => Self::Rect,
            "ring" | "circle" => Self::Ring,
            "pointer" | "cursor" | "trail" => Self::Pointer,
            _ => return None,
        })
    }
}

/// Готовые наборы параметров.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum ParticlePreset {
    Sparkle,
    Magic,
    Confetti,
    Fireworks,
    Sparks,
    Snow,
    Rain,
    Fire,
    Smoke,
    Bubbles,
    Hearts,
    Embers,
    /// Облачко, как при удалении значка из дока macOS.
    Poof,
    Dust,
    Stars,
}

impl ParticlePreset {
    pub fn parse(s: &str) -> Option<Self> {
        Some(match s.trim() {
            "sparkle" | "sparkles" => Self::Sparkle,
            "magic" => Self::Magic,
            "confetti" => Self::Confetti,
            "fireworks" => Self::Fireworks,
            "sparks" => Self::Sparks,
            "snow" => Self::Snow,
            "rain" => Self::Rain,
            "fire" => Self::Fire,
            "smoke" => Self::Smoke,
            "bubbles" => Self::Bubbles,
            "hearts" => Self::Hearts,
            "embers" => Self::Embers,
            "poof" => Self::Poof,
            "dust" => Self::Dust,
            "stars" => Self::Stars,
            _ => return None,
        })
    }

    pub fn config(self) -> EmitterConfig {
        let c = |hex: &str| Color::from_hex(hex);
        let base = EmitterConfig::default();
        match self {
            Self::Sparkle => EmitterConfig {
                shape: ParticleShape::Star,
                colors: vec![c("#ffffff"), c("#ffe9a8"), c("#a8d8ff")],
                size: (3.0, 7.0),
                size_end: 0.0,
                lifetime: (0.5, 1.1),
                speed: (8.0, 36.0),
                spread: 360.0,
                gravity: (0.0, -12.0),
                drag: 1.0,
                spin: 180.0,
                glow: 5.0,
                twinkle: 0.6,
                emitter: EmitterShape::Rect,
                burst: 30,
                ..base
            },
            Self::Magic => EmitterConfig {
                shape: ParticleShape::Glow,
                colors: vec![c("#b388ff"), c("#80d8ff"), c("#ff80ab")],
                size: (4.0, 9.0),
                size_end: 0.2,
                lifetime: (0.8, 1.6),
                speed: (15.0, 45.0),
                spread: 60.0,
                gravity: (0.0, -30.0),
                wobble: 8.0,
                emitter: EmitterShape::Line(2),
                burst: 40,
                ..base
            },
            Self::Confetti => EmitterConfig {
                shape: ParticleShape::Mixed,
                colors: vec![
                    c("#2fbf71"),
                    c("#f4a259"),
                    c("#4c9be8"),
                    c("#ef6f6c"),
                    c("#f7d154"),
                    c("#a86ff0"),
                ],
                size: (5.0, 10.0),
                size_end: 1.0,
                lifetime: (1.2, 2.2),
                speed: (140.0, 320.0),
                spread: 50.0,
                gravity: (0.0, 420.0),
                drag: 0.8,
                spin: 540.0,
                emitter: EmitterShape::Point(0.5, 0.5),
                burst: 90,
                ..base
            },
            Self::Fireworks => EmitterConfig {
                shape: ParticleShape::Circle,
                colors: vec![c("#ff5e5e"), c("#ffd24d"), c("#5ee0ff"), c("#b17bff"), c("#7dff9a")],
                size: (2.0, 4.0),
                size_end: 0.3,
                lifetime: (0.8, 1.6),
                speed: (160.0, 360.0),
                spread: 360.0,
                gravity: (0.0, 220.0),
                drag: 1.2,
                glow: 4.0,
                emitter: EmitterShape::Point(0.5, 0.45),
                burst: 90,
                ..base
            },
            Self::Sparks => EmitterConfig {
                shape: ParticleShape::Spark,
                colors: vec![c("#fff5c0"), c("#ffb347"), c("#ff6a00")],
                size: (1.5, 2.5),
                size_end: 0.5,
                lifetime: (0.4, 0.9),
                speed: (120.0, 340.0),
                spread: 360.0,
                gravity: (0.0, 300.0),
                drag: 0.6,
                glow: 3.0,
                emitter: EmitterShape::Point(0.5, 0.5),
                burst: 48,
                ..base
            },
            Self::Snow => EmitterConfig {
                shape: ParticleShape::Circle,
                colors: vec![c("#ffffff"), c("#e8f4ff")],
                size: (2.0, 5.0),
                size_end: 1.0,
                lifetime: (3.0, 6.0),
                speed: (10.0, 30.0),
                direction: 90.0,
                spread: 30.0,
                gravity: (0.0, 8.0),
                wobble: 12.0,
                emitter: EmitterShape::Line(0),
                burst: 60,
                ..base
            },
            Self::Rain => EmitterConfig {
                shape: ParticleShape::Spark,
                colors: vec![c("#9ecbffaa")],
                size: (1.0, 1.5),
                size_end: 1.0,
                lifetime: (0.5, 0.8),
                speed: (400.0, 600.0),
                direction: 100.0,
                spread: 4.0,
                emitter: EmitterShape::Line(0),
                burst: 80,
                ..base
            },
            Self::Fire => EmitterConfig {
                shape: ParticleShape::Glow,
                colors: vec![c("#ffdd55"), c("#ff9a1f"), c("#ff5a1f")],
                color_end: Some(c("#8a1a0000")),
                size: (6.0, 14.0),
                size_end: 0.15,
                lifetime: (0.5, 1.0),
                speed: (30.0, 80.0),
                spread: 25.0,
                gravity: (0.0, -60.0),
                wobble: 4.0,
                emitter: EmitterShape::Line(2),
                burst: 50,
                ..base
            },
            Self::Smoke => EmitterConfig {
                shape: ParticleShape::Glow,
                colors: vec![c("#9a9a9a66"), c("#77777755")],
                size: (8.0, 16.0),
                size_end: 2.5,
                lifetime: (1.5, 3.0),
                speed: (10.0, 30.0),
                spread: 30.0,
                wobble: 10.0,
                fade_in: 0.2,
                emitter: EmitterShape::Point(0.5, 0.9),
                burst: 30,
                ..base
            },
            Self::Bubbles => EmitterConfig {
                shape: ParticleShape::Ring,
                colors: vec![c("#aee8ffcc"), c("#d6f5ffcc")],
                size: (4.0, 10.0),
                size_end: 1.2,
                lifetime: (2.0, 4.0),
                speed: (20.0, 50.0),
                spread: 20.0,
                wobble: 10.0,
                emitter: EmitterShape::Line(2),
                burst: 30,
                ..base
            },
            // Сердечки взлетают с верхней грани и разлетаются веером —
            // с нижней грани и малой скоростью они всплывали под фоном
            // элемента и блёкли раньше, чем показывались над ним.
            Self::Hearts => EmitterConfig {
                shape: ParticleShape::Heart,
                colors: vec![c("#ff4d6d"), c("#ff8fa3"), c("#ffccd5")],
                size: (8.0, 14.0),
                size_end: 0.7,
                lifetime: (1.2, 2.2),
                speed: (60.0, 130.0),
                spread: 70.0,
                gravity: (0.0, -30.0),
                spin: 40.0,
                emitter: EmitterShape::Line(0),
                burst: 24,
                ..base
            },
            Self::Embers => EmitterConfig {
                shape: ParticleShape::Circle,
                colors: vec![c("#ffb347"), c("#ff6a00"), c("#ffd27f")],
                size: (1.5, 3.0),
                size_end: 0.4,
                lifetime: (1.0, 2.5),
                speed: (20.0, 60.0),
                spread: 50.0,
                gravity: (0.0, -20.0),
                wobble: 6.0,
                glow: 4.0,
                twinkle: 0.4,
                emitter: EmitterShape::Line(2),
                burst: 40,
                ..base
            },
            Self::Poof => EmitterConfig {
                shape: ParticleShape::Glow,
                colors: vec![c("#e8e8e8dd"), c("#ffffffcc")],
                size: (10.0, 20.0),
                size_end: 1.8,
                lifetime: (0.4, 0.7),
                speed: (40.0, 120.0),
                spread: 360.0,
                drag: 3.0,
                emitter: EmitterShape::Point(0.5, 0.5),
                burst: 24,
                ..base
            },
            Self::Dust => EmitterConfig {
                shape: ParticleShape::Circle,
                colors: vec![c("#ffffff99"), c("#dddddd88")],
                size: (1.0, 2.5),
                size_end: 0.5,
                lifetime: (0.6, 1.2),
                speed: (20.0, 80.0),
                spread: 120.0,
                gravity: (0.0, 60.0),
                emitter: EmitterShape::Line(2),
                burst: 30,
                ..base
            },
            Self::Stars => EmitterConfig {
                shape: ParticleShape::Star,
                colors: vec![c("#ffd700"), c("#ffffff"), c("#ffe066")],
                size: (4.0, 9.0),
                size_end: 0.3,
                lifetime: (0.8, 1.4),
                speed: (60.0, 160.0),
                spread: 360.0,
                gravity: (0.0, 150.0),
                spin: 360.0,
                glow: 4.0,
                emitter: EmitterShape::Point(0.5, 0.5),
                burst: 36,
                ..base
            },
        }
    }
}

/// Параметры эмиттера. Углы — в градусах, скорости — px/с.
#[derive(Clone, Debug, PartialEq)]
pub struct EmitterConfig {
    pub rate: f32,
    pub hover_rate: f32,
    pub burst: u32,
    pub lifetime: (f32, f32),
    pub speed: (f32, f32),
    pub direction: f32,
    pub spread: f32,
    pub gravity: (f32, f32),
    pub drag: f32,
    pub size: (f32, f32),
    pub size_end: f32,
    pub colors: Vec<Color>,
    pub color_end: Option<Color>,
    pub shape: ParticleShape,
    pub spin: f32,
    pub glow: f32,
    pub wobble: f32,
    pub twinkle: f32,
    pub fade_in: f32,
    pub emitter: EmitterShape,
    /// Частицы поверх содержимого (`false` — под ним).
    pub front: bool,
    pub max: usize,
}

impl Default for EmitterConfig {
    fn default() -> Self {
        Self {
            rate: 0.0,
            hover_rate: 0.0,
            burst: 30,
            lifetime: (0.8, 1.4),
            speed: (20.0, 60.0),
            direction: -90.0,
            spread: 60.0,
            gravity: (0.0, 0.0),
            drag: 0.0,
            size: (2.0, 5.0),
            size_end: 0.3,
            colors: vec![Color::WHITE],
            color_end: None,
            shape: ParticleShape::Circle,
            spin: 0.0,
            glow: 0.0,
            wobble: 0.0,
            twinkle: 0.0,
            fade_in: 0.08,
            emitter: EmitterShape::Point(0.5, 0.5),
            front: true,
            max: 400,
        }
    }
}

/// Поля, заданные явно (построителем или MSS) — поверх пресета.
#[derive(Clone, Debug, Default, PartialEq)]
struct Overrides {
    preset: Option<ParticlePreset>,
    rate: Option<f32>,
    hover_rate: Option<f32>,
    burst: Option<u32>,
    lifetime: Option<(f32, f32)>,
    speed: Option<(f32, f32)>,
    direction: Option<f32>,
    spread: Option<f32>,
    gravity: Option<(f32, f32)>,
    drag: Option<f32>,
    size: Option<(f32, f32)>,
    size_end: Option<f32>,
    colors: Option<Vec<Color>>,
    color_end: Option<Color>,
    shape: Option<ParticleShape>,
    spin: Option<f32>,
    glow: Option<f32>,
    wobble: Option<f32>,
    twinkle: Option<f32>,
    emitter: Option<EmitterShape>,
    front: Option<bool>,
    max: Option<usize>,
}

impl Overrides {
    /// Слить: `self` важнее `lower`.
    fn over(&self, lower: &Overrides) -> Overrides {
        macro_rules! pick {
            ($($f:ident),*) => { Overrides { $($f: self.$f.clone().or_else(|| lower.$f.clone()),)* } };
        }
        pick!(
            preset, rate, hover_rate, burst, lifetime, speed, direction, spread, gravity, drag, size, size_end,
            colors, color_end, shape, spin, glow, wobble, twinkle, emitter, front, max
        )
    }

    fn resolve(&self) -> EmitterConfig {
        let mut c = self.preset.map(|p| p.config()).unwrap_or_default();
        macro_rules! set {
            ($($f:ident),*) => { $(if let Some(v) = self.$f.clone() { c.$f = v; })* };
        }
        set!(
            rate, hover_rate, burst, lifetime, speed, direction, spread, gravity, drag, size, size_end, colors,
            shape, spin, glow, wobble, twinkle, emitter, front, max
        );
        if self.color_end.is_some() {
            c.color_end = self.color_end;
        }
        if c.colors.is_empty() {
            c.colors = vec![Color::WHITE];
        }
        c
    }

    fn from_style(style: &ComputedStyle) -> Overrides {
        let get = |k: &str| style.get(k);
        let text = |k: &str| -> Option<String> {
            get(k).map(|v| match v {
                StyleValue::String(s) => s.clone(),
                StyleValue::Number(n) => n.to_string(),
                StyleValue::Length(n, _) => n.to_string(),
                other => format!("{other:?}"),
            })
        };
        let num = |k: &str| text(k).and_then(|s| parse_scalar(&s));
        let range = |k: &str| text(k).and_then(|s| parse_range(&s));
        let color_of = |v: &StyleValue| match v {
            StyleValue::Color(c) => Some(mss_color_to_core(*c)),
            StyleValue::String(s) => MssColor::parse(s.trim()).map(mss_color_to_core),
            _ => None,
        };
        let mut colors: Option<Vec<Color>> = get("particle-colors").and_then(|v| match v {
            StyleValue::Color(c) => Some(vec![mss_color_to_core(*c)]),
            StyleValue::String(s) => {
                let v: Vec<Color> = s
                    .trim_matches('"')
                    .split(|ch: char| ch.is_whitespace() || ch == ',')
                    .filter(|t| !t.is_empty())
                    .filter_map(|t| MssColor::parse(t).map(mss_color_to_core))
                    .collect();
                (!v.is_empty()).then_some(v)
            }
            _ => None,
        });
        if let Some(c) = get("particle-color").and_then(color_of) {
            colors.get_or_insert_with(Vec::new).insert(0, c);
        }
        Overrides {
            preset: text("particle-preset").and_then(|s| ParticlePreset::parse(&s)),
            rate: num("particle-rate"),
            hover_rate: num("particle-hover-rate"),
            burst: num("particle-burst").map(|v| v.max(0.0) as u32),
            lifetime: range("particle-lifetime"),
            speed: range("particle-speed"),
            direction: num("particle-direction"),
            spread: num("particle-spread"),
            gravity: text("particle-gravity").and_then(|s| {
                let v: Vec<f32> = s.split_whitespace().filter_map(parse_scalar).collect();
                match v.as_slice() {
                    [y] => Some((0.0, *y)),
                    [x, y, ..] => Some((*x, *y)),
                    _ => None,
                }
            }),
            drag: num("particle-drag"),
            size: range("particle-size"),
            size_end: num("particle-size-end"),
            colors,
            color_end: get("particle-color-end").and_then(color_of),
            shape: text("particle-shape").and_then(|s| ParticleShape::parse(&s)),
            spin: num("particle-spin"),
            glow: num("particle-glow"),
            wobble: num("particle-wobble"),
            twinkle: num("particle-twinkle"),
            emitter: text("particle-emitter").and_then(|s| EmitterShape::parse(&s)),
            front: text("particle-layer").map(|s| s.trim() != "back"),
            max: num("particle-max").map(|v| v.max(1.0) as usize),
        }
    }
}

/// Число с единицей: `12px`, `0.6s`, `300ms`, `45deg`, `0.5turn`, `20%`.
fn parse_scalar(t: &str) -> Option<f32> {
    let t = t.trim().trim_matches('"');
    if let Some(v) = t.strip_suffix("ms") {
        return v.trim().parse::<f32>().ok().map(|v| v / 1000.0);
    }
    for suf in ["px", "deg", "s"] {
        if let Some(v) = t.strip_suffix(suf) {
            return v.trim().parse().ok();
        }
    }
    if let Some(v) = t.strip_suffix("turn") {
        return v.trim().parse::<f32>().ok().map(|v| v * 360.0);
    }
    if let Some(v) = t.strip_suffix("rad") {
        return v.trim().parse::<f32>().ok().map(|v| v.to_degrees());
    }
    t.parse().ok()
}

/// `a b` → (a, b); одно значение — (a, a).
fn parse_range(s: &str) -> Option<(f32, f32)> {
    let v: Vec<f32> = s.split_whitespace().filter_map(parse_scalar).collect();
    match v.as_slice() {
        [a] => Some((*a, *a)),
        [a, b, ..] => Some((a.min(*b), a.max(*b))),
        _ => None,
    }
}

/// Эмиттер частиц — слой или обёртка вокруг содержимого.
pub struct ParticleEmitter {
    child: Option<Box<dyn Widget>>,
    builder: Overrides,
    burst_token: u32,
    classes: Vec<String>,
}

impl Default for ParticleEmitter {
    fn default() -> Self {
        Self::new()
    }
}

impl ParticleEmitter {
    pub fn new() -> Self {
        Self { child: None, builder: Overrides::default(), burst_token: 0, classes: Vec::new() }
    }

    /// Обернуть содержимое: эмиттер займёт его место и будет видеть
    /// указатель над ним (`hover_rate`, `EmitterShape::Pointer`).
    pub fn child<M>(mut self, child: impl IntoWidget<M>) -> Self {
        self.child = Some(child.into_widget());
        self
    }

    pub fn preset(mut self, p: ParticlePreset) -> Self {
        self.builder.preset = Some(p);
        self
    }

    /// Всплеск при каждом изменении значения (идиома «инкремент счётчика»).
    pub fn burst_token(mut self, token: u32) -> Self {
        self.burst_token = token;
        self
    }

    pub fn burst(mut self, count: u32) -> Self {
        self.builder.burst = Some(count);
        self
    }

    pub fn rate(mut self, per_second: f32) -> Self {
        self.builder.rate = Some(per_second.max(0.0));
        self
    }

    pub fn hover_rate(mut self, per_second: f32) -> Self {
        self.builder.hover_rate = Some(per_second.max(0.0));
        self
    }

    pub fn lifetime(mut self, min_s: f32, max_s: f32) -> Self {
        self.builder.lifetime = Some((min_s, max_s));
        self
    }

    pub fn speed(mut self, min: f32, max: f32) -> Self {
        self.builder.speed = Some((min, max));
        self
    }

    pub fn direction(mut self, deg: f32, spread_deg: f32) -> Self {
        self.builder.direction = Some(deg);
        self.builder.spread = Some(spread_deg);
        self
    }

    pub fn gravity(mut self, x: f32, y: f32) -> Self {
        self.builder.gravity = Some((x, y));
        self
    }

    pub fn size(mut self, min: f32, max: f32) -> Self {
        self.builder.size = Some((min, max));
        self
    }

    pub fn colors(mut self, colors: Vec<Color>) -> Self {
        if !colors.is_empty() {
            self.builder.colors = Some(colors);
        }
        self
    }

    pub fn shape(mut self, shape: ParticleShape) -> Self {
        self.builder.shape = Some(shape);
        self
    }

    pub fn emitter(mut self, e: EmitterShape) -> Self {
        self.builder.emitter = Some(e);
        self
    }

    pub fn glow(mut self, px: f32) -> Self {
        self.builder.glow = Some(px);
        self
    }

    /// Частицы под содержимым, а не поверх.
    pub fn behind(mut self) -> Self {
        self.builder.front = Some(false);
        self
    }

    pub fn class(mut self, c: impl Into<String>) -> Self {
        crate::widget::push_classes(&mut self.classes, c.into());
        self
    }
}

impl Widget for ParticleEmitter {
    fn create_element(&self) -> Box<dyn Element> {
        let mut el = EmitterElement {
            id: ElementId::new(),
            bounds: Rect::zero(),
            has_child: self.child.is_some(),
            builder: self.builder.clone(),
            mss: Overrides::default(),
            config: EmitterConfig::default(),
            burst_token: self.burst_token,
            pending_bursts: 0,
            particles: Vec::new(),
            emit_acc: 0.0,
            hover: None,
            time: 0.0,
            rng: 0x2545_F491 ^ self.burst_token.wrapping_mul(0x9E37_79B9),
            classes: self.classes.clone(),
            dirty: DirtyFlags::LAYOUT | DirtyFlags::RENDER,
            fields: MssFields::new(),
        };
        el.config = el.builder.over(&el.mss).resolve();
        Box::new(el)
    }

    fn can_update(&self, other: &dyn Any) -> bool {
        other.is::<Self>()
    }
    fn as_any(&self) -> &dyn Any {
        self
    }
    fn as_any_mut(&mut self) -> &mut dyn Any {
        self
    }

    fn mount(&self, tree: &mut ElementTree, parent_id: ElementId) {
        if let Some(child) = &self.child {
            let el = child.create_element();
            let id = tree.insert_with_type_id(el, Some(parent_id), child.as_any().type_id());
            child.mount(tree, id);
        }
    }

    fn child_widgets(&self) -> Vec<&dyn Widget> {
        self.child.as_ref().map(|c| vec![c.as_ref() as &dyn Widget]).unwrap_or_default()
    }

    fn widget_classes(&self) -> &[String] {
        &self.classes
    }
}

#[derive(Clone, Copy)]
struct Particle {
    /// Положение относительно левого верхнего угла элемента.
    x: f32,
    y: f32,
    vx: f32,
    vy: f32,
    size: f32,
    color: Color,
    rot: f32,
    vrot: f32,
    age: f32,
    life: f32,
    shape: ParticleShape,
    phase: f32,
}

struct EmitterElement {
    id: ElementId,
    bounds: Rect,
    has_child: bool,
    builder: Overrides,
    mss: Overrides,
    config: EmitterConfig,
    burst_token: u32,
    pending_bursts: u32,
    particles: Vec<Particle>,
    emit_acc: f32,
    /// Указатель над элементом (локальные координаты).
    hover: Option<Point>,
    time: f32,
    rng: u32,
    classes: Vec<String>,
    dirty: DirtyFlags,
    fields: MssFields,
}

impl EmitterElement {
    fn rand(&mut self) -> f32 {
        let mut x = self.rng;
        x ^= x << 13;
        x ^= x >> 17;
        x ^= x << 5;
        self.rng = x;
        (x >> 8) as f32 / (1u32 << 24) as f32
    }

    fn range(&mut self, r: (f32, f32)) -> f32 {
        r.0 + (r.1 - r.0) * self.rand()
    }

    fn current_rate(&self) -> f32 {
        let hover = if self.hover.is_some() { self.config.hover_rate } else { 0.0 };
        self.config.rate + hover
    }

    fn spawn_origin(&mut self) -> (f32, f32) {
        let (w, h) = (self.bounds.size.width, self.bounds.size.height);
        match self.config.emitter {
            EmitterShape::Point(x, y) => (w * x, h * y),
            EmitterShape::Line(side) => {
                let t = self.rand();
                match side {
                    0 => (w * t, 0.0),
                    1 => (w, h * t),
                    3 => (0.0, h * t),
                    _ => (w * t, h),
                }
            }
            EmitterShape::Rect => (w * self.rand(), h * self.rand()),
            EmitterShape::Ring => {
                let a = self.rand() * TAU;
                (w * 0.5 + a.cos() * w * 0.5, h * 0.5 + a.sin() * h * 0.5)
            }
            EmitterShape::Pointer => match self.hover {
                Some(p) => (p.x - self.bounds.origin.x, p.y - self.bounds.origin.y),
                None => (w * 0.5, h * 0.5),
            },
        }
    }

    fn spawn(&mut self, n: usize) {
        let room = self.config.max.saturating_sub(self.particles.len());
        for _ in 0..n.min(room) {
            let (x, y) = self.spawn_origin();
            let c = &self.config;
            let (dir, spread, speed_r, life_r, size_r, spin) = (c.direction, c.spread, c.speed, c.lifetime, c.size, c.spin);
            let ncolors = c.colors.len();
            let shape = c.shape;
            let angle = (dir + (self.rand() - 0.5) * spread).to_radians();
            let speed = self.range(speed_r);
            let life = self.range(life_r).max(0.05);
            let size = self.range(size_r).max(0.3);
            let ci = (self.rand() * ncolors as f32) as usize % ncolors;
            let color = self.config.colors[ci];
            let shape = if shape == ParticleShape::Mixed {
                match (self.rand() * 3.0) as u32 {
                    0 => ParticleShape::Square,
                    1 => ParticleShape::Triangle,
                    _ => ParticleShape::Circle,
                }
            } else {
                shape
            };
            let rot = self.rand() * TAU;
            let vrot = (self.rand() * 2.0 - 1.0) * spin.to_radians();
            let phase = self.rand() * TAU;
            self.particles.push(Particle {
                x,
                y,
                vx: angle.cos() * speed,
                vy: angle.sin() * speed,
                size,
                color,
                rot,
                vrot,
                age: 0.0,
                life,
                shape,
                phase,
            });
        }
    }

    fn refresh_config(&mut self) {
        self.config = self.builder.over(&self.mss).resolve();
    }

    fn draw(&self, list: &mut DisplayList) {
        if self.particles.is_empty() {
            return;
        }
        let o = self.bounds.origin;
        let c = &self.config;
        // Точки фигур ниже уже экранные (`cx`, `cy` включают `o`), поэтому
        // холст — с нулевым началом: с началом в `o` смещение прибавлялось
        // дважды, и все фигуры из полигонов (звёзды, сердечки, конфетти,
        // искры) улетали за пределы экрана — видны были только круги и
        // свечение, которые рисуются прямоугольниками.
        let mut canvas = CanvasContext::new(Point::zero(), self.bounds.size);
        let mut canvas_used = false;
        for p in &self.particles {
            let t = (p.age / p.life).clamp(0.0, 1.0);
            let fade_in = if c.fade_in > 0.0 { (t / c.fade_in).min(1.0) } else { 1.0 };
            let mut alpha = fade_in * (1.0 - t);
            if c.twinkle > 0.0 {
                alpha *= 1.0 - c.twinkle * 0.5 * (1.0 + (self.time * 18.0 + p.phase).sin());
            }
            let base = match c.color_end {
                Some(end) => p.color.lerp(&end, t),
                None => p.color,
            };
            let color = base.with_alpha(base.a * alpha);
            if color.a <= 0.003 {
                continue;
            }
            let size = p.size * (1.0 + (c.size_end - 1.0) * t);
            if size <= 0.05 {
                continue;
            }
            let wob = if c.wobble > 0.0 { (self.time * 3.0 + p.phase).sin() * c.wobble * t.min(1.0) } else { 0.0 };
            let cx = o.x + p.x + wob;
            let cy = o.y + p.y;
            let half = size * 0.5;
            if c.glow > 0.0 && p.shape != ParticleShape::Glow {
                let g = c.glow;
                list.push_shadow(
                    Rect::new(Point::new(cx - half, cy - half), Size::new(size, size)),
                    color.with_alpha(color.a * 0.8),
                    g,
                    (0.0, 0.0),
                    [half; 4],
                );
            }
            let rotated = |pts: &[(f32, f32)]| -> Vec<(f32, f32)> {
                let (s, co) = p.rot.sin_cos();
                pts.iter().map(|&(dx, dy)| (cx + dx * co - dy * s, cy + dx * s + dy * co)).collect()
            };
            match p.shape {
                ParticleShape::Circle | ParticleShape::Mixed => {
                    list.push_rect(Rect::new(Point::new(cx - half, cy - half), Size::new(size, size)), color, [half; 4]);
                }
                ParticleShape::Glow => {
                    list.push_shadow(
                        Rect::new(Point::new(cx - half * 0.5, cy - half * 0.5), Size::new(half, half)),
                        color,
                        half.max(1.0),
                        (0.0, 0.0),
                        [half * 0.5; 4],
                    );
                }
                ParticleShape::Square => {
                    canvas.set_color(color);
                    canvas.fill_polygon(&rotated(&[(-half, -half), (half, -half), (half, half), (-half, half)]));
                    canvas_used = true;
                }
                ParticleShape::Triangle => {
                    canvas.set_color(color);
                    canvas.fill_polygon(&rotated(&[(0.0, -size * 0.6), (size * 0.55, size * 0.5), (-size * 0.55, size * 0.5)]));
                    canvas_used = true;
                }
                ParticleShape::Star => {
                    let mut pts = Vec::with_capacity(10);
                    for i in 0..10 {
                        let r = if i % 2 == 0 { half } else { half * 0.42 };
                        let a = -PI / 2.0 + i as f32 * PI / 5.0;
                        pts.push((a.cos() * r, a.sin() * r));
                    }
                    canvas.set_color(color);
                    canvas.fill_polygon_concave(&rotated(&pts));
                    canvas_used = true;
                }
                ParticleShape::Heart => {
                    let mut pts = Vec::with_capacity(24);
                    for i in 0..24 {
                        let a = i as f32 / 24.0 * TAU;
                        let x = 16.0 * a.sin().powi(3);
                        let y = -(13.0 * a.cos() - 5.0 * (2.0 * a).cos() - 2.0 * (3.0 * a).cos() - (4.0 * a).cos());
                        pts.push((x / 32.0 * size, y / 32.0 * size));
                    }
                    canvas.set_color(color);
                    canvas.fill_polygon_concave(&rotated(&pts));
                    canvas_used = true;
                }
                ParticleShape::Ring => {
                    canvas.set_color(color);
                    canvas.set_stroke_width((size * 0.14).max(1.0));
                    canvas.stroke_circle(cx, cy, half);
                    canvas_used = true;
                }
                ParticleShape::Spark => {
                    let v = (p.vx * p.vx + p.vy * p.vy).sqrt().max(1.0);
                    let len = (v * 0.035).clamp(size * 2.0, size * 14.0);
                    let (ux, uy) = (p.vx / v, p.vy / v);
                    let (nx, ny) = (-uy * half, ux * half);
                    canvas.set_color(color);
                    canvas.fill_polygon(&[
                        (cx + nx, cy + ny),
                        (cx - nx, cy - ny),
                        (cx - ux * len - nx * 0.3, cy - uy * len - ny * 0.3),
                        (cx - ux * len + nx * 0.3, cy - uy * len + ny * 0.3),
                    ]);
                    canvas_used = true;
                }
            }
        }
        if canvas_used {
            canvas.flush(list);
        }
    }
}

impl Element for EmitterElement {
    fn update(&mut self, widget: &dyn Widget, _ctx: &mut UpdateContext) {
        if let Some(w) = widget.as_any().downcast_ref::<ParticleEmitter>() {
            self.has_child = w.child.is_some();
            if self.builder != w.builder {
                self.builder = w.builder.clone();
                self.refresh_config();
            }
            if w.burst_token != self.burst_token {
                self.burst_token = w.burst_token;
                self.pending_bursts += 1;
            }
            self.mark_dirty(DirtyFlags::RENDER);
        }
    }

    fn layout(&mut self, c: Constraints) -> Size {
        let w = if c.max_width.is_finite() { c.max_width } else { 0.0 };
        let h = if c.max_height.is_finite() { c.max_height } else { 0.0 };
        self.bounds.size = Size::new(w, h);
        self.bounds.size
    }

    fn layout_hint(&self) -> LayoutHint {
        LayoutHint::Padding { left: 0.0, top: 0.0, right: 0.0, bottom: 0.0 }
    }

    fn build_display_list(&self, list: &mut DisplayList, _clip: Rect) {
        if !self.config.front {
            self.draw(list);
        }
    }

    fn post_build_display_list(&self, list: &mut DisplayList, _clip: Rect) {
        if self.config.front {
            self.draw(list);
        }
    }

    fn handle_event(&mut self, event: &Event, ctx: &mut EventContext) -> EventResult {
        if let Event::MouseMove(p) = event {
            let inside = self.bounds.contains(*p);
            let was = self.hover.is_some();
            self.hover = inside.then_some(*p);
            if inside != was && self.config.hover_rate > 0.0 {
                // Поток при наведении: встать в тик анимаций.
                use crate::widget::context::EventContextExt;
                ctx.request_paint();
            }
        }
        EventResult::Ignored
    }

    fn wants_animate_tick(&self) -> bool {
        self.pending_bursts > 0 || !self.particles.is_empty() || self.current_rate() > 0.0
    }

    fn needs_repaint(&self) -> bool {
        self.wants_animate_tick()
    }

    fn animate(&mut self, dt: Duration) -> bool {
        let dt = dt.as_secs_f32().min(0.05);
        self.time += dt;
        if self.bounds.size.width <= 0.0 && self.bounds.size.height <= 0.0 && !self.has_child {
            return self.pending_bursts > 0;
        }
        while self.pending_bursts > 0 {
            self.pending_bursts -= 1;
            let n = self.config.burst as usize;
            self.spawn(n);
        }
        let rate = self.current_rate();
        if rate > 0.0 {
            self.emit_acc += rate * dt;
            let n = self.emit_acc.floor();
            if n >= 1.0 {
                self.emit_acc -= n;
                self.spawn(n as usize);
            }
        } else {
            self.emit_acc = 0.0;
        }
        let (gx, gy) = self.config.gravity;
        let drag = self.config.drag;
        for p in &mut self.particles {
            p.vx += gx * dt;
            p.vy += gy * dt;
            if drag > 0.0 {
                let k = (1.0 - drag * dt).max(0.0);
                p.vx *= k;
                p.vy *= k;
            }
            p.x += p.vx * dt;
            p.y += p.vy * dt;
            p.rot += p.vrot * dt;
            p.age += dt;
        }
        self.particles.retain(|p| p.age < p.life);
        self.mark_dirty(DirtyFlags::RENDER);
        !self.particles.is_empty() || rate > 0.0
    }

    fn passthrough_hit_test(&self) -> bool {
        true
    }
    fn clip_content(&self) -> bool {
        false
    }
    fn children(&self) -> &[ElementId] {
        &[]
    }
    fn bounds(&self) -> Rect {
        self.bounds
    }
    fn set_position(&mut self, pos: Point) {
        self.bounds.origin = pos;
    }
    fn mark_dirty(&mut self, flags: DirtyFlags) {
        self.dirty |= flags;
    }
    fn clear_dirty(&mut self, flags: DirtyFlags) {
        self.dirty.remove(flags);
    }
    fn is_dirty(&self, flags: DirtyFlags) -> bool {
        self.dirty.contains(flags)
    }
    fn id(&self) -> ElementId {
        self.id
    }
    fn set_id(&mut self, id: ElementId) {
        self.id = id;
    }
    fn mount(&mut self, _tree: &mut ElementTree) {}
    fn set_classes(&mut self, classes: Vec<String>) {
        self.classes = classes;
    }
    fn get_classes(&self) -> &[String] {
        &self.classes
    }
    fn reset_mss_styles(&mut self) {
        self.fields.reset();
    }
    fn mss(&self) -> Option<&MssFields> {
        Some(&self.fields)
    }
    fn apply_computed_style(&mut self, style: &ComputedStyle) {
        self.fields.apply(style);
        self.mss = Overrides::from_style(style);
        self.refresh_config();
        self.mark_dirty(DirtyFlags::RENDER);
    }
    fn element_type_name(&self) -> &str {
        "ParticleEmitter"
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::testing::TestHarness;
    use crate::widget::WidgetExt;

    fn emitter_count(h: &mut TestHarness) -> usize {
        h.paint().commands().len()
    }

    #[test]
    fn scalar_and_range_parsing() {
        assert_eq!(parse_scalar("300ms"), Some(0.3));
        assert_eq!(parse_scalar("12px"), Some(12.0));
        assert_eq!(parse_scalar("-90deg"), Some(-90.0));
        assert_eq!(parse_scalar("0.5turn"), Some(180.0));
        assert_eq!(parse_range("0.6s 1.2s"), Some((0.6, 1.2)));
        assert_eq!(parse_range("5"), Some((5.0, 5.0)));
        assert_eq!(EmitterShape::parse("point 50% 100%"), Some(EmitterShape::Point(0.5, 1.0)));
        assert_eq!(EmitterShape::parse("line top"), Some(EmitterShape::Line(0)));
    }

    #[test]
    fn mss_configures_preset_and_overrides() {
        let w = ParticleEmitter::new().class("fx");
        let mut h = TestHarness::new(Box::new(w));
        h.apply_mss(".fx { particle-preset: fire; particle-rate: 25; particle-colors: \"#ff0000 #00ff00\"; particle-lifetime: 2s; }");
        h.layout(100.0, 100.0);
        // Непрерывный поток рождает частицы без всплесков.
        for _ in 0..30 {
            h.tree.animate(h.root_id, Duration::from_millis(16));
        }
        assert!(emitter_count(&mut h) > 3);
    }

    #[test]
    fn burst_token_spawns_particles_and_they_die() {
        let mut h = TestHarness::new(Box::new(ParticleEmitter::new().preset(ParticlePreset::Stars).burst(20).burst_token(0)));
        h.layout(100.0, 100.0);
        let before = emitter_count(&mut h);
        h.update_widget(Box::new(ParticleEmitter::new().preset(ParticlePreset::Stars).burst(20).burst_token(1)));
        h.layout(100.0, 100.0);
        h.tree.animate(h.root_id, Duration::from_millis(16));
        assert!(emitter_count(&mut h) > before + 10);
        for _ in 0..200 {
            h.tree.animate(h.root_id, Duration::from_millis(16));
        }
        assert_eq!(emitter_count(&mut h), before);
    }

    #[test]
    fn hover_rate_emits_only_under_pointer() {
        let content = crate::widgets::DecoratedBox::new().class("c");
        let w = ParticleEmitter::new().preset(ParticlePreset::Sparkle).hover_rate(60.0).child(content);
        let mut h = TestHarness::new(Box::new(crate::widgets::Column::new().child(w)));
        h.apply_mss(".c { width: 50; height: 50; }");
        h.layout(200.0, 200.0);
        for _ in 0..20 {
            h.tree.animate(h.root_id, Duration::from_millis(16));
        }
        let idle = emitter_count(&mut h);
        h.tree.handle_event(h.root_id, &Event::MouseMove(Point::new(25.0, 25.0)));
        for _ in 0..20 {
            h.tree.animate(h.root_id, Duration::from_millis(16));
        }
        assert!(emitter_count(&mut h) > idle, "при наведении должны появиться частицы");
    }
}
