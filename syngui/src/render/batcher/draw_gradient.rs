use crate::render::Vertex;
use super::Batcher;

impl Batcher {
    pub(super) fn add_linear_gradient_rect(
        &mut self,
        rect: crate::core::Rect,
        gradient: &crate::core::Gradient,
        corner_radius: [f32; 4],
        border: Option<&crate::render::Border>,
        _per_side_border: Option<&crate::render::PerSideBorder>,
    ) {
        use crate::core::Gradient;

        let stops = gradient.resolved_stops();
        if stops.len() < 2 {
            let c = stops.first().map(|(c, _)| *c).unwrap_or(crate::core::Color::TRANSPARENT);
            if let Some(b) = border {
                self.add_rect_with_border(rect, c, corner_radius, b.width, b.color);
            } else {
                self.add_rect(rect, c, corner_radius);
            }
            return;
        }

        match gradient {
            Gradient::Linear { angle_deg, .. } => {
                self.add_linear_gradient_strips(rect, &stops, *angle_deg, corner_radius, border, gradient);
            }
            Gradient::Radial { .. } | Gradient::Conic { .. } => {
                self.add_radial_gradient_approx(rect, gradient, corner_radius, border);
            }
        }
    }

    pub(super) fn add_linear_gradient_strips(
        &mut self,
        rect: crate::core::Rect,
        stops: &[(crate::core::Color, f32)],
        angle_deg: f32,
        corner_radius: [f32; 4],
        border: Option<&crate::render::Border>,
        gradient: &crate::core::Gradient,
    ) {
        let origin = rect.origin;
        let size = rect.size;
        let sf = self.scale_factor;

        let angle_rad = angle_deg.to_radians();
        let dir_x = angle_rad.sin();
        let dir_y = -angle_rad.cos();

        let is_horizontal = (dir_y.abs() < 0.001) && (dir_x.abs() > 0.999);
        let is_vertical = (dir_x.abs() < 0.001) && (dir_y.abs() > 0.999);

        if is_horizontal || is_vertical {
            for i in 0..stops.len() - 1 {
                let (c0, p0) = stops[i];
                let (c1, p1) = stops[i + 1];

                let strip_rect;
                let strip_uv_start;
                let strip_uv_end;

                if is_horizontal {
                    let (start_t, end_t, start_c, end_c) = if dir_x > 0.0 {
                        (p0, p1, c0, c1)
                    } else {
                        (1.0 - p1, 1.0 - p0, c1, c0)
                    };
                    let x0 = origin.x + start_t * size.width;
                    let x1 = origin.x + end_t * size.width;
                    strip_rect = crate::core::Rect::new(
                        crate::core::Point::new(x0, origin.y),
                        crate::core::Size::new(x1 - x0, size.height),
                    );
                    strip_uv_start = [start_t, 0.0];
                    strip_uv_end = [end_t, 1.0];
                    self.add_gradient_strip_quad(
                        strip_rect, start_c, end_c, corner_radius,
                        border, i, stops.len() - 1,
                        strip_uv_start, strip_uv_end, true, size,
                    );
                } else {
                    let (start_t, end_t, start_c, end_c) = if dir_y > 0.0 {
                        (p0, p1, c0, c1)
                    } else {
                        (1.0 - p1, 1.0 - p0, c1, c0)
                    };
                    let y0 = origin.y + start_t * size.height;
                    let y1 = origin.y + end_t * size.height;
                    strip_rect = crate::core::Rect::new(
                        crate::core::Point::new(origin.x, y0),
                        crate::core::Size::new(size.width, y1 - y0),
                    );
                    strip_uv_start = [0.0, start_t];
                    strip_uv_end = [1.0, end_t];
                    self.add_gradient_strip_quad(
                        strip_rect, start_c, end_c, corner_radius,
                        border, i, stops.len() - 1,
                        strip_uv_start, strip_uv_end, false, size,
                    );
                }
            }
        } else {
            let project = |x: f32, y: f32| -> f32 {
                let cx = (x - origin.x) / size.width - 0.5;
                let cy = (y - origin.y) / size.height - 0.5;
                (cx * dir_x + cy * dir_y) + 0.5
            };

            let t_tl = project(origin.x, origin.y);
            let t_tr = project(origin.x + size.width, origin.y);
            let t_br = project(origin.x + size.width, origin.y + size.height);
            let t_bl = project(origin.x, origin.y + size.height);

            let sample = |t: f32| -> crate::core::Color { gradient.sample(t) };

            let c_tl = self.apply_opacity(sample(t_tl).to_array());
            let c_tr = self.apply_opacity(sample(t_tr).to_array());
            let c_br = self.apply_opacity(sample(t_br).to_array());
            let c_bl = self.apply_opacity(sample(t_bl).to_array());

            let [p0, p1, p2, p3] = self.transform_quad([
                [origin.x, origin.y],
                [origin.x + size.width, origin.y],
                [origin.x + size.width, origin.y + size.height],
                [origin.x, origin.y + size.height],
            ]);

            let scaled_radius = [
                corner_radius[0] * sf,
                corner_radius[1] * sf,
                corner_radius[2] * sf,
                corner_radius[3] * sf,
            ];

            let diag_height_px = (size.height * sf).round();
            let diag_packed_hw = diag_height_px * 256.0 + 255.0;
            let border_data = if let Some(b) = border {
                let packed_rgb = {
                    let ri = (b.color.r * 255.0).round() as u32;
                    let gi = (b.color.g * 255.0).round() as u32;
                    let bi = (b.color.b * 255.0).round() as u32;
                    (ri * 65536 + gi * 256 + bi) as f32
                };
                [b.width * sf, packed_rgb, size.width * sf, diag_packed_hw]
            } else {
                [0.0, 0.0, size.width * sf, diag_packed_hw]
            };

            let state = self.current_batch_mut();
            let base = state.vertices.len() as u32;
            state.vertices.extend_from_slice(&[
                Vertex { position: p0, uv: [0.0, 0.0], color: c_tl, data: scaled_radius, data2: border_data },
                Vertex { position: p1, uv: [1.0, 0.0], color: c_tr, data: scaled_radius, data2: border_data },
                Vertex { position: p2, uv: [1.0, 1.0], color: c_br, data: scaled_radius, data2: border_data },
                Vertex { position: p3, uv: [0.0, 1.0], color: c_bl, data: scaled_radius, data2: border_data },
            ]);
            state.indices.extend_from_slice(&[base, base + 1, base + 2, base, base + 2, base + 3]);
        }
    }

    pub(super) fn add_gradient_strip_quad(
        &mut self,
        rect: crate::core::Rect,
        start_color: crate::core::Color,
        end_color: crate::core::Color,
        corner_radius: [f32; 4],
        border: Option<&crate::render::Border>,
        strip_idx: usize,
        total_strips: usize,
        strip_uv_start: [f32; 2],
        strip_uv_end: [f32; 2],
        horizontal: bool,
        full_size: crate::core::Size,
    ) {
        let sf = self.scale_factor;
        let c_start = self.apply_opacity(start_color.to_array());
        let c_end = self.apply_opacity(end_color.to_array());

        let origin = rect.origin;
        let size = rect.size;

        let [p0, p1, p2, p3] = self.transform_quad([
            [origin.x, origin.y],
            [origin.x + size.width, origin.y],
            [origin.x + size.width, origin.y + size.height],
            [origin.x, origin.y + size.height],
        ]);

        let scaled_radius = corner_radius.map(|r| r * sf);

        let height_px = (full_size.height * sf).round();
        let packed_hw = height_px * 256.0 + 255.0;

        let border_data = if let Some(b) = border {
            let has_border = (strip_idx == 0 || strip_idx == total_strips - 1) || total_strips <= 1;
            if has_border {
                let packed_rgb = {
                    let ri = (b.color.r * 255.0).round() as u32;
                    let gi = (b.color.g * 255.0).round() as u32;
                    let bi = (b.color.b * 255.0).round() as u32;
                    (ri * 65536 + gi * 256 + bi) as f32
                };
                [b.width * sf, packed_rgb, full_size.width * sf, packed_hw]
            } else {
                [0.0, 0.0, full_size.width * sf, packed_hw]
            }
        } else {
            [0.0, 0.0, full_size.width * sf, packed_hw]
        };

        let (c_tl, c_tr, c_br, c_bl) = if horizontal {
            (c_start, c_end, c_end, c_start)
        } else {
            (c_start, c_start, c_end, c_end)
        };

        let state = self.current_batch_mut();
        let base = state.vertices.len() as u32;
        state.vertices.extend_from_slice(&[
            Vertex { position: p0, uv: [strip_uv_start[0], strip_uv_start[1]], color: c_tl, data: scaled_radius, data2: border_data },
            Vertex { position: p1, uv: [strip_uv_end[0], strip_uv_start[1]], color: c_tr, data: scaled_radius, data2: border_data },
            Vertex { position: p2, uv: [strip_uv_end[0], strip_uv_end[1]], color: c_br, data: scaled_radius, data2: border_data },
            Vertex { position: p3, uv: [strip_uv_start[0], strip_uv_end[1]], color: c_bl, data: scaled_radius, data2: border_data },
        ]);
        state.indices.extend_from_slice(&[base, base + 1, base + 2, base, base + 2, base + 3]);
    }

    pub(super) fn add_radial_gradient_approx(
        &mut self,
        rect: crate::core::Rect,
        gradient: &crate::core::Gradient,
        _corner_radius: [f32; 4],
        _border: Option<&crate::render::Border>,
    ) {
        let origin = rect.origin;
        let size = rect.size;

        let grid = match gradient {
            crate::core::Gradient::Radial { quality, .. } => *quality as usize,
            crate::core::Gradient::Conic { quality, .. } => *quality as usize,
            _ => crate::core::GRADIENT_DEFAULT_QUALITY as usize,
        }.max(4);
        let cols = grid + 1;

        let (cx, cy) = match gradient {
            crate::core::Gradient::Radial { center, .. } => *center,
            crate::core::Gradient::Conic { center, .. } => *center,
            _ => (0.5, 0.5),
        };

        let is_circle = matches!(gradient,
            crate::core::Gradient::Radial { shape: crate::core::GradientShape::Circle, .. }
        );
        let is_conic = matches!(gradient, crate::core::Gradient::Conic { .. });
        let conic_from_angle = match gradient {
            crate::core::Gradient::Conic { from_angle, .. } => from_angle.to_radians(),
            _ => 0.0,
        };

        let half_min = size.width.min(size.height) / 2.0;

        let scaled_radius = [0.0f32; 4];
        let border_data = [0.0, 0.0, 0.0, 0.0];

        let transform = self.current_transform;
        let opacity = self.current_opacity;

        let mut verts = Vec::with_capacity(cols * cols);
        for row in 0..=grid {
            for col in 0..=grid {
                let u = col as f32 / grid as f32;
                let v = row as f32 / grid as f32;
                let px = origin.x + u * size.width;
                let py = origin.y + v * size.height;
                let pos = if transform == crate::core::Transform::identity() {
                    [px, py]
                } else {
                    let p = transform.transform_point(euclid::Point2D::new(px, py));
                    [p.x, p.y]
                };

                let t = if is_conic {
                    let dx = u - cx;
                    let dy = v - cy;
                    let angle = dy.atan2(dx) + std::f32::consts::PI - conic_from_angle;
                    (angle / std::f32::consts::TAU).rem_euclid(1.0)
                } else if is_circle {
                    let px_dx = (u - cx) * size.width;
                    let px_dy = (v - cy) * size.height;
                    (px_dx * px_dx + px_dy * px_dy).sqrt() / half_min
                } else {
                    let ndx = (u - cx) * 2.0;
                    let ndy = (v - cy) * 2.0;
                    (ndx * ndx + ndy * ndy).sqrt()
                };

                let color = gradient.sample(t.clamp(0.0, 1.0));
                let mut ca = color.to_array();
                ca[3] *= opacity;

                verts.push(Vertex {
                    position: pos,
                    uv: [u, v],
                    color: ca,
                    data: scaled_radius,
                    data2: border_data,
                });
            }
        }

        let state = self.current_batch_mut();
        let base = state.vertices.len() as u32;
        state.vertices.extend(verts);

        for row in 0..grid {
            for col in 0..grid {
                let tl = base + (row * cols + col) as u32;
                let tr = tl + 1;
                let bl = base + ((row + 1) * cols + col) as u32;
                let br = bl + 1;
                state.indices.extend_from_slice(&[tl, tr, br, tl, br, bl]);
            }
        }
    }
}
