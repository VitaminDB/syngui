use hashbrown::HashMap;
use std::collections::VecDeque;

const ATLAS_SIZE: u32 = 2048;
const TILE_SIZE: u32 = 256;
const TILES_PER_ROW: u32 = ATLAS_SIZE / TILE_SIZE;
const MAX_TILES: usize = (TILES_PER_ROW * TILES_PER_ROW) as usize;

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
    tiles: HashMap<TileKey, TileSlot>,
    lru: VecDeque<TileKey>,
    slot_occupied: Vec<bool>,
    pixels: Vec<u8>,
    dirty: bool,
}

impl TileAtlas {
    pub fn new(device: &wgpu::Device, queue: &wgpu::Queue) -> Self {
        let texture = device.create_texture(&wgpu::TextureDescriptor {
            label: Some("Tile Atlas"),
            size: wgpu::Extent3d {
                width: ATLAS_SIZE,
                height: ATLAS_SIZE,
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
            label: Some("Tile Sampler"),
            address_mode_u: wgpu::AddressMode::ClampToEdge,
            address_mode_v: wgpu::AddressMode::ClampToEdge,
            mag_filter: wgpu::FilterMode::Linear,
            min_filter: wgpu::FilterMode::Linear,
            ..Default::default()
        });

        let pixels = vec![0u8; (ATLAS_SIZE * ATLAS_SIZE * 4) as usize];

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
                bytes_per_row: Some(ATLAS_SIZE * 4),
                rows_per_image: None,
            },
            wgpu::Extent3d {
                width: ATLAS_SIZE,
                height: ATLAS_SIZE,
                depth_or_array_layers: 1,
            },
        );

        Self {
            texture,
            texture_view,
            sampler,
            tiles: HashMap::new(),
            lru: VecDeque::new(),
            slot_occupied: vec![false; MAX_TILES],
            pixels,
            dirty: false,
        }
    }

    pub fn get_tile(&mut self, key: &TileKey) -> Option<TileSlot> {
        if let Some(slot) = self.tiles.get(key).copied() {
            if let Some(pos) = self.lru.iter().position(|k| k == key) {
                self.lru.remove(pos);
            }
            self.lru.push_back(*key);
            Some(slot)
        } else {
            None
        }
    }

    pub fn insert_tile(&mut self, key: TileKey, rgba: &[u8]) -> TileSlot {
        if let Some(slot) = self.get_tile(&key) {
            return slot;
        }

        let expected = (TILE_SIZE * TILE_SIZE * 4) as usize;
        assert!(rgba.len() >= expected, "Tile RGBA data too small: {} < {}", rgba.len(), expected);

        let slot_index = self.find_or_evict_slot();

        let col = (slot_index as u32) % TILES_PER_ROW;
        let row = (slot_index as u32) / TILES_PER_ROW;
        let px = col * TILE_SIZE;
        let py = row * TILE_SIZE;

        for ty in 0..TILE_SIZE {
            let src_offset = (ty * TILE_SIZE * 4) as usize;
            let dst_offset = ((py + ty) * ATLAS_SIZE * 4 + px * 4) as usize;
            let row_bytes = (TILE_SIZE * 4) as usize;
            self.pixels[dst_offset..dst_offset + row_bytes]
                .copy_from_slice(&rgba[src_offset..src_offset + row_bytes]);
        }
        self.dirty = true;

        let slot = TileSlot {
            uv_x: px as f32 / ATLAS_SIZE as f32,
            uv_y: py as f32 / ATLAS_SIZE as f32,
            uv_w: TILE_SIZE as f32 / ATLAS_SIZE as f32,
            uv_h: TILE_SIZE as f32 / ATLAS_SIZE as f32,
            slot_index,
        };

        self.slot_occupied[slot_index] = true;
        self.tiles.insert(key, slot);
        self.lru.push_back(key);

        slot
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
                bytes_per_row: Some(ATLAS_SIZE * 4),
                rows_per_image: None,
            },
            wgpu::Extent3d {
                width: ATLAS_SIZE,
                height: ATLAS_SIZE,
                depth_or_array_layers: 1,
            },
        );
    }

    pub fn clear_provider(&mut self, provider_id: u8) {
        let keys_to_remove: Vec<TileKey> = self.tiles.keys()
            .filter(|k| k.provider_id == provider_id)
            .copied()
            .collect();

        for key in keys_to_remove {
            if let Some(slot) = self.tiles.remove(&key) {
                self.slot_occupied[slot.slot_index] = false;
            }
            self.lru.retain(|k| k != &key);
        }
    }

    fn find_or_evict_slot(&mut self) -> usize {
        if let Some(idx) = self.slot_occupied.iter().position(|&o| !o) {
            return idx;
        }

        if let Some(evict_key) = self.lru.pop_front() {
            if let Some(slot) = self.tiles.remove(&evict_key) {
                self.slot_occupied[slot.slot_index] = false;
                return slot.slot_index;
            }
        }

        0
    }
}
