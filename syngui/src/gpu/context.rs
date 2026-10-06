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
pub fn preferred_surface_format(caps: &wgpu::SurfaceCapabilities, adapter: &wgpu::Adapter) -> wgpu::TextureFormat {
    const QUALCOMM: u32 = 0x5143;
    let allowed = std::env::var("SYNGUI_SURFACE_RGBA").map_or(true, |v| v != "0");
    if allowed && adapter.get_info().vendor == QUALCOMM && caps.formats.contains(&wgpu::TextureFormat::Rgba8UnormSrgb) {
        return wgpu::TextureFormat::Rgba8UnormSrgb;
    }
    caps.formats.iter().copied().find(|f| f.is_srgb()).unwrap_or(caps.formats[0])
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
