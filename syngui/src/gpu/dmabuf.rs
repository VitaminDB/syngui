//! Кадры аппаратного декодера (dma-buf) прямо в текстуры GPU, без копии через
//! процессор: на GL (EGL) плоскости NV12 импортируются как EGLImage — Y как
//! `DRM_FORMAT_R8`, UV как `DRM_FORMAT_GR88` — и становятся обычными текстурами
//! wgpu (`R8Unorm`/`Rg8Unorm`), которые читает тот же `yuv.wgsl`.
//!
//! Декодер узнаёт, умеет ли рендерер импорт, по [`import_supported`] и только
//! тогда отдаёт кадры в dma-buf; иначе копирует их, как раньше (Vulkan — пока
//! копия).

use std::ffi::c_void;
use std::num::NonZeroU32;
use std::sync::atomic::{AtomicBool, Ordering};

static SUPPORTED: AtomicBool = AtomicBool::new(false);

/// Рендерер умеет показывать кадры из dma-buf ([`detect`] при создании GPU-кэша).
pub fn import_supported() -> bool {
    SUPPORTED.load(Ordering::Relaxed) && std::env::var_os("SYNGUI_NO_DMABUF_IMPORT").is_none()
}

const EGL_WIDTH: i32 = 0x3057;
const EGL_HEIGHT: i32 = 0x3056;
const EGL_NONE: i32 = 0x3038;
const EGL_LINUX_DMA_BUF_EXT: u32 = 0x3270;
const EGL_LINUX_DRM_FOURCC_EXT: i32 = 0x3271;
const EGL_DMA_BUF_PLANE0_FD_EXT: i32 = 0x3272;
const EGL_DMA_BUF_PLANE0_OFFSET_EXT: i32 = 0x3273;
const EGL_DMA_BUF_PLANE0_PITCH_EXT: i32 = 0x3274;
const GL_TEXTURE_2D: u32 = 0x0DE1;

/// `DRM_FORMAT_R8` — яркость.
pub const FOURCC_R8: u32 = u32::from_le_bytes(*b"R8  ");
/// `DRM_FORMAT_GR88` — цветность NV12 (U в R, V в G).
pub const FOURCC_GR88: u32 = u32::from_le_bytes(*b"GR88");

type CreateImage = unsafe extern "system" fn(*mut c_void, *mut c_void, u32, *mut c_void, *const i32) -> *mut c_void;
type DestroyImage = unsafe extern "system" fn(*mut c_void, *mut c_void) -> u32;
type ImageTargetTexture = unsafe extern "system" fn(u32, *mut c_void);

/// Проверить импорт: бэкенд GL на EGL с `EGL_EXT_image_dma_buf_import`.
pub fn detect(device: &wgpu::Device) {
    // SAFETY: только чтение сведений о контексте; устройство живо.
    let ok = unsafe {
        device.as_hal::<wgpu::hal::api::Gles>().is_some_and(|hal| {
            let ctx = hal.context();
            match (ctx.egl_instance(), ctx.raw_display()) {
                (Some(egl), Some(dpy)) => egl
                    .query_string(Some(*dpy), khronos_egl::EXTENSIONS)
                    .ok()
                    .and_then(|s| s.to_str().ok().map(|s| s.contains("EGL_EXT_image_dma_buf_import")))
                    .unwrap_or(false)
                    && egl.get_proc_address("glEGLImageTargetTexture2DOES").is_some(),
                _ => false,
            }
        })
    };
    SUPPORTED.store(ok, Ordering::Relaxed);
    log::info!("видео: кадры из dma-buf прямо в GPU — {}", if ok { "да (EGLImage)" } else { "нет, копия" });
}

/// Плоскость dma-buf → текстура wgpu (`format` — R8Unorm или Rg8Unorm).
#[allow(clippy::too_many_arguments)]
pub fn import_plane(
    device: &wgpu::Device,
    fd: i32,
    offset: u32,
    pitch: u32,
    width: u32,
    height: u32,
    fourcc: u32,
    format: wgpu::TextureFormat,
) -> Option<wgpu::Texture> {
    // SAFETY: функции EGL/GL берутся у того же EGL, что у wgpu; контекст
    // делается текущим `lock()`; текстура отдаётся wgpu, он её и удалит.
    unsafe {
        let hal = device.as_hal::<wgpu::hal::api::Gles>()?;
        let ctx = hal.context();
        let egl = ctx.egl_instance()?;
        let dpy = ctx.raw_display()?.as_ptr();
        let create: CreateImage = std::mem::transmute(egl.get_proc_address("eglCreateImageKHR")?);
        let destroy: DestroyImage = std::mem::transmute(egl.get_proc_address("eglDestroyImageKHR")?);
        let target: ImageTargetTexture = std::mem::transmute(egl.get_proc_address("glEGLImageTargetTexture2DOES")?);
        let attribs = [
            EGL_WIDTH,
            width as i32,
            EGL_HEIGHT,
            height as i32,
            EGL_LINUX_DRM_FOURCC_EXT,
            fourcc as i32,
            EGL_DMA_BUF_PLANE0_FD_EXT,
            fd,
            EGL_DMA_BUF_PLANE0_OFFSET_EXT,
            offset as i32,
            EGL_DMA_BUF_PLANE0_PITCH_EXT,
            pitch as i32,
            EGL_NONE,
        ];
        let gl = ctx.lock();
        let image = create(dpy, std::ptr::null_mut(), EGL_LINUX_DMA_BUF_EXT, std::ptr::null_mut(), attribs.as_ptr());
        if image.is_null() {
            log::warn!("видео: eglCreateImage dma-buf {width}x{height} {fourcc:08x} не удался");
            return None;
        }
        use glow::HasContext;
        let tex = gl.create_texture().ok();
        if let Some(t) = tex {
            gl.bind_texture(glow::TEXTURE_2D, Some(t));
            target(GL_TEXTURE_2D, image);
            gl.tex_parameter_i32(glow::TEXTURE_2D, glow::TEXTURE_MIN_FILTER, glow::LINEAR as i32);
            gl.tex_parameter_i32(glow::TEXTURE_2D, glow::TEXTURE_MAG_FILTER, glow::LINEAR as i32);
            gl.tex_parameter_i32(glow::TEXTURE_2D, glow::TEXTURE_WRAP_S, glow::CLAMP_TO_EDGE as i32);
            gl.tex_parameter_i32(glow::TEXTURE_2D, glow::TEXTURE_WRAP_T, glow::CLAMP_TO_EDGE as i32);
            gl.bind_texture(glow::TEXTURE_2D, None);
        }
        // Текстура держит память dma-buf сама: EGLImage больше не нужен.
        destroy(dpy, image);
        let err = gl.get_error();
        drop(gl);
        let tex = tex?;
        if err != glow::NO_ERROR {
            log::warn!("видео: импорт dma-buf в текстуру: ошибка GL {err:#x}");
            return None;
        }
        let size = wgpu::Extent3d { width, height, depth_or_array_layers: 1 };
        let hal_desc = wgpu::hal::TextureDescriptor {
            label: Some("YUV dma-buf"),
            size,
            mip_level_count: 1,
            sample_count: 1,
            dimension: wgpu::TextureDimension::D2,
            format,
            usage: wgpu::TextureUses::RESOURCE,
            memory_flags: wgpu::hal::MemoryFlags::empty(),
            view_formats: vec![],
        };
        let hal_tex = hal.texture_from_raw(NonZeroU32::new(tex.0.get())?, &hal_desc, None);
        drop(hal);
        Some(device.create_texture_from_hal::<wgpu::hal::api::Gles>(
            hal_tex,
            &wgpu::TextureDescriptor {
                label: Some("YUV dma-buf"),
                size,
                mip_level_count: 1,
                sample_count: 1,
                dimension: wgpu::TextureDimension::D2,
                format,
                usage: wgpu::TextureUsages::TEXTURE_BINDING,
                view_formats: &[],
            },
        ))
    }
}
