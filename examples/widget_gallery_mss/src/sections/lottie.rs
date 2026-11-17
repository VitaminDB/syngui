//! «Lottie и частицы»: собственные Lottie-анимации (JSON написан вручную,
//! `assets/lottie/*.json`) и эмиттеры частиц с пресетами и настройкой из MSS.
//!
//! Цвета анимаций берутся из текущей темы галереи: в JSON вместо цвета стоит
//! метка `"@accent"` (→ `[r,g,b,1]`) или, в стопах градиента, `"@@accent"`
//! (→ `r,g,b`). При смене темы композиции пересобираются с новыми цветами,
//! поэтому раздел перекрашивается вместе со всей галереей.

use std::collections::HashMap;
use std::fmt::Write as _;
use std::sync::{Arc, Mutex, OnceLock};

use syngui::lottie::Composition;
use syngui::prelude::*;
use syngui::widgets::{EmitKind, GestureDetector, Icon, Lottie, ParticleEmitter, ParticleSystem};

use super::{label, rx, section_card, section_title};
use crate::theme_data::{self, GalleryTheme};
use crate::GalleryCtx;

// ─── Анимации ───────────────────────────────────────────────────────────

/// Анимация витрины: ключ кэша, шаблон JSON, подпись и что в ней показано.
struct Anim {
    key: &'static str,
    json: &'static str,
    title: &'static str,
    caption: &'static str,
}

const SPINNER: Anim = Anim {
    key: "spinner",
    json: include_str!("../../assets/lottie/spinner.json"),
    title: "Загрузка",
    caption: "trim path · градиент · пунктир",
};
const HEART: Anim = Anim {
    key: "heart",
    json: include_str!("../../assets/lottie/heart.json"),
    title: "Пульс",
    caption: "Безье · радиальный градиент",
};
const CHECK: Anim = Anim {
    key: "check",
    json: include_str!("../../assets/lottie/check.json"),
    title: "Успех",
    caption: "рисование обводки · repeater · маркер",
};
const EQUALIZER: Anim = Anim {
    key: "equalizer",
    json: include_str!("../../assets/lottie/equalizer.json"),
    title: "Эквалайзер",
    caption: "rect · repeater-отражение",
};
const ORBIT: Anim = Anim {
    key: "orbit",
    json: include_str!("../../assets/lottie/orbit.json"),
    title: "Орбиты",
    caption: "parenting · путь по кривой",
};
const MORPH: Anim = Anim {
    key: "morph",
    json: include_str!("../../assets/lottie/morph.json"),
    title: "Морфинг",
    caption: "ключи контура · round corners",
};
const WAVE: Anim = Anim {
    key: "wave",
    json: include_str!("../../assets/lottie/wave.json"),
    title: "Волна",
    caption: "маска слоя · бегущая волна",
};
const BELL: Anim = Anim {
    key: "bell",
    json: include_str!("../../assets/lottie/bell.json"),
    title: "Уведомление",
    caption: "anchor · затухание",
};
const DOTS: Anim = Anim {
    key: "dots",
    json: include_str!("../../assets/lottie/dots.json"),
    title: "Точки",
    caption: "squash & stretch · hold-ключи",
};

const GRID: [&Anim; 8] = [&SPINNER, &HEART, &EQUALIZER, &ORBIT, &MORPH, &WAVE, &BELL, &DOTS];

/// Цвет темы по имени метки (`@accent`, `@@pink` …).
fn theme_color<'a>(t: &'a GalleryTheme, name: &str) -> &'a str {
    match name {
        "accent" => t.accent,
        "success" => t.success,
        "warning" => t.warning,
        "error" => t.error,
        "info" => t.info,
        "purple" => t.purple,
        "pink" => t.pink,
        "orange" => t.orange,
        "teal" => t.teal,
        "indigo" => t.indigo,
        "text" => t.text,
        "text_subtle" => t.text_subtle,
        "border" => t.border,
        "bg_surface" => t.bg_surface,
        "bg_elevated" => t.bg_elevated,
        _ => t.accent,
    }
}

/// `#rgb` / `#rrggbb[aa]` → sRGB 0..1 (как в Lottie); иное — серый.
fn hex_rgb(hex: &str) -> [f32; 3] {
    let h = hex.trim().trim_start_matches('#');
    let byte = |s: &str| u8::from_str_radix(s, 16).ok().map(|v| v as f32 / 255.0);
    let parsed = match h.len() {
        3 | 4 => {
            let d = |i: usize| byte(&h[i..i + 1].repeat(2));
            (|| Some([d(0)?, d(1)?, d(2)?]))()
        }
        6 | 8 => (|| Some([byte(&h[0..2])?, byte(&h[2..4])?, byte(&h[4..6])?]))(),
        _ => None,
    };
    parsed.unwrap_or([0.5, 0.5, 0.5])
}

/// Подставить цвета темы вместо меток `"@имя"` и `"@@имя"`.
fn themed_json(template: &str, theme: &GalleryTheme) -> String {
    let mut out = String::with_capacity(template.len() + 256);
    let mut rest = template;
    while let Some(pos) = rest.find("\"@") {
        out.push_str(&rest[..pos]);
        let after = &rest[pos + 2..];
        let (bare, after) = match after.strip_prefix('@') {
            Some(a) => (true, a),
            None => (false, after),
        };
        let Some(end) = after.find('"') else {
            out.push_str(&rest[pos..]);
            return out;
        };
        let [r, g, b] = hex_rgb(theme_color(theme, &after[..end]));
        if bare {
            let _ = write!(out, "{r:.4},{g:.4},{b:.4}");
        } else {
            let _ = write!(out, "[{r:.4},{g:.4},{b:.4},1]");
        }
        rest = &after[end + 1..];
    }
    out.push_str(rest);
    out
}

/// Композиция в цветах темы. Кэш по (тема, анимация): при смене скорости и
/// размера виджет получает ту же `Arc` и продолжает с текущего кадра.
fn composition(theme_id: &str, anim: &Anim) -> Arc<Composition> {
    static CACHE: OnceLock<Mutex<HashMap<(String, &'static str), Arc<Composition>>>> = OnceLock::new();
    let cache = CACHE.get_or_init(|| Mutex::new(HashMap::new()));
    let key = (theme_id.to_string(), anim.key);
    if let Some(c) = cache.lock().ok().and_then(|m| m.get(&key).cloned()) {
        return c;
    }
    let theme = theme_data::find_theme(theme_id).unwrap_or_else(theme_data::default_light);
    let comp = Arc::new(Composition::from_str(&themed_json(anim.json, &theme)).unwrap_or_else(|e| {
        log::error!("Lottie `{}`: {e}", anim.key);
        Composition::from_str(r#"{"fr":30,"ip":0,"op":1,"w":200,"h":200,"layers":[]}"#).expect("пустая композиция")
    }));
    if let Ok(mut m) = cache.lock() {
        m.insert(key, comp.clone());
    }
    comp
}

fn theme_id_signal() -> RwSignal<String> {
    use_context::<GalleryCtx>().current_theme_id
}

fn code(text: &str) -> impl Widget {
    DecoratedBox::new().child(Text::new(text).class("lt-code-text")).class("lt-code")
}

fn block_head(title: &str, hint: &str) -> impl Widget {
    Column::new()
        .gap(2.0)
        .child(Text::new(title).class("lt-block-title"))
        .child(label(hint))
}

/// Сетка анимаций с общим управлением: пауза, скорость, направление, размер.
fn build_lottie_grid() -> impl Widget {
    let theme_id = theme_id_signal();
    let playing = use_signal(true);
    let speed_idx = use_signal(2usize);
    let reverse = use_signal(false);
    let size = use_signal(128.0f32);
    const SPEEDS: [f32; 4] = [0.25, 0.5, 1.0, 2.0];

    let controls = Flex::new()
        .wrap()
        .gap(14.0)
        .cross_axis_alignment(CrossAxisAlignment::Center)
        .child(move || {
            let on = playing.get();
            Button::new(if on { "Пауза" } else { "Играть" })
                .leading_icon(if on { "\u{E034}" } else { "\u{E037}" })
                .class("secondary")
                .on_click(move || playing.set(!playing.get_untracked()))
        })
        .child(
            SegmentedButton::new(vec!["0.25×", "0.5×", "1×", "2×"])
                .selected(2)
                .on_change(move |i| speed_idx.set(i)),
        )
        .child(
            Row::new()
                .gap(8.0)
                .cross_axis_alignment(CrossAxisAlignment::Center)
                .child(Toggle::new().on(false).on_change(move |v| reverse.set(v)))
                .child(label("назад")),
        )
        .child(
            Row::new()
                .gap(8.0)
                .cross_axis_alignment(CrossAxisAlignment::Center)
                .child(label("размер"))
                .child(
                    Slider::new()
                        .range(72.0, 168.0)
                        .value(128.0)
                        .on_change(move |v| size.set(v))
                        .class("lt-slider"),
                ),
        );

    let grid = Column::new().child(move || {
        let id = theme_id.get();
        let sign = if reverse.get() { -1.0 } else { 1.0 };
        let speed = SPEEDS[speed_idx.get().min(SPEEDS.len() - 1)] * sign;
        let side = size.get().round();
        let mut row = Flex::new().wrap().gap(16.0);
        for a in GRID {
            row = row.child(
                DecoratedBox::new()
                    .child(
                        Column::new()
                            .gap(4.0)
                            .cross_axis_alignment(CrossAxisAlignment::Center)
                            .child(
                                DecoratedBox::new()
                                    .child(
                                        Lottie::new(composition(&id, a))
                                            .speed(speed)
                                            .subframe(true)
                                            .playing_signal(playing)
                                            .size(side, side),
                                    )
                                    .class("lt-stage"),
                            )
                            .child(Text::new(a.title).class("lt-card-title"))
                            .child(Text::new(a.caption).class("lt-card-caption")),
                    )
                    .class("lt-card lt-card-grid"),
            );
        }
        row
    });

    Column::new()
        .gap(14.0)
        .child(block_head(
            "Lottie — собственный парсер и растеризатор",
            "Анимации написаны вручную в JSON, цвета приходят из темы. Управление — общее для всей сетки.",
        ))
        .child(controls)
        .child(grid)
}

/// Сценарий «Отправить»: спиннер один круг → on_complete → галочка по маркеру.
fn build_scenario() -> impl Widget {
    let theme_id = theme_id_signal();
    let state = use_signal(0u8); // 0 — ждём, 1 — отправка, 2 — готово
    let stage = DecoratedBox::new()
        .child(rx(move || -> Box<dyn Widget> {
            let id = theme_id.get();
            match state.get() {
                1 => Box::new(
                    Lottie::new(composition(&id, &SPINNER))
                        .repeat(false)
                        .speed(0.75)
                        .subframe(true)
                        .on_complete(move || state.set(2))
                        .size(120.0, 120.0),
                ),
                2 => Box::new(
                    Lottie::new(composition(&id, &CHECK))
                        .marker("draw")
                        .repeat(false)
                        .subframe(true)
                        .size(120.0, 120.0),
                ),
                _ => Box::new(Icon::new("\u{E163}").class("lt-idle-icon")),
            }
        }))
        .class("lt-stage lt-stage-wide");
    let button = Row::new().child(move || {
        let s = state.get();
        let text = match s {
            1 => "Отправка…",
            2 => "Ещё раз",
            _ => "Отправить",
        };
        Button::new(text).on_click(move || {
            if state.get_untracked() != 1 {
                state.set(1);
            }
        })
    });
    DecoratedBox::new()
        .child(
            Column::new()
                .gap(10.0)
                .child(Text::new("Сценарий").class("lt-card-title"))
                .child(label("repeat(false) + on_complete, затем отрезок по маркеру «draw»"))
                .child(stage)
                .child(button)
                .child(code(".marker(\"draw\")\n.repeat(false)\n.on_complete(|| …)")),
        )
        .class("lt-card lt-card-wide")
}

/// Перемотка: прогресс-сигнал двусторонний — слайдер ведёт кадр на паузе.
fn build_scrub() -> impl Widget {
    let theme_id = theme_id_signal();
    let playing = use_signal(false);
    let progress = use_signal(0.3f32);
    DecoratedBox::new()
        .child(
            Column::new()
                .gap(10.0)
                .child(Text::new("Перемотка").class("lt-card-title"))
                .child(label("progress_signal: кадр по слайдеру, playing_signal: пуск/пауза"))
                .child(
                    DecoratedBox::new()
                        .child(move || {
                            Lottie::new(composition(&theme_id.get(), &ORBIT))
                                .autoplay(false)
                                .subframe(true)
                                .playing_signal(playing)
                                .progress_signal(progress)
                                .size(120.0, 120.0)
                        })
                        .class("lt-stage lt-stage-wide"),
                )
                .child(
                    Row::new()
                        .gap(10.0)
                        .cross_axis_alignment(CrossAxisAlignment::Center)
                        .child(move || {
                            let on = playing.get();
                            Button::new("").icon(if on { "\u{E034}" } else { "\u{E037}" })
                                .class("secondary")
                                .on_click(move || playing.set(!playing.get_untracked()))
                        })
                        .child(move || {
                            Slider::new()
                                .value(progress.get())
                                .on_change(move |v| progress.set(v))
                                .class("lt-slider")
                        }),
                )
                .child(code(".progress_signal(progress)\n.playing_signal(playing)")),
        )
        .class("lt-card lt-card-wide")
}

/// «Нравится»: Lottie-сердце + всплеск частиц-сердечек по клику.
fn build_like() -> impl Widget {
    let theme_id = theme_id_signal();
    let clicks = use_signal(0u32);
    let playing = use_signal(false);
    let progress = use_signal(0.0f32);
    DecoratedBox::new()
        .child(
            Column::new()
                .gap(10.0)
                .child(Text::new("Lottie + частицы").class("lt-card-title"))
                .child(label("клик: сердце бьётся, вылетают сердечки (ParticleEmitter)"))
                .child(move || {
                    let n = clicks.get();
                    DecoratedBox::new()
                        .child(
                            ParticleEmitter::new().burst_token(n).class("lt-like-fx").child(
                                GestureDetector::new()
                                    .on_click(move || {
                                        clicks.set(clicks.get_untracked() + 1);
                                        progress.set(0.0);
                                        playing.set(true);
                                    })
                                    .child(
                                        Lottie::new(composition(&theme_id.get(), &HEART))
                                            .autoplay(false)
                                            .repeat(false)
                                            .subframe(true)
                                            .playing_signal(playing)
                                            .progress_signal(progress)
                                            .size(120.0, 120.0),
                                    ),
                            ),
                        )
                        .class("lt-stage lt-stage-wide")
                })
                .child(move || Text::new(format!("♥ {}", 128 + clicks.get())).class("lt-like-count"))
                .child(code("particle-preset: hearts;\nparticle-color: var(--pink);")),
        )
        .class("lt-card lt-card-wide")
}

// ─── Частицы ────────────────────────────────────────────────────────────

/// Карточка эмиттера: сцена, название и строка MSS.
fn particle_card(title: &str, fx: &str, snippet: &str, content: Option<&str>) -> impl Widget {
    let stage: Box<dyn Widget> = match content {
        // Обёртка: эмиттер видит указатель над содержимым.
        Some(text) => Box::new(
            ParticleEmitter::new().class(fx.to_string()).child(
                DecoratedBox::new()
                    .child(Text::new(text.to_string()).class("lt-stage-text"))
                    .class("lt-pstage lt-pstage-center"),
            ),
        ),
        None => Box::new(DecoratedBox::new().child(ParticleEmitter::new().class(fx.to_string())).class("lt-pstage")),
    };
    DecoratedBox::new()
        .child(
            Column::new()
                .gap(6.0)
                .child(stage)
                .child(Text::new(title.to_string()).class("lt-card-title"))
                .child(Text::new(snippet.to_string()).class("lt-card-caption")),
        )
        .class("lt-card lt-card-grid")
}

/// Слой частиц позади содержимого (`particle-layer: back`).
fn back_layer_card() -> impl Widget {
    DecoratedBox::new()
        .child(
            Column::new()
                .gap(6.0)
                .child(
                    ParticleEmitter::new().class("lt-p-back").child(
                        DecoratedBox::new()
                            .child(
                                DecoratedBox::new()
                                    .child(Text::new("particle-layer: back").class("lt-glass-text"))
                                    .class("lt-glass"),
                            )
                            .class("lt-pstage lt-pstage-center"),
                    ),
                )
                .child(Text::new("Позади содержимого").class("lt-card-title"))
                .child(Text::new("stars · layer back").class("lt-card-caption")),
        )
        .class("lt-card lt-card-grid")
}

fn build_particles() -> impl Widget {
    let mut cards = Flex::new().wrap().gap(16.0);
    for (title, fx, snippet, content) in [
        ("Магия", "lt-p-magic", "magic · 3 цвета темы", None),
        ("Дождь", "lt-p-rain", "rain · var(--info)", None),
        ("Пузыри", "lt-p-bubbles", "bubbles · wobble", None),
        ("Угольки", "lt-p-embers", "embers · point 50% 92%", None),
        ("Пыль в луче", "lt-p-dust", "dust · twinkle", None),
        ("Портал", "lt-p-portal", "emitter: ring · glow", None),
        ("Кисть", "lt-p-trail", "emitter: pointer · hover-rate", Some("проведите курсором")),
    ] {
        cards = cards.child(particle_card(title, fx, snippet, content));
    }
    cards = cards.child(back_layer_card());

    // Все формы частиц на одинаковом потоке.
    let mut shapes = Flex::new().wrap().gap(10.0);
    for (i, name) in ["circle", "square", "triangle", "star", "spark", "ring", "glow", "heart", "mixed"]
        .into_iter()
        .enumerate()
    {
        shapes = shapes.child(
            Column::new()
                .gap(4.0)
                .cross_axis_alignment(CrossAxisAlignment::Center)
                .child(
                    DecoratedBox::new()
                        .child(ParticleEmitter::new().class(format!("lt-shape-fx lt-shape-{name} lt-tint-{}", i % 5)))
                        .class("lt-shape-stage"),
                )
                .child(Text::new(name).class("lt-card-caption")),
        );
    }

    Column::new()
        .gap(14.0)
        .child(block_head(
            "ParticleEmitter — пресеты и MSS",
            "Пресет задаёт поведение, MSS переопределяет любое поле: цвета — переменными темы.",
        ))
        .child(cards)
        .child(label("particle-shape: все формы"))
        .child(shapes)
        .child(code(
            ".lt-p-trail {\n    particle-emitter: pointer;   particle-hover-rate: 90;\n    particle-shape: glow;        particle-colors: \"var(--accent) var(--pink)\";\n}",
        ))
}

/// Старый `ParticleSystem`: всплеск по счётчику, палитра из темы.
fn build_particle_system() -> impl Widget {
    let theme_id = theme_id_signal();
    let token = use_signal(0u32);
    let kind = use_signal(0usize);
    let fire = move |k: usize| {
        kind.set(k);
        token.set(token.get_untracked() + 1);
    };
    DecoratedBox::new()
        .child(
            Column::new()
                .gap(10.0)
                .child(Text::new("ParticleSystem").class("lt-card-title"))
                .child(label("простой оверлей: всплеск по burst_token, палитра темы"))
                .child(
                    DecoratedBox::new()
                        .child(move || {
                            let theme = theme_data::find_theme(&theme_id.get()).unwrap_or_else(theme_data::default_light);
                            let palette = [theme.accent, theme.purple, theme.pink, theme.orange, theme.teal, theme.warning]
                                .iter()
                                .map(|h| Color::from_hex(h))
                                .collect();
                            let k = match kind.get() {
                                1 => EmitKind::Sparks,
                                2 => EmitKind::Fireworks,
                                _ => EmitKind::Confetti,
                            };
                            ParticleSystem::new(token.get()).kind(k).count(110).palette(palette)
                        })
                        .class("lt-pstage lt-pstage-ps"),
                )
                .child(
                    Row::new()
                        .gap(8.0)
                        .child(Button::new("Конфетти").class("secondary").on_click(move || fire(0)))
                        .child(Button::new("Искры").class("secondary").on_click(move || fire(1)))
                        .child(Button::new("Салют").class("secondary").on_click(move || fire(2))),
                ),
        )
        .class("lt-card lt-card-ps")
}

pub fn build_lottie_section() -> impl Widget {
    section_card(
        Column::new()
            .gap(26.0)
            .child(section_title("Lottie и частицы"))
            .child(build_lottie_grid())
            .child(
                Flex::new()
                    .wrap()
                    .gap(16.0)
                    .child(build_scenario())
                    .child(build_scrub())
                    .child(build_like())
                    .child(build_particle_system()),
            )
            .child(build_particles()),
    )
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Все шаблоны во всех темах разбираются без ошибок и меток не остаётся.
    #[test]
    fn templates_parse_in_every_theme() {
        let all = [&SPINNER, &HEART, &CHECK, &EQUALIZER, &ORBIT, &MORPH, &WAVE, &BELL, &DOTS];
        for theme in theme_data::builtin_themes() {
            for a in all {
                let json = themed_json(a.json, &theme);
                assert!(!json.contains("\"@"), "{}: осталась метка", a.key);
                let c = Composition::from_str(&json).unwrap_or_else(|e| panic!("{} / {}: {e}", a.key, theme.id));
                assert!(c.out_point > c.in_point, "{}", a.key);
            }
        }
    }

    #[test]
    fn hex_parsing() {
        assert_eq!(hex_rgb("#ff0000"), [1.0, 0.0, 0.0]);
        assert_eq!(hex_rgb("#0f0"), [0.0, 1.0, 0.0]);
        assert_eq!(hex_rgb("rgba(1,2,3)"), [0.5, 0.5, 0.5]);
    }
}
