pub mod context;
pub mod image_cache;
pub mod image_store;
pub mod pipeline;
pub mod renderer;
pub mod texture_pool;
#[cfg(feature = "map")]
pub mod tile_atlas;

pub use context::{GpuContext, GpuShared, WindowSurface};
pub use image_cache::ImageGpuCache;
pub use image_store::{ImageData, ImageHandle, ImageLoadState, ImageSource, ImageStore};
pub use pipeline::RenderPipeline;
pub use renderer::{RenderStats, Renderer};

/// Флаги wgpu Instance. Отладочные метки (labels) в GL-драйвер не передаём:
/// Mali (Android TV) в glPushDebugGroup читает фиксированные 1024 байта от
/// указателя на метку, игнорируя длину, и падает SIGSEGV, когда строка
/// оказывается у границы страницы. Остальные флаги — как в сборке
/// (`from_build_config`) с учётом переменных окружения WGPU_*.
pub fn instance_flags() -> wgpu::InstanceFlags {
    wgpu::InstanceFlags::from_build_config().with_env() | wgpu::InstanceFlags::DISCARD_HAL_LABELS
}
