use hashbrown::{HashMap, HashSet};
use std::sync::Arc;

use crate::text::line_break::breaks_before;
use crate::text::script::{script_of, Script};

const FONT_REGULAR: u8 = 0;
const FONT_EMOJI: u8 = 1;
const FONT_BOLD: u8 = 2;
const FONT_ICON: u8 = 3;
const FIRST_EXTRA_FONT: u8 = 4;

#[derive(Clone, Copy, Hash, Eq, PartialEq, Debug)]
pub struct GlyphKey {
    pub glyph_id: u16,
    pub size_px: u16,
    pub font_index: u8,
}

#[derive(Clone, Copy, Debug)]
pub struct CachedGlyph {
    pub uv_x: f32,
    pub uv_y: f32,
    pub uv_w: f32,
    pub uv_h: f32,
    pub width: u32,
    pub height: u32,
    pub bearing_x: f32,
    pub bearing_y: f32,
    pub advance: f32,
    pub is_color: bool,
}

#[derive(Clone, Debug)]
pub struct ShapedGlyph {
    pub x: f32,
    pub y: f32,
    pub glyph: CachedGlyph,
}

#[derive(Clone, Debug, Default)]
pub struct FontAtlasStats {
    pub glyph_count: usize,
    pub atlas_size: (u32, u32),
    pub pixels_bytes: usize,
    pub font_data_bytes: usize,
    pub total_bytes: usize,
    pub cursor_x: u32,
    pub cursor_y: u32,
    pub row_height: u32,
}

/// Вес начертания (100–900). `From<bool>` — старые вызовы с флагом «жирный»
/// (700 / 400) продолжают работать.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub struct FontWeight(pub u16);

impl From<bool> for FontWeight {
    fn from(bold: bool) -> Self {
        FontWeight(if bold { 700 } else { 400 })
    }
}

impl From<u16> for FontWeight {
    fn from(w: u16) -> Self {
        FontWeight(w)
    }
}

impl FontWeight {
    /// Класс, под который ищется начертание: 400, 500, 600 или 700. Тоньше
    /// 400 и тяжелее 700 отдельных начертаний не грузим.
    pub fn class(self) -> u16 {
        match self.0 {
            0..=449 => 400,
            450..=549 => 500,
            550..=649 => 600,
            _ => 700,
        }
    }
}

#[derive(Clone)]
struct FontFace {
    data: Arc<[u8]>,
    face_index: u32,
}

impl FontFace {
    fn new(data: Vec<u8>, face_index: u32) -> Option<Self> {
        if data.is_empty() {
            return None;
        }
        Some(Self {
            data: Arc::from(data),
            face_index,
        })
    }

    fn font_ref(&self) -> Option<swash::FontRef<'_>> {
        swash::FontRef::from_index(&self.data, self.face_index as usize)
    }
}

/// Единица раскладки: обычный символ или кластер эмодзи, который шейпер
/// свёл к глифам шрифта эмодзи (обычно к одной лигатуре).
enum Unit {
    Char(char),
    Cluster { keys: Vec<GlyphKey>, len: usize, first: char },
}

impl Unit {
    fn first_char(&self) -> char {
        match self {
            Unit::Char(c) => *c,
            Unit::Cluster { first, .. } => *first,
        }
    }

    /// Сколько символов исходного текста покрывает единица.
    fn len(&self) -> usize {
        match self {
            Unit::Char(_) => 1,
            Unit::Cluster { len, .. } => *len,
        }
    }
}

/// Длина кластера эмодзи, начинающегося с `chars[i]` (1 — не кластер):
/// базовый символ + VS16 / модификаторы тона кожи / теги (флаги областей) /
/// `ZWJ + эмодзи`; пара региональных индикаторов (флаг); keycap `1️⃣`.
fn emoji_cluster_len(chars: &[char], i: usize) -> usize {
    let c = chars[i] as u32;
    let at = |j: usize| chars.get(j).map(|&ch| ch as u32);
    let is_ri = |v: u32| (0x1F1E6..=0x1F1FF).contains(&v);
    if is_ri(c) {
        return if at(i + 1).is_some_and(is_ri) { 2 } else { 1 };
    }
    if matches!(chars[i], '0'..='9' | '#' | '*') {
        let mut j = i + 1;
        if at(j) == Some(0xFE0F) {
            j += 1;
        }
        return if at(j) == Some(0x20E3) { j + 1 - i } else { 1 };
    }
    let base = (0x1F000..=0x1FAFF).contains(&c) || (0x2600..=0x27BF).contains(&c) || (0x2300..=0x23FF).contains(&c)
        || (0x2B00..=0x2BFF).contains(&c) || (0x3297..=0x3299).contains(&c) || c == 0x00A9 || c == 0x00AE;
    if !base {
        return 1;
    }
    let mut j = i + 1;
    while let Some(v) = at(j) {
        if v == 0xFE0F || (0x1F3FB..=0x1F3FF).contains(&v) || (0xE0020..=0xE007F).contains(&v) {
            j += 1;
        } else if v == 0x200D && at(j + 1).is_some() {
            j += 2;
        } else {
            break;
        }
    }
    j - i
}

pub struct FontAtlas {
    faces: Vec<Option<FontFace>>,
    extra_fonts: HashMap<String, Option<(u8, u8)>>,
    fallback_faces: Vec<u8>,
    fallback_discovered: Vec<u8>,
    fallback_tried: HashSet<Script>,
    prefer_japanese: bool,
    prefer_korean: bool,
    next_font_index: u8,
    texture: wgpu::Texture,
    pub texture_view: wgpu::TextureView,
    pub sampler: wgpu::Sampler,
    atlas_width: u32,
    atlas_height: u32,
    cursor_x: u32,
    cursor_y: u32,
    row_height: u32,
    glyphs: HashMap<GlyphKey, CachedGlyph>,
    charmap_cache: HashMap<(char, u8), u16>,
    pixels: Vec<u8>,
    dirty: bool,
    /// Область атласа, изменённая с последней заливки: x0, y0, x1, y1
    /// (в пикселях, правая/нижняя границы исключительно).
    dirty_rect: [u32; 4],
    overflowed: bool,
    generation: u64,
    scale_factor: f32,
    /// Контекст растеризации swash — переиспользуется между глифами (у него
    /// свои кэши), а не создаётся на каждый новый глиф.
    scale_ctx: swash::scale::ScaleContext,
    /// Начертания 500/600 (Medium/SemiBold) по семейству (`None` — основное):
    /// индекс грани или `None`, если отдельного начертания нет (тогда берётся
    /// ближайшее: 500 → обычное, 600 → жирное).
    weight_faces: HashMap<(Option<String>, u16), Option<u8>>,
    /// Шейпленные кластеры эмодзи: строка кластера и кегль → глифы.
    emoji_clusters: HashMap<(String, u16), Option<Vec<GlyphKey>>>,
}

impl FontAtlas {
    pub fn new(device: &wgpu::Device, queue: &wgpu::Queue) -> Self {
        Self::with_config(device, queue, None)
    }

    pub fn with_config(
        device: &wgpu::Device,
        queue: &wgpu::Queue,
        preferred_family: Option<String>,
    ) -> Self {
        let atlas_width = 2048u32;
        let atlas_height = 2048u32;

        let texture = device.create_texture(&wgpu::TextureDescriptor {
            label: Some("Font Atlas"),
            size: wgpu::Extent3d {
                width: atlas_width,
                height: atlas_height,
                depth_or_array_layers: 1,
            },
            mip_level_count: 1,
            sample_count: 1,
            dimension: wgpu::TextureDimension::D2,
            format: wgpu::TextureFormat::Rgba8UnormSrgb,
            usage: wgpu::TextureUsages::TEXTURE_BINDING | wgpu::TextureUsages::COPY_DST,
            view_formats: &[],
        });

        let texture_view = texture.create_view(&wgpu::TextureViewDescriptor::default());

        let sampler = device.create_sampler(&wgpu::SamplerDescriptor {
            label: Some("Font Sampler"),
            address_mode_u: wgpu::AddressMode::ClampToEdge,
            address_mode_v: wgpu::AddressMode::ClampToEdge,
            mag_filter: wgpu::FilterMode::Linear,
            min_filter: wgpu::FilterMode::Linear,
            ..Default::default()
        });

        let pixels = vec![0u8; (atlas_width * atlas_height * 4) as usize];

        queue.write_texture(
            wgpu::TexelCopyTextureInfo {
                texture: &texture,
                mip_level: 0,
                origin: wgpu::Origin3d::ZERO,
                aspect: wgpu::TextureAspect::All,
            },
            &pixels,
            wgpu::TexelCopyBufferLayout {
                offset: 0,
                bytes_per_row: Some(atlas_width * 4),
                rows_per_image: None,
            },
            wgpu::Extent3d {
                width: atlas_width,
                height: atlas_height,
                depth_or_array_layers: 1,
            },
        );

        let (
            (font_data, font_face_index),
            (bold_font_data, bold_face_index),
            (emoji_font_data, emoji_face_index),
        ) = Self::load_all_fonts(preferred_family);

        let emoji = FontFace::new(emoji_font_data, emoji_face_index);
        if emoji.is_none() {
            log::warn!("No emoji font found — emoji will not render");
        }
        let faces = vec![
            FontFace::new(font_data, font_face_index),
            emoji,
            FontFace::new(bold_font_data, bold_face_index),
            None,
        ];

        Self {
            faces,
            extra_fonts: HashMap::new(),
            fallback_faces: Vec::new(),
            fallback_discovered: Vec::new(),
            fallback_tried: HashSet::new(),
            prefer_japanese: false,
            prefer_korean: false,
            next_font_index: FIRST_EXTRA_FONT,
            texture,
            texture_view,
            sampler,
            atlas_width,
            atlas_height,
            cursor_x: 1,
            cursor_y: 1,
            row_height: 0,
            glyphs: HashMap::new(),
            charmap_cache: HashMap::new(),
            pixels,
            dirty: false,
            dirty_rect: [u32::MAX, u32::MAX, 0, 0],
            overflowed: false,
            generation: 0,
            scale_factor: 1.0,
            scale_ctx: swash::scale::ScaleContext::new(),
            weight_faces: HashMap::new(),
            emoji_clusters: HashMap::new(),
        }
    }

    pub fn scale_factor(&self) -> f32 {
        self.scale_factor
    }

    pub fn set_scale_factor(&mut self, sf: f32) {
        self.scale_factor = sf.clamp(0.1, 8.0);
    }

    #[cfg(all(not(target_arch = "wasm32"), not(target_os = "android")))]
    fn load_all_fonts(
        preferred_family: Option<String>,
    ) -> ((Vec<u8>, u32), (Vec<u8>, u32), (Vec<u8>, u32)) {
        use crate::text::font_discovery;

        let family = preferred_family.as_deref();
        let font_data = font_discovery::discover_font(family);
        let bold_font_data = font_discovery::discover_bold_font(family);
        let emoji_font_data = font_discovery::discover_emoji_font();

        (font_data, bold_font_data, emoji_font_data)
    }

    #[cfg(target_os = "android")]
    fn load_all_fonts(
        preferred_family: Option<String>,
    ) -> ((Vec<u8>, u32), (Vec<u8>, u32), (Vec<u8>, u32)) {
        use crate::text::font_discovery_android;

        let family = preferred_family.as_deref();
        let font_data = font_discovery_android::discover_font(family);
        let bold_font_data = font_discovery_android::discover_bold_font(family);
        let emoji_font_data = font_discovery_android::discover_emoji_font();

        (font_data, bold_font_data, emoji_font_data)
    }

    #[cfg(target_arch = "wasm32")]
    fn load_all_fonts(
        _preferred_family: Option<String>,
    ) -> ((Vec<u8>, u32), (Vec<u8>, u32), (Vec<u8>, u32)) {
        ((Vec::new(), 0), (Vec::new(), 0), (Vec::new(), 0))
    }

    pub fn set_font_data(&mut self, data: Vec<u8>) {
        let Some(face) = FontFace::new(data, 0) else {
            return;
        };
        self.set_face(FONT_REGULAR, Some(face));
        self.charmap_cache.clear();
        self.reset_glyphs();
    }

    pub fn set_emoji_font_data(&mut self, data: Vec<u8>) {
        if let Some(face) = FontFace::new(data, 0) {
            self.replace_face(FONT_EMOJI, face);
        }
    }

    pub fn set_icon_font_data(&mut self, data: Vec<u8>) {
        if let Some(face) = FontFace::new(data, 0) {
            self.replace_face(FONT_ICON, face);
        }
    }

    /// Registers an app-provided CJK fallback face (`face_index` selects the
    /// face inside a `.ttc`). Consulted after the regular font for Han, Kana
    /// and Hangul characters, in registration order, before platform discovery.
    pub fn add_fallback_font(&mut self, data: Vec<u8>, face_index: u32) {
        if let Some(face) = FontFace::new(data, face_index) {
            self.push_fallback_face(face);
        }
    }

    /// Makes platform fallback discovery for Han prefer Japanese- or
    /// Korean-first families. Faces discovered under the old preference are
    /// dropped so the next miss rediscovers; app-provided faces stay.
    pub fn set_preferred_cjk(&mut self, japanese: bool, korean: bool) {
        if (self.prefer_japanese, self.prefer_korean) == (japanese, korean) {
            return;
        }
        self.prefer_japanese = japanese;
        self.prefer_korean = korean;
        self.fallback_tried.clear();
        let discovered = std::mem::take(&mut self.fallback_discovered);
        if discovered.is_empty() {
            return;
        }
        for font_index in discovered {
            self.fallback_faces.retain(|&i| i != font_index);
            self.charmap_cache.retain(|&(_, fi), _| fi != font_index);
            self.set_face(font_index, None);
        }
        self.reset_glyphs();
    }

    /// Bumps whenever previously returned glyph data stops being valid: a font
    /// slot replaced, a fallback face added, or the atlas reset after overflow.
    /// Anything caching shaped text must drop its cache when this changes.
    pub fn generation(&self) -> u64 {
        self.generation
    }

    /// Call once per frame before shaping. If a glyph did not fit during the
    /// previous frame the atlas is cleared and `generation` bumped, so the
    /// frame being built rasterizes only what it actually shows.
    pub fn begin_frame(&mut self) {
        if !self.overflowed {
            return;
        }
        self.overflowed = false;
        self.reset_glyphs();
        log::info!(
            "font_atlas: reset after overflow, generation {}",
            self.generation
        );
    }

    pub fn has_emoji_font(&self) -> bool {
        self.has_face(FONT_EMOJI)
    }

    fn reset_glyphs(&mut self) {
        self.glyphs.clear();
        self.pixels.fill(0);
        self.cursor_x = 1;
        self.cursor_y = 1;
        self.row_height = 0;
        self.generation += 1;
        self.dirty = true;
        self.dirty_rect = [0, 0, self.atlas_width, self.atlas_height];
    }

    #[inline]
    fn mark_dirty(&mut self, x: u32, y: u32, w: u32, h: u32) {
        self.dirty = true;
        let r = &mut self.dirty_rect;
        r[0] = r[0].min(x);
        r[1] = r[1].min(y);
        r[2] = r[2].max(x + w);
        r[3] = r[3].max(y + h);
    }

    fn face(&self, font_index: u8) -> Option<FontFace> {
        self.faces
            .get(font_index as usize)
            .and_then(|face| face.clone())
    }

    fn has_face(&self, font_index: u8) -> bool {
        self.faces
            .get(font_index as usize)
            .is_some_and(|face| face.is_some())
    }

    fn set_face(&mut self, font_index: u8, face: Option<FontFace>) {
        let i = font_index as usize;
        if self.faces.len() <= i {
            self.faces.resize(i + 1, None);
        }
        self.faces[i] = face;
    }

    fn replace_face(&mut self, font_index: u8, face: FontFace) {
        self.set_face(font_index, Some(face));
        self.charmap_cache.retain(|&(_, fi), _| fi != font_index);
        self.glyphs.retain(|key, _| key.font_index != font_index);
        self.generation += 1;
    }

    fn push_face(&mut self, face: Option<FontFace>) -> u8 {
        let font_index = self.next_font_index;
        self.next_font_index += 1;
        self.set_face(font_index, face);
        font_index
    }

    fn push_fallback_face(&mut self, face: FontFace) -> u8 {
        let font_index = self.push_face(Some(face));
        self.fallback_faces.push(font_index);
        self.generation += 1;
        font_index
    }

    fn cached_glyph_id(&self, ch: char, font_index: u8) -> Option<u16> {
        self.charmap_cache.get(&(ch, font_index)).copied()
    }

    fn lookup_and_cache_glyph_id(&mut self, ch: char, font_index: u8, face: &FontFace) -> u16 {
        let gid = face.font_ref().map(|fr| fr.charmap().map(ch)).unwrap_or(0);
        self.charmap_cache.insert((ch, font_index), gid);
        gid
    }

    /// Стоит ли сперва искать глиф в эмодзи-шрифте. Диапазон начинается с
    /// 0x1F000, а не 0x1F300: блок Enclosed Alphanumeric Supplement
    /// (🆕 🆗 🆒 🆓, 🈶) и Mahjong/Domino лежат ниже, и такие символы
    /// уходили в обычный шрифт — то есть в «квадратик». Промах здесь не
    /// страшен: `ensure_glyph` всё равно откатывается к regular.
    fn is_likely_emoji(ch: char) -> bool {
        let c = ch as u32;
        (0x1F000..=0x1FAFF).contains(&c)
            || (0x2600..=0x27BF).contains(&c)
            || (0xFE00..=0xFE0F).contains(&c)
            || (0x200D..=0x200D).contains(&c)
            || (0x2300..=0x23FF).contains(&c)
            || (0x2B00..=0x2BFF).contains(&c)
            || (0x25A0..=0x25FF).contains(&c)
            || (0x3297..=0x3299).contains(&c)
    }

    fn glyph_id_in(&mut self, ch: char, font_index: u8) -> Option<u16> {
        let glyph_id = match self.cached_glyph_id(ch, font_index) {
            Some(gid) => gid,
            None => {
                let face = self.face(font_index)?;
                self.lookup_and_cache_glyph_id(ch, font_index, &face)
            }
        };
        (glyph_id != 0).then_some(glyph_id)
    }

    fn ensure_glyph_in(&mut self, ch: char, size_px: u16, font_index: u8) -> Option<GlyphKey> {
        let glyph_id = self.glyph_id_in(ch, font_index)?;
        let key = GlyphKey {
            glyph_id,
            size_px,
            font_index,
        };
        if self.glyphs.contains_key(&key) {
            return Some(key);
        }
        let face = self.face(font_index)?;
        self.rasterize_glyph(face, glyph_id, size_px, key)
            .map(|_| key)
    }

    /// Глиф по его id в грани (результат шейпера).
    fn ensure_glyph_id(&mut self, glyph_id: u16, size_px: u16, font_index: u8) -> Option<GlyphKey> {
        let key = GlyphKey { glyph_id, size_px, font_index };
        if self.glyphs.contains_key(&key) {
            return Some(key);
        }
        let face = self.face(font_index)?;
        self.rasterize_glyph(face, glyph_id, size_px, key).map(|_| key)
    }

    /// Кластер эмодзи через rustybuzz на шрифте эмодзи: `🧑‍💻` становится одним
    /// глифом-лигатурой, а не 🧑 + 💻, VS16 не даёт лишнего аванса. `None` —
    /// шрифт кластер не знает (тогда посимвольно, как раньше).
    fn shape_emoji_cluster(&mut self, cluster: &str, size_px: u16) -> Option<Vec<GlyphKey>> {
        let key = (cluster.to_string(), size_px);
        if let Some(v) = self.emoji_clusters.get(&key) {
            return v.clone();
        }
        let shaped = (|| {
            let face = self.face(FONT_EMOJI)?;
            let ids: Vec<u16> = {
                let rb = rustybuzz::Face::from_slice(&face.data, face.face_index)?;
                let mut buf = rustybuzz::UnicodeBuffer::new();
                buf.push_str(cluster);
                let out = rustybuzz::shape(&rb, &[], buf);
                out.glyph_infos().iter().map(|g| g.glyph_id as u16).collect()
            };
            if ids.is_empty() || ids.contains(&0) {
                return None;
            }
            ids.into_iter()
                .map(|id| self.ensure_glyph_id(id, size_px, FONT_EMOJI))
                .collect::<Option<Vec<_>>>()
        })();
        self.emoji_clusters.insert(key, shaped.clone());
        shaped
    }

    /// Текст в единицы раскладки: кластеры эмодзи — шейпером, остальное —
    /// посимвольно.
    fn text_units(&mut self, text: &str, size_px: u16) -> Vec<Unit> {
        let chars: Vec<char> = text.chars().collect();
        let mut out = Vec::with_capacity(chars.len());
        let mut i = 0;
        while i < chars.len() {
            let n = emoji_cluster_len(&chars, i);
            if n > 1 {
                let cluster: String = chars[i..i + n].iter().collect();
                if let Some(keys) = self.shape_emoji_cluster(&cluster, size_px) {
                    out.push(Unit::Cluster { keys, len: n, first: chars[i] });
                    i += n;
                    continue;
                }
            }
            out.push(Unit::Char(chars[i]));
            i += 1;
        }
        out
    }

    fn cluster_advance(&self, keys: &[GlyphKey]) -> f32 {
        keys.iter().filter_map(|k| self.glyphs.get(k)).map(|g| g.advance).sum()
    }

    fn unit_advance(&mut self, unit: &Unit, size_px: u16, weight: FontWeight, font_family: Option<&str>) -> f32 {
        match unit {
            Unit::Char(c) => self.glyph_advance(*c, size_px, weight, font_family),
            Unit::Cluster { keys, .. } => self.cluster_advance(keys),
        }
    }

    fn emit_keys(&self, keys: &[GlyphKey], size_px: u16, x: &mut f32, y: f32, result: &mut Vec<ShapedGlyph>) {
        for k in keys {
            let Some(glyph) = self.glyphs.get(k).copied() else { continue };
            if glyph.width > 0 && glyph.height > 0 {
                result.push(ShapedGlyph {
                    x: *x + glyph.bearing_x,
                    y: y - glyph.bearing_y + size_px as f32,
                    glyph,
                });
            }
            *x += glyph.advance;
        }
    }

    #[allow(clippy::too_many_arguments)]
    fn emit_unit(
        &mut self,
        unit: &Unit,
        size_px: u16,
        weight: FontWeight,
        font_family: Option<&str>,
        x: &mut f32,
        y: f32,
        result: &mut Vec<ShapedGlyph>,
    ) {
        match unit {
            Unit::Char(c) => self.emit_glyph(*c, size_px, weight, font_family, x, y, result),
            Unit::Cluster { keys, .. } => self.emit_keys(keys, size_px, x, y, result),
        }
    }

    #[allow(clippy::too_many_arguments)]
    fn emit_unit_spaced(
        &mut self,
        unit: &Unit,
        size_px: u16,
        weight: FontWeight,
        font_family: Option<&str>,
        x: &mut f32,
        y: f32,
        letter_spacing: f32,
        result: &mut Vec<ShapedGlyph>,
    ) {
        match unit {
            Unit::Char(c) => {
                self.emit_glyph_spaced(*c, size_px, weight, font_family, x, y, letter_spacing, result)
            }
            Unit::Cluster { keys, .. } => {
                self.emit_keys(keys, size_px, x, y, result);
                *x += letter_spacing;
            }
        }
    }

    fn ensure_glyph(&mut self, ch: char, size_px: u16) -> Option<GlyphKey> {
        if Self::is_likely_emoji(ch) {
            if let Some(key) = self.ensure_glyph_in(ch, size_px, FONT_EMOJI) {
                return Some(key);
            }
        }
        if let Some(key) = self.ensure_glyph_in(ch, size_px, FONT_REGULAR) {
            return Some(key);
        }
        if let Some(key) = self.ensure_fallback_glyph(ch, size_px) {
            return Some(key);
        }
        if let Some(key) = self.ensure_glyph_in(ch, size_px, FONT_ICON) {
            return Some(key);
        }
        self.ensure_glyph_in(ch, size_px, FONT_EMOJI)
    }

    /// CJK fallback: the first registered fallback face whose charmap covers
    /// `ch` is used; when none does, the platform is asked once per script and
    /// the discovered face is appended. Bold text goes through the same
    /// regular-weight faces — CJK fallbacks carry no bold companion.
    fn ensure_fallback_glyph(&mut self, ch: char, size_px: u16) -> Option<GlyphKey> {
        let script = script_of(ch)?;
        for i in 0..self.fallback_faces.len() {
            let font_index = self.fallback_faces[i];
            if self.glyph_id_in(ch, font_index).is_some() {
                return self.ensure_glyph_in(ch, size_px, font_index);
            }
        }
        if !self.fallback_tried.insert(script) {
            return None;
        }
        let (data, face_index) = self.discover_fallback(script)?;
        let face = FontFace::new(data, face_index)?;
        let font_index = self.push_fallback_face(face);
        self.fallback_discovered.push(font_index);
        log::info!(
            "font_atlas: fallback face #{} loaded for {:?}",
            font_index,
            script
        );
        self.ensure_glyph_in(ch, size_px, font_index)
    }

    /// Глиф нужного веса: своё начертание семейства/основного шрифта, при
    /// его отсутствии — ближайшее (обычное или жирное) со всей цепочкой
    /// фолбэков (эмодзи, иконки, CJK).
    fn ensure_glyph_weighted(
        &mut self,
        ch: char,
        size_px: u16,
        weight: FontWeight,
        font_family: Option<&str>,
    ) -> Option<GlyphKey> {
        let class = weight.class();
        if matches!(class, 500 | 600) {
            if let Some(idx) = self.weight_face(font_family, class) {
                if let Some(k) = self.ensure_glyph_in(ch, size_px, idx) {
                    return Some(k);
                }
            }
        }
        let bold = class >= 600;
        match font_family {
            Some(fam) => self.ensure_glyph_family(ch, size_px, bold, fam),
            None if bold => self.ensure_glyph_bold(ch, size_px),
            None => self.ensure_glyph(ch, size_px),
        }
    }

    /// Отдельное начертание 500/600, лениво. Если шрифт такого не несёт,
    /// подбор font-kit (правила CSS) вернёт ближайшее — обычное или жирное;
    /// такое совпадение не дублируем, а отвечаем `None` (сработает фолбэк).
    fn weight_face(&mut self, family: Option<&str>, class: u16) -> Option<u8> {
        let key = (family.map(str::to_string), class);
        if let Some(&idx) = self.weight_faces.get(&key) {
            return idx;
        }
        let idx = self.discover_weight_face(family, class);
        self.weight_faces.insert(key, idx);
        idx
    }

    #[cfg(all(not(target_arch = "wasm32"), not(target_os = "android")))]
    fn discover_weight_face(&mut self, family: Option<&str>, class: u16) -> Option<u8> {
        let (data, face_index) = crate::text::font_discovery::discover_weight_font(family, class);
        let face = FontFace::new(data, face_index)?;
        // Тот же файл, что уже загружен (обычное/жирное) — отдельной грани нет.
        let same = |f: &FontFace| f.face_index == face.face_index && f.data[..] == face.data[..];
        if self.faces.iter().flatten().any(same) {
            return None;
        }
        log::info!("font_atlas: начертание {class} для {:?} загружено", family.unwrap_or("основного шрифта"));
        Some(self.push_face(Some(face)))
    }

    #[cfg(any(target_arch = "wasm32", target_os = "android"))]
    fn discover_weight_face(&mut self, _family: Option<&str>, _class: u16) -> Option<u8> {
        None
    }

    fn ensure_glyph_bold(&mut self, ch: char, size_px: u16) -> Option<GlyphKey> {
        if let Some(key) = self.ensure_glyph_in(ch, size_px, FONT_BOLD) {
            return Some(key);
        }
        self.ensure_glyph(ch, size_px)
    }

    fn load_font_family(&mut self, family: &str) -> Option<(u8, u8)> {
        if let Some(&indices) = self.extra_fonts.get(family) {
            return indices;
        }
        let indices = self.discover_font_family(family);
        if indices.is_none() {
            log::warn!(
                "Font family '{}' not found, falling back to primary",
                family
            );
        }
        self.extra_fonts.insert(family.to_string(), indices);
        indices
    }

    #[cfg(all(not(target_arch = "wasm32"), not(target_os = "android")))]
    fn discover_font_family(&mut self, family: &str) -> Option<(u8, u8)> {
        use crate::text::font_discovery;
        let (regular, regular_face) = font_discovery::discover_font(Some(family));
        let regular = FontFace::new(regular, regular_face)?;
        let (bold, bold_face) = font_discovery::discover_bold_font(Some(family));
        let idx_regular = self.push_face(Some(regular));
        let idx_bold = self.push_face(FontFace::new(bold, bold_face));
        Some((idx_regular, idx_bold))
    }

    #[cfg(target_os = "android")]
    fn discover_font_family(&mut self, _family: &str) -> Option<(u8, u8)> {
        None
    }

    #[cfg(target_arch = "wasm32")]
    fn discover_font_family(&mut self, _family: &str) -> Option<(u8, u8)> {
        None
    }

    #[cfg(all(not(target_arch = "wasm32"), not(target_os = "android")))]
    fn discover_fallback(&self, script: Script) -> Option<(Vec<u8>, u32)> {
        crate::text::font_discovery::discover_fallback_font(
            script,
            self.prefer_japanese,
            self.prefer_korean,
        )
    }

    #[cfg(target_os = "android")]
    fn discover_fallback(&self, script: Script) -> Option<(Vec<u8>, u32)> {
        crate::text::font_discovery_android::discover_fallback_font(
            script,
            self.prefer_japanese,
            self.prefer_korean,
        )
    }

    #[cfg(target_arch = "wasm32")]
    fn discover_fallback(&self, _script: Script) -> Option<(Vec<u8>, u32)> {
        None
    }

    fn ensure_glyph_family(
        &mut self,
        ch: char,
        size_px: u16,
        bold: bool,
        family: &str,
    ) -> Option<GlyphKey> {
        if let Some((idx_regular, idx_bold)) = self.load_font_family(family) {
            let font_index = if bold && self.has_face(idx_bold) {
                idx_bold
            } else {
                idx_regular
            };
            if let Some(key) = self.ensure_glyph_in(ch, size_px, font_index) {
                return Some(key);
            }
        }
        if bold {
            self.ensure_glyph_bold(ch, size_px)
        } else {
            self.ensure_glyph(ch, size_px)
        }
    }

    fn rasterize_glyph(
        &mut self,
        face: FontFace,
        glyph_id: u16,
        size_px: u16,
        key: GlyphKey,
    ) -> Option<()> {
        let mut context = std::mem::take(&mut self.scale_ctx);
        let rasterized = self.rasterize_with(&mut context, face, glyph_id, size_px, key);
        self.scale_ctx = context;
        rasterized
    }

    fn rasterize_with(
        &mut self,
        context: &mut swash::scale::ScaleContext,
        face: FontFace,
        glyph_id: u16,
        size_px: u16,
        key: GlyphKey,
    ) -> Option<()> {
        let font_ref = face.font_ref()?;
        use swash::zeno::Format;
        let mut scaler = context
            .builder(font_ref)
            .size(size_px as f32)
            .hint(true)
            .build();

        let glyph_metrics = font_ref.glyph_metrics(&[]);
        let scale = size_px as f32 / font_ref.metrics(&[]).units_per_em as f32;
        let advance = glyph_metrics.advance_width(glyph_id) * scale;

        let (image, is_color) = {
            let color_image = swash::scale::Render::new(&[
                swash::scale::Source::ColorBitmap(swash::scale::StrikeWith::BestFit),
                swash::scale::Source::ColorOutline(0),
            ])
            .format(Format::Subpixel)
            .render(&mut scaler, glyph_id);

            if let Some(img) = color_image {
                if img.placement.width > 0 && img.placement.height > 0 {
                    (Some(img), true)
                } else {
                    (None, false)
                }
            } else {
                let alpha_image = swash::scale::Render::new(&[swash::scale::Source::Outline])
                    .format(Format::Alpha)
                    .render(&mut scaler, glyph_id);
                (alpha_image, false)
            }
        };

        let image = match image {
            Some(img) if img.placement.width > 0 && img.placement.height > 0 => img,
            _ => {
                let cached = CachedGlyph {
                    uv_x: 0.0,
                    uv_y: 0.0,
                    uv_w: 0.0,
                    uv_h: 0.0,
                    width: 0,
                    height: 0,
                    bearing_x: 0.0,
                    bearing_y: 0.0,
                    advance,
                    is_color: false,
                };
                self.glyphs.insert(key, cached);
                return Some(());
            }
        };

        let glyph_w = image.placement.width;
        let glyph_h = image.placement.height;
        let padding = 1u32;

        if glyph_w >= 256 || glyph_h >= 256 {
            log::warn!(
                "font_atlas: oversized glyph gid={} font_index={} size={}px → {}x{} px",
                key.glyph_id,
                key.font_index,
                key.size_px,
                glyph_w,
                glyph_h,
            );
        }

        if self.cursor_x + glyph_w + padding > self.atlas_width {
            self.cursor_y += self.row_height + padding;
            self.cursor_x = padding;
            self.row_height = 0;
        }

        if self.cursor_y + glyph_h + padding > self.atlas_height {
            if !self.overflowed {
                log::warn!("Font atlas full — resetting at the next frame");
            }
            self.overflowed = true;
            return None;
        }

        let atlas_x = self.cursor_x;
        let atlas_y = self.cursor_y;

        if is_color {
            let src_stride = (glyph_w * 4) as usize;
            for row in 0..glyph_h {
                for col in 0..glyph_w {
                    let src_idx = (row as usize * src_stride) + (col as usize * 4);
                    let dst_idx = ((atlas_y + row) * self.atlas_width + atlas_x + col) as usize * 4;
                    if src_idx + 3 < image.data.len() && dst_idx + 3 < self.pixels.len() {
                        self.pixels[dst_idx] = image.data[src_idx];
                        self.pixels[dst_idx + 1] = image.data[src_idx + 1];
                        self.pixels[dst_idx + 2] = image.data[src_idx + 2];
                        self.pixels[dst_idx + 3] = image.data[src_idx + 3];
                    }
                }
            }
        } else {
            for row in 0..glyph_h {
                for col in 0..glyph_w {
                    let src_idx = (row * glyph_w + col) as usize;
                    let dst_idx = ((atlas_y + row) * self.atlas_width + atlas_x + col) as usize * 4;
                    if src_idx < image.data.len() && dst_idx + 3 < self.pixels.len() {
                        let alpha = image.data[src_idx];
                        self.pixels[dst_idx] = 255;
                        self.pixels[dst_idx + 1] = 255;
                        self.pixels[dst_idx + 2] = 255;
                        self.pixels[dst_idx + 3] = alpha;
                    }
                }
            }
        }

        self.cursor_x += glyph_w + padding;
        self.row_height = self.row_height.max(glyph_h);
        self.mark_dirty(atlas_x, atlas_y, glyph_w, glyph_h);

        let cached = CachedGlyph {
            uv_x: atlas_x as f32 / self.atlas_width as f32,
            uv_y: atlas_y as f32 / self.atlas_height as f32,
            uv_w: glyph_w as f32 / self.atlas_width as f32,
            uv_h: glyph_h as f32 / self.atlas_height as f32,
            width: glyph_w,
            height: glyph_h,
            bearing_x: image.placement.left as f32,
            bearing_y: image.placement.top as f32,
            advance,
            is_color,
        };

        self.glyphs.insert(key, cached);
        Some(())
    }

    pub fn memory_stats(&self) -> FontAtlasStats {
        let font_data_bytes = self
            .faces
            .iter()
            .flatten()
            .map(|face| face.data.len())
            .sum::<usize>();
        let pixels_bytes = self.pixels.len();
        let glyph_cache_bytes = self.glyphs.len() * std::mem::size_of::<(GlyphKey, CachedGlyph)>();
        FontAtlasStats {
            glyph_count: self.glyphs.len(),
            atlas_size: (self.atlas_width, self.atlas_height),
            pixels_bytes,
            font_data_bytes,
            total_bytes: pixels_bytes + font_data_bytes + glyph_cache_bytes,
            cursor_x: self.cursor_x,
            cursor_y: self.cursor_y,
            row_height: self.row_height,
        }
    }

    /// Заливает в GPU только изменённую область: новая буква — это десятки
    /// килобайт, а не 16 МБ всего атласа (на Mali такая заливка стоила
    /// десятки миллисекунд на каждом кадре с новым текстом).
    pub fn upload(&mut self, queue: &wgpu::Queue) {
        if !self.dirty {
            return;
        }
        self.dirty = false;
        let [x0, y0, x1, y1] = self.dirty_rect;
        self.dirty_rect = [u32::MAX, u32::MAX, 0, 0];
        let x1 = x1.min(self.atlas_width);
        let y1 = y1.min(self.atlas_height);
        if x0 >= x1 || y0 >= y1 {
            return;
        }
        let stride = (self.atlas_width * 4) as usize;
        let start = (y0 as usize) * stride + (x0 as usize) * 4;
        let end = ((y1 - 1) as usize) * stride + (x1 as usize) * 4;

        queue.write_texture(
            wgpu::TexelCopyTextureInfo {
                texture: &self.texture,
                mip_level: 0,
                origin: wgpu::Origin3d { x: x0, y: y0, z: 0 },
                aspect: wgpu::TextureAspect::All,
            },
            &self.pixels[start..end],
            wgpu::TexelCopyBufferLayout {
                offset: 0,
                bytes_per_row: Some(self.atlas_width * 4),
                rows_per_image: None,
            },
            wgpu::Extent3d {
                width: x1 - x0,
                height: y1 - y0,
                depth_or_array_layers: 1,
            },
        );
    }

    pub fn shape_text(
        &mut self,
        text: &str,
        size_px: u16,
        max_width: f32,
        weight: impl Into<FontWeight>,
        font_family: Option<&str>,
    ) -> Vec<ShapedGlyph> {
        let weight: FontWeight = weight.into();
        let mut result = Vec::new();
        let mut x = 0.0f32;
        let line_height = size_px as f32 * 1.3;
        let mut y = 0.0f32;
        let wrap_eps = f32::EPSILON * max_width.abs() * 64.0;

        let mut word_glyphs: Vec<(usize, f32)> = Vec::new();
        let mut word_width = 0.0f32;

        let units = self.text_units(text, size_px);
        let mut i = 0;
        let mut prev: Option<char> = None;

        while i < units.len() {
            let unit_idx = i;
            let ch = units[i].first_char();
            i += 1;
            let prev_ch = prev.replace(ch);

            if ch == '\n' {
                for &(wch, _) in &word_glyphs {
                    self.emit_unit(&units[wch], size_px, weight, font_family, &mut x, y, &mut result);
                }
                word_glyphs.clear();
                word_width = 0.0;
                x = 0.0;
                y += line_height;
                continue;
            }

            if ch == ' ' || breaks_before(prev_ch, ch) {
                for &(wch, _) in &word_glyphs {
                    self.emit_unit(&units[wch], size_px, weight, font_family, &mut x, y, &mut result);
                }
                word_glyphs.clear();
                word_width = 0.0;
                if ch == ' ' {
                    self.emit_unit(&units[unit_idx], size_px, weight, font_family, &mut x, y, &mut result);
                    continue;
                }
            }

            let advance = self.unit_advance(&units[unit_idx], size_px, weight, font_family);
            word_glyphs.push((unit_idx, advance));
            word_width += advance;

            if max_width > 0.0 && x + word_width > max_width + wrap_eps && x > 0.0 {
                x = 0.0;
                y += line_height;
            }

            if max_width > 0.0 && word_width > max_width + wrap_eps && word_glyphs.len() > 1 {
                let last = word_glyphs.pop().unwrap();
                for &(wch, _) in &word_glyphs {
                    self.emit_unit(&units[wch], size_px, weight, font_family, &mut x, y, &mut result);
                }
                word_glyphs.clear();
                x = 0.0;
                y += line_height;
                word_glyphs.push(last);
                word_width = last.1;
            }
        }

        for &(wch, _) in &word_glyphs {
            self.emit_unit(&units[wch], size_px, weight, font_family, &mut x, y, &mut result);
        }

        result
    }

    fn glyph_advance(
        &mut self,
        ch: char,
        size_px: u16,
        weight: FontWeight,
        font_family: Option<&str>,
    ) -> f32 {
        let key = self.ensure_glyph_weighted(ch, size_px, weight, font_family);
        let key = match key {
            Some(k) => k,
            None => return 0.0,
        };
        self.glyphs.get(&key).map(|g| g.advance).unwrap_or(0.0)
    }

    pub fn shape_text_spaced(
        &mut self,
        text: &str,
        size_px: u16,
        max_width: f32,
        weight: impl Into<FontWeight>,
        font_family: Option<&str>,
        letter_spacing: f32,
    ) -> Vec<ShapedGlyph> {
        let weight: FontWeight = weight.into();
        if letter_spacing.abs() < 0.01 {
            return self.shape_text(text, size_px, max_width, weight, font_family);
        }
        let mut result = Vec::new();
        let mut x = 0.0f32;
        let line_height = size_px as f32 * 1.3;
        let mut y = 0.0f32;
        let wrap_eps = f32::EPSILON * max_width.abs() * 64.0;

        let mut word_glyphs: Vec<(usize, f32)> = Vec::new();
        let mut word_width = 0.0f32;
        let units = self.text_units(text, size_px);
        let mut i = 0;
        let mut prev: Option<char> = None;

        while i < units.len() {
            let unit_idx = i;
            let ch = units[i].first_char();
            i += 1;
            let prev_ch = prev.replace(ch);

            if ch == '\n' {
                for &(wch, _) in &word_glyphs {
                    self.emit_unit_spaced(
                        &units[wch],
                        size_px,
                        weight,
                        font_family,
                        &mut x,
                        y,
                        letter_spacing,
                        &mut result,
                    );
                }
                word_glyphs.clear();
                word_width = 0.0;
                x = 0.0;
                y += line_height;
                continue;
            }

            if ch == ' ' || breaks_before(prev_ch, ch) {
                for &(wch, _) in &word_glyphs {
                    self.emit_unit_spaced(
                        &units[wch],
                        size_px,
                        weight,
                        font_family,
                        &mut x,
                        y,
                        letter_spacing,
                        &mut result,
                    );
                }
                word_glyphs.clear();
                word_width = 0.0;
                if ch == ' ' {
                    self.emit_unit_spaced(
                        &units[unit_idx],
                        size_px,
                        weight,
                        font_family,
                        &mut x,
                        y,
                        letter_spacing,
                        &mut result,
                    );
                    continue;
                }
            }

            let advance = self.unit_advance(&units[unit_idx], size_px, weight, font_family) + letter_spacing;
            word_glyphs.push((unit_idx, advance));
            word_width += advance;

            if max_width > 0.0 && x + word_width > max_width + wrap_eps && x > 0.0 {
                x = 0.0;
                y += line_height;
            }
            if max_width > 0.0 && word_width > max_width + wrap_eps && word_glyphs.len() > 1 {
                let last = word_glyphs.pop().unwrap();
                for &(wch, _) in &word_glyphs {
                    self.emit_unit_spaced(
                        &units[wch],
                        size_px,
                        weight,
                        font_family,
                        &mut x,
                        y,
                        letter_spacing,
                        &mut result,
                    );
                }
                word_glyphs.clear();
                x = 0.0;
                y += line_height;
                word_glyphs.push(last);
                word_width = last.1;
            }
        }

        for &(wch, _) in &word_glyphs {
            self.emit_unit_spaced(
                        &units[wch],
                size_px,
                weight,
                font_family,
                &mut x,
                y,
                letter_spacing,
                &mut result,
            );
        }
        result
    }

    fn emit_glyph_spaced(
        &mut self,
        ch: char,
        size_px: u16,
        weight: FontWeight,
        font_family: Option<&str>,
        x: &mut f32,
        y: f32,
        letter_spacing: f32,
        result: &mut Vec<ShapedGlyph>,
    ) {
        let key = self.ensure_glyph_weighted(ch, size_px, weight, font_family);
        let key = match key {
            Some(k) => k,
            None => return,
        };
        let glyph = match self.glyphs.get(&key) {
            Some(g) => *g,
            None => return,
        };
        if glyph.width > 0 && glyph.height > 0 {
            result.push(ShapedGlyph {
                x: *x + glyph.bearing_x,
                y: y - glyph.bearing_y + size_px as f32,
                glyph,
            });
        }
        *x += glyph.advance + letter_spacing;
    }

    fn emit_glyph(
        &mut self,
        ch: char,
        size_px: u16,
        weight: FontWeight,
        font_family: Option<&str>,
        x: &mut f32,
        y: f32,
        result: &mut Vec<ShapedGlyph>,
    ) {
        let key = self.ensure_glyph_weighted(ch, size_px, weight, font_family);
        let key = match key {
            Some(k) => k,
            None => return,
        };
        let glyph = match self.glyphs.get(&key) {
            Some(g) => *g,
            None => return,
        };
        if glyph.width > 0 && glyph.height > 0 {
            result.push(ShapedGlyph {
                x: *x + glyph.bearing_x,
                y: y - glyph.bearing_y + size_px as f32,
                glyph,
            });
        }
        *x += glyph.advance;
    }

    pub fn has_font(&self) -> bool {
        self.has_face(FONT_REGULAR)
    }
    pub fn measure_text_width(
        &mut self,
        text: &str,
        size_px: u16,
        pos: usize,
        font_family: Option<&str>,
    ) -> f32 {
        self.measure_text_width_styled(text, size_px, pos, false, font_family)
    }

    pub fn measure_text_width_styled(
        &mut self,
        text: &str,
        size_px: u16,
        pos: usize,
        weight: impl Into<FontWeight>,
        font_family: Option<&str>,
    ) -> f32 {
        let weight: FontWeight = weight.into();
        // `pos` — в символах исходного текста; кластер эмодзи идёт целиком.
        let mut x = 0.0f32;
        let mut char_count = 0;
        for unit in self.text_units(text, size_px) {
            if char_count >= pos {
                break;
            }
            x += self.unit_advance(&unit, size_px, weight, font_family);
            char_count += unit.len();
        }
        x
    }

    pub fn hit_test_char_position(
        &mut self,
        text: &str,
        size_px: u16,
        x_offset: f32,
        font_family: Option<&str>,
    ) -> usize {
        self.hit_test_char_position_styled(text, size_px, x_offset, false, font_family)
    }

    pub fn hit_test_char_position_styled(
        &mut self,
        text: &str,
        size_px: u16,
        x_offset: f32,
        weight: impl Into<FontWeight>,
        font_family: Option<&str>,
    ) -> usize {
        let weight: FontWeight = weight.into();
        // Индекс символа, перед которым встанет каретка: внутрь кластера
        // эмодзи она не попадает.
        let mut x = 0.0f32;
        let mut idx = 0;
        for unit in self.text_units(text, size_px) {
            let advance = self.unit_advance(&unit, size_px, weight, font_family);
            if x_offset < x + advance * 0.5 {
                return idx;
            }
            x += advance;
            idx += unit.len();
        }
        idx
    }
}

impl crate::widget::context::TextMeasure for crate::core::sync::Mutex<FontAtlas> {
    fn measure_text_width(&self, text: &str, font_size: f32, char_count: usize) -> f32 {
        let mut atlas = self.lock().unwrap_or_else(|e| e.into_inner());
        let sf = atlas.scale_factor();
        let size_px = ((font_size * sf).round() as u16).max(1);
        let phys = atlas.measure_text_width(text, size_px, char_count, None);
        phys / sf
    }

    fn measure_text_width_styled(
        &self,
        text: &str,
        font_size: f32,
        char_count: usize,
        bold: bool,
        font_family: Option<&str>,
    ) -> f32 {
        let mut atlas = self.lock().unwrap_or_else(|e| e.into_inner());
        let sf = atlas.scale_factor();
        let size_px = ((font_size * sf).round() as u16).max(1);
        let phys = atlas.measure_text_width_styled(text, size_px, char_count, bold, font_family);
        phys / sf
    }

    fn measure_text_width_styled_ls(
        &self,
        text: &str,
        font_size: f32,
        char_count: usize,
        bold: bool,
        font_family: Option<&str>,
        letter_spacing: f32,
    ) -> f32 {
        if letter_spacing.abs() < 0.01 {
            return self.measure_text_width_styled(text, font_size, char_count, bold, font_family);
        }
        let mut atlas = self.lock().unwrap_or_else(|e| e.into_inner());
        let sf = atlas.scale_factor();
        let size_px = ((font_size * sf).round() as u16).max(1);
        let phys_base =
            atlas.measure_text_width_styled(text, size_px, char_count, bold, font_family);
        let visible = text.chars().take(char_count).count();
        let phys = phys_base + (letter_spacing * sf) * (visible as f32);
        phys / sf
    }

    fn measure_text_width_weight(
        &self,
        text: &str,
        font_size: f32,
        char_count: usize,
        weight: u16,
        font_family: Option<&str>,
    ) -> f32 {
        let mut atlas = self.lock().unwrap_or_else(|e| e.into_inner());
        let sf = atlas.scale_factor();
        let size_px = ((font_size * sf).round() as u16).max(1);
        atlas.measure_text_width_styled(text, size_px, char_count, weight, font_family) / sf
    }

    fn measure_text_width_weight_ls(
        &self,
        text: &str,
        font_size: f32,
        char_count: usize,
        weight: u16,
        font_family: Option<&str>,
        letter_spacing: f32,
    ) -> f32 {
        let base = self.measure_text_width_weight(text, font_size, char_count, weight, font_family);
        if letter_spacing.abs() < 0.01 {
            return base;
        }
        base + letter_spacing * text.chars().take(char_count).count() as f32
    }

    fn hit_test_char_weight(
        &self,
        text: &str,
        font_size: f32,
        x_offset: f32,
        weight: u16,
        font_family: Option<&str>,
    ) -> usize {
        let mut atlas = self.lock().unwrap_or_else(|e| e.into_inner());
        let sf = atlas.scale_factor();
        let size_px = ((font_size * sf).round() as u16).max(1);
        atlas.hit_test_char_position_styled(text, size_px, x_offset * sf, weight, font_family)
    }

    fn hit_test_char(&self, text: &str, font_size: f32, x_offset: f32) -> usize {
        let mut atlas = self.lock().unwrap_or_else(|e| e.into_inner());
        let sf = atlas.scale_factor();
        let size_px = ((font_size * sf).round() as u16).max(1);
        atlas.hit_test_char_position(text, size_px, x_offset * sf, None)
    }

    fn hit_test_char_styled(
        &self,
        text: &str,
        font_size: f32,
        x_offset: f32,
        font_family: Option<&str>,
    ) -> usize {
        let mut atlas = self.lock().unwrap_or_else(|e| e.into_inner());
        let sf = atlas.scale_factor();
        let size_px = ((font_size * sf).round() as u16).max(1);
        atlas.hit_test_char_position_styled(text, size_px, x_offset * sf, false, font_family)
    }

    fn hit_test_char_weighted(
        &self,
        text: &str,
        font_size: f32,
        x_offset: f32,
        bold: bool,
        font_family: Option<&str>,
    ) -> usize {
        let mut atlas = self.lock().unwrap_or_else(|e| e.into_inner());
        let sf = atlas.scale_factor();
        let size_px = ((font_size * sf).round() as u16).max(1);
        atlas.hit_test_char_position_styled(text, size_px, x_offset * sf, bold, font_family)
    }
}

#[cfg(test)]
mod cluster_tests {
    use super::emoji_cluster_len;

    fn len(s: &str) -> usize {
        let chars: Vec<char> = s.chars().collect();
        emoji_cluster_len(&chars, 0)
    }

    #[test]
    fn clusters_are_detected() {
        assert_eq!(len("🧑\u{200D}💻x"), 3, "ZWJ-последовательность");
        assert_eq!(len("❤\u{FE0F}!"), 2, "VS16");
        assert_eq!(len("👍🏽"), 2, "тон кожи");
        assert_eq!(len("🇰🇿"), 2, "флаг");
        assert_eq!(len("1\u{FE0F}\u{20E3}"), 3, "keycap");
        assert_eq!(len("👨\u{200D}👩\u{200D}👧"), 5, "семья");
        assert_eq!(len("🙂 "), 1);
        assert_eq!(len("a"), 1);
        assert_eq!(len("1a"), 1);
    }

    /// Системный шрифт эмодзи сводит ZWJ-последовательность к одному глифу.
    #[cfg(all(not(target_arch = "wasm32"), not(target_os = "android")))]
    #[test]
    fn zwj_sequence_shapes_to_one_glyph() {
        let (data, idx) = crate::text::font_discovery::discover_emoji_font();
        if data.is_empty() {
            return;
        }
        let face = rustybuzz::Face::from_slice(&data, idx).expect("шрифт эмодзи");
        let mut buf = rustybuzz::UnicodeBuffer::new();
        buf.push_str("🧑\u{200D}💻");
        let out = rustybuzz::shape(&face, &[], buf);
        let ids: Vec<u32> = out.glyph_infos().iter().map(|g| g.glyph_id).collect();
        assert_eq!(ids.len(), 1, "ожидали лигатуру, получили {ids:?}");
        assert_ne!(ids[0], 0);
    }
}
