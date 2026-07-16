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
}

impl Easing {

    pub const CSS_EASE: Self = Self::CubicBezier(0.25, 0.1, 0.25, 1.0);
    pub const CSS_EASE_IN: Self = Self::CubicBezier(0.42, 0.0, 1.0, 1.0);
    pub const CSS_EASE_OUT: Self = Self::CubicBezier(0.0, 0.0, 0.58, 1.0);
    pub const CSS_EASE_IN_OUT: Self = Self::CubicBezier(0.42, 0.0, 0.58, 1.0);

    pub fn apply(&self, t: f32) -> f32 {
        let t = t.clamp(0.0, 1.0);
        match self {
            Easing::Linear => t,

            Easing::EaseInSine => {
                1.0 - ((t * std::f32::consts::FRAC_PI_2).cos())
            }
            Easing::EaseOutSine => {
                (t * std::f32::consts::FRAC_PI_2).sin()
            }
            Easing::EaseInOutSine => {
                -(((std::f32::consts::PI * t).cos()) - 1.0) / 2.0
            }

            Easing::EaseInQuad => t * t,
            Easing::EaseOutQuad => 1.0 - (1.0 - t) * (1.0 - t),
            Easing::EaseInOutQuad => {
                if t < 0.5 { 2.0 * t * t } else { 1.0 - (-2.0 * t + 2.0).powi(2) / 2.0 }
            }

            Easing::EaseInCubic => t * t * t,
            Easing::EaseOutCubic => 1.0 - (1.0 - t).powi(3),
            Easing::EaseInOutCubic => {
                if t < 0.5 { 4.0 * t * t * t } else { 1.0 - (-2.0 * t + 2.0).powi(3) / 2.0 }
            }

            Easing::EaseInQuart => t * t * t * t,
            Easing::EaseOutQuart => 1.0 - (1.0 - t).powi(4),
            Easing::EaseInOutQuart => {
                if t < 0.5 { 8.0 * t * t * t * t } else { 1.0 - (-2.0 * t + 2.0).powi(4) / 2.0 }
            }

            Easing::EaseInQuint => t * t * t * t * t,
            Easing::EaseOutQuint => 1.0 - (1.0 - t).powi(5),
            Easing::EaseInOutQuint => {
                if t < 0.5 { 16.0 * t.powi(5) } else { 1.0 - (-2.0 * t + 2.0).powi(5) / 2.0 }
            }

            Easing::EaseInExpo => {
                if t == 0.0 { 0.0 } else { 2.0_f32.powf(10.0 * t - 10.0) }
            }
            Easing::EaseOutExpo => {
                if t == 1.0 { 1.0 } else { 1.0 - 2.0_f32.powf(-10.0 * t) }
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

            Easing::EaseInCirc => {
                1.0 - (1.0 - t * t).sqrt()
            }
            Easing::EaseOutCirc => {
                (1.0 - (t - 1.0).powi(2)).sqrt()
            }
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
                    (2.0_f32.powf(-20.0 * t + 10.0) * ((20.0 * t - 11.125) * C5).sin()) / 2.0
                        + 1.0
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

            Easing::CubicBezier(x1, y1, x2, y2) => {
                cubic_bezier_sample(t, *x1, *y1, *x2, *y2)
            }
            Easing::Steps(n) => {
                if *n == 0 { t } else { (t * *n as f32).floor() / (*n as f32 - 1.0).max(1.0) }
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
            Easing::EaseInSine, Easing::EaseOutSine, Easing::EaseInOutSine,
            Easing::EaseInQuad, Easing::EaseOutQuad, Easing::EaseInOutQuad,
            Easing::EaseInCubic, Easing::EaseOutCubic, Easing::EaseInOutCubic,
            Easing::EaseInQuart, Easing::EaseOutQuart, Easing::EaseInOutQuart,
            Easing::EaseInQuint, Easing::EaseOutQuint, Easing::EaseInOutQuint,
            Easing::EaseInExpo, Easing::EaseOutExpo, Easing::EaseInOutExpo,
            Easing::EaseInCirc, Easing::EaseOutCirc, Easing::EaseInOutCirc,
            Easing::EaseInBounce, Easing::EaseOutBounce, Easing::EaseInOutBounce,
            Easing::EaseInElastic, Easing::EaseOutElastic, Easing::EaseInOutElastic,
            Easing::EaseInBack, Easing::EaseOutBack, Easing::EaseInOutBack,
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
            Easing::EaseInSine, Easing::EaseOutSine, Easing::EaseInOutSine,
            Easing::EaseInQuad, Easing::EaseOutQuad, Easing::EaseInOutQuad,
            Easing::EaseInCubic, Easing::EaseOutCubic, Easing::EaseInOutCubic,
            Easing::EaseInExpo, Easing::EaseOutExpo, Easing::EaseInOutExpo,
            Easing::EaseInCirc, Easing::EaseOutCirc, Easing::EaseInOutCirc,
        ];
        for e in &monotonic {
            let mut prev = e.apply(0.0);
            for i in 1..=20 {
                let t = i as f32 / 20.0;
                let val = e.apply(t);
                assert!(val >= prev - EPS, "{:?}: not monotonic at t={} (prev={}, cur={})", e, t, prev, val);
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
