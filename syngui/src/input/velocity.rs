//! Скорость пальца или указателя по последним точкам — для бросков
//! (fling): пролистать страницу, закрыть карточку смахиванием, инерция
//! панорамы.

use crate::core::Point;
use std::collections::VecDeque;
use std::time::Duration;
use web_time::Instant;

/// Окно, по которому считается скорость: старые точки не в счёт, иначе
/// палец, остановившийся перед отпусканием, всё равно «бросал» бы.
const WINDOW: Duration = Duration::from_millis(100);
/// Палец стоял дольше — скорость нулевая.
const STALE: Duration = Duration::from_millis(60);

#[derive(Debug, Default, Clone)]
pub struct VelocityTracker {
    samples: VecDeque<(Instant, Point)>,
}

impl VelocityTracker {
    pub fn new() -> Self {
        Self::default()
    }

    pub fn reset(&mut self) {
        self.samples.clear();
    }

    pub fn add(&mut self, pos: Point) {
        self.add_at(Instant::now(), pos);
    }

    pub fn add_at(&mut self, t: Instant, pos: Point) {
        self.samples.push_back((t, pos));
        while let Some(&(t0, _)) = self.samples.front() {
            if t.duration_since(t0) > WINDOW && self.samples.len() > 2 {
                self.samples.pop_front();
            } else {
                break;
            }
        }
    }

    /// Скорость в логических пикселях в секунду.
    pub fn velocity(&self) -> Point {
        self.velocity_at(Instant::now())
    }

    pub fn velocity_at(&self, now: Instant) -> Point {
        let (Some(&(t0, p0)), Some(&(t1, p1))) = (self.samples.front(), self.samples.back()) else {
            return Point::zero();
        };
        if now.duration_since(t1) > STALE {
            return Point::zero();
        }
        let dt = t1.duration_since(t0).as_secs_f32();
        if dt < 0.004 {
            return Point::zero();
        }
        Point::new((p1.x - p0.x) / dt, (p1.y - p0.y) / dt)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn constant_motion() {
        let t0 = Instant::now();
        let mut v = VelocityTracker::new();
        for i in 0..6 {
            v.add_at(t0 + Duration::from_millis(i * 10), Point::new(i as f32 * 10.0, 0.0));
        }
        let vel = v.velocity_at(t0 + Duration::from_millis(50));
        assert!((vel.x - 1000.0).abs() < 1.0, "{vel:?}");
        assert_eq!(v.velocity_at(t0 + Duration::from_millis(500)), Point::zero());
    }
}
