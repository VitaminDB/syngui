//! Рендерер без окна для покадровой отрисовки: GPU, шрифты и стили создаются
//! один раз, каждый кадр — свежее дерево виджетов, нарисованное в текстуру и
//! прочитанное в RGBA. Для экспорта видео (кадр таймлайна — дерево виджетов,
//! то же, что в окне предпросмотра), генерации превью и тестов.
//!
//! ```ignore
//! let mut off = syngui::offscreen::Offscreen::new((1920, 1080), &mss)?;
//! for i in 0..n {
//!     let rgba = off.render(move || Box::new(frame_widget(i)))?;
//!     encoder.write(&rgba);
//! }
//! ```
//! Работает на потоке, где создан (сигналы syngui — потоковые).

use std::sync::{Arc, Mutex};
use std::time::Duration;

use crate::core::{Color, Point, Rect, Size};
use crate::gpu::{GpuShared, Renderer};
use crate::layout::Constraints;
use crate::mss::{cascade, parse_stylesheet_str, StyleEngine};
use crate::render::DisplayList;
use crate::signal::{use_signal, RwSignal};
use crate::widget::context::TextMeasure;
use crate::widget::{ElementId, ElementTree, Widget};
use crate::widgets::Reactive;

type Build = Box<dyn FnOnce() -> Box<dyn Widget> + Send>;

pub struct Offscreen {
    gpu: GpuShared,
    renderer: Renderer,
    tree: ElementTree,
    engine: StyleEngine,
    root: ElementId,
    slot: Arc<Mutex<Option<Build>>>,
    tick: RwSignal<u64>,
    logical: Size,
    scale: f32,
    pw: u32,
    ph: u32,
    tex: wgpu::Texture,
    view: wgpu::TextureView,
    buf: wgpu::Buffer,
    row: u32,
    /// Фон под деревом (у видео — чёрный).
    pub background: Color,
    /// Ошибка GPU с последнего кадра (нехватка видеопамяти и т. п.): wgpu по
    /// умолчанию ронял на ней процесс — теперь кадр возвращает её ошибкой.
    gpu_error: Arc<Mutex<Option<String>>>,
    /// Устройство — экономная (встроенная) видеокарта.
    pub low_power: bool,
}

impl Offscreen {
    /// Кадр `size` логических пикселей при масштабе 1.
    pub fn new(size: (u32, u32), mss: &str) -> Result<Self, String> {
        Self::with_scale(size, 1.0, mss)
    }

    pub fn with_scale(size: (u32, u32), scale: f64, mss: &str) -> Result<Self, String> {
        Self::build(size, scale, mss, false)
    }

    /// На экономной (встроенной) видеокарте: дискретная занята (видеопамять
    /// под нейросетями) — кадры медленнее, но рисуются.
    pub fn low_power(size: (u32, u32), mss: &str) -> Result<Self, String> {
        Self::build(size, 1.0, mss, true)
    }

    fn build(size: (u32, u32), scale: f64, mss: &str, low: bool) -> Result<Self, String> {
        crate::signal::init_main_thread();
        crate::signal::allow_signal_reads_on_this_thread();
        let (gpu, renderer, format, pw, ph) = crate::shot::gpu_renderer(size, scale, !low)?;
        let gpu_error: Arc<Mutex<Option<String>>> = Arc::new(Mutex::new(None));
        {
            let ge = gpu_error.clone();
            gpu.device.on_uncaptured_error(Arc::new(move |e: wgpu::Error| {
                let msg = match &e {
                    wgpu::Error::OutOfMemory { .. } => "нехватка видеопамяти".to_string(),
                    other => other.to_string(),
                };
                log::warn!("offscreen GPU: {msg}");
                let mut g = ge.lock().unwrap_or_else(|p| p.into_inner());
                if g.is_none() {
                    *g = Some(msg);
                }
            }));
        }
        let mut tree = ElementTree::new();
        tree.text_measure = Some(renderer.font_atlas.clone() as Arc<dyn TextMeasure>);
        tree.image_store = Some(renderer.image_store.clone());
        let sheet = parse_stylesheet_str(mss).map_err(|e| format!("MSS: {e}"))?;
        let engine = StyleEngine::new(sheet);
        let logical = Size::new(size.0 as f32, size.1 as f32);
        crate::viewport::viewport_size().set(logical);

        // Один корень на всё время: реактивный узел перестраивается по построителю кадра.
        let slot: Arc<Mutex<Option<Build>>> = Arc::new(Mutex::new(None));
        let tick = use_signal(0u64);
        let s = slot.clone();
        let widget = Reactive::new(move || {
            let _ = tick.get();
            match s.lock().unwrap_or_else(|e| e.into_inner()).take() {
                Some(b) => vec![b()],
                None => Vec::new(),
            }
        });
        let element = widget.create_element();
        let root = tree.insert_with_type_id(element, None, widget.as_any().type_id());
        widget.mount(&mut tree, root);
        tree.set_root(root);

        let tex = gpu.device.create_texture(&wgpu::TextureDescriptor {
            label: Some("offscreen"),
            size: wgpu::Extent3d { width: pw, height: ph, depth_or_array_layers: 1 },
            mip_level_count: 1,
            sample_count: 1,
            dimension: wgpu::TextureDimension::D2,
            format,
            usage: wgpu::TextureUsages::RENDER_ATTACHMENT | wgpu::TextureUsages::COPY_SRC,
            view_formats: &[],
        });
        let view = tex.create_view(&Default::default());
        let row = (pw * 4).div_ceil(256) * 256;
        let buf = gpu.device.create_buffer(&wgpu::BufferDescriptor {
            label: Some("offscreen readback"),
            size: (row * ph) as u64,
            usage: wgpu::BufferUsages::COPY_DST | wgpu::BufferUsages::MAP_READ,
            mapped_at_creation: false,
        });
        Ok(Self { gpu, renderer, tree, engine, root, slot, tick, logical, scale: scale as f32, pw, ph, tex, view, buf, row, background: Color::from_srgb(0, 0, 0, 1.0), gpu_error, low_power: low })
    }

    /// Размер кадра в пикселях.
    pub fn pixel_size(&self) -> (u32, u32) {
        (self.pw, self.ph)
    }

    fn step(&mut self) -> DisplayList {
        let (tree, root) = (&mut self.tree, self.root);
        if let Some(store) = tree.image_store.as_ref() {
            store.lock().unwrap_or_else(|e| e.into_inner()).poll_bg();
        }
        crate::signal::drain_and_run_effects();
        for _ in 0..8 {
            if !tree.rebuild_if_needed(root) {
                break;
            }
            cascade::apply_styles_dirty(tree, &self.engine);
        }
        cascade::apply_styles_dirty(tree, &self.engine);
        tree.viewport_size = self.logical;
        tree.set_pixel_snap_scale(0.0);
        let c = Constraints::new(0.0, self.logical.width, 0.0, self.logical.height);
        tree.layout(root, c);
        if tree.rebuild_if_needed(root) {
            cascade::apply_styles_dirty(tree, &self.engine);
            tree.layout(root, c);
        }
        // анимации виджетов (переходы MSS) — сразу в конечное состояние: кадр
        // видео задаёт время сам
        tree.animate(root, Duration::from_secs(10));
        let mut list = DisplayList::new();
        list.set_surface_size(self.logical);
        list.set_scale_factor(self.scale);
        tree.build_display_list(root, &mut list, Rect::new(Point::zero(), self.logical));
        list
    }

    /// Нарисовать дерево `build` и вернуть RGBA кадра (`pixel_size`, строки без выравнивания).
    pub fn render(&mut self, build: impl FnOnce() -> Box<dyn Widget> + Send + 'static) -> Result<Vec<u8>, String> {
        *self.slot.lock().unwrap_or_else(|e| e.into_inner()) = Some(Box::new(build));
        let degraded0 = crate::gpu::image_cache::DEGRADED_UPLOADS.load(std::sync::atomic::Ordering::Relaxed);
        self.tick.update(|t| *t += 1);
        let mut list = self.step();
        // картинки, которые грузятся в фоне (файлы), — дождаться
        for _ in 0..200 {
            let loading = self.tree.image_store.as_ref().map(|s| s.lock().unwrap_or_else(|e| e.into_inner()).has_loading()).unwrap_or(false);
            if !loading {
                break;
            }
            std::thread::sleep(Duration::from_millis(5));
            list = self.step();
        }
        crate::shot::render_all(&mut self.renderer, &self.gpu, &self.view, (self.pw, self.ph), &list, self.background);
        if let Some(e) = self.gpu_error.lock().unwrap_or_else(|p| p.into_inner()).take() {
            return Err(format!("GPU: {e}"));
        }
        // кэш картинок уменьшил или выбросил текстуру из-за нехватки видеопамяти —
        // кадр вышел бы пиксельным: ошибка, вызывающий перерисует его на другой карте
        if crate::gpu::image_cache::DEGRADED_UPLOADS.load(std::sync::atomic::Ordering::Relaxed) != degraded0 {
            return Err("GPU: нехватка видеопамяти — текстура кадра уменьшена".into());
        }
        let mut enc = self.gpu.device.create_command_encoder(&Default::default());
        enc.copy_texture_to_buffer(
            wgpu::TexelCopyTextureInfo { texture: &self.tex, mip_level: 0, origin: wgpu::Origin3d::ZERO, aspect: wgpu::TextureAspect::All },
            wgpu::TexelCopyBufferInfo { buffer: &self.buf, layout: wgpu::TexelCopyBufferLayout { offset: 0, bytes_per_row: Some(self.row), rows_per_image: Some(self.ph) } },
            wgpu::Extent3d { width: self.pw, height: self.ph, depth_or_array_layers: 1 },
        );
        self.gpu.queue.submit([enc.finish()]);
        let slice = self.buf.slice(..);
        slice.map_async(wgpu::MapMode::Read, |_| {});
        let _ = self.gpu.device.poll(wgpu::PollType::wait_indefinitely());
        let data = slice.get_mapped_range();
        let mut rgba = Vec::with_capacity((self.pw * self.ph * 4) as usize);
        for y in 0..self.ph {
            let start = (y * self.row) as usize;
            rgba.extend_from_slice(&data[start..start + (self.pw * 4) as usize]);
        }
        drop(data);
        self.buf.unmap();
        if let Some(e) = self.gpu_error.lock().unwrap_or_else(|p| p.into_inner()).take() {
            return Err(format!("GPU: {e}"));
        }
        Ok(rgba)
    }
}
