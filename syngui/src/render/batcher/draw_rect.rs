use crate::render::Vertex;
use super::Batcher;

impl Batcher {
    pub(super) fn add_rect(
        &mut self,
        rect: crate::core::Rect,
        color: crate::core::Color,
        radius: [f32; 4],
    ) {
        self.add_rect_with_border(rect, color, radius, 0.0, crate::core::Color::TRANSPARENT);
    }

    pub(super) fn add_rect_with_border(
        &mut self,
        rect: crate::core::Rect,
        color: crate::core::Color,
        radius: [f32; 4],
        border_width: f32,
        border_color: crate::core::Color,
    ) {
        let color_array = self.apply_opacity(color.to_array());
        let border_alpha = self.apply_opacity([0.0, 0.0, 0.0, border_color.a])[3];
        let origin = rect.origin;
        let size = rect.size;

        let has_radius = radius.iter().any(|r| *r > 0.5);

        let expand = if has_radius { 1.0_f32 } else { 0.0 };

        let [p0, p1, p2, p3] = self.transform_quad([
            [origin.x - expand, origin.y - expand],
            [origin.x + size.width + expand, origin.y - expand],
            [origin.x + size.width + expand, origin.y + size.height + expand],
            [origin.x - expand, origin.y + size.height + expand],
        ]);

        let u_min = if expand > 0.0 { -expand / size.width } else { 0.0 };
        let u_max = if expand > 0.0 { 1.0 + expand / size.width } else { 1.0 };
        let v_min = if expand > 0.0 { -expand / size.height } else { 0.0 };
        let v_max = if expand > 0.0 { 1.0 + expand / size.height } else { 1.0 };

        let sf = self.scale_factor;
        let scaled_radius = [
            radius[0] * sf,
            radius[1] * sf,
            radius[2] * sf,
            radius[3] * sf,
        ];

        let packed_rgb = if border_width > 0.0 {
            let ri = (border_color.r * 255.0).round() as u32;
            let gi = (border_color.g * 255.0).round() as u32;
            let bi = (border_color.b * 255.0).round() as u32;
            (ri * 65536 + gi * 256 + bi) as f32
        } else {
            0.0
        };
        let height_px = (size.height * sf).round();
        let scaled_border = [
            border_width * sf,
            packed_rgb,
            size.width * sf,
            height_px * 256.0 + (border_alpha * 255.0).round(),
        ];

        let state = self.current_batch_mut();
        let base = state.vertices.len() as u32;

        state.vertices.extend_from_slice(&[
            Vertex { position: p0, uv: [u_min, v_min], color: color_array, data: scaled_radius, data2: scaled_border },
            Vertex { position: p1, uv: [u_max, v_min], color: color_array, data: scaled_radius, data2: scaled_border },
            Vertex { position: p2, uv: [u_max, v_max], color: color_array, data: scaled_radius, data2: scaled_border },
            Vertex { position: p3, uv: [u_min, v_max], color: color_array, data: scaled_radius, data2: scaled_border },
        ]);
        state.indices.extend_from_slice(&[base, base + 1, base + 2, base, base + 2, base + 3]);
    }

    pub(super) fn add_outline(
        &mut self,
        rect: crate::core::Rect,
        color: crate::core::Color,
        radius: [f32; 4],
        ring_width: f32,
    ) {
        let sf = self.scale_factor;
        let color_array = self.apply_opacity(color.to_array());
        let origin = rect.origin;
        let size = rect.size;

        let [p0, p1, p2, p3] = self.transform_quad([
            [origin.x, origin.y],
            [origin.x + size.width, origin.y],
            [origin.x + size.width, origin.y + size.height],
            [origin.x, origin.y + size.height],
        ]);

        let scaled_radius = radius.map(|r| r * sf);
        let data2 = [-3.0_f32, 0.0, ring_width * sf, 0.0];

        let state = self.current_batch_mut();
        let base = state.vertices.len() as u32;
        state.vertices.extend_from_slice(&[
            Vertex { position: p0, uv: [0.0, 0.0], color: color_array, data: scaled_radius, data2 },
            Vertex { position: p1, uv: [1.0, 0.0], color: color_array, data: scaled_radius, data2 },
            Vertex { position: p2, uv: [1.0, 1.0], color: color_array, data: scaled_radius, data2 },
            Vertex { position: p3, uv: [0.0, 1.0], color: color_array, data: scaled_radius, data2 },
        ]);
        state.indices.extend_from_slice(&[base, base + 1, base + 2, base, base + 2, base + 3]);
    }

    pub(super) fn add_rect_per_side_border(
        &mut self,
        rect: crate::core::Rect,
        color: crate::core::Color,
        radius: [f32; 4],
        widths: [f32; 4],
        border_color: crate::core::Color,
        _uniform_border: Option<&crate::render::Border>,
    ) {
        let color_array = self.apply_opacity(color.to_array());
        let origin = rect.origin;
        let size = rect.size;

        let has_radius = radius.iter().any(|r| *r > 0.5);
        let expand = if has_radius { 1.0_f32 } else { 0.0 };

        let sf = self.scale_factor;

        let (qx0, qy0, qx1, qy1, snapped_size) = if !has_radius {
            let x0 = (origin.x * sf).round() / sf;
            let y0 = (origin.y * sf).round() / sf;
            let x1 = ((origin.x + size.width) * sf).round() / sf;
            let y1 = ((origin.y + size.height) * sf).round() / sf;
            (x0, y0, x1, y1, crate::core::Size::new(x1 - x0, y1 - y0))
        } else {
            (
                origin.x,
                origin.y,
                origin.x + size.width,
                origin.y + size.height,
                size,
            )
        };

        let [p0, p1, p2, p3] = self.transform_quad([
            [qx0 - expand, qy0 - expand],
            [qx1 + expand, qy0 - expand],
            [qx1 + expand, qy1 + expand],
            [qx0 - expand, qy1 + expand],
        ]);

        let data_payload = if has_radius {
            [radius[0] * sf, radius[1] * sf, radius[2] * sf, radius[3] * sf]
        } else {
            [(snapped_size.width * sf).round(), (snapped_size.height * sf).round(), 0.0, 0.0]
        };

        let ri = (border_color.r * 255.0).round() as u32;
        let gi = (border_color.g * 255.0).round() as u32;
        let bi = (border_color.b * 255.0).round() as u32;
        let packed_rgb = (ri * 65536 + gi * 256 + bi) as f32;

        let border_alpha = self.apply_opacity([0.0, 0.0, 0.0, border_color.a])[3];

        let mode_flag = if has_radius { 1.0 } else { 50.0 };

        let final_data2 = [
            -(mode_flag + border_alpha),
            packed_rgb,
            (widths[0] * sf).round() * 256.0 + (widths[1] * sf).round(),
            (widths[2] * sf).round() * 256.0 + (widths[3] * sf).round(),
        ];

        let u_min = if expand > 0.0 { -expand / size.width } else { 0.0 };
        let u_max = if expand > 0.0 { 1.0 + expand / size.width } else { 1.0 };
        let v_min = if expand > 0.0 { -expand / size.height } else { 0.0 };
        let v_max = if expand > 0.0 { 1.0 + expand / size.height } else { 1.0 };

        let state = self.current_batch_mut();
        let base = state.vertices.len() as u32;

        state.vertices.extend_from_slice(&[
            Vertex { position: p0, uv: [u_min, v_min], color: color_array, data: data_payload, data2: final_data2 },
            Vertex { position: p1, uv: [u_max, v_min], color: color_array, data: data_payload, data2: final_data2 },
            Vertex { position: p2, uv: [u_max, v_max], color: color_array, data: data_payload, data2: final_data2 },
            Vertex { position: p3, uv: [u_min, v_max], color: color_array, data: data_payload, data2: final_data2 },
        ]);
        state.indices.extend_from_slice(&[base, base + 1, base + 2, base, base + 2, base + 3]);
    }
}
