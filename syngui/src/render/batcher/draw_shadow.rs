use crate::render::Vertex;
use super::Batcher;

impl Batcher {
    pub(super) fn add_shadow(
        &mut self,
        rect: crate::core::Rect,
        color: crate::core::Color,
        blur_radius: f32,
        offset: (f32, f32),
        corner_radius: [f32; 4],
    ) {
        let color_array = self.apply_opacity(color.to_array());

        let expanded_x = rect.origin.x - blur_radius + offset.0;
        let expanded_y = rect.origin.y - blur_radius + offset.1;
        let expanded_w = rect.size.width + blur_radius * 2.0;
        let expanded_h = rect.size.height + blur_radius * 2.0;

        let avg_radius = (corner_radius[0] + corner_radius[1] + corner_radius[2] + corner_radius[3]) / 4.0;
        let data = [blur_radius, avg_radius, rect.size.width, rect.size.height];

        let [p0, p1, p2, p3] = self.transform_quad([
            [expanded_x, expanded_y],
            [expanded_x + expanded_w, expanded_y],
            [expanded_x + expanded_w, expanded_y + expanded_h],
            [expanded_x, expanded_y + expanded_h],
        ]);

        let state = self.current_batch_mut();
        let base = state.vertices.len() as u32;
        state.vertices.extend_from_slice(&[
            Vertex { position: p0, uv: [0.0, 0.0], color: color_array, data, data2: [0.0; 4] },
            Vertex { position: p1, uv: [1.0, 0.0], color: color_array, data, data2: [0.0; 4] },
            Vertex { position: p2, uv: [1.0, 1.0], color: color_array, data, data2: [0.0; 4] },
            Vertex { position: p3, uv: [0.0, 1.0], color: color_array, data, data2: [0.0; 4] },
        ]);
        state.indices.extend_from_slice(&[base, base + 1, base + 2, base, base + 2, base + 3]);
    }

    pub(super) fn add_inner_shadow(
        &mut self,
        rect: crate::core::Rect,
        color: crate::core::Color,
        blur_radius: f32,
        offset: (f32, f32),
        corner_radius: [f32; 4],
    ) {
        let color_array = self.apply_opacity(color.to_array());
        let avg_radius = (corner_radius[0] + corner_radius[1] + corner_radius[2] + corner_radius[3]) / 4.0;
        let data = [blur_radius, avg_radius, rect.size.width, rect.size.height];
        let data2 = [offset.0, offset.1, 0.0, 0.0];

        let x = rect.origin.x;
        let y = rect.origin.y;
        let w = rect.size.width;
        let h = rect.size.height;

        let [p0, p1, p2, p3] = self.transform_quad([
            [x, y],
            [x + w, y],
            [x + w, y + h],
            [x, y + h],
        ]);

        let state = self.current_batch_mut();
        let base = state.vertices.len() as u32;
        state.vertices.extend_from_slice(&[
            Vertex { position: p0, uv: [0.0, 0.0], color: color_array, data, data2 },
            Vertex { position: p1, uv: [1.0, 0.0], color: color_array, data, data2 },
            Vertex { position: p2, uv: [1.0, 1.0], color: color_array, data, data2 },
            Vertex { position: p3, uv: [0.0, 1.0], color: color_array, data, data2 },
        ]);
        state.indices.extend_from_slice(&[base, base + 1, base + 2, base, base + 2, base + 3]);
    }
}
