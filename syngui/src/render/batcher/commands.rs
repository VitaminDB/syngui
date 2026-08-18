use crate::render::{DrawCommand, RenderOp, ShaderType};
use crate::text::FontAtlas;

use super::Batcher;

impl Batcher {
    pub(super) fn process_command(&mut self, cmd: &DrawCommand, font_atlas: &mut FontAtlas) {
        match cmd {
            DrawCommand::Rect { rect, color, corner_radius, border, per_side_border, clip_rect, .. } => {
                self.ensure_batch(ShaderType::Rect, None, *clip_rect);
                if let Some(psb) = per_side_border {
                    let bc = psb.color;
                    self.add_rect_per_side_border(*rect, *color, *corner_radius, psb.widths, bc, border.as_ref());
                } else if let Some(border) = border {
                    self.add_rect_with_border(*rect, *color, *corner_radius, border.width, border.color);
                } else {
                    self.add_rect(*rect, *color, *corner_radius);
                }
            }
            DrawCommand::Outline { rect, color, ring_width, corner_radius, clip_rect, .. } => {
                self.ensure_batch(ShaderType::Rect, None, *clip_rect);
                self.add_outline(*rect, *color, *corner_radius, *ring_width);
            }
            DrawCommand::GradientRect { rect, gradient, corner_radius, border, per_side_border, clip_rect, .. } => {
                self.ensure_batch(ShaderType::Rect, None, *clip_rect);
                self.add_linear_gradient_rect(*rect, gradient, *corner_radius, border.as_ref(), per_side_border.as_ref());
            }
            DrawCommand::Text { text, rect, color, font_size, font_weight, text_align, decoration, font_family, letter_spacing, text_shadow, bbox_sample, clip_rect, no_wrap, .. } => {
                if text.is_empty() {
                    return;
                }
                let sf = self.scale_factor;
                let phys_font_size = ((*font_size * sf) as u16).max(1);
                let phys_max_width = if *no_wrap { 0.0 } else { rect.size.width * sf };
                let bold = *font_weight >= 700;
                let phys_letter_spacing = *letter_spacing * sf;
                let glyphs = self.shape_text_cached_spacing(font_atlas, text, phys_font_size, phys_max_width, bold, font_family.as_deref(), phys_letter_spacing);
                if glyphs.is_empty() {
                    return;
                }

                use crate::render::Vertex;
                let mut glyph_min_y = f32::MAX;
                let mut glyph_max_y = f32::MIN;
                let mut glyph_min_x = f32::MAX;
                let mut glyph_max_x = f32::MIN;
                if let Some(sample) = bbox_sample.as_ref() {
                    let sample_glyphs = self.shape_text_cached_spacing(
                        font_atlas,
                        sample.as_str(),
                        phys_font_size,
                        f32::INFINITY,
                        false,
                        font_family.as_deref(),
                        phys_letter_spacing,
                    );
                    for glyph in sample_glyphs.iter() {
                        if glyph.glyph.width == 0 || glyph.glyph.height == 0 { continue; }
                        let gy = glyph.y / sf;
                        let gh = glyph.glyph.height as f32 / sf;
                        glyph_min_y = glyph_min_y.min(gy);
                        glyph_max_y = glyph_max_y.max(gy + gh);
                    }
                    for glyph in glyphs.iter() {
                        if glyph.glyph.width == 0 || glyph.glyph.height == 0 { continue; }
                        let gx = glyph.x / sf;
                        let gw = glyph.glyph.width as f32 / sf;
                        glyph_min_x = glyph_min_x.min(gx);
                        glyph_max_x = glyph_max_x.max(gx + gw);
                    }
                } else {
                    for glyph in glyphs.iter() {
                        if glyph.glyph.width == 0 || glyph.glyph.height == 0 { continue; }
                        let gy = glyph.y / sf;
                        let gh = glyph.glyph.height as f32 / sf;
                        glyph_min_y = glyph_min_y.min(gy);
                        glyph_max_y = glyph_max_y.max(gy + gh);
                        let gx = glyph.x / sf;
                        let gw = glyph.glyph.width as f32 / sf;
                        glyph_min_x = glyph_min_x.min(gx);
                        glyph_max_x = glyph_max_x.max(gx + gw);
                    }
                }
                let text_height = glyph_max_y - glyph_min_y;
                let text_width = glyph_max_x - glyph_min_x;

                let mut origin_y = if rect.size.height > 0.0 && text_height > 0.0 {
                    if text_align.is_top() {
                        rect.origin.y - glyph_min_y
                    } else if text_align.is_bottom() {
                        rect.origin.y + rect.size.height - text_height - glyph_min_y
                    } else {
                        rect.origin.y + (rect.size.height - text_height) / 2.0 - glyph_min_y
                    }
                } else {
                    rect.origin.y
                };

                if bbox_sample.is_some() && sf > 0.0 {
                    origin_y = (origin_y * sf).round() / sf;
                }

                let mut origin_x = if text_align.is_hcenter() && rect.size.width > text_width && text_width > 0.0 {
                    rect.origin.x + (rect.size.width - text_width) / 2.0 - glyph_min_x
                } else if text_align.is_right() && rect.size.width > text_width && text_width > 0.0 {
                    rect.origin.x + rect.size.width - text_width - glyph_min_x
                } else {
                    rect.origin.x
                };

                if bbox_sample.is_some() && sf > 0.0 {
                    origin_x = (origin_x * sf).round() / sf;
                }

                if let Some(shadow) = text_shadow {
                    self.ensure_batch(ShaderType::Text, None, *clip_rect);
                    let shadow_color = self.apply_opacity(shadow.color.to_array());
                    let shadow_offset_x = shadow.offset_x;
                    let shadow_offset_y = shadow.offset_y;
                    let blur = shadow.blur_radius.max(0.0);
                    let pad = blur.ceil();

                    for glyph in glyphs.iter() {
                        if glyph.glyph.width == 0 || glyph.glyph.height == 0 { continue; }
                        let gx = origin_x + glyph.x / sf + shadow_offset_x;
                        let gy = origin_y + glyph.y / sf + shadow_offset_y;
                        let gw = glyph.glyph.width as f32 / sf;
                        let gh = glyph.glyph.height as f32 / sf;
                        let (x, y, w, h) = if pad > 0.0 {
                            (gx - pad, gy - pad, gw + 2.0 * pad, gh + 2.0 * pad)
                        } else {
                            (gx, gy, gw, gh)
                        };
                        let uv_x0 = glyph.glyph.uv_x;
                        let uv_y0 = glyph.glyph.uv_y;
                        let uv_w = glyph.glyph.uv_w;
                        let uv_h = glyph.glyph.uv_h;
                        let uv_pad_x = if gw > 0.0 { pad * (uv_w / gw) } else { 0.0 };
                        let uv_pad_y = if gh > 0.0 { pad * (uv_h / gh) } else { 0.0 };
                        let uv_min_x = uv_x0;
                        let uv_min_y = uv_y0;
                        let uv_max_x = uv_x0 + uv_w;
                        let uv_max_y = uv_y0 + uv_h;
                        let uv_qx0 = uv_x0 - uv_pad_x;
                        let uv_qy0 = uv_y0 - uv_pad_y;
                        let uv_qw = uv_w + 2.0 * uv_pad_x;
                        let uv_qh = uv_h + 2.0 * uv_pad_y;
                        let is_color_flag = if glyph.glyph.is_color { 1.0 } else { 0.0 };
                        let d = [is_color_flag, blur, 0.0, 0.0];
                        let d2 = [uv_min_x, uv_min_y, uv_max_x, uv_max_y];
                        let [p0, p1, p2, p3] = self.transform_quad([
                            [x, y], [x + w, y], [x + w, y + h], [x, y + h],
                        ]);
                        let state = self.current_batch_mut();
                        let base = state.vertices.len() as u32;
                        state.vertices.extend_from_slice(&[
                            Vertex { position: p0, uv: [uv_qx0, uv_qy0], color: shadow_color, data: d, data2: d2 },
                            Vertex { position: p1, uv: [uv_qx0 + uv_qw, uv_qy0], color: shadow_color, data: d, data2: d2 },
                            Vertex { position: p2, uv: [uv_qx0 + uv_qw, uv_qy0 + uv_qh], color: shadow_color, data: d, data2: d2 },
                            Vertex { position: p3, uv: [uv_qx0, uv_qy0 + uv_qh], color: shadow_color, data: d, data2: d2 },
                        ]);
                        state.indices.extend_from_slice(&[base, base + 1, base + 2, base, base + 2, base + 3]);
                    }
                }

                self.ensure_batch(ShaderType::Text, None, *clip_rect);
                let color_array = self.apply_opacity(color.to_array());

                for glyph in glyphs.iter() {
                    if glyph.glyph.width == 0 || glyph.glyph.height == 0 { continue; }
                    let x = origin_x + glyph.x / sf;
                    let y = origin_y + glyph.y / sf;
                    let w = glyph.glyph.width as f32 / sf;
                    let h = glyph.glyph.height as f32 / sf;
                    let uv_x = glyph.glyph.uv_x;
                    let uv_y = glyph.glyph.uv_y;
                    let uv_w = glyph.glyph.uv_w;
                    let uv_h = glyph.glyph.uv_h;
                    let is_color_flag = if glyph.glyph.is_color { 1.0 } else { 0.0 };
                    let [p0, p1, p2, p3] = self.transform_quad([
                        [x, y], [x + w, y], [x + w, y + h], [x, y + h],
                    ]);
                    let state = self.current_batch_mut();
                    let base = state.vertices.len() as u32;
                    state.vertices.extend_from_slice(&[
                        Vertex { position: p0, uv: [uv_x, uv_y], color: color_array, data: [is_color_flag, 0.0, 0.0, 0.0], data2: [0.0; 4] },
                        Vertex { position: p1, uv: [uv_x + uv_w, uv_y], color: color_array, data: [is_color_flag, 0.0, 0.0, 0.0], data2: [0.0; 4] },
                        Vertex { position: p2, uv: [uv_x + uv_w, uv_y + uv_h], color: color_array, data: [is_color_flag, 0.0, 0.0, 0.0], data2: [0.0; 4] },
                        Vertex { position: p3, uv: [uv_x, uv_y + uv_h], color: color_array, data: [is_color_flag, 0.0, 0.0, 0.0], data2: [0.0; 4] },
                    ]);
                    state.indices.extend_from_slice(&[base, base + 1, base + 2, base, base + 2, base + 3]);
                }

                if *decoration != crate::mss::TextDecoration::None && text_width > 0.0 {
                    self.ensure_batch(ShaderType::Rect, None, *clip_rect);
                    let line_thickness = (*font_size * 0.07).max(1.0);
                    let line_y = match decoration {
                        crate::mss::TextDecoration::Underline => origin_y + text_height + 2.0,
                        crate::mss::TextDecoration::LineThrough => origin_y + text_height * 0.5,
                        _ => origin_y,
                    };
                    let line_rect = crate::core::Rect::new(
                        crate::core::Point::new(origin_x, line_y),
                        crate::core::Size::new(text_width, line_thickness),
                    );
                    let line_color = crate::core::Color::new(color_array[0], color_array[1], color_array[2], color_array[3]);
                    self.add_rect(line_rect, line_color, [0.0; 4]);
                }
            }
            DrawCommand::Shadow { rect, color, blur_radius, offset, corner_radius, inset, clip_rect, .. } => {
                if !self.buckets.is_empty() {
                    self.flush_all_buckets();
                }
                if *inset {
                    self.ensure_batch(ShaderType::InnerShadow, None, *clip_rect);
                    self.add_inner_shadow(*rect, *color, *blur_radius, *offset, *corner_radius);
                } else {
                    self.ensure_batch(ShaderType::Shadow, None, *clip_rect);
                    self.add_shadow(*rect, *color, *blur_radius, *offset, *corner_radius);
                }
            }
            DrawCommand::GlowShadow { rect, color, blur_radius, offset, corner_radius, clip_rect, .. } => {
                if !self.buckets.is_empty() {
                    self.flush_all_buckets();
                }
                self.ensure_batch(ShaderType::GlowShadow, None, *clip_rect);
                self.add_shadow(*rect, *color, *blur_radius, *offset, *corner_radius);
            }
            DrawCommand::TextSelection { text, sel_start, sel_end, base_x, y, height, font_size, color, font_family, clip_rect, .. } => {
                let sf = self.scale_factor;
                let phys_font_size = ((*font_size * sf) as u16).max(1);
                let ff = font_family.as_deref();
                let start_char_count = text[..*sel_start].chars().count();
                let text_before_start = &text[..*sel_start];
                let start_x_offset = font_atlas.measure_text_width(text_before_start, phys_font_size, start_char_count, ff);
                let start_x = *base_x + start_x_offset / sf;
                let end_char_count = text[..*sel_end].chars().count();
                let text_before_end = &text[..*sel_end];
                let end_x_offset = font_atlas.measure_text_width(text_before_end, phys_font_size, end_char_count, ff);
                let end_x = *base_x + end_x_offset / sf;
                let sel_width = (end_x - start_x).max(1.0);
                self.ensure_batch(ShaderType::Rect, None, *clip_rect);
                let sel_rect = crate::core::Rect::new(
                    crate::core::Point::new(start_x, *y),
                    crate::core::Size::new(sel_width, *height),
                );
                self.add_rect(sel_rect, *color, [2.0; 4]);
            }
            DrawCommand::TextCursor { text, cursor_pos, base_x, y, height, font_size, font_weight, color, font_family, clip_rect, .. } => {
                let sf = self.scale_factor;
                let phys_font_size = (*font_size * sf) as u16;
                let bold = *font_weight >= 600;
                let byte_pos = (*cursor_pos).min(text.len());
                let text_before_cursor = &text[..byte_pos];
                let char_count = text_before_cursor.chars().count();
                let cursor_x_offset = font_atlas.measure_text_width_styled(text_before_cursor, phys_font_size, char_count, bold, font_family.as_deref());
                let cursor_x = *base_x + cursor_x_offset / sf;
                self.ensure_batch(ShaderType::Rect, None, *clip_rect);
                let cursor_rect = crate::core::Rect::new(
                    crate::core::Point::new(cursor_x, *y),
                    crate::core::Size::new(1.5, *height),
                );
                self.add_rect(cursor_rect, *color, [0.0; 4]);
            }
            DrawCommand::PushClip { .. } => {
                self.flush_all_buckets();
            }
            DrawCommand::PopClip => {
                self.flush_all_buckets();
            }
            DrawCommand::ZBarrier => {
                self.flush_all_buckets();
            }
            DrawCommand::PushTransform(transform) => {
                self.transform_stack.push(self.current_transform);
                self.current_transform = transform.then(&self.current_transform);
            }
            DrawCommand::PopTransform => {
                if let Some(prev) = self.transform_stack.pop() {
                    self.current_transform = prev;
                }
            }
            DrawCommand::PushOpacity(opacity) => {
                self.opacity_stack.push(self.current_opacity);
                self.current_opacity *= opacity;
            }
            DrawCommand::PopOpacity => {
                if let Some(prev) = self.opacity_stack.pop() {
                    self.current_opacity = prev;
                }
            }
            DrawCommand::BeginEffectLayer { effect, bounds } => {
                self.flush_all_buckets();
                self.ops.push(RenderOp::BeginEffect { effect: effect.clone(), bounds: *bounds });
            }
            DrawCommand::EndEffectLayer { .. } => {
                self.flush_all_buckets();
                self.ops.push(RenderOp::EndEffect);
            }
            DrawCommand::Canvas { vertices, indices, clip_rect, .. } => {
                self.ensure_batch(ShaderType::Rect, None, *clip_rect);
                let opacity = self.current_opacity;
                let transform = self.current_transform;
                let is_identity = transform == crate::core::Transform::identity();
                let state = self.current_batch_mut();
                let base = state.vertices.len() as u32;
                use crate::render::Vertex;
                state.vertices.reserve(vertices.len());
                state.indices.reserve(indices.len());
                state.vertices.extend(vertices.iter().map(|v| {
                    let pos = if is_identity {
                        v.position
                    } else {
                        let p = transform.transform_point(
                            euclid::Point2D::new(v.position[0], v.position[1]),
                        );
                        [p.x, p.y]
                    };
                    let mut color = v.color;
                    color[3] *= opacity;
                    Vertex { position: pos, uv: v.uv, color, data: v.data, data2: v.data2 }
                }));
                state.indices.extend(indices.iter().map(|idx| base + idx));
            }
            DrawCommand::LineStrip { points, color, width, clip_rect, .. } => {
                self.ensure_batch(ShaderType::Line, None, *clip_rect);
                let opacity = self.current_opacity;
                let transform = self.current_transform;
                let is_identity = transform == crate::core::Transform::identity();
                let color_array = {
                    let mut c = color.to_array();
                    c[3] *= opacity;
                    c
                };
                let feather = 1.0_f32;
                let half_w = *width * 0.5 + feather;
                let state = self.current_batch_mut();
                use crate::render::Vertex;
                let seg_count = points.len().saturating_sub(1);
                state.vertices.reserve(seg_count * 4);
                state.indices.reserve(seg_count * 6);
                for i in 0..seg_count {
                    let a = points[i];
                    let b = points[i + 1];
                    let dx = b[0] - a[0];
                    let dy = b[1] - a[1];
                    let len = (dx * dx + dy * dy).sqrt();
                    if len < 0.001 { continue; }
                    let inv_len = 1.0 / len;
                    let dir = [dx * inv_len, dy * inv_len];
                    let norm = [-dir[1], dir[0]];
                    let ext = [dir[0] * feather, dir[1] * feather];
                    let a_ext = [a[0] - ext[0], a[1] - ext[1]];
                    let b_ext = [b[0] + ext[0], b[1] + ext[1]];
                    let n_off = [norm[0] * half_w, norm[1] * half_w];
                    let corners: [[f32; 2]; 4] = [
                        [a_ext[0] + n_off[0], a_ext[1] + n_off[1]],
                        [b_ext[0] + n_off[0], b_ext[1] + n_off[1]],
                        [b_ext[0] - n_off[0], b_ext[1] - n_off[1]],
                        [a_ext[0] - n_off[0], a_ext[1] - n_off[1]],
                    ];
                    let base = state.vertices.len() as u32;
                    let data = [*width, feather, 0.0, 0.0];
                    let (transformed_corners, data2) = if is_identity {
                        (corners, [a[0], a[1], b[0], b[1]])
                    } else {
                        use wide::f32x4;
                        let xs = f32x4::new([corners[0][0], corners[1][0], corners[2][0], corners[3][0]]);
                        let ys = f32x4::new([corners[0][1], corners[1][1], corners[2][1], corners[3][1]]);
                        let xs_out = f32x4::splat(transform.m11) * xs + f32x4::splat(transform.m21) * ys + f32x4::splat(transform.m31);
                        let ys_out = f32x4::splat(transform.m12) * xs + f32x4::splat(transform.m22) * ys + f32x4::splat(transform.m32);
                        let xo: [f32; 4] = xs_out.into();
                        let yo: [f32; 4] = ys_out.into();
                        let pa = transform.transform_point(euclid::Point2D::new(a[0], a[1]));
                        let pb = transform.transform_point(euclid::Point2D::new(b[0], b[1]));
                        (
                            [[xo[0], yo[0]], [xo[1], yo[1]], [xo[2], yo[2]], [xo[3], yo[3]]],
                            [pa.x, pa.y, pb.x, pb.y],
                        )
                    };
                    state.vertices.extend_from_slice(&[
                        Vertex { position: transformed_corners[0], uv: [0.0, 0.0], color: color_array, data, data2 },
                        Vertex { position: transformed_corners[1], uv: [0.0, 0.0], color: color_array, data, data2 },
                        Vertex { position: transformed_corners[2], uv: [0.0, 0.0], color: color_array, data, data2 },
                        Vertex { position: transformed_corners[3], uv: [0.0, 0.0], color: color_array, data, data2 },
                    ]);
                    state.indices.extend_from_slice(&[base, base + 1, base + 2, base, base + 2, base + 3]);
                }
            }
            DrawCommand::Image { rect, texture_id, uv_rect, color, clip_rect, .. } => {
                self.ensure_batch(ShaderType::Image, Some(*texture_id), *clip_rect);
                let color_array = self.apply_opacity(color.to_array());
                let [p0, p1, p2, p3] = self.transform_quad([
                    [rect.origin.x, rect.origin.y],
                    [rect.origin.x + rect.size.width, rect.origin.y],
                    [rect.origin.x + rect.size.width, rect.origin.y + rect.size.height],
                    [rect.origin.x, rect.origin.y + rect.size.height],
                ]);
                let data = [2.0, 0.0, 0.0, 0.0];
                use crate::render::Vertex;
                let state = self.current_batch_mut();
                let base = state.vertices.len() as u32;
                state.vertices.extend_from_slice(&[
                    Vertex { position: p0, uv: [uv_rect.origin.x, uv_rect.origin.y], color: color_array, data, data2: [0.0; 4] },
                    Vertex { position: p1, uv: [uv_rect.origin.x + uv_rect.size.width, uv_rect.origin.y], color: color_array, data, data2: [0.0; 4] },
                    Vertex { position: p2, uv: [uv_rect.origin.x + uv_rect.size.width, uv_rect.origin.y + uv_rect.size.height], color: color_array, data, data2: [0.0; 4] },
                    Vertex { position: p3, uv: [uv_rect.origin.x, uv_rect.origin.y + uv_rect.size.height], color: color_array, data, data2: [0.0; 4] },
                ]);
                state.indices.extend_from_slice(&[base, base + 1, base + 2, base, base + 2, base + 3]);
            }
            DrawCommand::Cached(_) | DrawCommand::Custom { .. } => {}
        }
    }
}
