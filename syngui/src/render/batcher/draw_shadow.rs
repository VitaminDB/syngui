use super::Batcher;
use crate::render::Vertex;

impl Batcher {
    /// Внешняя тень. `cutout` — не рисовать её внутри бокса самого элемента
    /// (как `box-shadow` в CSS): тени собираются в первый теневой батч группы
    /// и могут лечь после заливки элемента — без выреза они затемняли бы его,
    /// а сквозь полупрозрачный фон просвечивали бы. Свечение (`GlowShadow`)
    /// рисуется и поверх элемента, ему вырез не нужен.
    pub(super) fn add_shadow(
        &mut self,
        rect: crate::core::Rect,
        color: crate::core::Color,
        blur_radius: f32,
        offset: (f32, f32),
        corner_radius: [f32; 4],
        cutout: bool,
    ) {
        let color_array = self.apply_opacity(color.to_array());
        let data2 = [offset.0, offset.1, if cutout { 1.0 } else { 0.0 }, 0.0];

        // 3σ (σ = blur/2, как в CSS) — дальше тень меньше 0,2 %; тот же запас в shadow.wgsl
        let pad = blur_radius.max(0.0) * 1.5;
        let expanded_x = rect.origin.x - pad + offset.0;
        let expanded_y = rect.origin.y - pad + offset.1;
        let expanded_w = rect.size.width + pad * 2.0;
        let expanded_h = rect.size.height + pad * 2.0;

        let avg_radius =
            (corner_radius[0] + corner_radius[1] + corner_radius[2] + corner_radius[3]) / 4.0;
        let data = [blur_radius, avg_radius, rect.size.width, rect.size.height];

        // Вырез бокса: внутри элемента (сжатого на радиус скругления и пиксель) тень нулевая —
        // рисуем рамку из четырёх квадратов вокруг, а не квадрат целиком: на Adreno 505 тени карточек
        // стола считали exp/sqrt по всей их площади каждый кадр листания.
        let (ox0, oy0, ox1, oy1) = (expanded_x, expanded_y, expanded_x + expanded_w, expanded_y + expanded_h);
        let rmax = corner_radius.iter().copied().fold(0.0f32, f32::max);
        let inset = rmax + 1.0;
        let (ix0, iy0) = ((rect.origin.x + inset).max(ox0), (rect.origin.y + inset).max(oy0));
        let (ix1, iy1) = ((rect.origin.x + rect.size.width - inset).min(ox1), (rect.origin.y + rect.size.height - inset).min(oy1));
        let ring = cutout && ix1 - ix0 > 8.0 && iy1 - iy0 > 8.0;
        let quads: Vec<[f32; 4]> = if ring {
            vec![[ox0, oy0, ox1, iy0], [ox0, iy1, ox1, oy1], [ox0, iy0, ix0, iy1], [ix1, iy0, ox1, iy1]]
        } else {
            vec![[ox0, oy0, ox1, oy1]]
        };
        let color_array = if self.srgb_vertex { super::encode_srgb(color_array) } else { color_array };
        for [x0, y0, x1, y1] in quads {
            if x1 - x0 <= 0.0 || y1 - y0 <= 0.0 {
                continue;
            }
            let uv = |x: f32, y: f32| [(x - ox0) / expanded_w.max(1e-3), (y - oy0) / expanded_h.max(1e-3)];
            let [p0, p1, p2, p3] = self.transform_quad([[x0, y0], [x1, y0], [x1, y1], [x0, y1]]);
            let state = self.current_batch_mut();
            let base = state.vertices.len() as u32;
            state.vertices.extend_from_slice(&[
                Vertex { position: p0, uv: uv(x0, y0), color: color_array, data, data2 },
                Vertex { position: p1, uv: uv(x1, y0), color: color_array, data, data2 },
                Vertex { position: p2, uv: uv(x1, y1), color: color_array, data, data2 },
                Vertex { position: p3, uv: uv(x0, y1), color: color_array, data, data2 },
            ]);
            state.indices.extend_from_slice(&[base, base + 1, base + 2, base, base + 2, base + 3]);
        }
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
        let avg_radius =
            (corner_radius[0] + corner_radius[1] + corner_radius[2] + corner_radius[3]) / 4.0;
        let data = [blur_radius, avg_radius, rect.size.width, rect.size.height];
        let data2 = [offset.0, offset.1, 0.0, 0.0];

        let x = rect.origin.x;
        let y = rect.origin.y;
        let w = rect.size.width;
        let h = rect.size.height;

        let [p0, p1, p2, p3] =
            self.transform_quad([[x, y], [x + w, y], [x + w, y + h], [x, y + h]]);

        let state = self.current_batch_mut();
        let base = state.vertices.len() as u32;
        state.vertices.extend_from_slice(&[
            Vertex {
                position: p0,
                uv: [0.0, 0.0],
                color: color_array,
                data,
                data2,
            },
            Vertex {
                position: p1,
                uv: [1.0, 0.0],
                color: color_array,
                data,
                data2,
            },
            Vertex {
                position: p2,
                uv: [1.0, 1.0],
                color: color_array,
                data,
                data2,
            },
            Vertex {
                position: p3,
                uv: [0.0, 1.0],
                color: color_array,
                data,
                data2,
            },
        ]);
        state
            .indices
            .extend_from_slice(&[base, base + 1, base + 2, base, base + 2, base + 3]);
    }
}
