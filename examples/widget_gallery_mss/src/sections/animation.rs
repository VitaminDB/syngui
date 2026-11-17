use syngui::animation::{Animation, Easing};
use syngui::mss::{parse_stylesheet_str, KeyframesDefinition};
use syngui::prelude::*;
use syngui::widgets::*;

use super::{label, section_card, section_title};

fn gallery_mss() -> String {
    let theme = crate::theme_data::default_light();
    format!("{}\n{}", theme.to_mss(), crate::styles::component_styles())
}

/// Extract f32 value from a StyleValue (Number or Length in px)
fn sv_f32(v: &syngui::mss::StyleValue) -> f32 {
    match v {
        syngui::mss::StyleValue::Number(n) => *n,
        syngui::mss::StyleValue::Length(n, _) => *n,
        _ => 0.0,
    }
}

/// Build a tween Animation from a @keyframes definition (from->to only)
fn anim_from_kf(
    kf: &KeyframesDefinition,
    prop: &str,
    easing: Easing,
    duration_ms: u32,
) -> Animation {
    let from = kf
        .steps
        .first()
        .and_then(|s| s.declarations.get(prop))
        .map(sv_f32)
        .unwrap_or(0.0);
    let to = kf
        .steps
        .last()
        .and_then(|s| s.declarations.get(prop))
        .map(sv_f32)
        .unwrap_or(0.0);
    Animation::tween(easing)
        .from(from)
        .to(to)
        .duration_ms(duration_ms)
        .build()
}

fn anim_from_kf_delay(
    kf: &KeyframesDefinition,
    prop: &str,
    easing: Easing,
    duration_ms: u32,
    delay: u32,
) -> Animation {
    let from = kf
        .steps
        .first()
        .and_then(|s| s.declarations.get(prop))
        .map(sv_f32)
        .unwrap_or(0.0);
    let to = kf
        .steps
        .last()
        .and_then(|s| s.declarations.get(prop))
        .map(sv_f32)
        .unwrap_or(0.0);
    Animation::tween(easing)
        .from(from)
        .to(to)
        .duration_ms(duration_ms)
        .delay_ms(delay)
        .build()
}

pub fn build_animation_section() -> impl Widget {
    // Parse keyframes from embedded MSS
    let mss = gallery_mss();
    let ss = parse_stylesheet_str(&mss).unwrap();
    let kf_slide = ss.get_keyframes("slide-right");
    let kf_scale = ss.get_keyframes("scale-pulse");
    let kf_rotate = ss.get_keyframes("rotate-full");
    let kf_fade = ss.get_keyframes("fade-in-out");
    let kf_breathing = ss.get_keyframes("breathing");
    let kf_slide_up = ss.get_keyframes("slide-up-fade");
    let kf_combined = ss.get_keyframes("combined");
    let kf_scale_x = ss.get_keyframes("scale-x-stretch");

    section_card(
        Column::new()
            .gap(16.0)
            .child(section_title("Animation"))
            // --- Basic Easing ---
            .child(label("Basic Easing"))
            .child(
                Column::new()
                    .gap(4.0)
                    .child(label("Translate X — EaseOutQuad"))
                    .child(
                        Animated::new(
                            DecoratedBox::new()
                                .class("anim-box-blue")
                                .class("anim-box-size-sm"),
                        )
                        .translate_x(anim_from_kf(
                            kf_slide.unwrap(),
                            "translate-x",
                            Easing::EaseOutQuad,
                            2000,
                        ))
                        .repeat(true),
                    ),
            )
            .child(
                Column::new()
                    .gap(4.0)
                    .child(label("Translate X — EaseOutBounce"))
                    .child(
                        Animated::new(
                            DecoratedBox::new()
                                .class("anim-box-red")
                                .class("anim-box-size-sm"),
                        )
                        .translate_x(anim_from_kf(
                            kf_slide.unwrap(),
                            "translate-x",
                            Easing::EaseOutBounce,
                            2500,
                        ))
                        .repeat(true),
                    ),
            )
            .child(
                Column::new()
                    .gap(4.0)
                    .child(label("Translate X — EaseOutElastic"))
                    .child(
                        Animated::new(
                            DecoratedBox::new()
                                .class("anim-box-purple")
                                .class("anim-box-size-sm"),
                        )
                        .translate_x(anim_from_kf(
                            kf_slide.unwrap(),
                            "translate-x",
                            Easing::EaseOutElastic,
                            2000,
                        ))
                        .repeat(true),
                    ),
            )
            .child(
                Column::new()
                    .gap(4.0)
                    .child(label("Translate X — EaseInOutBack"))
                    .child(
                        Animated::new(
                            DecoratedBox::new()
                                .class("anim-box-amber")
                                .class("anim-box-size-sm"),
                        )
                        .translate_x(anim_from_kf(
                            kf_slide.unwrap(),
                            "translate-x",
                            Easing::EaseInOutBack,
                            2000,
                        ))
                        .repeat(true),
                    ),
            )
            .child(
                Column::new()
                    .gap(4.0)
                    .child(label("Translate X — CubicBezier(0.68, -0.55, 0.27, 1.55)"))
                    .child(
                        Animated::new(
                            DecoratedBox::new()
                                .class("anim-box-pink")
                                .class("anim-box-size-sm"),
                        )
                        .translate_x(anim_from_kf(
                            kf_slide.unwrap(),
                            "translate-x",
                            Easing::CubicBezier(0.68, -0.55, 0.27, 1.55),
                            2000,
                        ))
                        .repeat(true),
                    ),
            )
            // --- Transform ---
            .child(label("Transform"))
            .child(
                Column::new()
                    .gap(4.0)
                    .child(label("Scale — EaseInOutQuad"))
                    .child(
                        Animated::new(
                            DecoratedBox::new()
                                .class("anim-box-green")
                                .class("anim-box-size"),
                        )
                        .scale(anim_from_kf(
                            kf_scale.unwrap(),
                            "scale",
                            Easing::EaseInOutQuad,
                            2000,
                        ))
                        .repeat(true),
                    ),
            )
            .child(
                Column::new()
                    .gap(4.0)
                    .child(label("Scale X only (horizontal stretch)"))
                    .child(
                        Animated::new(
                            DecoratedBox::new()
                                .class("anim-box-cyan")
                                .class("anim-box-size"),
                        )
                        .scale_x(anim_from_kf(
                            kf_scale_x.unwrap(),
                            "scale-x",
                            Easing::EaseInOutCubic,
                            1500,
                        ))
                        .repeat(true),
                    ),
            )
            .child(
                Column::new()
                    .gap(4.0)
                    .child(label("Rotate — 360° EaseInOutExpo"))
                    .child(
                        Animated::new(
                            DecoratedBox::new()
                                .class("anim-box-orange")
                                .class("anim-box-size"),
                        )
                        .rotate(anim_from_kf(
                            kf_rotate.unwrap(),
                            "rotate",
                            Easing::EaseInOutExpo,
                            3000,
                        ))
                        .repeat(true),
                    ),
            )
            // --- Opacity & Crossfade ---
            .child(label("Opacity"))
            .child(
                Column::new()
                    .gap(4.0)
                    .child(label("Opacity Fade — EaseInOutSine"))
                    .child(
                        Animated::new(
                            DecoratedBox::new()
                                .class("anim-box-purple")
                                .class("anim-box-size"),
                        )
                        .opacity(anim_from_kf(
                            kf_fade.unwrap(),
                            "opacity",
                            Easing::EaseInOutSine,
                            1500,
                        ))
                        .repeat(true),
                    ),
            )
            .child(
                Column::new()
                    .gap(4.0)
                    .child(label("Color Crossfade (two overlaid boxes)"))
                    .child(
                        Stack::new()
                            .child(
                                Animated::new(
                                    DecoratedBox::new()
                                        .class("anim-box-blue")
                                        .class("anim-box-size"),
                                )
                                .opacity(
                                    Animation::tween(Easing::Linear)
                                        .from(1.0)
                                        .to(0.0)
                                        .duration_ms(3000)
                                        .build(),
                                )
                                .repeat(true),
                            )
                            .child(
                                Animated::new(
                                    DecoratedBox::new()
                                        .class("anim-box-red")
                                        .class("anim-box-size"),
                                )
                                .opacity(
                                    Animation::tween(Easing::Linear)
                                        .from(0.0)
                                        .to(1.0)
                                        .duration_ms(3000)
                                        .build(),
                                )
                                .repeat(true),
                            ),
                    ),
            )
            // --- Spring Physics ---
            .child(label("Spring Physics"))
            .child(
                Column::new()
                    .gap(4.0)
                    .child(label("Spring — stiffness 180, damping 12"))
                    .child(
                        Animated::new(
                            DecoratedBox::new()
                                .class("anim-box-amber")
                                .class("anim-box-size-sm"),
                        )
                        .translate_x(
                            Animation::spring()
                                .from(0.0)
                                .to(250.0)
                                .stiffness(180.0)
                                .damping(12.0)
                                .build(),
                        )
                        .repeat(true),
                    ),
            )
            .child(
                Column::new()
                    .gap(4.0)
                    .child(label("Spring — stiffness 400, damping 8 (bouncy)"))
                    .child(
                        Animated::new(
                            DecoratedBox::new()
                                .class("anim-box-teal")
                                .class("anim-box-size-sm"),
                        )
                        .translate_x(
                            Animation::spring()
                                .from(0.0)
                                .to(250.0)
                                .stiffness(400.0)
                                .damping(8.0)
                                .build(),
                        )
                        .repeat(true),
                    ),
            )
            // --- Repeat Modes ---
            .child(label("Repeat Modes"))
            .child(
                Column::new()
                    .gap(4.0)
                    .child(label("PingPong (auto-reverse, infinite)"))
                    .child(
                        Animated::new(
                            DecoratedBox::new()
                                .class("anim-box-purple")
                                .class("anim-box-size-sm"),
                        )
                        .translate_x(anim_from_kf(
                            kf_slide.unwrap(),
                            "translate-x",
                            Easing::EaseInOutQuad,
                            1500,
                        ))
                        .repeat_mode(RepeatMode::PingPong(0)),
                    ),
            )
            .child(
                Column::new()
                    .gap(4.0)
                    .child(label("PingPong Scale (breathing effect)"))
                    .child(
                        Animated::new(
                            DecoratedBox::new()
                                .class("anim-box-red")
                                .class("anim-box-size"),
                        )
                        .scale(anim_from_kf(
                            kf_breathing.unwrap(),
                            "scale",
                            Easing::EaseInOutSine,
                            1000,
                        ))
                        .repeat_mode(RepeatMode::PingPong(0)),
                    ),
            )
            // --- Delay ---
            .child(label("Delay"))
            .child(
                Column::new()
                    .gap(4.0)
                    .child(label("Staggered delay (0ms, 200ms, 400ms)"))
                    .child(
                        Row::new()
                            .gap(8.0)
                            .child(
                                Animated::new(
                                    DecoratedBox::new()
                                        .class("anim-box-blue")
                                        .class("anim-box-size-sm"),
                                )
                                .translate_y(anim_from_kf(
                                    kf_slide_up.unwrap(),
                                    "translate-y",
                                    Easing::EaseOutCubic,
                                    800,
                                ))
                                .opacity(anim_from_kf(
                                    kf_slide_up.unwrap(),
                                    "opacity",
                                    Easing::EaseOutCubic,
                                    800,
                                ))
                                .repeat(true),
                            )
                            .child(
                                Animated::new(
                                    DecoratedBox::new()
                                        .class("anim-box-purple")
                                        .class("anim-box-size-sm"),
                                )
                                .translate_y(anim_from_kf_delay(
                                    kf_slide_up.unwrap(),
                                    "translate-y",
                                    Easing::EaseOutCubic,
                                    800,
                                    200,
                                ))
                                .opacity(anim_from_kf_delay(
                                    kf_slide_up.unwrap(),
                                    "opacity",
                                    Easing::EaseOutCubic,
                                    800,
                                    200,
                                ))
                                .repeat(true),
                            )
                            .child(
                                Animated::new(
                                    DecoratedBox::new()
                                        .class("anim-box-pink")
                                        .class("anim-box-size-sm"),
                                )
                                .translate_y(anim_from_kf_delay(
                                    kf_slide_up.unwrap(),
                                    "translate-y",
                                    Easing::EaseOutCubic,
                                    800,
                                    400,
                                ))
                                .opacity(anim_from_kf_delay(
                                    kf_slide_up.unwrap(),
                                    "opacity",
                                    Easing::EaseOutCubic,
                                    800,
                                    400,
                                ))
                                .repeat(true),
                            ),
                    ),
            )
            // --- Combined ---
            .child(label("Combined Animations"))
            .child(
                Column::new()
                    .gap(4.0)
                    .child(label("Translate + Scale + Rotate + Opacity"))
                    .child(
                        Animated::new(
                            DecoratedBox::new()
                                .class("anim-box-indigo")
                                .class("anim-box-size"),
                        )
                        .translate_x(anim_from_kf(
                            kf_combined.unwrap(),
                            "translate-x",
                            Easing::EaseInOutQuad,
                            2500,
                        ))
                        .scale(anim_from_kf(
                            kf_combined.unwrap(),
                            "scale",
                            Easing::EaseInOutSine,
                            2500,
                        ))
                        .rotate(anim_from_kf(
                            kf_combined.unwrap(),
                            "rotate",
                            Easing::Linear,
                            2500,
                        ))
                        .opacity(anim_from_kf(
                            kf_combined.unwrap(),
                            "opacity",
                            Easing::EaseInOutQuad,
                            2500,
                        ))
                        .repeat_mode(RepeatMode::PingPong(0)),
                    ),
            ),
    )
}
