use hashbrown::HashMap;
use std::collections::VecDeque;

const TILE_SIZE: u32 = 256;
const MIN_ATLAS_SIZE: u32 = 2048;

#[derive(Clone, Copy, Hash, Eq, PartialEq, Debug)]
pub struct TileKey {
    pub x: u32,
    pub y: u32,
    pub z: u8,
    pub provider_id: u8,
}

#[derive(Clone, Copy, Debug)]
pub struct TileSlot {
    pub uv_x: f32,
    pub uv_y: f32,
    pub uv_w: f32,
    pub uv_h: f32,
    slot_index: usize,
}

pub struct TileAtlas {
    texture: wgpu::Texture,
    pub texture_view: wgpu::TextureView,
    pub sampler: wgpu::Sampler,
    atlas_width: u32,
    atlas_height: u32,
    tiles_per_row: u32,
    generation: u32,
    tiles: HashMap<TileKey, TileSlot>,
    lru: VecDeque<TileKey>,
    slot_occupied: Vec<bool>,
    slot_in_frame: Vec<bool>,
    pending: Vec<(usize, Vec<u8>)>,
}

impl TileAtlas {
    pub fn new(device: &wgpu::Device, queue: &wgpu::Queue) -> Self {
        Self::with_capacity(device, queue, 0)
    }

    pub fn with_capacity(device: &wgpu::Device, queue: &wgpu::Queue, tiles_needed: usize) -> Self {
        let max_dim = Self::max_side(device);
        let (atlas_width, atlas_height) =
            Self::grow_to_fit(MIN_ATLAS_SIZE, MIN_ATLAS_SIZE, tiles_needed, max_dim);
        let (texture, texture_view) = Self::create_texture(device, queue, atlas_width, atlas_height);

        let sampler = device.create_sampler(&wgpu::SamplerDescriptor {
            label: Some("Tile Sampler"),
            address_mode_u: wgpu::AddressMode::ClampToEdge,
            address_mode_v: wgpu::AddressMode::ClampToEdge,
            mag_filter: wgpu::FilterMode::Linear,
            min_filter: wgpu::FilterMode::Linear,
            ..Default::default()
        });

        let tiles_per_row = atlas_width / TILE_SIZE;
        let slots = (tiles_per_row * (atlas_height / TILE_SIZE)) as usize;

        Self {
            texture,
            texture_view,
            sampler,
            atlas_width,
            atlas_height,
            tiles_per_row,
            generation: 0,
            tiles: HashMap::new(),
            lru: VecDeque::new(),
            slot_occupied: vec![false; slots],
            slot_in_frame: vec![false; slots],
            pending: Vec::new(),
        }
    }

    fn max_side(device: &wgpu::Device) -> u32 {
        (device.limits().max_texture_dimension_2d / TILE_SIZE).max(1) * TILE_SIZE
    }

    fn grow_to_fit(mut width: u32, mut height: u32, tiles_needed: usize, max_side: u32) -> (u32, u32) {
        width = width.min(max_side).max(TILE_SIZE);
        height = height.min(max_side).max(TILE_SIZE);
        loop {
            let slots = ((width / TILE_SIZE) * (height / TILE_SIZE)) as usize;
            if slots >= tiles_needed {
                return (width, height);
            }
            if width <= height && width * 2 <= max_side {
                width *= 2;
            } else if height * 2 <= max_side {
                height *= 2;
            } else {
                return (width, height);
            }
        }
    }

    fn create_texture(
        device: &wgpu::Device,
        queue: &wgpu::Queue,
        width: u32,
        height: u32,
    ) -> (wgpu::Texture, wgpu::TextureView) {
        let texture = device.create_texture(&wgpu::TextureDescriptor {
            label: Some("Tile Atlas"),
            size: wgpu::Extent3d {
                width,
                height,
                depth_or_array_layers: 1,
            },
            mip_level_count: 1,
            sample_count: 1,
            dimension: wgpu::TextureDimension::D2,
            format: wgpu::TextureFormat::Rgba8UnormSrgb,
            usage: wgpu::TextureUsages::TEXTURE_BINDING | wgpu::TextureUsages::COPY_DST,
            view_formats: &[],
        });

        let band = vec![0u8; (width * TILE_SIZE * 4) as usize];
        let mut written = 0u32;
        while written < height {
            let rows = TILE_SIZE.min(height - written);
            queue.write_texture(
                wgpu::TexelCopyTextureInfo {
                    texture: &texture,
                    mip_level: 0,
                    origin: wgpu::Origin3d { x: 0, y: written, z: 0 },
                    aspect: wgpu::TextureAspect::All,
                },
                &band[..(width * rows * 4) as usize],
                wgpu::TexelCopyBufferLayout {
                    offset: 0,
                    bytes_per_row: Some(width * 4),
                    rows_per_image: None,
                },
                wgpu::Extent3d {
                    width,
                    height: rows,
                    depth_or_array_layers: 1,
                },
            );
            written += rows;
        }

        let view = texture.create_view(&wgpu::TextureViewDescriptor::default());
        (texture, view)
    }

    pub fn capacity(&self) -> usize {
        self.slot_occupied.len()
    }

    pub fn atlas_dimensions(&self) -> (u32, u32) {
        (self.atlas_width, self.atlas_height)
    }

    pub fn generation(&self) -> u32 {
        self.generation
    }

    pub fn ensure_capacity(&mut self, device: &wgpu::Device, queue: &wgpu::Queue, tiles_needed: usize) -> bool {
        if tiles_needed <= self.capacity() {
            return false;
        }

        let (width, height) = Self::grow_to_fit(
            self.atlas_width,
            self.atlas_height,
            tiles_needed,
            Self::max_side(device),
        );
        if width == self.atlas_width && height == self.atlas_height {
            return false;
        }

        let (texture, view) = Self::create_texture(device, queue, width, height);
        self.texture = texture;
        self.texture_view = view;
        self.atlas_width = width;
        self.atlas_height = height;
        self.tiles_per_row = width / TILE_SIZE;
        let slots = (self.tiles_per_row * (height / TILE_SIZE)) as usize;
        self.slot_occupied = vec![false; slots];
        self.slot_in_frame = vec![false; slots];
        self.tiles.clear();
        self.lru.clear();
        self.pending.clear();
        self.generation = self.generation.wrapping_add(1);
        true
    }

    pub fn end_frame(&mut self) {
        for used in self.slot_in_frame.iter_mut() {
            *used = false;
        }
    }

    pub fn get_tile(&mut self, key: &TileKey) -> Option<TileSlot> {
        let slot = self.tiles.get(key).copied()?;
        if let Some(pos) = self.lru.iter().position(|k| k == key) {
            self.lru.remove(pos);
        }
        self.lru.push_back(*key);
        self.slot_in_frame[slot.slot_index] = true;
        Some(slot)
    }

    pub fn insert_tile(&mut self, key: TileKey, rgba: &[u8]) -> Option<TileSlot> {
        if let Some(slot) = self.get_tile(&key) {
            return Some(slot);
        }

        let expected = (TILE_SIZE * TILE_SIZE * 4) as usize;
        if rgba.len() < expected {
            log::warn!(
                "TileAtlas: тайл {:?} отброшен — данных {} байт вместо {}",
                key,
                rgba.len(),
                expected
            );
            return None;
        }

        let slot_index = self.find_or_evict_slot()?;

        let col = (slot_index as u32) % self.tiles_per_row;
        let row = (slot_index as u32) / self.tiles_per_row;
        let px = col * TILE_SIZE;
        let py = row * TILE_SIZE;

        self.pending.push((slot_index, rgba[..expected].to_vec()));

        let inset = 0.5;
        let slot = TileSlot {
            uv_x: (px as f32 + inset) / self.atlas_width as f32,
            uv_y: (py as f32 + inset) / self.atlas_height as f32,
            uv_w: (TILE_SIZE as f32 - inset * 2.0) / self.atlas_width as f32,
            uv_h: (TILE_SIZE as f32 - inset * 2.0) / self.atlas_height as f32,
            slot_index,
        };

        self.slot_occupied[slot_index] = true;
        self.slot_in_frame[slot_index] = true;
        self.tiles.insert(key, slot);
        self.lru.push_back(key);

        Some(slot)
    }

    pub fn upload(&mut self, queue: &wgpu::Queue) {
        if self.pending.is_empty() {
            return;
        }

        for (slot_index, rgba) in self.pending.drain(..) {
            let col = (slot_index as u32) % self.tiles_per_row;
            let row = (slot_index as u32) / self.tiles_per_row;
            queue.write_texture(
                wgpu::TexelCopyTextureInfo {
                    texture: &self.texture,
                    mip_level: 0,
                    origin: wgpu::Origin3d {
                        x: col * TILE_SIZE,
                        y: row * TILE_SIZE,
                        z: 0,
                    },
                    aspect: wgpu::TextureAspect::All,
                },
                &rgba,
                wgpu::TexelCopyBufferLayout {
                    offset: 0,
                    bytes_per_row: Some(TILE_SIZE * 4),
                    rows_per_image: Some(TILE_SIZE),
                },
                wgpu::Extent3d {
                    width: TILE_SIZE,
                    height: TILE_SIZE,
                    depth_or_array_layers: 1,
                },
            );
        }
    }

    pub fn clear_provider(&mut self, provider_id: u8) {
        let keys_to_remove: Vec<TileKey> = self.tiles.keys()
            .filter(|k| k.provider_id == provider_id)
            .copied()
            .collect();

        for key in keys_to_remove {
            if let Some(slot) = self.tiles.remove(&key) {
                self.slot_occupied[slot.slot_index] = false;
                self.slot_in_frame[slot.slot_index] = false;
                self.pending.retain(|(idx, _)| *idx != slot.slot_index);
            }
            self.lru.retain(|k| k != &key);
        }
    }

    fn find_or_evict_slot(&mut self) -> Option<usize> {
        if let Some(idx) = self.slot_occupied.iter().position(|&o| !o) {
            return Some(idx);
        }

        let evict_key = self
            .lru
            .iter()
            .find(|k| {
                self.tiles
                    .get(*k)
                    .map(|s| !self.slot_in_frame[s.slot_index])
                    .unwrap_or(false)
            })
            .copied()?;

        self.lru.retain(|k| k != &evict_key);
        let slot = self.tiles.remove(&evict_key)?;
        self.slot_occupied[slot.slot_index] = false;
        Some(slot.slot_index)
    }
}
