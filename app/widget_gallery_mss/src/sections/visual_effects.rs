use syngui::mgui;
use syngui::prelude::*;
use syngui::widgets::*;

use super::{label, section_card, section_title};

// ── Helper: цветной блок-подложка для демонстрации фильтров ──────────────────

fn fx_subject(gradient_class: &str, filter_class: &str) -> impl Widget {
    DecoratedBox::new()
        .class("fx-demo-box")
        .class(gradient_class)
        .class(filter_class)
        .child(mgui! {
            Column::new().gap(4.0) => [
                Text::new("Aa Bb").bold().class("fx-demo-text").style("font-size", 18.0_f32),
                Row::new().gap(6.0) => [
                    Text::new("1 2 3").class("fx-demo-text").style("font-size", 12.0_f32),
                    Icon::new("★").class("fx-demo-text"),
                ],
            ]
        })
}

// ── Helper: карточка одного фильтр-эффекта ───────────────────────────────────

fn filter_card(gradient_class: &str, filter_class: &str, label_text: &str, mss_code: &str) -> impl Widget {
    mgui! {
        Column::new().gap(6.0) => [
            fx_subject(gradient_class, filter_class),
            Text::new(label_text).bold().class("fx-label"),
            Text::new(mss_code).class("fx-code"),
        ]
    }
}

// ── Helper: карточка тени/outline ────────────────────────────────────────────

fn shadow_card(extra_class: &str, label_text: &str, mss_code: &str) -> impl Widget {
    mgui! {
        Column::new().gap(6.0) => [
            DecoratedBox::new()
                .class("fx-shadow-subject")
                .class(extra_class)
                .child(
                    Text::new(label_text).bold().class("section-title-text").style("font-size", 13.0_f32)
                ),
            Text::new(label_text).bold().class("fx-label"),
            Text::new(mss_code).class("fx-code"),
        ]
    }
}

// ═══════════════════════════════════════════════════════════════════════════════

pub fn build_visual_effects_section() -> impl Widget {
    mgui! {
        Column::new().gap(28.0) => [
            Text::new("Visual Effects").class("page-title"),
            Text::new("GPU-ускоренные эффекты через MSS: фильтры, тени, обводки, glassmorphism")
                .class("page-subtitle"),

            // ─── Тени (работают сейчас) ───────────────────────────────────────
            section_card(mgui! {
                Column::new().gap(16.0) => [
                    section_title("Тени (Drop Shadow / Inner Shadow)"),
                    label("box-shadow — внешние и внутренние тени с blur-радиусом"),
                    Row::new().gap(16.0) => [
                        shadow_card("fx-drop-shadow-sm",      "Small",    "box-shadow: 0 2 8 rgba(0,0,0,.15)"),
                        shadow_card("fx-drop-shadow-md",      "Medium",   "box-shadow: 0 4 16 rgba(0,0,0,.2)"),
                        shadow_card("fx-drop-shadow-lg",      "Large",    "box-shadow: 0 8 32 rgba(0,0,0,.25)"),
                        shadow_card("fx-drop-shadow-colored", "Colored",  "box-shadow: 0 4 20 rgba(99,102,241,.5)"),
                    ],
                    Row::new().gap(16.0) => [
                        shadow_card("fx-inner-shadow",        "Inner",      "box-shadow: inset 0 2 8 rgba(0,0,0,.2)"),
                        shadow_card("fx-inner-shadow-deep",   "Inner Deep", "box-shadow: inset 0 4 16 rgba(0,0,0,.3)"),
                        shadow_card("fx-inner-shadow-top",    "Inner Top",  "box-shadow: inset 0 -4 12 rgba(0,0,0,.25)"),
                    ],
                ]
            }),

            // ─── Glow (Additive Blend) ────────────────────────────────────────
            section_card(mgui! {
                Column::new().gap(16.0) => [
                    section_title("Glow (Additive Blend)"),
                    label("glow — аддитивное свечение: как box-shadow, но с additive blend (свет накапливается)"),
                    Row::new().gap(16.0) => [
                        shadow_card("fx-glow-blue",  "Blue Glow",  "glow: 0 0 24 rgba(99,102,241,.8)"),
                        shadow_card("fx-glow-cyan",  "Cyan Glow",  "glow: 0 0 20 rgba(34,211,238,.7)"),
                        shadow_card("fx-glow-pink",  "Pink Glow",  "glow: 0 0 28 rgba(236,72,153,.75)"),
                        shadow_card("fx-glow-green", "Green Glow", "glow: 0 0 22 rgba(34,197,94,.7)"),
                    ],
                    Row::new().gap(16.0) => [
                        shadow_card("fx-glow-multi", "Multi Glow", "glow: ... indigo, ... pink"),
                        shadow_card("fx-glow-neon",  "Neon",       "glow: 3 layers + border"),
                    ],
                ]
            }),

            // ─── CSS Фильтры ───────────────────────────────────────────────────
            section_card(mgui! {
                Column::new().gap(16.0) => [
                    section_title("CSS Фильтры (filter)"),
                    label("filter: blur(), grayscale(), sepia(), invert(), brightness(), contrast(), pixelate()…"),
                    Row::new().gap(16.0) => [
                        filter_card("gradient-sunset",  "fx-blur-4",     "Blur 4px",     "filter: blur(4px)"),
                        filter_card("gradient-ocean",   "fx-blur-8",     "Blur 8px",     "filter: blur(8px)"),
                        filter_card("gradient-sunset",  "fx-grayscale",  "Grayscale",    "filter: grayscale(100%)"),
                        filter_card("gradient-sunset",  "fx-sepia",      "Sepia",        "filter: sepia(80%)"),
                    ],
                    Row::new().gap(16.0) => [
                        filter_card("gradient-ocean",   "fx-invert",     "Invert",       "filter: invert(100%)"),
                        filter_card("gradient-sunset",  "fx-brightness", "Brightness",   "filter: brightness(1.5)"),
                        filter_card("gradient-ocean",   "fx-contrast",   "Contrast",     "filter: contrast(2.0)"),
                        filter_card("gradient-sunset",  "fx-pixelate",   "Pixelate",     "filter: pixelate(8px)"),
                    ],
                    Row::new().gap(16.0) => [
                        filter_card("gradient-ocean",   "fx-chroma",        "Chromatic Aberr.", "filter: chromatic-aberration(3px)"),
                        filter_card("gradient-sunset",  "fx-edge",          "Edge Detect",      "filter: edge-detect(0.3)"),
                        filter_card("gradient-ocean",   "fx-scanlines",     "Scanlines/CRT",    "filter: crt(0.5)"),
                        filter_card("gradient-sunset",  "fx-displacement",  "Displacement",     "filter: wave(4px, 0.5)"),
                    ],
                ]
            }),

            // ─── Overlay эффекты ───────────────────────────────────────────────
            section_card(mgui! {
                Column::new().gap(16.0) => [
                    section_title("Overlay эффекты"),
                    label("color-tint — цветной оверлей; noise — зернистость; vignette — затемнение краёв"),
                    Row::new().gap(16.0) => [
                        filter_card("gradient-ocean",  "fx-tint-red",  "Red Tint",   "color-tint: rgba(255,60,0,.35)"),
                        filter_card("gradient-sunset", "fx-tint-blue", "Blue Tint",  "color-tint: rgba(0,120,255,.35)"),
                        filter_card("gradient-ocean",  "fx-noise",     "Noise/Grain","noise: 0.35"),
                        filter_card("gradient-sunset", "fx-vignette",  "Vignette",   "vignette: 0.7"),
                    ],
                ]
            }),

            // ─── Outline / Focus Ring ──────────────────────────────────────────
            section_card(mgui! {
                Column::new().gap(16.0) => [
                    section_title("Outline (SDF обводка)"),
                    label("outline-width + outline-color + outline-offset — SDF кольцо с поддержкой скруглённых углов"),
                    Row::new().gap(24.0) => [
                        shadow_card("fx-outline-default", "Default",  "outline-width: 2px;\noutline-color: #6366f1"),
                        shadow_card("fx-outline-wide",    "Wide",     "outline-width: 4px;\noutline-color: #22c55e"),
                        shadow_card("fx-outline-offset",  "Offset",   "outline-width: 2px;\noutline-offset: 4px"),
                        shadow_card("fx-outline-rounded", "Rounded",  "border-radius: 16px;\noutline-width: 2px"),
                    ],
                ]
            }),

            // ─── Glassmorphism ─────────────────────────────────────────────────
            section_card(mgui! {
                Column::new().gap(16.0) => [
                    section_title("Glassmorphism (Backdrop Blur)"),
                    label("backdrop-filter: blur() — размытие содержимого ПОД прозрачным элементом"),
                    DecoratedBox::new().class("fx-glass-scene") => [
                        Row::new().gap(16.0) => [
                            DecoratedBox::new().class("fx-glass-card") => [
                                Column::new().gap(8.0) => [
                                    Text::new("Glass Card").bold().class("fx-demo-text"),
                                    Text::new("backdrop-filter: blur(12px)").class("fx-glass-subtitle").style("font-size", 11.0_f32),
                                ]
                            ],
                            DecoratedBox::new().class("fx-glass-card-dark") => [
                                Column::new().gap(8.0) => [
                                    Text::new("Dark Glass").bold().class("fx-demo-text"),
                                    Text::new("backdrop-filter: blur(8px)").class("fx-glass-subtitle").style("font-size", 11.0_f32),
                                ]
                            ],
                        ],
                    ],
                ]
            }),

            // ─── Цепочки фильтров ─────────────────────────────────────────────
            section_card(mgui! {
                Column::new().gap(16.0) => [
                    section_title("Цепочки эффектов (filter chain)"),
                    label("Несколько фильтров через пробел: filter: blur(2px) grayscale(50%)"),
                    Row::new().gap(16.0) => [
                        filter_card("gradient-sunset", "fx-chain-1", "Blur + Grayscale",   "filter: blur(2px) grayscale(70%)"),
                        filter_card("gradient-ocean",  "fx-chain-2", "Sepia + Vignette",   "filter: sepia(60%) vignette(0.5)"),
                        filter_card("gradient-sunset", "fx-chain-3", "Brightness + Noise", "filter: brightness(1.2) noise(0.2)"),
                        filter_card("gradient-ocean",  "fx-chain-4", "Invert + Chroma",    "filter: invert(100%) chromatic-aberration(2px)"),
                    ],
                ]
            }),

            // ─── Filter Transitions ───────────────────────────────────────────
            section_card(mgui! {
                Column::new().gap(16.0) => [
                    section_title("Filter Transitions"),
                    label("transition: filter 400ms ease — плавная интерполяция фильтров при hover"),
                    Row::new().gap(16.0) => [
                        filter_card("gradient-sunset", "fx-trans-blur",      "Hover → Blur",     "hover: filter: blur(6px)"),
                        filter_card("gradient-ocean",  "fx-trans-grayscale", "Hover → Grayscale", "hover: filter: grayscale(100%)"),
                        filter_card("gradient-sunset", "fx-trans-sepia",     "Hover → Sepia",     "hover: filter: sepia(80%)"),
                        filter_card("gradient-ocean",  "fx-trans-bright",    "Hover → Bright",    "hover: filter: brightness(1.5)"),
                    ],
                ]
            }),

            // ─── Keyframe Animations ──────────────────────────────────────────
            section_card(mgui! {
                Column::new().gap(16.0) => [
                    section_title("@keyframes анимации"),
                    label("animation-name + animation-duration + animation-iteration-count: infinite"),
                    Row::new().gap(16.0) => [
                        filter_card("gradient-sunset", "fx-anim-pulse",    "Pulse",         "@keyframes pulse { opacity }"),
                        filter_card("gradient-ocean",  "fx-anim-breathe",  "Breathe",       "@keyframes breathe { blur }"),
                        filter_card("gradient-sunset", "fx-anim-hue",      "Hue Rotate",    "@keyframes hue { hue-shift }"),
                    ],
                ]
            }),
        ]
    }
}
