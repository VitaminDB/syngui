mod commands;
mod draw_gradient;
mod draw_rect;
mod draw_shadow;

use std::collections::HashMap;
use std::hash::{Hash, Hasher};
use std::sync::Arc;
use crate::core::Transform;
use crate::render::{Batch, ClipRect, RenderOp, ShaderType, TextureId, Vertex};
use crate::text::FontAtlas;
use crate::text::font_atlas::ShapedGlyph;

const MAX_VERTICES_PER_BATCH: usize = 65536;

#[derive(Clone, PartialEq, Eq)]
struct ShapedTextKey {
    text_hash: u64,
    font_size: u16,
    max_width_bits: u32,
    bold: bool,
    font_family_hash: u64,
    letter_spacing_bits: u32,
}

impl Hash for ShapedTextKey {
    fn hash<H: Hasher>(&self, state: &mut H) {
        self.text_hash.hash(state);
        self.font_size.hash(state);
        self.max_width_bits.hash(state);
        self.bold.hash(state);
        self.font_family_hash.hash(state);
        self.letter_spacing_bits.hash(state);
    }
}

#[derive(Clone, Copy, PartialEq, Eq, Hash)]
struct BatchKey {
    shader_type: ShaderType,
    texture: Option<TextureId>,
    clip_rect: ClipRect,
}

impl BatchKey {
    fn shader_order(&self) -> u8 {
        match self.shader_type {
            ShaderType::Shadow => 0,
            ShaderType::InnerShadow => 1,
            ShaderType::Rect => 2,
            ShaderType::Line => 3,
            ShaderType::Image => 4,
            ShaderType::Text => 5,
            ShaderType::GlowShadow => 6,
            ShaderType::Effect => 7,
        }
    }
}

pub struct Batcher {
    pub(self) ops: Vec<RenderOp>,
    pub(self) buckets: HashMap<BatchKey, BatchState>,
    pub(self) current_key: Option<BatchKey>,
    pub(self) scale_factor: f32,
    pub(self) opacity_stack: Vec<f32>,
    pub(self) current_opacity: f32,
    pub(self) transform_stack: Vec<Transform>,
    pub(self) current_transform: Transform,
    pub(self) shaped_cache: HashMap<ShapedTextKey, (Arc<Vec<ShapedGlyph>>, u64)>,
    pub(self) frame_counter: u64,
    pub(self) atlas_generation: u64,
}

struct BatchState {
    vertices: Vec<Vertex>,
    indices: Vec<u32>,
}

impl Batcher {
    pub fn new() -> Self {
        Self {
            ops: Vec::new(),
            buckets: HashMap::new(),
            current_key: None,
            scale_factor: 1.0,
            opacity_stack: Vec::new(),
            current_opacity: 1.0,
            transform_stack: Vec::new(),
            current_transform: Transform::identity(),
            shaped_cache: HashMap::new(),
            frame_counter: 0,
            atlas_generation: 0,
        }
    }

    pub fn set_scale_factor(&mut self, scale_factor: f32) {
        self.scale_factor = scale_factor;
    }

    pub fn process(&mut self, display_list: &crate::render::DisplayList, font_atlas: &mut FontAtlas) -> Vec<RenderOp> {
        self.ops.clear();
        self.buckets.clear();
        self.current_key = None;
        self.opacity_stack.clear();
        self.current_opacity = 1.0;
        self.transform_stack.clear();
        self.current_transform = Transform::identity();
        font_atlas.begin_frame();
        let atlas_generation = font_atlas.generation();
        if atlas_generation != self.atlas_generation {
            self.atlas_generation = atlas_generation;
            self.shaped_cache.clear();
        }
        self.frame_counter += 1;
        if self.frame_counter % 4 == 0 {
            let cutoff = self.frame_counter.saturating_sub(2);
            self.shaped_cache.retain(|_, (_, last_used)| *last_used >= cutoff);
        }

        for cmd in display_list.normal_commands() {
            self.process_command(cmd, font_atlas);
        }
        self.flush_all_buckets();

        for level in display_list.overlay_level_slices() {
            for cmd in level {
                self.process_command(cmd, font_atlas);
            }
            self.flush_all_buckets();
        }

        std::mem::take(&mut self.ops)
    }

    pub(self) fn apply_opacity(&self, mut color: [f32; 4]) -> [f32; 4] {
        color[3] *= self.current_opacity;
        color
    }

    /// Сохраняет ли текущая трансформация физическую пиксельную сетку.
    ///
    /// Снап глифов к целым пикселям имеет смысл, только если то, что нарисовано
    /// снапнутым, таким и останется. Прокрутка сдвигает содержимое чистой
    /// трансляцией на целое число физических пикселей (`ScrollView` округляет
    /// смещение сам) — под ней снап обязателен: без него строки, отъехавшие на
    /// долю пикселя, размазываются Linear-сэмплером и текст «мылится» при
    /// прокрутке. Под масштабом, поворотом или дробным сдвигом анимации снап
    /// отключён — там он заставлял бы текст дрожать на пиксель.
    pub(self) fn transform_keeps_pixel_grid(&self) -> bool {
        let t = &self.current_transform;
        if t.m11 != 1.0 || t.m12 != 0.0 || t.m21 != 0.0 || t.m22 != 1.0 {
            return false;
        }
        let sf = self.scale_factor;
        let aligned = |v: f32| {
            let phys = v * sf;
            (phys - phys.round()).abs() < 0.01
        };
        aligned(t.m31) && aligned(t.m32)
    }

    #[inline]
    pub(self) fn transform_quad(&self, corners: [[f32; 2]; 4]) -> [[f32; 2]; 4] {
        if self.current_transform == Transform::identity() {
            return corners;
        }
        use wide::f32x4;
        let t = &self.current_transform;
        let xs = f32x4::new([corners[0][0], corners[1][0], corners[2][0], corners[3][0]]);
        let ys = f32x4::new([corners[0][1], corners[1][1], corners[2][1], corners[3][1]]);
        let xs_out = f32x4::splat(t.m11) * xs + f32x4::splat(t.m21) * ys + f32x4::splat(t.m31);
        let ys_out = f32x4::splat(t.m12) * xs + f32x4::splat(t.m22) * ys + f32x4::splat(t.m32);
        let xo: [f32; 4] = xs_out.into();
        let yo: [f32; 4] = ys_out.into();
        [
            [xo[0], yo[0]],
            [xo[1], yo[1]],
            [xo[2], yo[2]],
            [xo[3], yo[3]],
        ]
    }

    pub(self) fn ensure_batch(&mut self, shader: ShaderType, texture: Option<TextureId>, clip: ClipRect) {
        let key = BatchKey { shader_type: shader, texture, clip_rect: clip };
        self.buckets.entry(key).or_insert_with(|| BatchState {
            vertices: Vec::with_capacity(256),
            indices: Vec::with_capacity(384),
        });
        self.current_key = Some(key);
    }

    pub(self) fn current_batch_mut(&mut self) -> &mut BatchState {
        self.buckets.get_mut(&self.current_key.unwrap()).unwrap()
    }

    pub(self) fn flush_all_buckets(&mut self) {
        let mut entries: Vec<(BatchKey, BatchState)> = self.buckets.drain().collect();
        if entries.is_empty() {
            return;
        }

        entries.sort_by(|(a, _), (b, _)| {
            a.clip_rect.enabled.cmp(&b.clip_rect.enabled)
                .then(a.clip_rect.x.cmp(&b.clip_rect.x))
                .then(a.clip_rect.y.cmp(&b.clip_rect.y))
                .then(a.clip_rect.width.cmp(&b.clip_rect.width))
                .then(a.clip_rect.height.cmp(&b.clip_rect.height))
                .then(a.shader_order().cmp(&b.shader_order()))
                .then(a.texture.map(|t| t.0).cmp(&b.texture.map(|t| t.0)))
        });

        for (key, state) in entries {
            if state.vertices.is_empty() {
                continue;
            }
            if state.vertices.len() <= MAX_VERTICES_PER_BATCH {
                self.ops.push(RenderOp::Draw(Batch {
                    vertices: state.vertices,
                    indices: state.indices,
                    shader_type: key.shader_type,
                    texture: key.texture,
                    clip_rect: key.clip_rect,
                    vertex_offset: 0,
                    index_offset: 0,
                }));
            } else {
                let mut v_offset = 0;
                let mut i_offset = 0;
                while v_offset < state.vertices.len() {
                    let v_end = (v_offset + MAX_VERTICES_PER_BATCH).min(state.vertices.len());
                    let base_vertex = v_offset as u32;
                    let max_vertex = v_end as u32;
                    let mut i_end = i_offset;
                    while i_end < state.indices.len() && state.indices[i_end] < max_vertex {
                        i_end += 1;
                    }
                    let chunk_indices: Vec<u32> = state.indices[i_offset..i_end]
                        .iter()
                        .map(|&idx| idx - base_vertex)
                        .collect();
                    self.ops.push(RenderOp::Draw(Batch {
                        vertices: state.vertices[v_offset..v_end].to_vec(),
                        indices: chunk_indices,
                        shader_type: key.shader_type,
                        texture: key.texture,
                        clip_rect: key.clip_rect,
                        vertex_offset: 0,
                        index_offset: 0,
                    }));
                    v_offset = v_end;
                    i_offset = i_end;
                }
            }
        }
        self.current_key = None;
    }

    pub(self) fn shape_text_cached_spacing(
        &mut self,
        font_atlas: &mut FontAtlas,
        text: &str,
        font_size: u16,
        max_width: f32,
        bold: bool,
        font_family: Option<&str>,
        letter_spacing: f32,
    ) -> Arc<Vec<ShapedGlyph>> {
        use std::hash::Hasher;
        let mut hasher = std::collections::hash_map::DefaultHasher::new();
        text.hash(&mut hasher);
        let text_hash = hasher.finish();

        let mut fam_hasher = std::collections::hash_map::DefaultHasher::new();
        font_family.hash(&mut fam_hasher);
        let font_family_hash = fam_hasher.finish();

        let key = ShapedTextKey {
            text_hash,
            font_size,
            max_width_bits: max_width.to_bits(),
            bold,
            font_family_hash,
            letter_spacing_bits: letter_spacing.to_bits(),
        };

        if let Some((glyphs, last_used)) = self.shaped_cache.get_mut(&key) {
            *last_used = self.frame_counter;
            return Arc::clone(glyphs);
        }

        let glyphs = Arc::new(font_atlas.shape_text_spaced(text, font_size, max_width, bold, font_family, letter_spacing));
        self.shaped_cache.insert(key, (Arc::clone(&glyphs), self.frame_counter));
        glyphs
    }
}

impl Default for Batcher {
    fn default() -> Self {
        Self::new()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Прокрутка сдвигает содержимое на целое число физических пикселей —
    /// снап глифов под таким сдвигом остаётся корректным.
    #[test]
    fn scroll_translation_keeps_pixel_grid() {
        let mut b = Batcher::new();
        b.set_scale_factor(2.0);
        // Смещение прокрутки, округлённое так же, как это делает ScrollView.
        let offset = (137.4_f32 * 2.0).trunc() / 2.0;
        b.current_transform = Transform::translation(-offset, -offset);
        assert!(b.transform_keeps_pixel_grid());
    }

    /// Дробный сдвиг анимации сетку ломает — снап должен отключаться, иначе
    /// текст дрожит на пиксель во время движения.
    #[test]
    fn fractional_translation_breaks_pixel_grid() {
        let mut b = Batcher::new();
        b.set_scale_factor(1.0);
        b.current_transform = Transform::translation(0.0, -12.37);
        assert!(!b.transform_keeps_pixel_grid());
    }

    /// Масштаб и поворот сетку не сохраняют при любом смещении.
    #[test]
    fn scale_breaks_pixel_grid() {
        let mut b = Batcher::new();
        b.set_scale_factor(1.0);
        b.current_transform = Transform::scale(1.5, 1.5);
        assert!(!b.transform_keeps_pixel_grid());
        b.current_transform = Transform::identity();
        assert!(b.transform_keeps_pixel_grid());
    }
}
