pub struct GpuShared {
    pub instance: wgpu::Instance,
    pub adapter: wgpu::Adapter,
    pub device: wgpu::Device,
    pub queue: wgpu::Queue,
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
        self.window_surface.resize(&self.shared.device, width, height);
    }

    pub fn split(self) -> (GpuShared, WindowSurface) {
        (self.shared, self.window_surface)
    }
}
