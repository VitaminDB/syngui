use std::sync::Arc;
use std::time::Duration;
use web_time::Instant;
use winit::event_loop::ActiveEventLoop;

pub struct SplashConfig {
    pub image_data: &'static [u8],
    pub background_color: [u8; 3],
    pub window_width: u32,
    pub window_height: u32,
    pub min_display_ms: u64,
    pub transparent: bool,
}

impl SplashConfig {
    pub fn new(image_data: &'static [u8]) -> Self {
        Self {
            image_data,
            background_color: [255, 255, 255],
            window_width: 480,
            window_height: 360,
            min_display_ms: 1500,
            transparent: false,
        }
    }
}

#[allow(dead_code)]
pub(super) enum SplashWindow {
    Cpu {
        window: Arc<winit::window::Window>,
        _surface: softbuffer::Surface<Arc<winit::window::Window>, Arc<winit::window::Window>>,
        created_at: Instant,
        min_display: Duration,
    },
    Gpu {
        window: Arc<winit::window::Window>,
        _device: wgpu::Device,
        _queue: wgpu::Queue,
        created_at: Instant,
        min_display: Duration,
    },
}

impl SplashWindow {
    pub fn create(event_loop: &ActiveEventLoop, config: &SplashConfig) -> Option<Self> {
        let img = match image::load_from_memory(config.image_data) {
            Ok(img) => img.to_rgba8(),
            Err(e) => {
                log::warn!("Splash: не удалось декодировать изображение: {e}");
                return None;
            }
        };

        if config.transparent {
            Self::create_gpu(event_loop, config, &img)
        } else {
            Self::create_cpu(event_loop, config, &img)
        }
    }

    pub fn can_close(&self) -> bool {
        match self {
            Self::Cpu { created_at, min_display, .. } |
            Self::Gpu { created_at, min_display, .. } => {
                created_at.elapsed() >= *min_display
            }
        }
    }

    pub fn wait_and_close(self) {
        while !self.can_close() {
            std::thread::sleep(Duration::from_millis(16));
        }
    }

    fn create_window(
        event_loop: &ActiveEventLoop,
        config: &SplashConfig,
    ) -> Option<Arc<winit::window::Window>> {
        let win_w = config.window_width;
        let win_h = config.window_height;

        let mut attributes = winit::window::Window::default_attributes()
            .with_decorations(false)
            .with_resizable(false)
            .with_transparent(config.transparent)
            .with_inner_size(winit::dpi::PhysicalSize::new(win_w, win_h))
            .with_title("Splash");

        if let Some(monitor) = event_loop.primary_monitor().or_else(|| event_loop.available_monitors().next()) {
            let screen = monitor.size();
            let pos_x = (screen.width.saturating_sub(win_w)) / 2;
            let pos_y = (screen.height.saturating_sub(win_h)) / 2;
            attributes = attributes.with_position(winit::dpi::PhysicalPosition::new(
                pos_x as i32 + monitor.position().x,
                pos_y as i32 + monitor.position().y,
            ));
        }

        match event_loop.create_window(attributes) {
            Ok(w) => Some(Arc::new(w)),
            Err(e) => {
                log::warn!("Splash: не удалось создать окно: {e}");
                None
            }
        }
    }

    fn compose_image(
        buf_w: u32,
        buf_h: u32,
        img: &image::RgbaImage,
        bg: Option<[u8; 3]>,
    ) -> Vec<u8> {
        let img_w = img.width();
        let img_h = img.height();
        let mut rgba = vec![0u8; (buf_w * buf_h * 4) as usize];

        if let Some(bg) = bg {
            for pixel in rgba.chunks_exact_mut(4) {
                pixel[0] = bg[0];
                pixel[1] = bg[1];
                pixel[2] = bg[2];
                pixel[3] = 255;
            }
        }

        let max_w = buf_w as f32 * 0.8;
        let max_h = buf_h as f32 * 0.8;
        let scale = (max_w / img_w as f32).min(max_h / img_h as f32).min(1.0);
        let scaled_w = (img_w as f32 * scale) as u32;
        let scaled_h = (img_h as f32 * scale) as u32;
        let offset_x = (buf_w - scaled_w) / 2;
        let offset_y = (buf_h - scaled_h) / 2;

        let src = img.as_raw();
        for y in 0..scaled_h {
            let src_y = ((y as f32 / scale) as u32).min(img_h - 1);
            for x in 0..scaled_w {
                let src_x = ((x as f32 / scale) as u32).min(img_w - 1);
                let si = ((src_y * img_w + src_x) * 4) as usize;
                let a = src[si + 3] as u32;
                if a == 0 { continue; }

                let di = (((offset_y + y) * buf_w + offset_x + x) * 4) as usize;
                if a == 255 || bg.is_none() {
                    rgba[di] = src[si];
                    rgba[di + 1] = src[si + 1];
                    rgba[di + 2] = src[si + 2];
                    rgba[di + 3] = src[si + 3];
                } else {
                    let inv = 255 - a;
                    rgba[di] = ((src[si] as u32 * a + rgba[di] as u32 * inv) / 255) as u8;
                    rgba[di + 1] = ((src[si + 1] as u32 * a + rgba[di + 1] as u32 * inv) / 255) as u8;
                    rgba[di + 2] = ((src[si + 2] as u32 * a + rgba[di + 2] as u32 * inv) / 255) as u8;
                    rgba[di + 3] = 255;
                }
            }
        }

        rgba
    }

    fn create_cpu(
        event_loop: &ActiveEventLoop,
        config: &SplashConfig,
        img: &image::RgbaImage,
    ) -> Option<Self> {
        let window = Self::create_window(event_loop, config)?;

        let context = softbuffer::Context::new(window.clone()).ok()?;
        let mut surface = softbuffer::Surface::new(&context, window.clone()).ok()?;

        let actual_size = window.inner_size();
        let buf_w = actual_size.width.max(1);
        let buf_h = actual_size.height.max(1);

        let _ = surface.resize(
            std::num::NonZeroU32::new(buf_w).unwrap(),
            std::num::NonZeroU32::new(buf_h).unwrap(),
        );

        let rgba = Self::compose_image(buf_w, buf_h, img, Some(config.background_color));

        if let Ok(mut buffer) = surface.buffer_mut() {
            for (i, pixel) in rgba.chunks_exact(4).enumerate() {
                buffer[i] = (pixel[0] as u32) << 16 | (pixel[1] as u32) << 8 | pixel[2] as u32;
            }
            let _ = buffer.present();
        }

        Some(Self::Cpu {
            window,
            _surface: surface,
            created_at: Instant::now(),
            min_display: Duration::from_millis(config.min_display_ms),
        })
    }

    fn create_gpu(
        event_loop: &ActiveEventLoop,
        config: &SplashConfig,
        img: &image::RgbaImage,
    ) -> Option<Self> {
        let window = Self::create_window(event_loop, config)?;

        let actual_size = window.inner_size();
        let buf_w = actual_size.width.max(1);
        let buf_h = actual_size.height.max(1);

        let rgba = Self::compose_image(buf_w, buf_h, img, None);

        let instance = wgpu::Instance::new(&wgpu::InstanceDescriptor {
            backends: wgpu::Backends::all(),
            ..Default::default()
        });

        let surface = instance.create_surface(window.clone()).ok()?;

        let adapter = pollster::block_on(instance.request_adapter(&wgpu::RequestAdapterOptions {
            power_preference: wgpu::PowerPreference::LowPower,
            compatible_surface: Some(&surface),
            force_fallback_adapter: false,
        })).ok()?;

        let (device, queue) = pollster::block_on(adapter.request_device(
            &wgpu::DeviceDescriptor {
                label: Some("Splash GPU"),
                required_features: wgpu::Features::empty(),
                required_limits: wgpu::Limits::default(),
                memory_hints: wgpu::MemoryHints::MemoryUsage,
                trace: wgpu::Trace::Off,
                experimental_features: wgpu::ExperimentalFeatures::disabled(),
            },
        )).ok()?;

        let caps = surface.get_capabilities(&adapter);
        let format = caps.formats.iter().find(|f| f.is_srgb()).copied()
            .unwrap_or(caps.formats[0]);

        let alpha_mode = if caps.alpha_modes.contains(&wgpu::CompositeAlphaMode::PreMultiplied) {
            wgpu::CompositeAlphaMode::PreMultiplied
        } else if caps.alpha_modes.contains(&wgpu::CompositeAlphaMode::PostMultiplied) {
            wgpu::CompositeAlphaMode::PostMultiplied
        } else {
            caps.alpha_modes[0]
        };

        surface.configure(&device, &wgpu::SurfaceConfiguration {
            usage: wgpu::TextureUsages::RENDER_ATTACHMENT,
            format,
            width: buf_w,
            height: buf_h,
            present_mode: wgpu::PresentMode::AutoVsync,
            alpha_mode,
            view_formats: vec![],
            desired_maximum_frame_latency: 1,
        });

        let texture_size = wgpu::Extent3d {
            width: buf_w,
            height: buf_h,
            depth_or_array_layers: 1,
        };
        let texture = device.create_texture(&wgpu::TextureDescriptor {
            label: Some("Splash Texture"),
            size: texture_size,
            mip_level_count: 1,
            sample_count: 1,
            dimension: wgpu::TextureDimension::D2,
            format: wgpu::TextureFormat::Rgba8UnormSrgb,
            usage: wgpu::TextureUsages::TEXTURE_BINDING | wgpu::TextureUsages::COPY_DST,
            view_formats: &[],
        });
        queue.write_texture(
            wgpu::TexelCopyTextureInfo {
                texture: &texture,
                mip_level: 0,
                origin: wgpu::Origin3d::ZERO,
                aspect: wgpu::TextureAspect::All,
            },
            &rgba,
            wgpu::TexelCopyBufferLayout {
                offset: 0,
                bytes_per_row: Some(4 * buf_w),
                rows_per_image: Some(buf_h),
            },
            texture_size,
        );

        let texture_view = texture.create_view(&wgpu::TextureViewDescriptor::default());
        let sampler = device.create_sampler(&wgpu::SamplerDescriptor {
            mag_filter: wgpu::FilterMode::Linear,
            min_filter: wgpu::FilterMode::Linear,
            ..Default::default()
        });

        let bind_group_layout = device.create_bind_group_layout(&wgpu::BindGroupLayoutDescriptor {
            label: Some("Splash BGL"),
            entries: &[
                wgpu::BindGroupLayoutEntry {
                    binding: 0,
                    visibility: wgpu::ShaderStages::FRAGMENT,
                    ty: wgpu::BindingType::Texture {
                        multisampled: false,
                        view_dimension: wgpu::TextureViewDimension::D2,
                        sample_type: wgpu::TextureSampleType::Float { filterable: true },
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

        let bind_group = device.create_bind_group(&wgpu::BindGroupDescriptor {
            label: Some("Splash BG"),
            layout: &bind_group_layout,
            entries: &[
                wgpu::BindGroupEntry { binding: 0, resource: wgpu::BindingResource::TextureView(&texture_view) },
                wgpu::BindGroupEntry { binding: 1, resource: wgpu::BindingResource::Sampler(&sampler) },
            ],
        });

        let shader = device.create_shader_module(wgpu::ShaderModuleDescriptor {
            label: Some("Splash Shader"),
            source: wgpu::ShaderSource::Wgsl(SPLASH_SHADER.into()),
        });

        let pipeline_layout = device.create_pipeline_layout(&wgpu::PipelineLayoutDescriptor {
            label: Some("Splash PL"),
            bind_group_layouts: &[&bind_group_layout],
            immediate_size: 0,
        });

        let pipeline = device.create_render_pipeline(&wgpu::RenderPipelineDescriptor {
            label: Some("Splash Pipeline"),
            layout: Some(&pipeline_layout),
            vertex: wgpu::VertexState {
                module: &shader,
                entry_point: Some("vs_main"),
                buffers: &[],
                compilation_options: Default::default(),
            },
            fragment: Some(wgpu::FragmentState {
                module: &shader,
                entry_point: Some("fs_main"),
                targets: &[Some(wgpu::ColorTargetState {
                    format,
                    blend: Some(wgpu::BlendState::PREMULTIPLIED_ALPHA_BLENDING),
                    write_mask: wgpu::ColorWrites::ALL,
                })],
                compilation_options: Default::default(),
            }),
            primitive: wgpu::PrimitiveState {
                topology: wgpu::PrimitiveTopology::TriangleStrip,
                strip_index_format: None,
                ..Default::default()
            },
            depth_stencil: None,
            multisample: wgpu::MultisampleState::default(),
            multiview_mask: None,
            cache: None,
        });

        let output = surface.get_current_texture().ok()?;
        let view = output.texture.create_view(&wgpu::TextureViewDescriptor::default());

        let mut encoder = device.create_command_encoder(&wgpu::CommandEncoderDescriptor {
            label: Some("Splash Encoder"),
        });

        {
            let mut pass = encoder.begin_render_pass(&wgpu::RenderPassDescriptor {
                label: Some("Splash Pass"),
                color_attachments: &[Some(wgpu::RenderPassColorAttachment {
                    view: &view,
                    resolve_target: None,
                    ops: wgpu::Operations {
                        load: wgpu::LoadOp::Clear(wgpu::Color::TRANSPARENT),
                        store: wgpu::StoreOp::Store,
                    },
                    depth_slice: None,
                })],
                ..Default::default()
            });

            pass.set_pipeline(&pipeline);
            pass.set_bind_group(0, &bind_group, &[]);
            pass.draw(0..4, 0..1);
        }

        queue.submit(std::iter::once(encoder.finish()));
        output.present();

        Some(Self::Gpu {
            window,
            _device: device,
            _queue: queue,
            created_at: Instant::now(),
            min_display: Duration::from_millis(config.min_display_ms),
        })
    }
}

const SPLASH_SHADER: &str = r#"
struct VertexOutput {
    @builtin(position) position: vec4<f32>,
    @location(0) uv: vec2<f32>,
};

@vertex
fn vs_main(@builtin(vertex_index) idx: u32) -> VertexOutput {
    // Triangle strip: 4 вершины → fullscreen quad
    var pos = array<vec2<f32>, 4>(
        vec2<f32>(-1.0, -1.0),
        vec2<f32>( 1.0, -1.0),
        vec2<f32>(-1.0,  1.0),
        vec2<f32>( 1.0,  1.0),
    );
    var uv = array<vec2<f32>, 4>(
        vec2<f32>(0.0, 1.0),
        vec2<f32>(1.0, 1.0),
        vec2<f32>(0.0, 0.0),
        vec2<f32>(1.0, 0.0),
    );
    var out: VertexOutput;
    out.position = vec4<f32>(pos[idx], 0.0, 1.0);
    out.uv = uv[idx];
    return out;
}

@group(0) @binding(0) var tex: texture_2d<f32>;
@group(0) @binding(1) var samp: sampler;

@fragment
fn fs_main(in: VertexOutput) -> @location(0) vec4<f32> {
    let c = textureSample(tex, samp, in.uv);
    // Premultiply alpha для Wayland/X11 compositor
    return vec4<f32>(c.rgb * c.a, c.a);
}
"#;
