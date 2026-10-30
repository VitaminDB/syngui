pub struct GpuShared {
    pub instance: wgpu::Instance,
    pub adapter: wgpu::Adapter,
    pub device: wgpu::Device,
    pub queue: wgpu::Queue,
}

/// Формат поверхности окна: sRGB, а на Adreno (Qualcomm) — RGBA8, если он есть.
/// Дисплей Qualcomm берёт сжатые (UBWC) кадры только в ABGR/XBGR8888: с BGRA
/// (XRGB) окно во весь экран выводится на план несжатым — вдвое больше трафика
/// памяти у GPU и дисплея. Остальным GPU — первый sRGB-формат, как раньше.
/// `SYNGUI_SURFACE_RGBA=0` — не выбирать RGBA8 (сравнение, отладка).
static HDR_WANTED: std::sync::atomic::AtomicBool = std::sync::atomic::AtomicBool::new(false);
static HDR_SURFACE: std::sync::atomic::AtomicBool = std::sync::atomic::AtomicBool::new(false);

/// Приложение хочет HDR-окно (видеоплеер, игры): поверхность FP16 в линейном extended sRGB
/// (scRGB, 1.0 — белый интерфейса, света выше), если её даёт композитор (протокол управления цветом
/// `wp_color_management_v1`: Vulkan WSI Mesa объявляет её сам). Вызывать до открытия окна.
/// `SYNGUI_HDR=0` — не просить.
pub fn request_hdr_surface(on: bool) {
    HDR_WANTED.store(on, std::sync::atomic::Ordering::Relaxed);
}

static HDR_OUTPUT: std::sync::atomic::AtomicBool = std::sync::atomic::AtomicBool::new(false);

/// Приложение просило HDR-окно.
pub fn hdr_requested() -> bool {
    HDR_WANTED.load(std::sync::atomic::Ordering::Relaxed) && std::env::var("SYNGUI_HDR").map_or(true, |v| v != "0")
}

/// Окно сейчас в HDR (FP16 extended sRGB).
pub fn hdr_surface() -> bool {
    HDR_SURFACE.load(std::sync::atomic::Ordering::Relaxed)
}

/// Монитор окна в HDR-режиме (сообщает приложение — у synshell это `synwm msg outputs`). Только
/// тогда HDR-видео идёт в окно светом без тонмаппинга; на SDR-мониторе тонмаппинг делает само окно —
/// точнее композитора, оно знает пик ролика.
pub fn set_hdr_output(on: bool) {
    HDR_OUTPUT.store(on, std::sync::atomic::Ordering::Relaxed);
}

/// HDR-видео — светом без тонмаппинга: HDR-окно на HDR-мониторе.
pub fn hdr_passthrough() -> bool {
    hdr_surface() && HDR_OUTPUT.load(std::sync::atomic::Ordering::Relaxed)
}

pub fn preferred_surface_format(
    caps: &wgpu::SurfaceCapabilities,
    adapter: &wgpu::Adapter,
) -> wgpu::TextureFormat {
    const QUALCOMM: u32 = 0x5143;
    let hdr = HDR_WANTED.load(std::sync::atomic::Ordering::Relaxed)
        && std::env::var("SYNGUI_HDR").map_or(true, |v| v != "0")
        && caps.formats.contains(&wgpu::TextureFormat::Rgba16Float);
    HDR_SURFACE.store(hdr, std::sync::atomic::Ordering::Relaxed);
    if hdr {
        log::info!("gpu: HDR-окно — Rgba16Float, extended sRGB linear");
        return wgpu::TextureFormat::Rgba16Float;
    }
    let allowed = std::env::var("SYNGUI_SURFACE_RGBA").map_or(true, |v| v != "0");
    if allowed
        && adapter.get_info().vendor == QUALCOMM
        && caps.formats.contains(&wgpu::TextureFormat::Rgba8UnormSrgb)
    {
        return wgpu::TextureFormat::Rgba8UnormSrgb;
    }
    caps.formats
        .iter()
        .copied()
        .find(|f| f.is_srgb())
        .unwrap_or(caps.formats[0])
}

pub struct WindowSurface {
    pub surface: wgpu::Surface<'static>,
    pub surface_config: wgpu::SurfaceConfiguration,
}

impl WindowSurface {
    pub fn resize(&mut self, device: &wgpu::Device, width: u32, height: u32) {
        if width > 0 && height > 0 {
            self.surface_config.width = width;
            self.surface_config.height = height;
            self.surface.configure(device, &self.surface_config);
        }
    }
}

pub struct GpuContext {
    pub shared: GpuShared,
    pub window_surface: WindowSurface,
}

impl GpuContext {
    pub fn resize(&mut self, width: u32, height: u32) {
        self.window_surface
            .resize(&self.shared.device, width, height);
    }

    pub fn split(self) -> (GpuShared, WindowSurface) {
        (self.shared, self.window_surface)
    }
}
