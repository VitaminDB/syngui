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

    /// Шаг интегрирования. Полу-неявный Эйлер устойчив, только пока шаг мал
    /// относительно жёсткости и трения: при `dt > 2·mass/damping` множитель
    /// `(1 − damping·dt/mass)` уходит меньше −1, и каждый кадр не гасит
    /// колебание, а усиливает его. Просевший кадр (подгрузка данных,
    /// свёрнутое окно, тяжёлая перерисовка) давал ровно это: «резинка»
    /// прокрутки улетала на тысячи пикселей, содержимое уезжало за пределы
    /// экрана и область выглядела пустой. Поэтому длинный кадр отыгрываем
    /// подшагами устойчивой длины, а совсем большой пропуск времени
    /// обрезаем — догонять физику за секунду простоя незачем.
    pub fn update(&self, current: f32, target: f32, velocity: f32, dt_secs: f32) -> (f32, f32) {
        if !dt_secs.is_finite() || dt_secs <= 0.0 {
            return (current, velocity);
        }

        const MAX_DT: f32 = 0.25;
        let mut remaining = dt_secs.min(MAX_DT);
        let step_limit = self.stable_step();

        let mut position = current;
        let mut vel = velocity;

        while remaining > 0.0 {
            let step = remaining.min(step_limit);
            let displacement = position - target;
            let spring_force = -self.stiffness * displacement;
            let damping_force = -self.damping * vel;
            let acceleration = (spring_force + damping_force) / self.mass;

            vel += acceleration * step;
            position += vel * step;
            remaining -= step;
        }

        (position, vel)
    }

    /// Наибольший шаг, на котором интегрирование остаётся устойчивым:
    /// половина от предела по собственной частоте `2/ω` и по трению
    /// `2·mass/damping`, но не длиннее кадра 120 Гц.
    fn stable_step(&self) -> f32 {
        const MAX_STEP: f32 = 1.0 / 120.0;
        let mass = self.mass.max(f32::EPSILON);

        let mut limit = MAX_STEP;
        if self.stiffness > 0.0 {
            let omega = (self.stiffness / mass).sqrt();
            if omega > 0.0 {
                limit = limit.min(1.0 / omega);
            }
        }
        if self.damping > 0.0 {
            limit = limit.min(mass / self.damping);
        }
        limit.max(1.0 / 4000.0)
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

    /// Длинный кадр не должен раскачивать пружину: при `damping·dt > 2·mass`
    /// наивный шаг переворачивает скорость и уносит значение в тысячи.
    /// Именно так «резинка» прокрутки утаскивала содержимое за экран.
    #[test]
    fn long_frame_does_not_blow_up() {
        let s = Spring::new().with_stiffness(300.0).with_damping(25.0);
        let mut pos = 80.0_f32;
        let mut vel = -1500.0_f32;
        for _ in 0..40 {
            let (p, v) = s.update(pos, 0.0, vel, 0.25);
            pos = p;
            vel = v;
            assert!(
                pos.abs() < 500.0,
                "растяжение убежало: pos={pos}, vel={vel}"
            );
        }
        assert!(pos.abs() < 1.0, "пружина обязана успокоиться: pos={pos}");
    }

    /// На нормальном кадре поведение прежнее: пружина сходится к цели.
    #[test]
    fn short_frames_still_converge() {
        let s = Spring::new().with_stiffness(300.0).with_damping(25.0);
        let mut pos = 50.0_f32;
        let mut vel = 0.0_f32;
        for _ in 0..300 {
            let (p, v) = s.update(pos, 0.0, vel, 1.0 / 60.0);
            pos = p;
            vel = v;
        }
        assert!(pos.abs() < 0.5, "pos={pos}");
    }

    #[test]
    fn zero_and_negative_dt_are_noop() {
        let s = Spring::default();
        assert_eq!(s.update(3.0, 0.0, 1.0, 0.0), (3.0, 1.0));
        assert_eq!(s.update(3.0, 0.0, 1.0, -0.5), (3.0, 1.0));
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
