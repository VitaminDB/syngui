use crate::animation::{Easing, Spring};
use std::time::Duration;

#[derive(Clone, Debug)]
pub enum Animation {
    Spring {
        spring: Spring,
        target: f32,
        current: f32,
        initial: f32,
        velocity: f32,
    },
    Tween {
        from: f32,
        to: f32,
        duration: Duration,
        elapsed: Duration,
        delay: Duration,
        easing: Easing,
    },
    Sequence {
        animations: Vec<Animation>,
        current_index: usize,
    },
    Constant(f32),
}

impl Animation {
    pub fn spring() -> SpringAnimationBuilder {
        SpringAnimationBuilder::new()
    }

    pub fn tween(easing: Easing) -> TweenAnimationBuilder {
        TweenAnimationBuilder::new(easing)
    }

    pub fn constant(value: f32) -> Self {
        Self::Constant(value)
    }

    pub fn current_value(&self) -> f32 {
        match self {
            Self::Spring { current, .. } => *current,
            Self::Tween { from, to, duration, elapsed, delay, easing } => {
                if *elapsed <= *delay {
                    *from
                } else if *elapsed >= *delay + *duration {
                    *to
                } else {
                    let active = *elapsed - *delay;
                    let t = active.as_secs_f32() / duration.as_secs_f32();
                    let eased = easing.apply(t);
                    from + (to - from) * eased
                }
            }
            Self::Sequence { animations, current_index } => {
                if let Some(anim) = animations.get(*current_index) {
                    anim.current_value()
                } else {
                    0.0
                }
            }
            Self::Constant(v) => *v,
        }
    }

    pub fn initial_value(&self) -> f32 {
        match self {
            Self::Spring { initial, .. } => *initial,
            Self::Tween { from, .. } => *from,
            Self::Sequence { animations, .. } => {
                animations.first().map_or(0.0, |a| a.initial_value())
            }
            Self::Constant(v) => *v,
        }
    }

    pub fn target_value(&self) -> f32 {
        match self {
            Self::Spring { target, .. } => *target,
            Self::Tween { to, .. } => *to,
            Self::Sequence { .. } => 0.0,
            Self::Constant(v) => *v,
        }
    }

    pub fn set_target(&mut self, target: f32) {
        match self {
            Self::Spring { target: t, .. } => *t = target,
            Self::Tween { to, .. } => *to = target,
            Self::Constant(v) => *v = target,
            _ => {}
        }
    }

    pub fn tick(&mut self, dt: Duration) -> bool {
        match self {
            Self::Spring { spring, target, current, velocity, .. } => {
                let dt_secs = dt.as_secs_f32();
                let (new_pos, new_vel) = spring.update(*current, *target, *velocity, dt_secs);
                *current = new_pos;
                *velocity = new_vel;
                !spring.is_at_rest(*current - *target, *velocity)
            }
            Self::Tween { duration, elapsed, delay, .. } => {
                *elapsed += dt;
                *elapsed < *delay + *duration
            }
            Self::Sequence { animations, current_index } => {
                if let Some(anim) = animations.get_mut(*current_index) {
                    if !anim.tick(dt) {
                        *current_index += 1;
                    }
                }
                *current_index < animations.len()
            }
            Self::Constant(_) => false,
        }
    }

    pub fn is_complete(&self) -> bool {
        match self {
            Self::Spring { spring, current, target, velocity, .. } => {
                spring.is_at_rest(*current - *target, *velocity)
            }
            Self::Tween { duration, elapsed, delay, .. } => *elapsed >= *delay + *duration,
            Self::Sequence { animations, current_index } => *current_index >= animations.len(),
            Self::Constant(_) => true,
        }
    }

    pub fn reset(&mut self) {
        match self {
            Self::Spring { initial, current, velocity, .. } => {
                *current = *initial;
                *velocity = 0.0;
            }
            Self::Tween { elapsed, .. } => {
                *elapsed = Duration::ZERO;
            }
            Self::Sequence { current_index, .. } => {
                *current_index = 0;
            }
            _ => {}
        }
    }
}

impl Default for Animation {
    fn default() -> Self {
        Self::Constant(0.0)
    }
}

pub struct SpringAnimationBuilder {
    spring: Spring,
    from: f32,
    to: f32,
    delay_ms: u32,
}

impl SpringAnimationBuilder {
    fn new() -> Self {
        Self {
            spring: Spring::new(),
            from: 0.0,
            to: 1.0,
            delay_ms: 0,
        }
    }

    pub fn from(mut self, value: f32) -> Self {
        self.from = value;
        self
    }

    pub fn to(mut self, value: f32) -> Self {
        self.to = value;
        self
    }

    pub fn stiffness(mut self, value: f32) -> Self {
        self.spring.stiffness = value;
        self
    }

    pub fn damping(mut self, value: f32) -> Self {
        self.spring.damping = value;
        self
    }

    pub fn mass(mut self, value: f32) -> Self {
        self.spring.mass = value;
        self
    }

    pub fn delay_ms(mut self, ms: u32) -> Self {
        self.delay_ms = ms;
        self
    }

    pub fn duration_ms(self, _ms: u32) -> Self {
        self
    }

    pub fn build(self) -> Animation {
        Animation::Spring {
            spring: self.spring,
            target: self.to,
            current: self.from,
            initial: self.from,
            velocity: 0.0,
        }
    }
}

impl From<SpringAnimationBuilder> for Animation {
    fn from(builder: SpringAnimationBuilder) -> Self {
        builder.build()
    }
}

pub struct TweenAnimationBuilder {
    easing: Easing,
    from: f32,
    to: f32,
    duration_ms: u32,
    delay_ms: u32,
}

impl TweenAnimationBuilder {
    fn new(easing: Easing) -> Self {
        Self {
            easing,
            from: 0.0,
            to: 1.0,
            duration_ms: 300,
            delay_ms: 0,
        }
    }

    pub fn from(mut self, value: f32) -> Self {
        self.from = value;
        self
    }

    pub fn to(mut self, value: f32) -> Self {
        self.to = value;
        self
    }

    pub fn duration_ms(mut self, ms: u32) -> Self {
        self.duration_ms = ms;
        self
    }

    pub fn delay_ms(mut self, ms: u32) -> Self {
        self.delay_ms = ms;
        self
    }

    pub fn build(self) -> Animation {
        Animation::Tween {
            from: self.from,
            to: self.to,
            duration: Duration::from_millis(self.duration_ms as u64),
            elapsed: Duration::ZERO,
            delay: Duration::from_millis(self.delay_ms as u64),
            easing: self.easing,
        }
    }
}

impl From<TweenAnimationBuilder> for Animation {
    fn from(builder: TweenAnimationBuilder) -> Self {
        builder.build()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn ms(millis: u64) -> Duration {
        Duration::from_millis(millis)
    }

    #[test]
    fn constant_value() {
        let a = Animation::constant(42.0);
        assert_eq!(a.current_value(), 42.0);
        assert_eq!(a.initial_value(), 42.0);
        assert_eq!(a.target_value(), 42.0);
    }

    #[test]
    fn constant_is_always_complete() {
        let a = Animation::constant(1.0);
        assert!(a.is_complete());
    }

    #[test]
    fn constant_tick_returns_false() {
        let mut a = Animation::constant(1.0);
        assert!(!a.tick(ms(16)));
    }

    #[test]
    fn constant_set_target() {
        let mut a = Animation::constant(1.0);
        a.set_target(5.0);
        assert_eq!(a.current_value(), 5.0);
    }

    #[test]
    fn default_is_constant_zero() {
        let a = Animation::default();
        assert_eq!(a.current_value(), 0.0);
        assert!(a.is_complete());
    }

    #[test]
    fn tween_builder() {
        let a = Animation::tween(Easing::Linear)
            .from(0.0)
            .to(100.0)
            .duration_ms(500)
            .build();
        assert_eq!(a.current_value(), 0.0);
        assert_eq!(a.initial_value(), 0.0);
        assert_eq!(a.target_value(), 100.0);
    }

    #[test]
    fn tween_starts_at_from() {
        let a = Animation::tween(Easing::Linear).from(10.0).to(20.0).duration_ms(300).build();
        assert_eq!(a.current_value(), 10.0);
    }

    #[test]
    fn tween_not_complete_at_start() {
        let a = Animation::tween(Easing::Linear).duration_ms(300).build();
        assert!(!a.is_complete());
    }

    #[test]
    fn tween_linear_midpoint() {
        let mut a = Animation::tween(Easing::Linear)
            .from(0.0).to(100.0).duration_ms(1000).build();
        a.tick(ms(500));
        let val = a.current_value();
        assert!((val - 50.0).abs() < 1.0, "midpoint should be ~50, got {}", val);
    }

    #[test]
    fn tween_completes_at_duration() {
        let mut a = Animation::tween(Easing::Linear)
            .from(0.0).to(100.0).duration_ms(300).build();
        let needs_more = a.tick(ms(300));
        assert!(!needs_more);
        assert!(a.is_complete());
        assert_eq!(a.current_value(), 100.0);
    }

    #[test]
    fn tween_past_duration_returns_to() {
        let mut a = Animation::tween(Easing::Linear)
            .from(0.0).to(50.0).duration_ms(100).build();
        a.tick(ms(200));
        assert_eq!(a.current_value(), 50.0);
    }

    #[test]
    fn tween_with_delay() {
        let mut a = Animation::tween(Easing::Linear)
            .from(0.0).to(100.0).duration_ms(100).delay_ms(50).build();
        a.tick(ms(25));
        assert_eq!(a.current_value(), 0.0);
        a.tick(ms(75));
        let val = a.current_value();
        assert!((val - 50.0).abs() < 5.0, "should be ~50, got {}", val);
    }

    #[test]
    fn tween_tick_returns_true_while_active() {
        let mut a = Animation::tween(Easing::Linear).duration_ms(100).build();
        assert!(a.tick(ms(50)));
        assert!(!a.tick(ms(60)));
    }

    #[test]
    fn tween_reset() {
        let mut a = Animation::tween(Easing::Linear)
            .from(0.0).to(100.0).duration_ms(100).build();
        a.tick(ms(50));
        assert!(a.current_value() > 0.0);
        a.reset();
        assert_eq!(a.current_value(), 0.0);
        assert!(!a.is_complete());
    }

    #[test]
    fn tween_set_target() {
        let mut a = Animation::tween(Easing::Linear)
            .from(0.0).to(100.0).duration_ms(100).build();
        a.set_target(200.0);
        assert_eq!(a.target_value(), 200.0);
    }

    #[test]
    fn tween_from_builder_trait() {
        let a: Animation = Animation::tween(Easing::Linear).from(0.0).to(1.0).duration_ms(100).into();
        assert_eq!(a.initial_value(), 0.0);
    }

    #[test]
    fn spring_builder() {
        let a = Animation::spring()
            .from(0.0)
            .to(100.0)
            .stiffness(300.0)
            .damping(25.0)
            .mass(1.5)
            .build();
        assert_eq!(a.current_value(), 0.0);
        assert_eq!(a.initial_value(), 0.0);
        assert_eq!(a.target_value(), 100.0);
    }

    #[test]
    fn spring_tick_moves_toward_target() {
        let mut a = Animation::spring().from(0.0).to(1.0).build();
        a.tick(ms(16));
        assert!(a.current_value() > 0.0);
    }

    #[test]
    fn spring_converges() {
        let mut a = Animation::spring().from(0.0).to(1.0).build();
        for _ in 0..600 {
            a.tick(ms(16));
        }
        assert!((a.current_value() - 1.0).abs() < 0.01);
        assert!(a.is_complete());
    }

    #[test]
    fn spring_reset() {
        let mut a = Animation::spring().from(0.0).to(1.0).build();
        a.tick(ms(100));
        a.reset();
        assert_eq!(a.current_value(), 0.0);
    }

    #[test]
    fn spring_set_target() {
        let mut a = Animation::spring().from(0.0).to(1.0).build();
        a.set_target(5.0);
        assert_eq!(a.target_value(), 5.0);
    }

    #[test]
    fn spring_from_builder_trait() {
        let a: Animation = Animation::spring().from(0.0).to(1.0).into();
        assert_eq!(a.initial_value(), 0.0);
    }

    #[test]
    fn spring_duration_ms_ignored() {
        let a = Animation::spring().from(0.0).to(1.0).duration_ms(100).build();
        assert_eq!(a.target_value(), 1.0);
    }

    #[test]
    fn sequence_plays_first_animation() {
        let a = Animation::Sequence {
            animations: vec![
                Animation::tween(Easing::Linear).from(0.0).to(1.0).duration_ms(100).build(),
                Animation::tween(Easing::Linear).from(1.0).to(2.0).duration_ms(100).build(),
            ],
            current_index: 0,
        };
        assert_eq!(a.current_value(), 0.0);
        assert_eq!(a.initial_value(), 0.0);
    }

    #[test]
    fn sequence_advances_after_first_completes() {
        let mut a = Animation::Sequence {
            animations: vec![
                Animation::tween(Easing::Linear).from(0.0).to(1.0).duration_ms(100).build(),
                Animation::tween(Easing::Linear).from(1.0).to(2.0).duration_ms(100).build(),
            ],
            current_index: 0,
        };
        a.tick(ms(100));
        a.tick(ms(1));
        let val = a.current_value();
        assert!((val - 1.0).abs() < 0.05, "should be ~1.0, got {}", val);
    }

    #[test]
    fn sequence_completes_when_all_done() {
        let mut a = Animation::Sequence {
            animations: vec![
                Animation::tween(Easing::Linear).from(0.0).to(1.0).duration_ms(50).build(),
            ],
            current_index: 0,
        };
        a.tick(ms(50));
        a.tick(ms(1));
        assert!(a.is_complete());
    }

    #[test]
    fn sequence_empty_is_complete() {
        let a = Animation::Sequence { animations: vec![], current_index: 0 };
        assert!(a.is_complete());
        assert_eq!(a.current_value(), 0.0);
    }

    #[test]
    fn sequence_reset() {
        let mut a = Animation::Sequence {
            animations: vec![
                Animation::tween(Easing::Linear).from(0.0).to(1.0).duration_ms(50).build(),
            ],
            current_index: 1,
        };
        a.reset();
        if let Animation::Sequence { current_index, .. } = a {
            assert_eq!(current_index, 0);
        }
    }
}
