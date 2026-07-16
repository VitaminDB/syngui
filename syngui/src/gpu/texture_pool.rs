#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct PoolHandle(pub usize);

struct PoolEntry {
    texture: wgpu::Texture,
    view: wgpu::TextureView,
    bind_group: wgpu::BindGroup,
    in_use: bool,
    last_used_frame: u64,
}

pub struct TexturePool {
    entries: Vec<PoolEntry>,
    bind_group_layout: wgpu::BindGroupLayout,
    sampler: wgpu::Sampler,
    width: u32,
    height: u32,
    format: wgpu::TextureFormat,
    frame_counter: u64,
}

const MAX_POOL_SIZE: usize = 6;
const EVICTION_FRAMES: u64 = 120;

impl TexturePool {
    pub fn new(device: &wgpu::Device, format: wgpu::TextureFormat, width: u32, height: u32) -> Self {
        let bind_group_layout = device.create_bind_group_layout(&wgpu::BindGroupLayoutDescriptor {
            label: Some("TexturePool BGL"),
            entries: &[
                wgpu::BindGroupLayoutEntry {
                    binding: 0,
                    visibility: wgpu::ShaderStages::FRAGMENT,
                    ty: wgpu::BindingType::Texture {
                        sample_type: wgpu::TextureSampleType::Float { filterable: true },
                        view_dimension: wgpu::TextureViewDimension::D2,
                        multisampled: false,
                    },
                    count: None,
                },
                wgpu::BindGroupLayoutEntry {
                    binding: 1,
                    visibility: wgpu::ShaderStages::FRAGMENT,
                    ty: wgpu::BindingType::Sampler(wgpu::SamplerBindingType::Filtering),
                    count: None,
                },
            ],
        });

        let sampler = device.create_sampler(&wgpu::SamplerDescriptor {
            label: Some("TexturePool Sampler"),
            address_mode_u: wgpu::AddressMode::ClampToEdge,
            address_mode_v: wgpu::AddressMode::ClampToEdge,
            address_mode_w: wgpu::AddressMode::ClampToEdge,
            mag_filter: wgpu::FilterMode::Linear,
            min_filter: wgpu::FilterMode::Linear,
            mipmap_filter: wgpu::MipmapFilterMode::Linear,
            ..Default::default()
        });

        Self {
            entries: Vec::new(),
            bind_group_layout,
            sampler,
            width,
            height,
            format,
            frame_counter: 0,
        }
    }

    pub fn acquire(&mut self, device: &wgpu::Device) -> PoolHandle {
        for (i, entry) in self.entries.iter_mut().enumerate() {
            if !entry.in_use {
                entry.in_use = true;
                entry.last_used_frame = self.frame_counter;
                return PoolHandle(i);
            }
        }

        if self.entries.len() >= MAX_POOL_SIZE {
            log::warn!("TexturePool: growing beyond target size {} (now {})", MAX_POOL_SIZE, self.entries.len() + 1);
        }

        let entry = self.create_entry(device);
        let idx = self.entries.len();
        self.entries.push(entry);
        self.entries[idx].in_use = true;
        PoolHandle(idx)
    }

    pub fn release(&mut self, handle: PoolHandle) {
        if let Some(entry) = self.entries.get_mut(handle.0) {
            entry.in_use = false;
            entry.last_used_frame = self.frame_counter;
        }
    }

    pub fn view(&self, handle: PoolHandle) -> &wgpu::TextureView {
        &self.entries[handle.0].view
    }

    pub fn bind_group(&self, handle: PoolHandle) -> &wgpu::BindGroup {
        &self.entries[handle.0].bind_group
    }

    pub fn texture(&self, handle: PoolHandle) -> &wgpu::Texture {
        &self.entries[handle.0].texture
    }

    pub fn bind_group_layout(&self) -> &wgpu::BindGroupLayout {
        &self.bind_group_layout
    }

    pub fn end_frame(&mut self) {
        self.frame_counter += 1;

        self.entries.retain(|entry| {
            if entry.in_use {
                return true;
            }
            self.frame_counter - entry.last_used_frame < EVICTION_FRAMES
        });
    }

    pub fn resize(&mut self, device: &wgpu::Device, width: u32, height: u32) {
        if self.width == width && self.height == height {
            return;
        }
        self.width = width;
        self.height = height;
        let count = self.entries.len();
        self.entries.clear();
        for _ in 0..count {
            let entry = self.create_entry(device);
            self.entries.push(entry);
        }
    }

    pub fn pool_size(&self) -> usize {
        self.entries.len()
    }

    fn create_entry(&self, device: &wgpu::Device) -> PoolEntry {
        let texture = device.create_texture(&wgpu::TextureDescriptor {
            label: Some("TexturePool Entry"),
            size: wgpu::Extent3d {
                width: self.width.max(1),
                height: self.height.max(1),
                depth_or_array_layers: 1,
            },
            mip_level_count: 1,
            sample_count: 1,
            dimension: wgpu::TextureDimension::D2,
            format: self.format,
            usage: wgpu::TextureUsages::RENDER_ATTACHMENT
                | wgpu::TextureUsages::TEXTURE_BINDING
                | wgpu::TextureUsages::COPY_SRC
                | wgpu::TextureUsages::COPY_DST,
            view_formats: &[],
        });

        let view = texture.create_view(&wgpu::TextureViewDescriptor::default());

        let bind_group = device.create_bind_group(&wgpu::BindGroupDescriptor {
            label: Some("TexturePool Entry BG"),
            layout: &self.bind_group_layout,
            entries: &[
                wgpu::BindGroupEntry {
                    binding: 0,
                    resource: wgpu::BindingResource::TextureView(&view),
                },
                wgpu::BindGroupEntry {
                    binding: 1,
                    resource: wgpu::BindingResource::Sampler(&self.sampler),
                },
            ],
        });

        PoolEntry {
            texture,
            view,
            bind_group,
            in_use: false,
            last_used_frame: self.frame_counter,
        }
    }
}
