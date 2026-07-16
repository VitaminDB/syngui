use super::types::DataPoint;

#[derive(Debug, Clone, Copy)]
pub struct LinearScale {
    pub domain: (f64, f64),
    pub range: (f32, f32),
}

impl LinearScale {
    pub fn new(domain: (f64, f64), range: (f32, f32)) -> Self {
        Self { domain, range }
    }

    pub fn map(&self, value: f64) -> f32 {
        let span = self.domain.1 - self.domain.0;
        if span.abs() < 1e-12 {
            return (self.range.0 + self.range.1) * 0.5;
        }
        let t = (value - self.domain.0) / span;
        self.range.0 + t as f32 * (self.range.1 - self.range.0)
    }

    pub fn invert(&self, pixel: f32) -> f64 {
        let span = self.range.1 - self.range.0;
        if span.abs() < 1e-6 {
            return (self.domain.0 + self.domain.1) * 0.5;
        }
        let t = (pixel - self.range.0) / span;
        self.domain.0 + t as f64 * (self.domain.1 - self.domain.0)
    }
}

pub fn nice_number(x: f64, round: bool) -> f64 {
    if x <= 0.0 {
        return 0.0;
    }
    let exp = x.log10().floor();
    let frac = x / 10.0_f64.powf(exp);
    let nice = if round {
        if frac < 1.5 {
            1.0
        } else if frac < 3.0 {
            2.0
        } else if frac < 7.0 {
            5.0
        } else {
            10.0
        }
    } else {
        if frac <= 1.0 {
            1.0
        } else if frac <= 2.0 {
            2.0
        } else if frac <= 5.0 {
            5.0
        } else {
            10.0
        }
    };
    nice * 10.0_f64.powf(exp)
}

pub fn compute_ticks(min: f64, max: f64, desired_count: usize) -> Vec<f64> {
    if desired_count == 0 || (max - min).abs() < 1e-12 {
        return vec![min];
    }

    let range = nice_number(max - min, false);
    let step = nice_number(range / desired_count as f64, true);

    if step <= 0.0 {
        return vec![min];
    }

    let graph_min = (min / step).floor() * step;
    let graph_max = (max / step).ceil() * step;

    let mut ticks = Vec::new();
    let mut v = graph_min;
    // Safety limit to avoid infinite loops
    let max_ticks = desired_count * 4;
    while v <= graph_max + step * 0.5 && ticks.len() < max_ticks {
        ticks.push(v);
        v += step;
    }
    ticks
}

pub fn data_extent(
    series: &[super::types::Series],
    visible: &[bool],
) -> (f64, f64, f64, f64) {
    let mut x_min = f64::INFINITY;
    let mut x_max = f64::NEG_INFINITY;
    let mut y_min = f64::INFINITY;
    let mut y_max = f64::NEG_INFINITY;

    for (i, s) in series.iter().enumerate() {
        if i < visible.len() && !visible[i] {
            continue;
        }
        for dp in &s.data {
            x_min = x_min.min(dp.x);
            x_max = x_max.max(dp.x);
            y_min = y_min.min(dp.y);
            y_max = y_max.max(dp.y);
        }
    }

    if x_min > x_max {
        return (0.0, 1.0, 0.0, 1.0);
    }

    let y_range = y_max - y_min;
    if y_range.abs() < 1e-12 {
        let pad = if y_min.abs() < 1e-12 { 1.0 } else { y_min.abs() * 0.1 };
        y_min -= pad;
        y_max += pad;
    } else {
        let padding = y_range * 0.05;
        y_min -= padding;
        y_max += padding;
    }

    if (x_max - x_min).abs() < 1e-12 {
        x_min -= 1.0;
        x_max += 1.0;
    }

    (x_min, x_max, y_min, y_max)
}

pub fn catmull_rom_to_bezier(
    points: &[(f32, f32)],
    tension: f32,
) -> Vec<((f32, f32), (f32, f32), (f32, f32), (f32, f32))> {
    if points.len() < 2 {
        return Vec::new();
    }

    let n = points.len();
    let alpha = (1.0 - tension) / 6.0;
    let mut segments = Vec::with_capacity(n - 1);

    for i in 0..n - 1 {
        let p0 = if i == 0 { points[0] } else { points[i - 1] };
        let p1 = points[i];
        let p2 = points[i + 1];
        let p3 = if i + 2 < n { points[i + 2] } else { points[n - 1] };

        let cp1 = (
            p1.0 + alpha * (p2.0 - p0.0),
            p1.1 + alpha * (p2.1 - p0.1),
        );
        let cp2 = (
            p2.0 - alpha * (p3.0 - p1.0),
            p2.1 - alpha * (p3.1 - p1.1),
        );

        segments.push((p1, cp1, cp2, p2));
    }

    segments
}

pub fn segment_dashed(
    points: &[(f32, f32)],
    dash_len: f32,
    gap_len: f32,
) -> Vec<Vec<(f32, f32)>> {
    if points.len() < 2 || dash_len <= 0.0 {
        return vec![points.to_vec()];
    }

    let mut result: Vec<Vec<(f32, f32)>> = Vec::new();
    let mut current_segment: Vec<(f32, f32)> = Vec::new();
    let mut drawing = true;
    let mut remaining = dash_len;

    current_segment.push(points[0]);

    for i in 1..points.len() {
        let (x0, y0) = points[i - 1];
        let (x1, y1) = points[i];
        let dx = x1 - x0;
        let dy = y1 - y0;
        let seg_len = (dx * dx + dy * dy).sqrt();

        if seg_len < 1e-6 {
            continue;
        }

        let nx = dx / seg_len;
        let ny = dy / seg_len;
        let mut consumed = 0.0;

        while consumed < seg_len - 1e-6 {
            let available = seg_len - consumed;

            if remaining <= available {
                let px = x0 + nx * (consumed + remaining);
                let py = y0 + ny * (consumed + remaining);
                consumed += remaining;

                if drawing {
                    current_segment.push((px, py));
                    if current_segment.len() >= 2 {
                        result.push(current_segment);
                    }
                    current_segment = Vec::new();
                    drawing = false;
                    remaining = gap_len;
                } else {
                    current_segment.push((px, py));
                    drawing = true;
                    remaining = dash_len;
                }
            } else {
                remaining -= available;
                consumed = seg_len;

                if drawing {
                    current_segment.push((x1, y1));
                } else if remaining <= 1e-6 {
                    current_segment.push((x1, y1));
                    drawing = true;
                    remaining = dash_len;
                }
            }
        }
    }

    if drawing && current_segment.len() >= 2 {
        result.push(current_segment);
    }

    result
}

pub fn format_tick_value(value: f64) -> String {
    let abs = value.abs();
    if abs < 1e-10 {
        "0".to_string()
    } else if abs >= 1e6 {
        format!("{:.1}M", value / 1e6)
    } else if abs >= 1e3 {
        format!("{:.1}K", value / 1e3)
    } else if abs == abs.floor() && abs < 1e4 {
        format!("{:.0}", value)
    } else if abs >= 1.0 {
        format!("{:.1}", value)
    } else {
        format!("{:.2}", value)
    }
}

pub fn nearest_point_index(data: &[DataPoint], target_x: f64) -> Option<usize> {
    if data.is_empty() {
        return None;
    }
    if data.len() == 1 {
        return Some(0);
    }

    let mut lo = 0;
    let mut hi = data.len() - 1;
    while hi - lo > 1 {
        let mid = (lo + hi) / 2;
        if data[mid].x < target_x {
            lo = mid;
        } else {
            hi = mid;
        }
    }

    let d_lo = (data[lo].x - target_x).abs();
    let d_hi = (data[hi].x - target_x).abs();
    if d_lo <= d_hi {
        Some(lo)
    } else {
        Some(hi)
    }
}

pub fn polar_to_cartesian(cx: f32, cy: f32, r: f32, angle_rad: f32) -> (f32, f32) {
    (cx + r * angle_rad.cos(), cy - r * angle_rad.sin())
}

pub fn regular_polygon_points(cx: f32, cy: f32, r: f32, n: usize, start_angle: f32) -> Vec<(f32, f32)> {
    if n == 0 { return Vec::new(); }
    let step = std::f32::consts::TAU / n as f32;
    (0..n).map(|i| {
        let angle = start_angle + i as f32 * step;
        polar_to_cartesian(cx, cy, r, angle)
    }).collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_linear_scale() {
        let scale = LinearScale::new((0.0, 100.0), (0.0, 500.0));
        assert!((scale.map(0.0) - 0.0).abs() < 1e-4);
        assert!((scale.map(50.0) - 250.0).abs() < 1e-4);
        assert!((scale.map(100.0) - 500.0).abs() < 1e-4);
    }

    #[test]
    fn test_nice_number() {
        assert!((nice_number(11.0, true) - 10.0).abs() < 1e-6);
        assert!((nice_number(27.0, true) - 20.0).abs() < 1e-6);
        assert!((nice_number(0.073, true) - 0.1).abs() < 1e-8);
    }

    #[test]
    fn test_compute_ticks() {
        let ticks = compute_ticks(0.0, 100.0, 5);
        assert!(ticks.len() >= 3);
        assert!(ticks[0] <= 0.0);
        assert!(*ticks.last().unwrap() >= 100.0);
    }

    #[test]
    fn test_format_tick_value() {
        assert_eq!(format_tick_value(0.0), "0");
        assert_eq!(format_tick_value(42.0), "42");
        assert_eq!(format_tick_value(1500.0), "1.5K");
        assert_eq!(format_tick_value(2500000.0), "2.5M");
    }

    #[test]
    fn test_nearest_point() {
        let data = vec![
            DataPoint::new(1.0, 10.0),
            DataPoint::new(3.0, 20.0),
            DataPoint::new(5.0, 30.0),
            DataPoint::new(7.0, 40.0),
        ];
        assert_eq!(nearest_point_index(&data, 2.8), Some(1));
        assert_eq!(nearest_point_index(&data, 6.0), Some(2));
        assert_eq!(nearest_point_index(&data, 0.0), Some(0));
    }
}
