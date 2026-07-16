mod effects;
mod pipelines;
mod render;

use crate::gpu::GpuShared;
use crate::gpu::image_cache::ImageGpuCache;
use crate::gpu::image_store::ImageStore;
use crate::render::{Batcher, ShaderType};
use crate::text::FontAtlas;
use std::sync::Arc;
use crate::core::sync::Mutex;
use wgpu::util::DeviceExt;

const MAX_CLIP_SLOTS: usize = 64;

const UNIFORM_ALIGN: usize = 256;

#[repr(C)]
#[derive(Clone, Copy, Debug, bytemuck::Pod, bytemuck::Zeroable)]
struct Uniforms {
    resolution: [f32; 2],
    time: f32,
    scale_factor: f32,
    clip_rect: [f32; 4],
    clip_corner_radius: [f32; 4],
}

#[repr(C)]
#[derive(Clone, Copy, Debug, bytemuck::Pod, bytemuck::Zeroable)]
struct UniformSlot {
    uniforms: Uniforms,
    _pad: [u8; UNIFORM_ALIGN - std::mem::size_of::<Uniforms>()],
}

#[repr(C)]
#[derive(Clone, Copy, Debug, bytemuck::Pod, bytemuck::Zeroable)]
struct BlurUniforms {
    resolution: [f32; 2],
    direction: [f32; 2],
    radius: f32,
    _padding: f32,
    _padding2: [f32; 2],
}

#[repr(C)]
#[derive(Clone, Copy, Debug, bytemuck::Pod, bytemuck::Zeroable)]
struct PostProcessUniforms {
    resolution: [f32; 2],
    effect_type: f32,
    intensity: f32,
    params: [f32; 4],
    time: f32,
    _padding: [f32; 3],
    params2: [f32; 4],
    bounds: [f32; 4],
}

pub struct RenderStats {
    pub draw_calls: usize,
    pub vertex_count: usize,
}

impl Default for RenderStats {
    fn default() -> Self {
        Self { draw_calls: 0, vertex_count: 0 }
    }
}

pub struct Renderer {
    rect_pipeline: wgpu::RenderPipeline,
    text_pipeline: wgpu::RenderPipeline,
    shadow_pipeline: wgpu::RenderPipeline,
    inner_shadow_pipeline: wgpu::RenderPipeline,
    line_pipeline: wgpu::RenderPipeline,
    blur_pipeline: wgpu::RenderPipeline,
    postprocess_pipeline: wgpu::RenderPipeline,
    blit_pipeline: wgpu::RenderPipeline,
    glow_shadow_pipeline: wgpu::RenderPipeline,
    glow_blit_pipeline: wgpu::RenderPipeline,
    image_pipeline: wgpu::RenderPipeline,

    uniform_buffer: wgpu::Buffer,
    uniform_bind_group: wgpu::BindGroup,
    blur_uniform_buffer: wgpu::Buffer,
    blur_uniform_bind_group: wgpu::BindGroup,
    postprocess_uniform_buffer: wgpu::Buffer,
    postprocess_uniform_bind_group: wgpu::BindGroup,

    text_bind_group: wgpu::BindGroup,

    pub font_atlas: std::sync::Arc<crate::core::sync::Mutex<FontAtlas>>,

    image_gpu_cache: ImageGpuCache,
    pub image_store: Arc<Mutex<ImageStore>>,

    #[cfg(feature = "map")]
    tile_atlas_bind_group: Option<wgpu::BindGroup>,
    #[cfg(feature = "map")]
    pub tile_atlas: Option<std::sync::Arc<crate::core::sync::Mutex<crate::gpu::tile_atlas::TileAtlas>>>,

    batcher: Batcher,
    width: u32,
    height: u32,
    logical_width: u32,
    logical_height: u32,

    scene_texture: Option<wgpu::Texture>,
    scene_view: Option<wgpu::TextureView>,
    scene_bind_group: Option<wgpu::BindGroup>,
    surface_format: wgpu::TextureFormat,

    texture_pool: crate::gpu::texture_pool::TexturePool,
    fullscreen_vertex_buffer: wgpu::Buffer,
    fullscreen_index_buffer: wgpu::Buffer,

    start_time: web_time::Instant,

    gpu_buffers: Vec<GpuBatchBuffers>,

    staging_belt: Option<wgpu::util::StagingBelt>,
    throughput_buffer: Option<wgpu::Buffer>,
    staging_belt_enabled: bool,
}

const FULLSCREEN_VERTICES: [[f32; 4]; 4] = [
    [-1.0, -1.0, 0.0, 1.0],
    [ 1.0, -1.0, 1.0, 1.0],
    [ 1.0,  1.0, 1.0, 0.0],
    [-1.0,  1.0, 0.0, 0.0],
];

const FULLSCREEN_INDICES: [u32; 6] = [0, 1, 2, 0, 2, 3];

struct GpuBatchBuffers {
    vertex_buffer: wgpu::Buffer,
    index_buffer: wgpu::Buffer,
    index_count: u32,
    shader_type: ShaderType,
    clip_rect: crate::render::ClipRect,
    texture_id: Option<crate::render::TextureId>,
    uniform_offset: u32,
}

#[derive(Clone, Copy, Debug)]
enum EffectTarget {
    Scene,
    Pool(crate::gpu::texture_pool::PoolHandle),
}

#[allow(dead_code)]
enum EffectRenderStep {
    DrawBatches {
        target: EffectTarget,
        buf_range: std::ops::Range<usize>,
        clear: bool,
        clear_color: [f64; 4],
    },
    BlurPass {
        source: crate::gpu::texture_pool::PoolHandle,
        dest: crate::gpu::texture_pool::PoolHandle,
        radius: f32,
        direction: [f32; 2],
    },
    PostProcess {
        source: crate::gpu::texture_pool::PoolHandle,
        dest: crate::gpu::texture_pool::PoolHandle,
        effect_type: f32,
        intensity: f32,
        params: [f32; 4],
        params2: [f32; 4],
        time: f32,
        bounds: [f32; 4],
    },
    Composite {
        source: crate::gpu::texture_pool::PoolHandle,
        dest: EffectTarget,
    },
    CompositeAdditive {
        source: crate::gpu::texture_pool::PoolHandle,
        dest: EffectTarget,
    },
    CompositeBounded {
        source: crate::gpu::texture_pool::PoolHandle,
        dest: EffectTarget,
        bounds: [f32; 4],
    },
    CopySceneToPool {
        dest: crate::gpu::texture_pool::PoolHandle,
    },
}

impl Renderer {
    pub fn new(
        gpu: &GpuShared,
        surface_format: wgpu::TextureFormat,
        width: u32,
        height: u32,
        logical_width: u32,
        logical_height: u32,
        preferred_font_family: Option<String>,
    ) -> Self {
        let uniform_buffer_size = (UNIFORM_ALIGN * MAX_CLIP_SLOTS) as u64;
        let uniform_buffer = gpu.device.create_buffer(&wgpu::BufferDescriptor {
            label: Some("Uniform Buffer"),
            size: uniform_buffer_size,
            usage: wgpu::BufferUsages::UNIFORM | wgpu::BufferUsages::COPY_DST,
            mapped_at_creation: false,
        });
        let uniform_bgl = gpu.device.create_bind_group_layout(&wgpu::BindGroupLayoutDescriptor {
            label: Some("Uniform BGL"),
            entries: &[wgpu::BindGroupLayoutEntry {
                binding: 0,
                visibility: wgpu::ShaderStages::VERTEX | wgpu::ShaderStages::FRAGMENT,
                ty: wgpu::BindingType::Buffer {
                    ty: wgpu::BufferBindingType::Uniform,
                    has_dynamic_offset: true,
                    min_binding_size: wgpu::BufferSize::new(std::mem::size_of::<Uniforms>() as u64),
                },
                count: None,
            }],
        });
        let uniform_bind_group = gpu.device.create_bind_group(&wgpu::BindGroupDescriptor {
            label: Some("Uniform BG"),
            layout: &uniform_bgl,
            entries: &[wgpu::BindGroupEntry {
                binding: 0,
                resource: wgpu::BindingResource::Buffer(wgpu::BufferBinding {
                    buffer: &uniform_buffer,
                    offset: 0,
                    size: wgpu::BufferSize::new(std::mem::size_of::<Uniforms>() as u64),
                }),
            }],
        });

        let font_atlas = FontAtlas::with_config(&gpu.device, &gpu.queue, preferred_font_family);

        let text_bgl = gpu.device.create_bind_group_layout(&wgpu::BindGroupLayoutDescriptor {
            label: Some("Text BGL"),
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
        let text_bind_group = gpu.device.create_bind_group(&wgpu::BindGroupDescriptor {
            label: Some("Text BG"),
            layout: &text_bgl,
            entries: &[
                wgpu::BindGroupEntry {
                    binding: 0,
                    resource: wgpu::BindingResource::TextureView(&font_atlas.texture_view),
                },
                wgpu::BindGroupEntry {
                    binding: 1,
                    resource: wgpu::BindingResource::Sampler(&font_atlas.sampler),
                },
            ],
        });

        let texture_pool = crate::gpu::texture_pool::TexturePool::new(
            &gpu.device, surface_format, width, height,
        );

        let blur_uniforms = BlurUniforms {
            resolution: [width as f32, height as f32],
            direction: [1.0, 0.0],
            radius: 8.0,
            _padding: 0.0,
            _padding2: [0.0; 2],
        };
        let blur_uniform_buffer = gpu.device.create_buffer_init(&wgpu::util::BufferInitDescriptor {
            label: Some("Blur Uniform Buffer"),
            contents: bytemuck::cast_slice(&[blur_uniforms]),
            usage: wgpu::BufferUsages::UNIFORM | wgpu::BufferUsages::COPY_DST,
        });
        let blur_uniform_bgl = gpu.device.create_bind_group_layout(&wgpu::BindGroupLayoutDescriptor {
            label: Some("Blur Uniform BGL"),
            entries: &[wgpu::BindGroupLayoutEntry {
                binding: 0,
                visibility: wgpu::ShaderStages::VERTEX | wgpu::ShaderStages::FRAGMENT,
                ty: wgpu::BindingType::Buffer {
                    ty: wgpu::BufferBindingType::Uniform,
                    has_dynamic_offset: false,
                    min_binding_size: None,
                },
                count: None,
            }],
        });
        let blur_uniform_bind_group = gpu.device.create_bind_group(&wgpu::BindGroupDescriptor {
            label: Some("Blur Uniform BG"),
            layout: &blur_uniform_bgl,
            entries: &[wgpu::BindGroupEntry {
                binding: 0,
                resource: blur_uniform_buffer.as_entire_binding(),
            }],
        });

        let pp_uniforms = PostProcessUniforms {
            resolution: [width as f32, height as f32],
            effect_type: 0.0,
            intensity: 0.0,
            params: [0.0; 4],
            time: 0.0,
            _padding: [0.0; 3],
            params2: [0.0; 4],
            bounds: [0.0, 0.0, 1.0, 1.0],
        };
        let postprocess_uniform_buffer = gpu.device.create_buffer_init(&wgpu::util::BufferInitDescriptor {
            label: Some("PostProcess Uniform Buffer"),
            contents: bytemuck::cast_slice(&[pp_uniforms]),
            usage: wgpu::BufferUsages::UNIFORM | wgpu::BufferUsages::COPY_DST,
        });
        let pp_uniform_bgl = gpu.device.create_bind_group_layout(&wgpu::BindGroupLayoutDescriptor {
            label: Some("PostProcess Uniform BGL"),
            entries: &[wgpu::BindGroupLayoutEntry {
                binding: 0,
                visibility: wgpu::ShaderStages::VERTEX | wgpu::ShaderStages::FRAGMENT,
                ty: wgpu::BindingType::Buffer {
                    ty: wgpu::BufferBindingType::Uniform,
                    has_dynamic_offset: false,
                    min_binding_size: None,
                },
                count: None,
            }],
        });
        let postprocess_uniform_bind_group = gpu.device.create_bind_group(&wgpu::BindGroupDescriptor {
            label: Some("PostProcess Uniform BG"),
            layout: &pp_uniform_bgl,
            entries: &[wgpu::BindGroupEntry {
                binding: 0,
                resource: postprocess_uniform_buffer.as_entire_binding(),
            }],
        });

        let fullscreen_vertex_buffer = gpu.device.create_buffer_init(&wgpu::util::BufferInitDescriptor {
            label: Some("Fullscreen Vertex Buffer"),
            contents: bytemuck::cast_slice(&FULLSCREEN_VERTICES),
            usage: wgpu::BufferUsages::VERTEX,
        });
        let fullscreen_index_buffer = gpu.device.create_buffer_init(&wgpu::util::BufferInitDescriptor {
            label: Some("Fullscreen Index Buffer"),
            contents: bytemuck::cast_slice(&FULLSCREEN_INDICES),
            usage: wgpu::BufferUsages::INDEX,
        });

        let rect_shader = gpu.device.create_shader_module(wgpu::ShaderModuleDescriptor {
            label: Some("Rect Shader"),
            source: wgpu::ShaderSource::Wgsl(include_str!("../rect.wgsl").into()),
        });
        let rect_pipeline_layout = gpu.device.create_pipeline_layout(&wgpu::PipelineLayoutDescriptor {
            label: Some("Rect Pipeline Layout"),
            bind_group_layouts: &[&uniform_bgl],
            immediate_size: 0,
        });
        let rect_pipeline = Self::create_pipeline(
            &gpu.device, "Rect Pipeline", &rect_pipeline_layout, &rect_shader, surface_format,
        );

        let text_shader = gpu.device.create_shader_module(wgpu::ShaderModuleDescriptor {
            label: Some("Text Shader"),
            source: wgpu::ShaderSource::Wgsl(include_str!("../text.wgsl").into()),
        });
        let text_pipeline_layout = gpu.device.create_pipeline_layout(&wgpu::PipelineLayoutDescriptor {
            label: Some("Text Pipeline Layout"),
            bind_group_layouts: &[&uniform_bgl, &text_bgl],
            immediate_size: 0,
        });
        let text_pipeline = Self::create_pipeline(
            &gpu.device, "Text Pipeline", &text_pipeline_layout, &text_shader, surface_format,
        );

        let shadow_shader = gpu.device.create_shader_module(wgpu::ShaderModuleDescriptor {
            label: Some("Shadow Shader"),
            source: wgpu::ShaderSource::Wgsl(include_str!("../shadow.wgsl").into()),
        });
        let shadow_pipeline_layout = gpu.device.create_pipeline_layout(&wgpu::PipelineLayoutDescriptor {
            label: Some("Shadow Pipeline Layout"),
            bind_group_layouts: &[&uniform_bgl],
            immediate_size: 0,
        });
        let shadow_pipeline = Self::create_pipeline(
            &gpu.device, "Shadow Pipeline", &shadow_pipeline_layout, &shadow_shader, surface_format,
        );

        let glow_shadow_pipeline = Self::create_pipeline_additive(
            &gpu.device, "Glow Shadow Pipeline", &shadow_pipeline_layout, &shadow_shader, surface_format,
        );

        let inner_shadow_shader = gpu.device.create_shader_module(wgpu::ShaderModuleDescriptor {
            label: Some("Inner Shadow Shader"),
            source: wgpu::ShaderSource::Wgsl(include_str!("../inner_shadow.wgsl").into()),
        });
        let inner_shadow_pipeline_layout = gpu.device.create_pipeline_layout(&wgpu::PipelineLayoutDescriptor {
            label: Some("Inner Shadow Pipeline Layout"),
            bind_group_layouts: &[&uniform_bgl],
            immediate_size: 0,
        });
        let inner_shadow_pipeline = Self::create_pipeline(
            &gpu.device, "Inner Shadow Pipeline", &inner_shadow_pipeline_layout, &inner_shadow_shader, surface_format,
        );

        let line_shader = gpu.device.create_shader_module(wgpu::ShaderModuleDescriptor {
            label: Some("Line Shader"),
            source: wgpu::ShaderSource::Wgsl(include_str!("../line.wgsl").into()),
        });
        let line_pipeline_layout = gpu.device.create_pipeline_layout(&wgpu::PipelineLayoutDescriptor {
            label: Some("Line Pipeline Layout"),
            bind_group_layouts: &[&uniform_bgl],
            immediate_size: 0,
        });
        let line_pipeline = Self::create_pipeline(
            &gpu.device, "Line Pipeline", &line_pipeline_layout, &line_shader, surface_format,
        );

        let blur_shader = gpu.device.create_shader_module(wgpu::ShaderModuleDescriptor {
            label: Some("Blur Shader"),
            source: wgpu::ShaderSource::Wgsl(include_str!("../blur.wgsl").into()),
        });
        let blur_pipeline_layout = gpu.device.create_pipeline_layout(&wgpu::PipelineLayoutDescriptor {
            label: Some("Blur Pipeline Layout"),
            bind_group_layouts: &[&blur_uniform_bgl, texture_pool.bind_group_layout()],
            immediate_size: 0,
        });
        let blur_pipeline = Self::create_fullscreen_pipeline(
            &gpu.device, "Blur Pipeline", &blur_pipeline_layout, &blur_shader, surface_format,
        );

        let postprocess_shader = gpu.device.create_shader_module(wgpu::ShaderModuleDescriptor {
            label: Some("PostProcess Shader"),
            source: wgpu::ShaderSource::Wgsl(include_str!("../postprocess.wgsl").into()),
        });
        let postprocess_pipeline_layout = gpu.device.create_pipeline_layout(&wgpu::PipelineLayoutDescriptor {
            label: Some("PostProcess Pipeline Layout"),
            bind_group_layouts: &[&pp_uniform_bgl, texture_pool.bind_group_layout()],
            immediate_size: 0,
        });
        let postprocess_pipeline = Self::create_fullscreen_pipeline(
            &gpu.device, "PostProcess Pipeline", &postprocess_pipeline_layout, &postprocess_shader, surface_format,
        );

        let blit_shader = gpu.device.create_shader_module(wgpu::ShaderModuleDescriptor {
            label: Some("Blit Shader"),
            source: wgpu::ShaderSource::Wgsl(include_str!("../blit.wgsl").into()),
        });
        let blit_pipeline_layout = gpu.device.create_pipeline_layout(&wgpu::PipelineLayoutDescriptor {
            label: Some("Blit Pipeline Layout"),
            bind_group_layouts: &[texture_pool.bind_group_layout()],
            immediate_size: 0,
        });
        let blit_pipeline = Self::create_fullscreen_pipeline(
            &gpu.device, "Blit Pipeline", &blit_pipeline_layout, &blit_shader, surface_format,
        );

        let glow_blit_pipeline = Self::create_fullscreen_pipeline_additive(
            &gpu.device, "Glow Blit Pipeline", &blit_pipeline_layout, &blit_shader, surface_format,
        );

        let image_gpu_cache = ImageGpuCache::new(&gpu.device);
        let image_shader = gpu.device.create_shader_module(wgpu::ShaderModuleDescriptor {
            label: Some("Image Shader"),
            source: wgpu::ShaderSource::Wgsl(include_str!("../text.wgsl").into()),
        });
        let image_pipeline_layout = gpu.device.create_pipeline_layout(&wgpu::PipelineLayoutDescriptor {
            label: Some("Image Pipeline Layout"),
            bind_group_layouts: &[&uniform_bgl, image_gpu_cache.bind_group_layout()],
            immediate_size: 0,
        });
        let image_pipeline = Self::create_pipeline(
            &gpu.device, "Image Pipeline", &image_pipeline_layout, &image_shader, surface_format,
        );

        Self {
            rect_pipeline,
            text_pipeline,
            shadow_pipeline,
            inner_shadow_pipeline,
            line_pipeline,
            blur_pipeline,
            postprocess_pipeline,
            blit_pipeline,
            glow_shadow_pipeline,
            glow_blit_pipeline,
            image_pipeline,
            uniform_buffer,
            uniform_bind_group,
            blur_uniform_buffer,
            blur_uniform_bind_group,
            postprocess_uniform_buffer,
            postprocess_uniform_bind_group,
            text_bind_group,
            font_atlas: std::sync::Arc::new(crate::core::sync::Mutex::new(font_atlas)),
            image_gpu_cache,
            image_store: Arc::new(Mutex::new(ImageStore::new())),
            #[cfg(feature = "map")]
            tile_atlas_bind_group: None,
            #[cfg(feature = "map")]
            tile_atlas: None,
            batcher: Batcher::new(),
            width,
            height,
            logical_width,
            logical_height,
            scene_texture: None,
            scene_view: None,
            scene_bind_group: None,
            surface_format,
            texture_pool,
            fullscreen_vertex_buffer,
            fullscreen_index_buffer,
            start_time: web_time::Instant::now(),
            gpu_buffers: Vec::new(),
            staging_belt: None,
            throughput_buffer: None,
            staging_belt_enabled: false,
        }
    }

    pub fn set_staging_belt(&mut self, enabled: bool, device: &wgpu::Device) {
        self.staging_belt_enabled = enabled;
        if enabled {
            if self.staging_belt.is_none() {
                self.staging_belt = Some(wgpu::util::StagingBelt::new(device.clone(), 1024 * 1024));
            }
            if self.throughput_buffer.is_none() {
                self.throughput_buffer = Some(device.create_buffer(&wgpu::BufferDescriptor {
                    label: Some("throughput-belt"),
                    size: 4 * 1024 * 1024,
                    usage: wgpu::BufferUsages::COPY_DST | wgpu::BufferUsages::VERTEX,
                    mapped_at_creation: false,
                }));
            }
        }
    }

    pub fn staging_belt_enabled(&self) -> bool {
        self.staging_belt_enabled
    }

    #[cfg(feature = "map")]
    pub fn ensure_tile_atlas(&mut self, gpu: &GpuShared) {
        if self.tile_atlas.is_some() {
            return;
        }

        let tile_atlas = crate::gpu::tile_atlas::TileAtlas::new(&gpu.device, &gpu.queue);

        let tile_bind_group = gpu.device.create_bind_group(&wgpu::BindGroupDescriptor {
            label: Some("Tile Atlas BG"),
            layout: self.image_gpu_cache.bind_group_layout(),
            entries: &[
                wgpu::BindGroupEntry {
                    binding: 0,
                    resource: wgpu::BindingResource::TextureView(&tile_atlas.texture_view),
                },
                wgpu::BindGroupEntry {
                    binding: 1,
                    resource: wgpu::BindingResource::Sampler(&tile_atlas.sampler),
                },
            ],
        });

        self.tile_atlas = Some(std::sync::Arc::new(crate::core::sync::Mutex::new(tile_atlas)));
        self.tile_atlas_bind_group = Some(tile_bind_group);
    }

    fn ensure_scene_texture(&mut self, device: &wgpu::Device) {
        if self.scene_texture.is_some() {
            return;
        }
        let texture = device.create_texture(&wgpu::TextureDescriptor {
            label: Some("Scene Texture"),
            size: wgpu::Extent3d {
                width: self.width.max(1),
                height: self.height.max(1),
                depth_or_array_layers: 1,
            },
            mip_level_count: 1,
            sample_count: 1,
            dimension: wgpu::TextureDimension::D2,
            format: self.surface_format,
            usage: wgpu::TextureUsages::RENDER_ATTACHMENT
                | wgpu::TextureUsages::TEXTURE_BINDING
                | wgpu::TextureUsages::COPY_SRC
                | wgpu::TextureUsages::COPY_DST,
            view_formats: &[],
        });
        let view = texture.create_view(&wgpu::TextureViewDescriptor::default());
        let bind_group = device.create_bind_group(&wgpu::BindGroupDescriptor {
            label: Some("Scene BG"),
            layout: self.texture_pool.bind_group_layout(),
            entries: &[
                wgpu::BindGroupEntry {
                    binding: 0,
                    resource: wgpu::BindingResource::TextureView(&view),
                },
                wgpu::BindGroupEntry {
                    binding: 1,
                    resource: wgpu::BindingResource::Sampler(
                        &device.create_sampler(&wgpu::SamplerDescriptor {
                            label: Some("Scene Sampler"),
                            mag_filter: wgpu::FilterMode::Linear,
                            min_filter: wgpu::FilterMode::Linear,
                            ..Default::default()
                        }),
                    ),
                },
            ],
        });
        self.scene_texture = Some(texture);
        self.scene_view = Some(view);
        self.scene_bind_group = Some(bind_group);
    }

    pub fn resize(
        &mut self,
        device: &wgpu::Device,
        width: u32,
        height: u32,
        logical_width: u32,
        logical_height: u32,
    ) {
        self.width = width;
        self.height = height;
        self.logical_width = logical_width;
        self.logical_height = logical_height;
        self.scene_texture = None;
        self.scene_view = None;
        self.scene_bind_group = None;
        self.texture_pool.resize(device, width, height);
    }

    pub fn font_atlas_stats(&self) -> crate::text::FontAtlasStats {
        self.font_atlas.lock().unwrap().memory_stats()
    }
}
