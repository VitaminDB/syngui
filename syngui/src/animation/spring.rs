#[derive(Clone, Copy, Debug)]
pub struct Spring {
    pub stiffness: f32,
    pub damping: f32,
    pub mass: f32,
}

impl Spring {
    pub fn new() -> Self {
        Self::default()
    }

    pub fn with_stiffness(mut self, stiffness: f32) -> Self {
        self.stiffness = stiffness;
        self
    }

    pub fn with_damping(mut self, damping: f32) -> Self {
        self.damping = damping;
        self
    }

    pub fn with_mass(mut self, mass: f32) -> Self {
        self.mass = mass;
        self
    }

    pub fn update(&self, current: f32, target: f32, velocity: f32, dt_secs: f32) -> (f32, f32) {
        let displacement = current - target;
        let spring_force = -self.stiffness * displacement;
        let damping_force = -self.damping * velocity;
        let acceleration = (spring_force + damping_force) / self.mass;

        let new_velocity = velocity + acceleration * dt_secs;
        let new_position = current + new_velocity * dt_secs;

        (new_position, new_velocity)
    }

    pub fn is_at_rest(&self, displacement: f32, velocity: f32) -> bool {
        const EPSILON: f32 = 0.001;
        displacement.abs() < EPSILON && velocity.abs() < EPSILON
    }
}

impl Default for Spring {
    fn default() -> Self {
        Self {
            stiffness: 200.0,
            damping: 20.0,
            mass: 1.0,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn default_values() {
        let s = Spring::default();
        assert_eq!(s.stiffness, 200.0);
        assert_eq!(s.damping, 20.0);
        assert_eq!(s.mass, 1.0);
    }

    #[test]
    fn new_is_default() {
        let a = Spring::new();
        let b = Spring::default();
        assert_eq!(a.stiffness, b.stiffness);
        assert_eq!(a.damping, b.damping);
        assert_eq!(a.mass, b.mass);
    }

    #[test]
    fn builder_stiffness() {
        let s = Spring::new().with_stiffness(500.0);
        assert_eq!(s.stiffness, 500.0);
        assert_eq!(s.damping, 20.0);
    }

    #[test]
    fn builder_damping() {
        let s = Spring::new().with_damping(30.0);
        assert_eq!(s.damping, 30.0);
    }

    #[test]
    fn builder_mass() {
        let s = Spring::new().with_mass(2.0);
        assert_eq!(s.mass, 2.0);
    }

    #[test]
    fn builder_chain() {
        let s = Spring::new().with_stiffness(100.0).with_damping(10.0).with_mass(0.5);
        assert_eq!(s.stiffness, 100.0);
        assert_eq!(s.damping, 10.0);
        assert_eq!(s.mass, 0.5);
    }

    #[test]
    fn update_moves_toward_target() {
        let s = Spring::default();
        let (pos, _vel) = s.update(0.0, 1.0, 0.0, 1.0 / 60.0);
        assert!(pos > 0.0, "should move toward target from 0 to 1");
    }

    #[test]
    fn update_from_above_target() {
        let s = Spring::default();
        let (pos, _vel) = s.update(2.0, 1.0, 0.0, 1.0 / 60.0);
        assert!(pos < 2.0, "should move toward target from 2 to 1");
    }

    #[test]
    fn update_at_target_with_zero_velocity() {
        let s = Spring::default();
        let (pos, vel) = s.update(1.0, 1.0, 0.0, 1.0 / 60.0);
        assert!((pos - 1.0).abs() < 1e-5);
        assert!(vel.abs() < 1e-5);
    }

    #[test]
    fn spring_converges() {
        let s = Spring::default();
        let mut pos = 0.0;
        let mut vel = 0.0;
        let target = 1.0;
        let dt = 1.0 / 60.0;
        for _ in 0..600 {
            let (p, v) = s.update(pos, target, vel, dt);
            pos = p;
            vel = v;
        }
        assert!((pos - target).abs() < 0.01, "spring should converge, pos={}", pos);
        assert!(vel.abs() < 0.01);
    }

    #[test]
    fn is_at_rest_true() {
        let s = Spring::default();
        assert!(s.is_at_rest(0.0001, 0.0001));
        assert!(s.is_at_rest(0.0, 0.0));
    }

    #[test]
    fn is_at_rest_false_displacement() {
        let s = Spring::default();
        assert!(!s.is_at_rest(0.01, 0.0));
    }

    #[test]
    fn is_at_rest_false_velocity() {
        let s = Spring::default();
        assert!(!s.is_at_rest(0.0, 0.01));
    }

    #[test]
    fn high_damping_no_oscillation() {
        let s = Spring::new().with_stiffness(100.0).with_damping(50.0);
        let mut pos = 0.0;
        let mut vel = 0.0;
        let dt = 1.0 / 60.0;
        let mut overshot = false;
        for _ in 0..300 {
            let (p, v) = s.update(pos, 1.0, vel, dt);
            pos = p;
            vel = v;
            if pos > 1.01 {
                overshot = true;
            }
        }
        assert!(!overshot, "high damping should not overshoot significantly");
    }

    #[test]
    fn low_damping_oscillates() {
        let s = Spring::new().with_stiffness(200.0).with_damping(2.0);
        let mut pos = 0.0;
        let mut vel = 0.0;
        let dt = 1.0 / 60.0;
        let mut overshot = false;
        for _ in 0..60 {
            let (p, v) = s.update(pos, 1.0, vel, dt);
            pos = p;
            vel = v;
            if pos > 1.05 {
                overshot = true;
                break;
            }
        }
        assert!(overshot, "low damping should overshoot");
    }
}
