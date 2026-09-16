mod commands;
mod draw_gradient;
mod draw_rect;
mod draw_shadow;

use crate::core::Transform;
use crate::render::{Batch, ClipRect, RenderOp, ShaderType, TextureId, Vertex};
use crate::text::font_atlas::ShapedGlyph;
use crate::text::FontAtlas;
use std::collections::HashMap;
use std::hash::{Hash, Hasher};
use std::sync::Arc;

/// Ограничивающий прямоугольник батча в логических координатах после
/// трансформации: `[min_x, min_y, max_x, max_y]`.
type Bbox = [f32; 4];

fn bbox_overlaps(a: &Bbox, b: &Bbox) -> bool {
    a[0] < b[2] && b[0] < a[2] && a[1] < b[3] && b[1] < a[3]
}

fn bbox_union(a: &mut Bbox, b: &Bbox) {
    a[0] = a[0].min(b[0]);
    a[1] = a[1].min(b[1]);
    a[2] = a[2].max(b[2]);
    a[3] = a[3].max(b[3]);
}

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

/// Ключ батча: пайплайн, текстура и клип. Клип — uniform на батч плюс
/// scissor (на Mali это бесплатно; вариант с клипом в вершинах давал два
/// лишних varying на фрагмент и +30 % времени GPU).
#[derive(Clone, Copy, PartialEq, Eq, Hash)]
struct BatchKey {
    shader_type: ShaderType,
    texture: Option<TextureId>,
    clip_rect: ClipRect,
}

/// Открытый батч текущей группы. Батчи хранятся в порядке отрисовки;
/// новый примитив уходит в ближайший с конца батч с тем же ключом, если
/// между ними нет батча, чей bbox пересекает примитив (иначе порядок
/// художника нарушился бы). Внешние тени (`Shadow`) — исключение: они всегда
/// собираются в первый теневой батч группы и не блокируют слияние — тень под
/// соседней карточкой визуально неотличима от тени над её краем, а draw
/// call'ов это экономит по одному на элемент.
struct OpenBatch {
    key: BatchKey,
    state: BatchState,
    bbox: Bbox,
    /// bbox неизвестен (примитив без геометрии заранее) — считается
    /// бесконечным: и блокирует, и пересекается со всем.
    bbox_unknown: bool,
}

pub struct Batcher {
    pub(self) ops: Vec<RenderOp>,
    pub(self) buckets: Vec<OpenBatch>,
    pub(self) current: Option<usize>,
    pub(self) scale_factor: f32,
    /// На сколько логических пикселей растягивать сплошную заливку за
    /// границы клипа (см. `set_clip_expand`).
    pub(self) clip_expand: f32,
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
            buckets: Vec::new(),
            current: None,
            scale_factor: 1.0,
            clip_expand: 0.0,
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

    pub fn process(
        &mut self,
        display_list: &crate::render::DisplayList,
        font_atlas: &mut FontAtlas,
    ) -> Vec<RenderOp> {
        self.ops.clear();
        self.buckets.clear();
        self.current = None;
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
            self.shaped_cache
                .retain(|_, (_, last_used)| *last_used >= cutoff);
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

    /// Bbox прямоугольника после текущей трансформации.
    pub(self) fn rect_bbox(&self, rect: crate::core::Rect) -> Bbox {
        let o = rect.origin;
        let sz = rect.size;
        let q = self.transform_quad([
            [o.x, o.y],
            [o.x + sz.width, o.y],
            [o.x + sz.width, o.y + sz.height],
            [o.x, o.y + sz.height],
        ]);
        let mut b = [f32::MAX, f32::MAX, f32::MIN, f32::MIN];
        for p in q {
            b[0] = b[0].min(p[0]);
            b[1] = b[1].min(p[1]);
            b[2] = b[2].max(p[0]);
            b[3] = b[3].max(p[1]);
        }
        b
    }

    /// Батч для примитива с известным прямоугольником (до трансформации).
    /// Заливка, которую обрезает клип с дробными (в физических пикселях)
    /// границами, растягивается на пиксель наружу — точный край ей всё равно
    /// задают ножницы. Без этого на краевом пикселе заливка покрывает лишь
    /// часть площади, а содержимое под ней (картинка шире бокса) — всю, и
    /// из-под заливки выглядывает полоска.
    pub(self) fn set_clip_expand(&mut self, clip: &ClipRect) {
        self.clip_expand = if clip.enabled && !clip.is_pixel_aligned(self.scale_factor) {
            1.0
        } else {
            0.0
        };
    }

    pub(self) fn ensure_batch_rect(
        &mut self,
        shader: ShaderType,
        texture: Option<TextureId>,
        clip: ClipRect,
        rect: crate::core::Rect,
    ) {
        let bbox = self.rect_bbox(rect);
        self.ensure_batch_bbox(shader, texture, clip, Some(bbox));
    }

    /// Батч для примитива без известной геометрии: сливается только с
    /// последним батчем того же ключа, иначе открывает новый.
    pub(self) fn ensure_batch(
        &mut self,
        shader: ShaderType,
        texture: Option<TextureId>,
        clip: ClipRect,
    ) {
        self.ensure_batch_bbox(shader, texture, clip, None);
    }

    pub(self) fn ensure_batch_bbox(
        &mut self,
        shader: ShaderType,
        texture: Option<TextureId>,
        clip: ClipRect,
        bbox: Option<Bbox>,
    ) {
        let key = BatchKey {
            shader_type: shader,
            texture,
            clip_rect: clip,
        };
        let mut target: Option<usize> = None;
        if shader == ShaderType::Shadow {
            target = self.buckets.iter().position(|b| b.key == key);
        } else {
            for i in (0..self.buckets.len()).rev() {
                let b = &self.buckets[i];
                if b.key == key {
                    target = Some(i);
                    break;
                }
                if b.key.shader_type == ShaderType::Shadow {
                    continue;
                }
                let blocks = match (&bbox, b.bbox_unknown) {
                    (Some(bb), false) => bbox_overlaps(&b.bbox, bb),
                    _ => true,
                };
                if blocks {
                    break;
                }
            }
        }
        let idx = match target {
            Some(i) => i,
            None => {
                self.buckets.push(OpenBatch {
                    key,
                    state: BatchState {
                        vertices: Vec::with_capacity(256),
                        indices: Vec::with_capacity(384),
                    },
                    bbox: [f32::MAX, f32::MAX, f32::MIN, f32::MIN],
                    bbox_unknown: false,
                });
                self.buckets.len() - 1
            }
        };
        let b = &mut self.buckets[idx];
        match bbox {
            Some(bb) => bbox_union(&mut b.bbox, &bb),
            None => b.bbox_unknown = true,
        }
        self.current = Some(idx);
    }

    pub(self) fn current_batch_mut(&mut self) -> &mut BatchState {
        let idx = self.current.expect("ensure_batch перед добавлением геометрии");
        &mut self.buckets[idx].state
    }

    /// Выдать все открытые батчи группы в порядке отрисовки.
    pub(self) fn flush_all_buckets(&mut self) {
        if self.buckets.is_empty() {
            return;
        }
        for OpenBatch { key, state, .. } in self.buckets.drain(..) {
            if state.vertices.is_empty() {
                continue;
            }
            self.ops.push(RenderOp::Draw(Batch {
                vertices: state.vertices,
                indices: state.indices,
                shader_type: key.shader_type,
                texture: key.texture,
                clip_rect: key.clip_rect,
                vertex_offset: 0,
                index_offset: 0,
            }));
        }
        self.current = None;
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

        let glyphs = Arc::new(font_atlas.shape_text_spaced(
            text,
            font_size,
            max_width,
            bold,
            font_family,
            letter_spacing,
        ));
        self.shaped_cache
            .insert(key, (Arc::clone(&glyphs), self.frame_counter));
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
