use crate::animation::Easing;
use crate::animation::transition::{AnimatedPropertyMap, AnimatedValue, easing_from_str};
use crate::mss::KeyframesDefinition;

pub type KeyframeValues = AnimatedPropertyMap;

#[derive(Clone, Copy, Debug, PartialEq, Eq, Default)]
pub enum AnimDirection {
    #[default]
    Normal,
    Reverse,
    Alternate,
    AlternateReverse,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Default)]
pub enum AnimFillMode {
    #[default]
    None,
    Forwards,
    Backwards,
    Both,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Default)]
pub enum AnimPlayState {
    #[default]
    Running,
    Paused,
}

#[derive(Clone, Debug)]
pub struct KeyframeAnimation {
    pub keyframes: KeyframesDefinition,
    pub duration_secs: f32,
    pub easing: Easing,
    pub iterations: f32,
    pub delay_secs: f32,
    pub direction: AnimDirection,
    pub fill_mode: AnimFillMode,
    pub play_state: AnimPlayState,
    pub elapsed: f32,
    pub completed_iterations: f32,
    /// Был ли уже первый tick. Первый dt после создания не начисляется:
    /// он накоплен ДО старта анимации (простой цикла, долгий кадр) и
    /// мгновенно «доигрывал» бы короткую анимацию до конца — на экране
    /// оставался застывший стартовый кадр.
    ticked: bool,
}

impl KeyframeAnimation {
    pub fn new(
        keyframes: KeyframesDefinition,
        duration_secs: f32,
        easing: Easing,
        iterations: f32,
    ) -> Self {
        Self {
            keyframes,
            duration_secs: duration_secs.max(0.001),
            easing,
            iterations,
            delay_secs: 0.0,
            direction: AnimDirection::default(),
            fill_mode: AnimFillMode::default(),
            play_state: AnimPlayState::default(),
            elapsed: 0.0,
            completed_iterations: 0.0,
            ticked: false,
        }
    }

    pub fn from_style(
        style: &crate::mss::ComputedStyle,
        stylesheet: &crate::mss::StyleSheet,
    ) -> Option<Self> {
        let shorthand = style
            .get("animation")
            .and_then(|v| v.as_string())
            .and_then(parse_animation_shorthand);

        let name = match style.animation_name() {
            Some(n) => n.to_string(),
            None => shorthand.as_ref().and_then(|sh| sh.name.clone())?,
        };
        if name == "none" {
            return None;
        }
        let keyframes = stylesheet.get_keyframes(&name)?.clone();

        let duration_secs = style
            .animation_duration_ms()
            .map(|ms| ms as f32 / 1000.0)
            .or_else(|| shorthand.as_ref().and_then(|sh| sh.duration_secs))
            .unwrap_or(1.0);

        let easing = style
            .animation_easing()
            .map(easing_from_str)
            .or_else(|| shorthand.as_ref().and_then(|sh| sh.easing))
            .unwrap_or(Easing::Linear);

        let iterations = style
            .animation_repeat()
            .map(parse_iterations)
            .or_else(|| shorthand.as_ref().and_then(|sh| sh.iterations))
            .unwrap_or(1.0);

        let delay_secs = style
            .animation_delay_ms()
            .map(|ms| ms as f32 / 1000.0)
            .or_else(|| shorthand.as_ref().and_then(|sh| sh.delay_secs))
            .unwrap_or(0.0);

        let direction = style
            .animation_direction()
            .and_then(parse_direction)
            .or_else(|| shorthand.as_ref().and_then(|sh| sh.direction))
            .unwrap_or_default();

        let fill_mode = style
            .animation_fill_mode()
            .and_then(parse_fill_mode)
            .or_else(|| shorthand.as_ref().and_then(|sh| sh.fill_mode))
            .unwrap_or_default();

        let play_state = style
            .animation_play_state()
            .and_then(parse_play_state)
            .or_else(|| shorthand.as_ref().and_then(|sh| sh.play_state))
            .unwrap_or_default();

        let mut anim = Self::new(keyframes, duration_secs, easing, iterations);
        anim.delay_secs = delay_secs.max(0.0);
        anim.direction = direction;
        anim.fill_mode = fill_mode;
        anim.play_state = play_state;
        Some(anim)
    }

    pub fn tick(&mut self, dt_secs: f32) -> bool {
        if self.play_state == AnimPlayState::Paused {
            return self.is_running();
        }
        if !self.ticked {
            self.ticked = true;
            return self.is_running();
        }
        self.elapsed += dt_secs;
        let active = (self.elapsed - self.delay_secs).max(0.0);
        let total_iterations = active / self.duration_secs;
        self.completed_iterations = total_iterations.floor();
        if total_iterations >= self.iterations {
            return false;
        }
        true
    }

    pub fn is_running(&self) -> bool {
        let active = (self.elapsed - self.delay_secs).max(0.0);
        (active / self.duration_secs) < self.iterations
    }

    fn effective_progress(&self) -> Option<f32> {
        if self.elapsed < self.delay_secs {
            if matches!(self.fill_mode, AnimFillMode::Backwards | AnimFillMode::Both) {
                return Some(self.easing.apply(self.iteration_t(0, 0.0)));
            }
            return None;
        }

        let active = self.elapsed - self.delay_secs;
        let raw_iter = active / self.duration_secs;

        if raw_iter >= self.iterations {
            if matches!(self.fill_mode, AnimFillMode::Forwards | AnimFillMode::Both) {
                let last_iter_idx = if self.iterations.is_finite() {
                    (self.iterations.ceil() as u32).saturating_sub(1)
                } else {
                    0
                };
                return Some(self.easing.apply(self.iteration_t(last_iter_idx, 1.0)));
            }
            return None;
        }

        let iter_idx = raw_iter.floor() as u32;
        let local = raw_iter.fract();
        let (iter_idx, local) = if local == 0.0 && raw_iter > 0.0 {
            (iter_idx.saturating_sub(1), 1.0)
        } else {
            (iter_idx, local)
        };

        Some(self.easing.apply(self.iteration_t(iter_idx, local)))
    }

    fn iteration_t(&self, iter_idx: u32, local_t: f32) -> f32 {
        match self.direction {
            AnimDirection::Normal => local_t,
            AnimDirection::Reverse => 1.0 - local_t,
            AnimDirection::Alternate => {
                if iter_idx % 2 == 0 { local_t } else { 1.0 - local_t }
            }
            AnimDirection::AlternateReverse => {
                if iter_idx % 2 == 0 { 1.0 - local_t } else { local_t }
            }
        }
    }

    pub fn current_values(&self) -> AnimatedPropertyMap {
        let t = match self.effective_progress() {
            Some(v) => v,
            None => return AnimatedPropertyMap::new(),
        };
        let steps = &self.keyframes.steps;

        if steps.is_empty() {
            return AnimatedPropertyMap::new();
        }

        let (from_step, to_step, local_t) = find_bracket(steps, t);

        let mut values = AnimatedPropertyMap::new();

        let all_props: std::collections::HashSet<&str> = from_step.declarations.keys()
            .chain(to_step.declarations.keys())
            .map(|s| s.as_str())
            .collect();

        for prop in all_props {
            let from_val = from_step.declarations.get(prop)
                .map(|sv| AnimatedValue::from_style_value(sv, prop));
            let to_val = to_step.declarations.get(prop)
                .map(|sv| AnimatedValue::from_style_value(sv, prop));

            let result = match (from_val, to_val) {
                (Some(a), Some(b)) if !matches!(a, AnimatedValue::None) && !matches!(b, AnimatedValue::None) => {
                    a.lerp(&b, local_t)
                }
                (Some(a), _) if !matches!(a, AnimatedValue::None) => a,
                (_, Some(b)) if !matches!(b, AnimatedValue::None) => b,
                _ => continue,
            };

            if !matches!(result, AnimatedValue::None) {
                values.set(prop, result);
            }
        }

        values
    }
}

fn find_bracket<'a>(
    steps: &'a [crate::mss::KeyframeStep],
    t: f32,
) -> (&'a crate::mss::KeyframeStep, &'a crate::mss::KeyframeStep, f32) {
    if steps.len() == 1 {
        return (&steps[0], &steps[0], 0.0);
    }

    let mut from_idx = 0;
    for (i, step) in steps.iter().enumerate() {
        if step.position <= t {
            from_idx = i;
        }
    }

    let to_idx = (from_idx + 1).min(steps.len() - 1);

    if from_idx == to_idx {
        return (&steps[from_idx], &steps[to_idx], 0.0);
    }

    let from_pos = steps[from_idx].position;
    let to_pos = steps[to_idx].position;
    let span = to_pos - from_pos;
    let local_t = if span > 0.0 { ((t - from_pos) / span).clamp(0.0, 1.0) } else { 1.0 };

    (&steps[from_idx], &steps[to_idx], local_t)
}

#[derive(Clone, Debug, Default)]
pub(crate) struct AnimationShorthand {
    pub name: Option<String>,
    pub duration_secs: Option<f32>,
    pub delay_secs: Option<f32>,
    pub easing: Option<Easing>,
    pub iterations: Option<f32>,
    pub direction: Option<AnimDirection>,
    pub fill_mode: Option<AnimFillMode>,
    pub play_state: Option<AnimPlayState>,
}

pub(crate) fn parse_animation_shorthand(s: &str) -> Option<AnimationShorthand> {
    let tokens = tokenize_shorthand(s);
    if tokens.is_empty() {
        return None;
    }

    let mut sh = AnimationShorthand::default();
    let mut time_slot = 0u8;

    for tok in tokens {
        let lower = tok.to_ascii_lowercase();

        if let Some(secs) = parse_time(&lower) {
            match time_slot {
                0 => { sh.duration_secs = Some(secs); time_slot = 1; }
                1 => { sh.delay_secs = Some(secs); time_slot = 2; }
                _ => {}
            }
            continue;
        }

        if lower == "infinite" {
            sh.iterations = Some(f32::INFINITY);
            continue;
        }
        if let Ok(n) = lower.parse::<f32>() {
            sh.iterations = Some(n);
            continue;
        }

        if is_timing_function(&lower) {
            sh.easing = Some(easing_from_str(&lower));
            continue;
        }

        if let Some(d) = parse_direction(&lower) {
            sh.direction = Some(d);
            continue;
        }

        if let Some(p) = parse_play_state(&lower) {
            sh.play_state = Some(p);
            continue;
        }

        if lower == "none" {
            if sh.name.is_none() {
                sh.name = Some("none".to_string());
            } else {
                sh.fill_mode = Some(AnimFillMode::None);
            }
            continue;
        }
        if let Some(f) = parse_fill_mode(&lower) {
            sh.fill_mode = Some(f);
            continue;
        }

        if sh.name.is_none() {
            sh.name = Some(tok);
        }
    }

    if sh.name.is_none()
        && sh.duration_secs.is_none()
        && sh.delay_secs.is_none()
        && sh.easing.is_none()
        && sh.iterations.is_none()
        && sh.direction.is_none()
        && sh.fill_mode.is_none()
        && sh.play_state.is_none()
    {
        return None;
    }

    Some(sh)
}

fn tokenize_shorthand(s: &str) -> Vec<String> {
    let mut out = Vec::new();
    let mut cur = String::new();
    let mut depth = 0i32;

    for ch in s.chars() {
        match ch {
            '(' => { depth += 1; cur.push(ch); }
            ')' => { depth = (depth - 1).max(0); cur.push(ch); }
            c if c.is_whitespace() && depth == 0 => {
                if !cur.is_empty() {
                    out.push(std::mem::take(&mut cur));
                }
            }
            ',' if depth == 0 => {
                if !cur.is_empty() {
                    out.push(std::mem::take(&mut cur));
                }
                break;
            }
            _ => cur.push(ch),
        }
    }
    if !cur.is_empty() {
        out.push(cur);
    }
    out
}

fn parse_time(token: &str) -> Option<f32> {
    if let Some(num) = token.strip_suffix("ms") {
        num.parse::<f32>().ok().map(|v| v / 1000.0)
    } else if let Some(num) = token.strip_suffix('s') {
        num.parse::<f32>().ok()
    } else {
        None
    }
}

fn is_timing_function(token: &str) -> bool {
    matches!(
        token,
        "linear" | "ease" | "ease-in" | "ease-out" | "ease-in-out" | "step-start" | "step-end"
    ) || token.starts_with("cubic-bezier(")
        || token.starts_with("steps(")
}

pub(crate) fn parse_direction(s: &str) -> Option<AnimDirection> {
    match s.to_ascii_lowercase().as_str() {
        "normal" => Some(AnimDirection::Normal),
        "reverse" => Some(AnimDirection::Reverse),
        "alternate" => Some(AnimDirection::Alternate),
        "alternate-reverse" => Some(AnimDirection::AlternateReverse),
        _ => None,
    }
}

pub(crate) fn parse_fill_mode(s: &str) -> Option<AnimFillMode> {
    match s.to_ascii_lowercase().as_str() {
        "none" => Some(AnimFillMode::None),
        "forwards" => Some(AnimFillMode::Forwards),
        "backwards" => Some(AnimFillMode::Backwards),
        "both" => Some(AnimFillMode::Both),
        _ => None,
    }
}

pub(crate) fn parse_play_state(s: &str) -> Option<AnimPlayState> {
    match s.to_ascii_lowercase().as_str() {
        "running" => Some(AnimPlayState::Running),
        "paused" => Some(AnimPlayState::Paused),
        _ => None,
    }
}

fn parse_iterations(s: &str) -> f32 {
    if s.eq_ignore_ascii_case("infinite") {
        f32::INFINITY
    } else {
        s.parse::<f32>().unwrap_or(1.0)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::mss::{KeyframeStep, StyleValue};
    use std::collections::HashMap;

    fn make_step(pos: f32, opacity: f32) -> KeyframeStep {
        let mut decls = HashMap::new();
        decls.insert("opacity".to_string(), StyleValue::Number(opacity));
        KeyframeStep { position: pos, declarations: decls }
    }

    #[test]
    fn keyframe_animation_basic_progress() {
        let kf = KeyframesDefinition {
            name: "fade".to_string(),
            steps: vec![make_step(0.0, 0.0), make_step(1.0, 1.0)],
        };
        let mut anim = KeyframeAnimation::new(kf, 1.0, Easing::Linear, 1.0);

        let vals = anim.current_values();
        assert!((vals.opacity().unwrap() - 0.0).abs() < 0.01);

        anim.tick(0.0); // прогрев: первый tick не начисляет dt
        anim.tick(0.5);
        let vals = anim.current_values();
        assert!((vals.opacity().unwrap() - 0.5).abs() < 0.01);

        anim.tick(0.49);
        let vals = anim.current_values();
        assert!((vals.opacity().unwrap() - 0.99).abs() < 0.02);
    }

    #[test]
    fn keyframe_animation_infinite_loops() {
        let kf = KeyframesDefinition {
            name: "pulse".to_string(),
            steps: vec![make_step(0.0, 1.0), make_step(0.5, 0.3), make_step(1.0, 1.0)],
        };
        let mut anim = KeyframeAnimation::new(kf, 1.0, Easing::Linear, f32::INFINITY);

        for _ in 0..100 {
            assert!(anim.tick(0.1));
        }
        assert!(anim.is_running());
    }

    #[test]
    fn keyframe_animation_finite_ends() {
        let kf = KeyframesDefinition {
            name: "once".to_string(),
            steps: vec![make_step(0.0, 0.0), make_step(1.0, 1.0)],
        };
        let mut anim = KeyframeAnimation::new(kf, 0.5, Easing::Linear, 1.0);

        anim.tick(0.0); // прогрев: первый tick не начисляет dt
        assert!(anim.tick(0.3));
        assert!(anim.is_running());
        assert!(!anim.tick(0.3));
        assert!(!anim.is_running());
    }

    #[test]
    fn keyframe_three_steps() {
        let kf = KeyframesDefinition {
            name: "three".to_string(),
            steps: vec![
                make_step(0.0, 0.0),
                make_step(0.5, 1.0),
                make_step(1.0, 0.5),
            ],
        };
        let mut anim = KeyframeAnimation::new(kf, 1.0, Easing::Linear, 1.0);

        anim.tick(0.0); // прогрев: первый tick не начисляет dt
        anim.tick(0.25);
        let vals = anim.current_values();
        assert!((vals.opacity().unwrap() - 0.5).abs() < 0.01);

        anim.elapsed = 0.75;
        let vals = anim.current_values();
        assert!((vals.opacity().unwrap() - 0.75).abs() < 0.01);
    }

    #[test]
    fn shorthand_minimal_name_and_duration() {
        let sh = parse_animation_shorthand("spin 1s").unwrap();
        assert_eq!(sh.name.as_deref(), Some("spin"));
        assert!((sh.duration_secs.unwrap() - 1.0).abs() < 1e-6);
        assert!(sh.delay_secs.is_none());
        assert!(sh.easing.is_none());
    }

    #[test]
    fn shorthand_duration_easing_iterations() {
        let sh = parse_animation_shorthand("pulse 2s ease-in-out infinite").unwrap();
        assert_eq!(sh.name.as_deref(), Some("pulse"));
        assert!((sh.duration_secs.unwrap() - 2.0).abs() < 1e-6);
        assert!(sh.easing.is_some());
        assert_eq!(sh.iterations, Some(f32::INFINITY));
    }

    #[test]
    fn shorthand_full_spec_all_fields() {
        let sh = parse_animation_shorthand(
            "pop 300ms 100ms ease-out 2 reverse forwards paused",
        )
        .unwrap();
        assert_eq!(sh.name.as_deref(), Some("pop"));
        assert!((sh.duration_secs.unwrap() - 0.3).abs() < 1e-6);
        assert!((sh.delay_secs.unwrap() - 0.1).abs() < 1e-6);
        assert_eq!(sh.iterations, Some(2.0));
        assert_eq!(sh.direction, Some(AnimDirection::Reverse));
        assert_eq!(sh.fill_mode, Some(AnimFillMode::Forwards));
        assert_eq!(sh.play_state, Some(AnimPlayState::Paused));
    }

    #[test]
    fn shorthand_ms_units() {
        let sh = parse_animation_shorthand("bubble-pop-in 220ms ease-out").unwrap();
        assert_eq!(sh.name.as_deref(), Some("bubble-pop-in"));
        assert!((sh.duration_secs.unwrap() - 0.22).abs() < 1e-6);
        assert!(sh.easing.is_some());
    }

    #[test]
    fn shorthand_cubic_bezier_preserved() {
        let sh = parse_animation_shorthand("foo 1s cubic-bezier(0.2, 0.9, 0.3, 1.0) 3").unwrap();
        assert_eq!(sh.name.as_deref(), Some("foo"));
        assert!(sh.easing.is_some());
        assert_eq!(sh.iterations, Some(3.0));
    }

    #[test]
    fn shorthand_none_is_name_when_first() {
        let sh = parse_animation_shorthand("none 1s").unwrap();
        assert_eq!(sh.name.as_deref(), Some("none"));
    }

    #[test]
    fn shorthand_none_is_fill_mode_when_name_exists() {
        let sh = parse_animation_shorthand("spin 1s none").unwrap();
        assert_eq!(sh.name.as_deref(), Some("spin"));
        assert_eq!(sh.fill_mode, Some(AnimFillMode::None));
    }

    #[test]
    fn shorthand_empty_returns_none() {
        assert!(parse_animation_shorthand("").is_none());
        assert!(parse_animation_shorthand("   ").is_none());
    }

    #[test]
    fn paused_does_not_advance_elapsed() {
        let kf = KeyframesDefinition {
            name: "x".to_string(),
            steps: vec![make_step(0.0, 0.0), make_step(1.0, 1.0)],
        };
        let mut anim = KeyframeAnimation::new(kf, 1.0, Easing::Linear, 1.0);
        anim.play_state = AnimPlayState::Paused;

        anim.tick(0.5);
        anim.tick(0.5);
        assert_eq!(anim.elapsed, 0.0);
    }

    #[test]
    fn reverse_direction_inverts_progress() {
        let kf = KeyframesDefinition {
            name: "x".to_string(),
            steps: vec![make_step(0.0, 0.0), make_step(1.0, 1.0)],
        };
        let mut anim = KeyframeAnimation::new(kf, 1.0, Easing::Linear, 1.0);
        anim.direction = AnimDirection::Reverse;

        anim.tick(0.0); // прогрев: первый tick не начисляет dt
        anim.tick(0.25);
        let vals = anim.current_values();
        assert!((vals.opacity().unwrap() - 0.75).abs() < 0.01);
    }

    #[test]
    fn fill_mode_forwards_freezes_at_end() {
        let kf = KeyframesDefinition {
            name: "x".to_string(),
            steps: vec![make_step(0.0, 0.0), make_step(1.0, 1.0)],
        };
        let mut anim = KeyframeAnimation::new(kf, 0.5, Easing::Linear, 1.0);
        anim.fill_mode = AnimFillMode::Forwards;

        anim.tick(0.0); // прогрев: первый tick не начисляет dt
        anim.tick(1.0);
        assert!(!anim.is_running());
        let vals = anim.current_values();
        assert!((vals.opacity().unwrap() - 1.0).abs() < 0.01);
    }

    #[test]
    fn fill_mode_none_hides_after_end() {
        let kf = KeyframesDefinition {
            name: "x".to_string(),
            steps: vec![make_step(0.0, 0.0), make_step(1.0, 1.0)],
        };
        let mut anim = KeyframeAnimation::new(kf, 0.5, Easing::Linear, 1.0);
        anim.fill_mode = AnimFillMode::None;

        anim.tick(0.0); // прогрев: первый tick не начисляет dt
        anim.tick(1.0);
        let vals = anim.current_values();
        assert!(vals.opacity().is_none());
    }

    #[test]
    fn fill_mode_backwards_applies_during_delay() {
        let kf = KeyframesDefinition {
            name: "x".to_string(),
            steps: vec![make_step(0.0, 0.2), make_step(1.0, 1.0)],
        };
        let mut anim = KeyframeAnimation::new(kf, 1.0, Easing::Linear, 1.0);
        anim.delay_secs = 0.5;
        anim.fill_mode = AnimFillMode::Backwards;

        anim.tick(0.2);
        let vals = anim.current_values();
        assert!((vals.opacity().unwrap() - 0.2).abs() < 0.01);
    }

    #[test]
    fn delay_postpones_active_window() {
        let kf = KeyframesDefinition {
            name: "x".to_string(),
            steps: vec![make_step(0.0, 0.0), make_step(1.0, 1.0)],
        };
        let mut anim = KeyframeAnimation::new(kf, 1.0, Easing::Linear, 1.0);
        anim.delay_secs = 0.5;

        anim.tick(0.0); // прогрев: первый tick не начисляет dt
        anim.tick(0.3);
        assert!(anim.current_values().opacity().is_none());

        anim.tick(0.7);
        let vals = anim.current_values();
        assert!((vals.opacity().unwrap() - 0.5).abs() < 0.01);
    }

    #[test]
    fn alternate_swaps_direction_per_iteration() {
        let kf = KeyframesDefinition {
            name: "x".to_string(),
            steps: vec![make_step(0.0, 0.0), make_step(1.0, 1.0)],
        };
        let mut anim = KeyframeAnimation::new(kf, 1.0, Easing::Linear, 4.0);
        anim.direction = AnimDirection::Alternate;

        anim.elapsed = 1.25;
        let vals = anim.current_values();
        assert!((vals.opacity().unwrap() - 0.75).abs() < 0.05);
    }
}
