#[derive(Clone, Copy, Debug, PartialEq)]
pub enum Easing {
    Linear,

    EaseInSine,
    EaseOutSine,
    EaseInOutSine,

    EaseInQuad,
    EaseOutQuad,
    EaseInOutQuad,

    EaseInCubic,
    EaseOutCubic,
    EaseInOutCubic,

    EaseInQuart,
    EaseOutQuart,
    EaseInOutQuart,

    EaseInQuint,
    EaseOutQuint,
    EaseInOutQuint,

    EaseInExpo,
    EaseOutExpo,
    EaseInOutExpo,

    EaseInCirc,
    EaseOutCirc,
    EaseInOutCirc,

    EaseInBack,
    EaseOutBack,
    EaseInOutBack,

    EaseInElastic,
    EaseOutElastic,
    EaseInOutElastic,

    EaseInBounce,
    EaseOutBounce,
    EaseInOutBounce,

    CubicBezier(f32, f32, f32, f32),

    Steps(u32),

    /// Затухающая пружина: жёсткость и демпфирование как у
    /// [`Spring`](crate::animation::Spring). Кривая нормирована так, что к
    /// `t = 1` пружина успокаивается; при малом демпфировании она
    /// перелетает цель и возвращается (в MSS — `spring(k, c)`).
    Spring { stiffness: f32, damping: f32 },
}

impl Easing {
    pub const CSS_EASE: Self = Self::CubicBezier(0.25, 0.1, 0.25, 1.0);
    pub const CSS_EASE_IN: Self = Self::CubicBezier(0.42, 0.0, 1.0, 1.0);
    pub const CSS_EASE_OUT: Self = Self::CubicBezier(0.0, 0.0, 0.58, 1.0);
    pub const CSS_EASE_IN_OUT: Self = Self::CubicBezier(0.42, 0.0, 0.58, 1.0);

    /// Material 3 «emphasized»: медленный разгон, резкое торможение —
    /// для движений, на которые смотрят (открытие панелей, смена вкладок).
    pub const EMPHASIZED: Self = Self::CubicBezier(0.2, 0.0, 0.0, 1.0);
    /// Material 3 «emphasized decelerate»: появление на экране.
    pub const EMPHASIZED_DECELERATE: Self = Self::CubicBezier(0.05, 0.7, 0.1, 1.0);
    /// Material 3 «emphasized accelerate»: уход с экрана.
    pub const EMPHASIZED_ACCELERATE: Self = Self::CubicBezier(0.3, 0.0, 0.8, 0.15);
    /// Material 3 «standard»: мелкие утилитарные переходы.
    pub const STANDARD: Self = Self::CubicBezier(0.2, 0.0, 0.0, 1.0);
    pub const STANDARD_DECELERATE: Self = Self::CubicBezier(0.0, 0.0, 0.0, 1.0);
    pub const STANDARD_ACCELERATE: Self = Self::CubicBezier(0.3, 0.0, 1.0, 1.0);
    /// Мягкая пружина без перелёта — «перетекание» размера и положения.
    pub const SPRING_SMOOTH: Self = Self::Spring { stiffness: 380.0, damping: 38.0 };
    /// Пружина с лёгким перелётом — появление карточек, значков.
    pub const SPRING_BOUNCY: Self = Self::Spring { stiffness: 320.0, damping: 22.0 };

    /// Кривая по имени из MSS: `ease-out-cubic`, `emphasized`,
    /// `cubic-bezier(x1,y1,x2,y2)`, `steps(n)`, `spring(k, c)`.
    /// `None` — имя неизвестно.
    pub fn parse(s: &str) -> Option<Self> {
        let s = s.trim();
        if let Some(inner) = s.strip_prefix("cubic-bezier(").and_then(|r| r.strip_suffix(')')) {
            let v: Vec<f32> = inner.split(',').filter_map(|x| x.trim().parse().ok()).collect();
            return (v.len() == 4).then(|| Easing::CubicBezier(v[0], v[1], v[2], v[3]));
        }
        if let Some(inner) = s.strip_prefix("steps(").and_then(|r| r.strip_suffix(')')) {
            return inner.split(',').next()?.trim().parse().ok().map(Easing::Steps);
        }
        if let Some(inner) = s.strip_prefix("spring(").and_then(|r| r.strip_suffix(')')) {
            let v: Vec<f32> = inner.split(',').filter_map(|x| x.trim().parse().ok()).collect();
            return match v.as_slice() {
                [] => Some(Easing::SPRING_SMOOTH),
                [k] => Some(Easing::Spring { stiffness: *k, damping: 2.0 * k.sqrt() }),
                [k, c, ..] => Some(Easing::Spring { stiffness: *k, damping: *c }),
            };
        }
        Some(match s {
            "linear" => Easing::Linear,
            "ease" => Easing::CSS_EASE,
            "ease-in" => Easing::CSS_EASE_IN,
            "ease-out" => Easing::CSS_EASE_OUT,
            "ease-in-out" => Easing::CSS_EASE_IN_OUT,
            "ease-in-sine" => Easing::EaseInSine,
            "ease-out-sine" => Easing::EaseOutSine,
            "ease-in-out-sine" => Easing::EaseInOutSine,
            "ease-in-quad" => Easing::EaseInQuad,
            "ease-out-quad" => Easing::EaseOutQuad,
            "ease-in-out-quad" => Easing::EaseInOutQuad,
            "ease-in-cubic" => Easing::EaseInCubic,
            "ease-out-cubic" => Easing::EaseOutCubic,
            "ease-in-out-cubic" => Easing::EaseInOutCubic,
            "ease-in-quart" => Easing::EaseInQuart,
            "ease-out-quart" => Easing::EaseOutQuart,
            "ease-in-out-quart" => Easing::EaseInOutQuart,
            "ease-in-quint" => Easing::EaseInQuint,
            "ease-out-quint" => Easing::EaseOutQuint,
            "ease-in-out-quint" => Easing::EaseInOutQuint,
            "ease-in-expo" => Easing::EaseInExpo,
            "ease-out-expo" => Easing::EaseOutExpo,
            "ease-in-out-expo" => Easing::EaseInOutExpo,
            "ease-in-circ" => Easing::EaseInCirc,
            "ease-out-circ" => Easing::EaseOutCirc,
            "ease-in-out-circ" => Easing::EaseInOutCirc,
            "ease-in-back" => Easing::EaseInBack,
            "ease-out-back" => Easing::EaseOutBack,
            "ease-in-out-back" => Easing::EaseInOutBack,
            "ease-in-elastic" => Easing::EaseInElastic,
            "ease-out-elastic" => Easing::EaseOutElastic,
            "ease-in-out-elastic" => Easing::EaseInOutElastic,
            "ease-in-bounce" => Easing::EaseInBounce,
            "ease-out-bounce" => Easing::EaseOutBounce,
            "ease-in-out-bounce" => Easing::EaseInOutBounce,
            "emphasized" => Easing::EMPHASIZED,
            "emphasized-decelerate" => Easing::EMPHASIZED_DECELERATE,
            "emphasized-accelerate" => Easing::EMPHASIZED_ACCELERATE,
            "standard" => Easing::STANDARD,
            "standard-decelerate" => Easing::STANDARD_DECELERATE,
            "standard-accelerate" => Easing::STANDARD_ACCELERATE,
            "spring" => Easing::SPRING_SMOOTH,
            "spring-bouncy" => Easing::SPRING_BOUNCY,
            _ => return None,
        })
    }

    /// Перелетает ли кривая за цель (значения вне 0..1): такому переходу
    /// нельзя обрезать содержимое по промежуточному размеру «в ноль».
    pub fn overshoots(&self) -> bool {
        match self {
            Easing::EaseInBack | Easing::EaseOutBack | Easing::EaseInOutBack => true,
            Easing::EaseInElastic | Easing::EaseOutElastic | Easing::EaseInOutElastic => true,
            Easing::CubicBezier(_, y1, _, y2) => *y1 < 0.0 || *y1 > 1.0 || *y2 < 0.0 || *y2 > 1.0,
            Easing::Spring { stiffness, damping } => *damping < 2.0 * stiffness.max(0.0).sqrt(),
            _ => false,
        }
    }

    pub fn apply(&self, t: f32) -> f32 {
        let t = t.clamp(0.0, 1.0);
        match self {
            Easing::Linear => t,
            Easing::Spring { stiffness, damping } => spring_response(t, *stiffness, *damping),

            Easing::EaseInSine => 1.0 - ((t * std::f32::consts::FRAC_PI_2).cos()),
            Easing::EaseOutSine => (t * std::f32::consts::FRAC_PI_2).sin(),
            Easing::EaseInOutSine => -(((std::f32::consts::PI * t).cos()) - 1.0) / 2.0,

            Easing::EaseInQuad => t * t,
            Easing::EaseOutQuad => 1.0 - (1.0 - t) * (1.0 - t),
            Easing::EaseInOutQuad => {
                if t < 0.5 {
                    2.0 * t * t
                } else {
                    1.0 - (-2.0 * t + 2.0).powi(2) / 2.0
                }
            }

            Easing::EaseInCubic => t * t * t,
            Easing::EaseOutCubic => 1.0 - (1.0 - t).powi(3),
            Easing::EaseInOutCubic => {
                if t < 0.5 {
                    4.0 * t * t * t
                } else {
                    1.0 - (-2.0 * t + 2.0).powi(3) / 2.0
                }
            }

            Easing::EaseInQuart => t * t * t * t,
            Easing::EaseOutQuart => 1.0 - (1.0 - t).powi(4),
            Easing::EaseInOutQuart => {
                if t < 0.5 {
                    8.0 * t * t * t * t
                } else {
                    1.0 - (-2.0 * t + 2.0).powi(4) / 2.0
                }
            }

            Easing::EaseInQuint => t * t * t * t * t,
            Easing::EaseOutQuint => 1.0 - (1.0 - t).powi(5),
            Easing::EaseInOutQuint => {
                if t < 0.5 {
                    16.0 * t.powi(5)
                } else {
                    1.0 - (-2.0 * t + 2.0).powi(5) / 2.0
                }
            }

            Easing::EaseInExpo => {
                if t == 0.0 {
                    0.0
                } else {
                    2.0_f32.powf(10.0 * t - 10.0)
                }
            }
            Easing::EaseOutExpo => {
                if t == 1.0 {
                    1.0
                } else {
                    1.0 - 2.0_f32.powf(-10.0 * t)
                }
            }
            Easing::EaseInOutExpo => {
                if t == 0.0 {
                    0.0
                } else if t == 1.0 {
                    1.0
                } else if t < 0.5 {
                    2.0_f32.powf(20.0 * t - 10.0) / 2.0
                } else {
                    (2.0 - 2.0_f32.powf(-20.0 * t + 10.0)) / 2.0
                }
            }

            Easing::EaseInCirc => 1.0 - (1.0 - t * t).sqrt(),
            Easing::EaseOutCirc => (1.0 - (t - 1.0).powi(2)).sqrt(),
            Easing::EaseInOutCirc => {
                if t < 0.5 {
                    (1.0 - (1.0 - (2.0 * t).powi(2)).sqrt()) / 2.0
                } else {
                    ((1.0 - (-2.0 * t + 2.0).powi(2)).sqrt() + 1.0) / 2.0
                }
            }

            Easing::EaseInBack => {
                const C1: f32 = 1.70158;
                const C3: f32 = C1 + 1.0;
                C3 * t * t * t - C1 * t * t
            }
            Easing::EaseOutBack => {
                const C1: f32 = 1.70158;
                const C3: f32 = C1 + 1.0;
                1.0 + C3 * (t - 1.0).powi(3) + C1 * (t - 1.0).powi(2)
            }
            Easing::EaseInOutBack => {
                const C1: f32 = 1.70158;
                const C2: f32 = C1 * 1.525;
                if t < 0.5 {
                    ((2.0 * t).powi(2) * ((C2 + 1.0) * 2.0 * t - C2)) / 2.0
                } else {
                    ((2.0 * t - 2.0).powi(2) * ((C2 + 1.0) * (t * 2.0 - 2.0) + C2) + 2.0) / 2.0
                }
            }

            Easing::EaseInElastic => {
                const C4: f32 = (2.0 * std::f32::consts::PI) / 3.0;
                if t == 0.0 {
                    0.0
                } else if t == 1.0 {
                    1.0
                } else {
                    -(2.0_f32.powf(10.0 * t - 10.0)) * ((t * 10.0 - 10.75) * C4).sin()
                }
            }
            Easing::EaseOutElastic => {
                const C4: f32 = (2.0 * std::f32::consts::PI) / 3.0;
                if t == 0.0 {
                    0.0
                } else if t == 1.0 {
                    1.0
                } else {
                    2.0_f32.powf(-10.0 * t) * ((t * 10.0 - 0.75) * C4).sin() + 1.0
                }
            }
            Easing::EaseInOutElastic => {
                const C5: f32 = (2.0 * std::f32::consts::PI) / 4.5;
                if t == 0.0 {
                    0.0
                } else if t == 1.0 {
                    1.0
                } else if t < 0.5 {
                    -(2.0_f32.powf(20.0 * t - 10.0) * ((20.0 * t - 11.125) * C5).sin()) / 2.0
                } else {
                    (2.0_f32.powf(-20.0 * t + 10.0) * ((20.0 * t - 11.125) * C5).sin()) / 2.0 + 1.0
                }
            }

            Easing::EaseOutBounce => ease_out_bounce(t),
            Easing::EaseInBounce => 1.0 - ease_out_bounce(1.0 - t),
            Easing::EaseInOutBounce => {
                if t < 0.5 {
                    (1.0 - ease_out_bounce(1.0 - 2.0 * t)) / 2.0
                } else {
                    (1.0 + ease_out_bounce(2.0 * t - 1.0)) / 2.0
                }
            }

            Easing::CubicBezier(x1, y1, x2, y2) => cubic_bezier_sample(t, *x1, *y1, *x2, *y2),
            Easing::Steps(n) => {
                if *n == 0 {
                    t
                } else {
                    (t * *n as f32).floor() / (*n as f32 - 1.0).max(1.0)
                }
            }
        }
    }
}

fn ease_out_bounce(t: f32) -> f32 {
    if t < 1.0 / 2.75 {
        7.5625 * t * t
    } else if t < 2.0 / 2.75 {
        let t = t - 1.5 / 2.75;
        7.5625 * t * t + 0.75
    } else if t < 2.5 / 2.75 {
        let t = t - 2.25 / 2.75;
        7.5625 * t * t + 0.9375
    } else {
        let t = t - 2.625 / 2.75;
        7.5625 * t * t + 0.984375
    }
}

/// Отклик пружины `x'' = -k·(x-1) - c·x'` из покоя в нуле, время
/// растянуто так, чтобы к `t = 1` огибающая колебаний затухла до 0,1 %.
fn spring_response(t: f32, stiffness: f32, damping: f32) -> f32 {
    if t >= 1.0 {
        return 1.0;
    }
    let k = stiffness.max(1e-3);
    let c = damping.max(0.0);
    let w0 = k.sqrt();
    let zeta = c / (2.0 * w0);
    // ln(1000) ≈ 6.9: за это время огибающая e^(-ζω₀τ) падает в 1000 раз.
    let settle = 6.9 / (zeta.max(0.05) * w0);
    let tau = t * settle;
    if zeta < 1.0 {
        let wd = w0 * (1.0 - zeta * zeta).sqrt();
        let e = (-zeta * w0 * tau).exp();
        1.0 - e * ((wd * tau).cos() + zeta * w0 / wd * (wd * tau).sin())
    } else if (zeta - 1.0).abs() < 1e-3 {
        let e = (-w0 * tau).exp();
        1.0 - e * (1.0 + w0 * tau)
    } else {
        let s = (zeta * zeta - 1.0).sqrt();
        let r1 = -w0 * (zeta - s);
        let r2 = -w0 * (zeta + s);
        1.0 - (r2 * (r1 * tau).exp() - r1 * (r2 * tau).exp()) / (r2 - r1)
    }
}

fn cubic_bezier_sample(t: f32, x1: f32, y1: f32, x2: f32, y2: f32) -> f32 {
    let s = find_t_for_x(t, x1, x2);
    bezier_component(s, y1, y2)
}

fn bezier_component(s: f32, c1: f32, c2: f32) -> f32 {
    let s2 = s * s;
    let s3 = s2 * s;
    let inv = 1.0 - s;
    let inv2 = inv * inv;
    3.0 * inv2 * s * c1 + 3.0 * inv * s2 * c2 + s3
}

fn bezier_component_deriv(s: f32, c1: f32, c2: f32) -> f32 {
    let s2 = s * s;
    let inv = 1.0 - s;
    3.0 * inv * inv * c1 + 6.0 * inv * s * (c2 - c1) + 3.0 * s2 * (1.0 - c2)
}

fn find_t_for_x(t: f32, x1: f32, x2: f32) -> f32 {
    let mut s = t;
    for _ in 0..8 {
        let x = bezier_component(s, x1, x2) - t;
        let dx = bezier_component_deriv(s, x1, x2);
        if dx.abs() < 1e-7 {
            break;
        }
        s -= x / dx;
        s = s.clamp(0.0, 1.0);
    }
    s
}

impl Default for Easing {
    fn default() -> Self {
        Easing::EaseOutQuad
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    const EPS: f32 = 1e-4;

    fn assert_near(a: f32, b: f32, msg: &str) {
        assert!((a - b).abs() < EPS, "{msg}: expected {b}, got {a}");
    }

    #[test]
    fn all_easings_zero_and_one() {
        let easings = [
            Easing::Linear,
            Easing::EaseInSine,
            Easing::EaseOutSine,
            Easing::EaseInOutSine,
            Easing::EaseInQuad,
            Easing::EaseOutQuad,
            Easing::EaseInOutQuad,
            Easing::EaseInCubic,
            Easing::EaseOutCubic,
            Easing::EaseInOutCubic,
            Easing::EaseInQuart,
            Easing::EaseOutQuart,
            Easing::EaseInOutQuart,
            Easing::EaseInQuint,
            Easing::EaseOutQuint,
            Easing::EaseInOutQuint,
            Easing::EaseInExpo,
            Easing::EaseOutExpo,
            Easing::EaseInOutExpo,
            Easing::EaseInCirc,
            Easing::EaseOutCirc,
            Easing::EaseInOutCirc,
            Easing::EaseInBounce,
            Easing::EaseOutBounce,
            Easing::EaseInOutBounce,
            Easing::EaseInElastic,
            Easing::EaseOutElastic,
            Easing::EaseInOutElastic,
            Easing::EaseInBack,
            Easing::EaseOutBack,
            Easing::EaseInOutBack,
        ];
        for e in &easings {
            assert_near(e.apply(0.0), 0.0, &format!("{:?} at 0", e));
            assert_near(e.apply(1.0), 1.0, &format!("{:?} at 1", e));
        }
    }

    #[test]
    fn all_easings_clamp_negative() {
        let e = Easing::Linear;
        assert_eq!(e.apply(-1.0), 0.0);
    }

    #[test]
    fn all_easings_clamp_above_one() {
        let e = Easing::Linear;
        assert_eq!(e.apply(2.0), 1.0);
    }

    #[test]
    fn linear_midpoint() {
        assert_eq!(Easing::Linear.apply(0.5), 0.5);
        assert_eq!(Easing::Linear.apply(0.25), 0.25);
    }

    #[test]
    fn ease_in_quad_midpoint() {
        assert_near(Easing::EaseInQuad.apply(0.5), 0.25, "EaseInQuad(0.5)");
    }

    #[test]
    fn ease_out_quad_midpoint() {
        assert_near(Easing::EaseOutQuad.apply(0.5), 0.75, "EaseOutQuad(0.5)");
    }

    #[test]
    fn ease_in_out_quad_midpoint() {
        assert_near(Easing::EaseInOutQuad.apply(0.5), 0.5, "EaseInOutQuad(0.5)");
    }

    #[test]
    fn ease_in_cubic() {
        assert_near(Easing::EaseInCubic.apply(0.5), 0.125, "EaseInCubic(0.5)");
    }

    #[test]
    fn ease_out_cubic() {
        assert_near(Easing::EaseOutCubic.apply(0.5), 0.875, "EaseOutCubic(0.5)");
    }

    #[test]
    fn ease_in_expo_boundaries() {
        assert_eq!(Easing::EaseInExpo.apply(0.0), 0.0);
        assert_near(Easing::EaseInExpo.apply(1.0), 1.0, "EaseInExpo(1)");
    }

    #[test]
    fn ease_out_expo_boundaries() {
        assert_near(Easing::EaseOutExpo.apply(0.0), 0.0, "EaseOutExpo(0)");
        assert_eq!(Easing::EaseOutExpo.apply(1.0), 1.0);
    }

    #[test]
    fn ease_out_bounce_at_1() {
        assert_near(ease_out_bounce(1.0), 1.0, "bounce(1.0)");
    }

    #[test]
    fn ease_out_bounce_at_0() {
        assert_near(ease_out_bounce(0.0), 0.0, "bounce(0.0)");
    }

    #[test]
    fn ease_out_bounce_first_region() {
        let t = 0.3;
        let result = ease_out_bounce(t);
        assert!(result > 0.0 && result < 1.0);
    }

    #[test]
    fn cubic_bezier_linear() {
        let e = Easing::CubicBezier(0.0, 0.0, 1.0, 1.0);
        assert_near(e.apply(0.0), 0.0, "bezier linear 0");
        assert_near(e.apply(0.5), 0.5, "bezier linear 0.5");
        assert_near(e.apply(1.0), 1.0, "bezier linear 1");
    }

    #[test]
    fn css_ease_boundaries() {
        let e = Easing::CSS_EASE;
        assert_near(e.apply(0.0), 0.0, "CSS_EASE(0)");
        assert_near(e.apply(1.0), 1.0, "CSS_EASE(1)");
    }

    #[test]
    fn css_ease_in_slower_start() {
        let e = Easing::CSS_EASE_IN;
        assert!(e.apply(0.25) < 0.25);
    }

    #[test]
    fn css_ease_out_faster_start() {
        let e = Easing::CSS_EASE_OUT;
        assert!(e.apply(0.25) > 0.25);
    }

    #[test]
    fn steps_discrete() {
        let e = Easing::Steps(4);
        assert_near(e.apply(0.0), 0.0, "Steps(4) at 0");
        assert_near(e.apply(0.25), 1.0 / 3.0, "Steps(4) at 0.25");
        assert_near(e.apply(0.5), 2.0 / 3.0, "Steps(4) at 0.5");
        assert_near(e.apply(1.0), 4.0 / 3.0, "Steps(4) at 1");
    }

    #[test]
    fn steps_zero_is_linear() {
        let e = Easing::Steps(0);
        assert_eq!(e.apply(0.5), 0.5);
    }

    #[test]
    fn default_is_ease_out_quad() {
        assert_eq!(Easing::default(), Easing::EaseOutQuad);
    }

    #[test]
    fn standard_easings_monotonic() {
        let monotonic = [
            Easing::Linear,
            Easing::EaseInSine,
            Easing::EaseOutSine,
            Easing::EaseInOutSine,
            Easing::EaseInQuad,
            Easing::EaseOutQuad,
            Easing::EaseInOutQuad,
            Easing::EaseInCubic,
            Easing::EaseOutCubic,
            Easing::EaseInOutCubic,
            Easing::EaseInExpo,
            Easing::EaseOutExpo,
            Easing::EaseInOutExpo,
            Easing::EaseInCirc,
            Easing::EaseOutCirc,
            Easing::EaseInOutCirc,
        ];
        for e in &monotonic {
            let mut prev = e.apply(0.0);
            for i in 1..=20 {
                let t = i as f32 / 20.0;
                let val = e.apply(t);
                assert!(
                    val >= prev - EPS,
                    "{:?}: not monotonic at t={} (prev={}, cur={})",
                    e,
                    t,
                    prev,
                    val
                );
                prev = val;
            }
        }
    }

    #[test]
    fn ease_in_back_undershoots() {
        let val = Easing::EaseInBack.apply(0.3);
        assert!(val < 0.0, "EaseInBack should undershoot: got {}", val);
    }

    #[test]
    fn ease_out_back_overshoots() {
        let val = Easing::EaseOutBack.apply(0.5);
        assert!(val > 0.5, "EaseOutBack should be ahead at 0.5");
    }
}
