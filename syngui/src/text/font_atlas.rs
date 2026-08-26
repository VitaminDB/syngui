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
        Some(Self { data: Arc::from(data), face_index })
    }

    fn font_ref(&self) -> Option<swash::FontRef<'_>> {
        swash::FontRef::from_index(&self.data, self.face_index as usize)
    }
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
    overflowed: bool,
    generation: u64,
    scale_factor: f32,
}

impl FontAtlas {
    pub fn new(device: &wgpu::Device, queue: &wgpu::Queue) -> Self {
        Self::with_config(device, queue, None)
    }

    pub fn with_config(device: &wgpu::Device, queue: &wgpu::Queue, preferred_family: Option<String>) -> Self {
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

        let ((font_data, font_face_index), (bold_font_data, bold_face_index), (emoji_font_data, emoji_face_index)) = Self::load_all_fonts(preferred_family);

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
            overflowed: false,
            generation: 0,
            scale_factor: 1.0,
        }
    }

    pub fn scale_factor(&self) -> f32 {
        self.scale_factor
    }

    pub fn set_scale_factor(&mut self, sf: f32) {
        self.scale_factor = sf.clamp(0.1, 8.0);
    }

    #[cfg(all(not(target_arch = "wasm32"), not(target_os = "android")))]
    fn load_all_fonts(preferred_family: Option<String>) -> ((Vec<u8>, u32), (Vec<u8>, u32), (Vec<u8>, u32)) {
        use crate::text::font_discovery;

        let family = preferred_family.as_deref();
        let font_data = font_discovery::discover_font(family);
        let bold_font_data = font_discovery::discover_bold_font(family);
        let emoji_font_data = font_discovery::discover_emoji_font();

        (font_data, bold_font_data, emoji_font_data)
    }

    #[cfg(target_os = "android")]
    fn load_all_fonts(preferred_family: Option<String>) -> ((Vec<u8>, u32), (Vec<u8>, u32), (Vec<u8>, u32)) {
        use crate::text::font_discovery_android;

        let family = preferred_family.as_deref();
        let font_data = font_discovery_android::discover_font(family);
        let bold_font_data = font_discovery_android::discover_bold_font(family);
        let emoji_font_data = font_discovery_android::discover_emoji_font();

        (font_data, bold_font_data, emoji_font_data)
    }

    #[cfg(target_arch = "wasm32")]
    fn load_all_fonts(_preferred_family: Option<String>) -> ((Vec<u8>, u32), (Vec<u8>, u32), (Vec<u8>, u32)) {
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
        log::info!("font_atlas: reset after overflow, generation {}", self.generation);
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
    }

    fn face(&self, font_index: u8) -> Option<FontFace> {
        self.faces.get(font_index as usize).and_then(|face| face.clone())
    }

    fn has_face(&self, font_index: u8) -> bool {
        self.faces.get(font_index as usize).is_some_and(|face| face.is_some())
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

    fn is_likely_emoji(ch: char) -> bool {
        let c = ch as u32;
        (0x1F300..=0x1FAFF).contains(&c)
            || (0x2600..=0x27BF).contains(&c)
            || (0xFE00..=0xFE0F).contains(&c)
            || (0x200D..=0x200D).contains(&c)
            || (0x2300..=0x23FF).contains(&c)
            || (0x2B50..=0x2B55).contains(&c)
            || (0x25A0..=0x25FF).contains(&c)
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
        let key = GlyphKey { glyph_id, size_px, font_index };
        if self.glyphs.contains_key(&key) {
            return Some(key);
        }
        let face = self.face(font_index)?;
        self.rasterize_glyph(face, glyph_id, size_px, key).map(|_| key)
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
        log::info!("font_atlas: fallback face #{} loaded for {:?}", font_index, script);
        self.ensure_glyph_in(ch, size_px, font_index)
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
            log::warn!("Font family '{}' not found, falling back to primary", family);
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
        crate::text::font_discovery::discover_fallback_font(script, self.prefer_japanese, self.prefer_korean)
    }

    #[cfg(target_os = "android")]
    fn discover_fallback(&self, script: Script) -> Option<(Vec<u8>, u32)> {
        crate::text::font_discovery_android::discover_fallback_font(script, self.prefer_japanese, self.prefer_korean)
    }

    #[cfg(target_arch = "wasm32")]
    fn discover_fallback(&self, _script: Script) -> Option<(Vec<u8>, u32)> {
        None
    }

    fn ensure_glyph_family(&mut self, ch: char, size_px: u16, bold: bool, family: &str) -> Option<GlyphKey> {
        if let Some((idx_regular, idx_bold)) = self.load_font_family(family) {
            let font_index = if bold && self.has_face(idx_bold) { idx_bold } else { idx_regular };
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

    fn rasterize_glyph(&mut self, face: FontFace, glyph_id: u16, size_px: u16, key: GlyphKey) -> Option<()> {
        let font_ref = face.font_ref()?;

        use swash::scale::ScaleContext;
        use swash::zeno::Format;

        let mut context = ScaleContext::new();
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
                let alpha_image = swash::scale::Render::new(&[
                    swash::scale::Source::Outline,
                ])
                .format(Format::Alpha)
                .render(&mut scaler, glyph_id);
                (alpha_image, false)
            }
        };

        let image = match image {
            Some(img) if img.placement.width > 0 && img.placement.height > 0 => img,
            _ => {
                let cached = CachedGlyph {
                    uv_x: 0.0, uv_y: 0.0, uv_w: 0.0, uv_h: 0.0,
                    width: 0, height: 0,
                    bearing_x: 0.0, bearing_y: 0.0,
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
        self.dirty = true;

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
        let font_data_bytes = self.faces.iter().flatten().map(|face| face.data.len()).sum::<usize>();
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

    pub fn upload(&mut self, queue: &wgpu::Queue) {
        if !self.dirty {
            return;
        }
        self.dirty = false;

        queue.write_texture(
            wgpu::TexelCopyTextureInfo {
                texture: &self.texture,
                mip_level: 0,
                origin: wgpu::Origin3d::ZERO,
                aspect: wgpu::TextureAspect::All,
            },
            &self.pixels,
            wgpu::TexelCopyBufferLayout {
                offset: 0,
                bytes_per_row: Some(self.atlas_width * 4),
                rows_per_image: None,
            },
            wgpu::Extent3d {
                width: self.atlas_width,
                height: self.atlas_height,
                depth_or_array_layers: 1,
            },
        );
    }

    pub fn shape_text(&mut self, text: &str, size_px: u16, max_width: f32, bold: bool, font_family: Option<&str>) -> Vec<ShapedGlyph> {
        let mut result = Vec::new();
        let mut x = 0.0f32;
        let line_height = size_px as f32 * 1.3;
        let mut y = 0.0f32;
        let wrap_eps = f32::EPSILON * max_width.abs() * 64.0;

        let mut word_glyphs: Vec<(char, f32)> = Vec::new();
        let mut word_width = 0.0f32;

        let chars: Vec<char> = text.chars().collect();
        let mut i = 0;
        let mut prev: Option<char> = None;

        while i < chars.len() {
            let ch = chars[i];
            i += 1;
            let prev_ch = prev.replace(ch);

            if ch == '\n' {
                for &(wch, _) in &word_glyphs {
                    self.emit_glyph(wch, size_px, bold, font_family, &mut x, y, &mut result);
                }
                word_glyphs.clear();
                word_width = 0.0;
                x = 0.0;
                y += line_height;
                continue;
            }

            if ch == ' ' || breaks_before(prev_ch, ch) {
                for &(wch, _) in &word_glyphs {
                    self.emit_glyph(wch, size_px, bold, font_family, &mut x, y, &mut result);
                }
                word_glyphs.clear();
                word_width = 0.0;
                if ch == ' ' {
                    self.emit_glyph(ch, size_px, bold, font_family, &mut x, y, &mut result);
                    continue;
                }
            }

            let advance = self.glyph_advance(ch, size_px, bold, font_family);
            word_glyphs.push((ch, advance));
            word_width += advance;

            if max_width > 0.0 && x + word_width > max_width + wrap_eps && x > 0.0 {
                x = 0.0;
                y += line_height;
            }

            if max_width > 0.0 && word_width > max_width + wrap_eps && word_glyphs.len() > 1 {
                let last = word_glyphs.pop().unwrap();
                for &(wch, _) in &word_glyphs {
                    self.emit_glyph(wch, size_px, bold, font_family, &mut x, y, &mut result);
                }
                word_glyphs.clear();
                x = 0.0;
                y += line_height;
                word_glyphs.push(last);
                word_width = last.1;
            }
        }

        for &(wch, _) in &word_glyphs {
            self.emit_glyph(wch, size_px, bold, font_family, &mut x, y, &mut result);
        }

        result
    }

    fn glyph_advance(&mut self, ch: char, size_px: u16, bold: bool, font_family: Option<&str>) -> f32 {
        let key = match font_family {
            Some(fam) => self.ensure_glyph_family(ch, size_px, bold, fam),
            None if bold => self.ensure_glyph_bold(ch, size_px),
            None => self.ensure_glyph(ch, size_px),
        };
        let key = match key {
            Some(k) => k,
            None => return 0.0,
        };
        self.glyphs.get(&key).map(|g| g.advance).unwrap_or(0.0)
    }

    pub fn shape_text_spaced(&mut self, text: &str, size_px: u16, max_width: f32, bold: bool, font_family: Option<&str>, letter_spacing: f32) -> Vec<ShapedGlyph> {
        if letter_spacing.abs() < 0.01 {
            return self.shape_text(text, size_px, max_width, bold, font_family);
        }
        let mut result = Vec::new();
        let mut x = 0.0f32;
        let line_height = size_px as f32 * 1.3;
        let mut y = 0.0f32;
        let wrap_eps = f32::EPSILON * max_width.abs() * 64.0;

        let mut word_glyphs: Vec<(char, f32)> = Vec::new();
        let mut word_width = 0.0f32;
        let chars: Vec<char> = text.chars().collect();
        let mut i = 0;
        let mut prev: Option<char> = None;

        while i < chars.len() {
            let ch = chars[i];
            i += 1;
            let prev_ch = prev.replace(ch);

            if ch == '\n' {
                for &(wch, _) in &word_glyphs {
                    self.emit_glyph_spaced(wch, size_px, bold, font_family, &mut x, y, letter_spacing, &mut result);
                }
                word_glyphs.clear();
                word_width = 0.0;
                x = 0.0;
                y += line_height;
                continue;
            }

            if ch == ' ' || breaks_before(prev_ch, ch) {
                for &(wch, _) in &word_glyphs {
                    self.emit_glyph_spaced(wch, size_px, bold, font_family, &mut x, y, letter_spacing, &mut result);
                }
                word_glyphs.clear();
                word_width = 0.0;
                if ch == ' ' {
                    self.emit_glyph_spaced(ch, size_px, bold, font_family, &mut x, y, letter_spacing, &mut result);
                    continue;
                }
            }

            let advance = self.glyph_advance(ch, size_px, bold, font_family) + letter_spacing;
            word_glyphs.push((ch, advance));
            word_width += advance;

            if max_width > 0.0 && x + word_width > max_width + wrap_eps && x > 0.0 {
                x = 0.0;
                y += line_height;
            }
            if max_width > 0.0 && word_width > max_width + wrap_eps && word_glyphs.len() > 1 {
                let last = word_glyphs.pop().unwrap();
                for &(wch, _) in &word_glyphs {
                    self.emit_glyph_spaced(wch, size_px, bold, font_family, &mut x, y, letter_spacing, &mut result);
                }
                word_glyphs.clear();
                x = 0.0;
                y += line_height;
                word_glyphs.push(last);
                word_width = last.1;
            }
        }

        for &(wch, _) in &word_glyphs {
            self.emit_glyph_spaced(wch, size_px, bold, font_family, &mut x, y, letter_spacing, &mut result);
        }
        result
    }

    fn emit_glyph_spaced(&mut self, ch: char, size_px: u16, bold: bool, font_family: Option<&str>, x: &mut f32, y: f32, letter_spacing: f32, result: &mut Vec<ShapedGlyph>) {
        let key = match font_family {
            Some(fam) => self.ensure_glyph_family(ch, size_px, bold, fam),
            None if bold => self.ensure_glyph_bold(ch, size_px),
            None => self.ensure_glyph(ch, size_px),
        };
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

    fn emit_glyph(&mut self, ch: char, size_px: u16, bold: bool, font_family: Option<&str>, x: &mut f32, y: f32, result: &mut Vec<ShapedGlyph>) {
        let key = match font_family {
            Some(fam) => self.ensure_glyph_family(ch, size_px, bold, fam),
            None if bold => self.ensure_glyph_bold(ch, size_px),
            None => self.ensure_glyph(ch, size_px),
        };
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
    pub fn measure_text_width(&mut self, text: &str, size_px: u16, pos: usize, font_family: Option<&str>) -> f32 {
        self.measure_text_width_styled(text, size_px, pos, false, font_family)
    }

    pub fn measure_text_width_styled(&mut self, text: &str, size_px: u16, pos: usize, bold: bool, font_family: Option<&str>) -> f32 {
        let mut x = 0.0f32;
        let mut char_count = 0;

        for ch in text.chars() {
            if char_count >= pos {
                break;
            }

            let key = match font_family {
                Some(fam) => self.ensure_glyph_family(ch, size_px, bold, fam),
                None => if bold { self.ensure_glyph_bold(ch, size_px) } else { self.ensure_glyph(ch, size_px) },
            };
            let key = match key {
                Some(k) => k,
                None => {
                    char_count += 1;
                    continue;
                }
            };

            let glyph = match self.glyphs.get(&key) {
                Some(g) => *g,
                None => {
                    char_count += 1;
                    continue;
                }
            };

            x += glyph.advance;
            char_count += 1;
        }
        
        x
    }

    pub fn hit_test_char_position(&mut self, text: &str, size_px: u16, x_offset: f32, font_family: Option<&str>) -> usize {
        let mut x = 0.0f32;
        let mut best_idx = 0;

        for (idx, ch) in text.chars().enumerate() {
            let key = match font_family {
                Some(fam) => self.ensure_glyph_family(ch, size_px, false, fam),
                None => self.ensure_glyph(ch, size_px),
            };
            let key = match key {
                Some(k) => k,
                None => { continue; }
            };
            let advance = match self.glyphs.get(&key) {
                Some(g) => g.advance,
                None => { continue; }
            };
            let mid = x + advance * 0.5;
            if x_offset < mid {
                return idx;
            }
            x += advance;
            best_idx = idx + 1;
        }
        best_idx
    }
}

impl crate::widget::context::TextMeasure for crate::core::sync::Mutex<FontAtlas> {
    fn measure_text_width(&self, text: &str, font_size: f32, char_count: usize) -> f32 {
        let mut atlas = self.lock().unwrap();
        let sf = atlas.scale_factor();
        let size_px = ((font_size * sf).round() as u16).max(1);
        let phys = atlas.measure_text_width(text, size_px, char_count, None);
        phys / sf
    }

    fn measure_text_width_styled(&self, text: &str, font_size: f32, char_count: usize, bold: bool, font_family: Option<&str>) -> f32 {
        let mut atlas = self.lock().unwrap();
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
        let mut atlas = self.lock().unwrap();
        let sf = atlas.scale_factor();
        let size_px = ((font_size * sf).round() as u16).max(1);
        let phys_base = atlas.measure_text_width_styled(text, size_px, char_count, bold, font_family);
        let visible = text.chars().take(char_count).count();
        let phys = phys_base + (letter_spacing * sf) * (visible as f32);
        phys / sf
    }

    fn hit_test_char(&self, text: &str, font_size: f32, x_offset: f32) -> usize {
        let mut atlas = self.lock().unwrap();
        let sf = atlas.scale_factor();
        let size_px = ((font_size * sf).round() as u16).max(1);
        atlas.hit_test_char_position(text, size_px, x_offset * sf, None)
    }

    fn hit_test_char_styled(&self, text: &str, font_size: f32, x_offset: f32, font_family: Option<&str>) -> usize {
        let mut atlas = self.lock().unwrap();
        let sf = atlas.scale_factor();
        let size_px = ((font_size * sf).round() as u16).max(1);
        atlas.hit_test_char_position(text, size_px, x_offset * sf, font_family)
    }
}
