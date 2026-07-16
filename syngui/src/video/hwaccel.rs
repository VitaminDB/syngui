#![allow(unsafe_op_in_unsafe_fn)]

use std::cell::Cell;
use std::ffi::CString;
use std::os::raw::c_int;
use std::ptr;

use ffmpeg_next::ffi;

use super::error::VideoError;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum HwAccel {
    None,
    Auto,
    Vaapi,
    Nvdec,
    VideoToolbox,
    D3D11Va,
    Dxva2,
    Vulkan,
}

impl Default for HwAccel {
    fn default() -> Self {
        Self::None
    }
}

impl HwAccel {
    pub fn platform_default() -> Self {
        #[cfg(target_os = "linux")]
        {
            if std::path::Path::new("/proc/driver/nvidia/version").exists() {
                return Self::Nvdec;
            }
            return Self::Vaapi;
        }
        #[cfg(target_os = "macos")]
        {
            return Self::VideoToolbox;
        }
        #[cfg(target_os = "windows")]
        {
            return Self::D3D11Va;
        }
        #[allow(unreachable_code)]
        Self::None
    }

    fn to_av_type(self) -> Option<ffi::AVHWDeviceType> {
        let resolved = match self {
            Self::Auto => Self::platform_default(),
            other => other,
        };
        match resolved {
            Self::None | Self::Auto => None,
            Self::Vaapi => Some(ffi::AVHWDeviceType::AV_HWDEVICE_TYPE_VAAPI),
            Self::Nvdec => Some(ffi::AVHWDeviceType::AV_HWDEVICE_TYPE_CUDA),
            Self::VideoToolbox => Some(ffi::AVHWDeviceType::AV_HWDEVICE_TYPE_VIDEOTOOLBOX),
            Self::D3D11Va => Some(ffi::AVHWDeviceType::AV_HWDEVICE_TYPE_D3D11VA),
            Self::Dxva2 => Some(ffi::AVHWDeviceType::AV_HWDEVICE_TYPE_DXVA2),
            Self::Vulkan => Some(ffi::AVHWDeviceType::AV_HWDEVICE_TYPE_VULKAN),
        }
    }

    pub(crate) fn hw_pix_fmt(self) -> Option<ffi::AVPixelFormat> {
        let resolved = match self {
            Self::Auto => Self::platform_default(),
            other => other,
        };
        match resolved {
            Self::None | Self::Auto => None,
            Self::Vaapi => Some(ffi::AVPixelFormat::AV_PIX_FMT_VAAPI),
            Self::Nvdec => Some(ffi::AVPixelFormat::AV_PIX_FMT_CUDA),
            Self::VideoToolbox => Some(ffi::AVPixelFormat::AV_PIX_FMT_VIDEOTOOLBOX),
            Self::D3D11Va => Some(ffi::AVPixelFormat::AV_PIX_FMT_D3D11),
            Self::Dxva2 => Some(ffi::AVPixelFormat::AV_PIX_FMT_DXVA2_VLD),
            Self::Vulkan => Some(ffi::AVPixelFormat::AV_PIX_FMT_VULKAN),
        }
    }

    pub fn nvdec_codec_name(self, codec_id: ffi::AVCodecID) -> Option<&'static str> {
        let resolved = match self {
            Self::Auto => Self::platform_default(),
            other => other,
        };
        if !matches!(resolved, Self::Nvdec) {
            return None;
        }
        Some(match codec_id {
            ffi::AVCodecID::AV_CODEC_ID_H264 => "h264_cuvid",
            ffi::AVCodecID::AV_CODEC_ID_HEVC => "hevc_cuvid",
            ffi::AVCodecID::AV_CODEC_ID_AV1 => "av1_cuvid",
            ffi::AVCodecID::AV_CODEC_ID_VP9 => "vp9_cuvid",
            ffi::AVCodecID::AV_CODEC_ID_VP8 => "vp8_cuvid",
            ffi::AVCodecID::AV_CODEC_ID_MPEG4 => "mpeg4_cuvid",
            ffi::AVCodecID::AV_CODEC_ID_MPEG2VIDEO => "mpeg2_cuvid",
            ffi::AVCodecID::AV_CODEC_ID_MPEG1VIDEO => "mpeg1_cuvid",
            ffi::AVCodecID::AV_CODEC_ID_VC1 => "vc1_cuvid",
            _ => return None,
        })
    }

    pub fn label(self) -> &'static str {
        match self {
            Self::None => "none",
            Self::Auto => "auto",
            Self::Vaapi => "vaapi",
            Self::Nvdec => "nvdec",
            Self::VideoToolbox => "videotoolbox",
            Self::D3D11Va => "d3d11va",
            Self::Dxva2 => "dxva2",
            Self::Vulkan => "vulkan",
        }
    }
}

thread_local! {
    static CURRENT_HW_PIX_FMT: Cell<i32> = Cell::new(ffi::AVPixelFormat::AV_PIX_FMT_NONE as i32);
}

pub struct HwContext {
    device_ref: *mut ffi::AVBufferRef,
    hw_pix_fmt: ffi::AVPixelFormat,
    accel: HwAccel,
}

unsafe impl Send for HwContext {}

impl HwContext {
    pub fn try_init(accel: HwAccel) -> Result<Self, VideoError> {
        let resolved = match accel {
            HwAccel::Auto => HwAccel::platform_default(),
            other => other,
        };
        let device_type = resolved
            .to_av_type()
            .ok_or_else(|| VideoError::Other(format!("hwaccel: {} не поддерживается", resolved.label())))?;
        let hw_pix_fmt = resolved
            .hw_pix_fmt()
            .ok_or_else(|| VideoError::Other(format!("hwaccel: pix_fmt для {} не найден", resolved.label())))?;

        let mut device_ref: *mut ffi::AVBufferRef = ptr::null_mut();
        let rc = unsafe {
            ffi::av_hwdevice_ctx_create(
                &mut device_ref,
                device_type,
                ptr::null(),
                ptr::null_mut(),
                0,
            )
        };
        if rc < 0 || device_ref.is_null() {
            return Err(VideoError::DecoderInit(format!(
                "av_hwdevice_ctx_create({}) вернул {rc}",
                accel.label()
            )));
        }
        log::info!("hwaccel: {} устройство инициализировано", resolved.label());
        Ok(Self {
            device_ref,
            hw_pix_fmt,
            accel: resolved,
        })
    }

    /// # Safety
    pub unsafe fn attach_to(&self, codec_ctx: *mut ffi::AVCodecContext) {
        CURRENT_HW_PIX_FMT.with(|cell| cell.set(self.hw_pix_fmt as i32));
        (*codec_ctx).hw_device_ctx = ffi::av_buffer_ref(self.device_ref);
        (*codec_ctx).get_format = Some(get_format_trampoline);
    }

    pub fn transfer_to_cpu(
        &self,
        hw_frame: &ffmpeg_next::frame::Video,
    ) -> Result<ffmpeg_next::frame::Video, VideoError> {
        let mut sw_frame = ffmpeg_next::frame::Video::empty();
        let rc = unsafe {
            ffi::av_hwframe_transfer_data(sw_frame.as_mut_ptr(), hw_frame.as_ptr(), 0)
        };
        if rc < 0 {
            return Err(VideoError::DecoderInit(format!(
                "av_hwframe_transfer_data вернул {rc}"
            )));
        }
        if let Some(pts) = hw_frame.pts() {
            sw_frame.set_pts(Some(pts));
        }
        Ok(sw_frame)
    }

    pub fn hw_pix_fmt(&self) -> ffi::AVPixelFormat {
        self.hw_pix_fmt
    }

    pub fn label(&self) -> &'static str {
        self.accel.label()
    }
}

impl Drop for HwContext {
    fn drop(&mut self) {
        CURRENT_HW_PIX_FMT.with(|cell| {
            cell.set(ffi::AVPixelFormat::AV_PIX_FMT_NONE as i32);
        });
        if !self.device_ref.is_null() {
            unsafe {
                ffi::av_buffer_unref(&mut self.device_ref);
            }
        }
    }
}

unsafe extern "C" fn get_format_trampoline(
    ctx: *mut ffi::AVCodecContext,
    fmts: *const ffi::AVPixelFormat,
) -> ffi::AVPixelFormat {
    if fmts.is_null() {
        return ffi::AVPixelFormat::AV_PIX_FMT_NONE;
    }
    let target = CURRENT_HW_PIX_FMT.with(|cell| cell.get());

    let mut p = fmts;
    let mut fallback = ffi::AVPixelFormat::AV_PIX_FMT_NONE;
    while *p != ffi::AVPixelFormat::AV_PIX_FMT_NONE {
        if (*p) as i32 == target {
            if setup_hw_frames_ctx(ctx, *p) {
                return *p;
            } else {
                log::warn!("hwaccel: setup_hw_frames_ctx для {:?} провалился", *p);
            }
        }
        if fallback == ffi::AVPixelFormat::AV_PIX_FMT_NONE {
            fallback = *p;
        }
        p = p.add(1);
    }
    fallback
}

/// # Safety
unsafe fn setup_hw_frames_ctx(
    ctx: *mut ffi::AVCodecContext,
    hw_pix_fmt: ffi::AVPixelFormat,
) -> bool {
    let device_ref = (*ctx).hw_device_ctx;
    if device_ref.is_null() {
        return false;
    }
    let frames_ref = ffi::av_hwframe_ctx_alloc(device_ref);
    if frames_ref.is_null() {
        log::warn!("hwaccel: av_hwframe_ctx_alloc вернул null");
        return false;
    }
    let frames_ctx = (*frames_ref).data as *mut ffi::AVHWFramesContext;
    (*frames_ctx).format = hw_pix_fmt;
    (*frames_ctx).sw_format = ffi::AVPixelFormat::AV_PIX_FMT_NV12;
    (*frames_ctx).width = (*ctx).coded_width.max((*ctx).width);
    (*frames_ctx).height = (*ctx).coded_height.max((*ctx).height);
    (*frames_ctx).initial_pool_size = 20;
    let rc = ffi::av_hwframe_ctx_init(frames_ref);
    if rc < 0 {
        log::warn!("hwaccel: av_hwframe_ctx_init вернул {rc}");
        let mut tmp = frames_ref;
        ffi::av_buffer_unref(&mut tmp);
        return false;
    }
    (*ctx).hw_frames_ctx = frames_ref;
    true
}

#[allow(dead_code)]
fn _ensure_cstring_imported(_s: &CString) -> c_int {
    0
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn platform_default_resolves() {
        let d = HwAccel::platform_default();
        assert!(matches!(
            d,
            HwAccel::Vaapi
                | HwAccel::Nvdec
                | HwAccel::VideoToolbox
                | HwAccel::D3D11Va
                | HwAccel::None
        ));
    }

    #[test]
    fn auto_maps_to_platform() {
        let auto_fmt = HwAccel::Auto.hw_pix_fmt();
        let plat_fmt = HwAccel::platform_default().hw_pix_fmt();
        assert_eq!(auto_fmt, plat_fmt);
    }

    #[test]
    fn none_has_no_av_type() {
        assert!(HwAccel::None.to_av_type().is_none());
        assert!(HwAccel::None.hw_pix_fmt().is_none());
    }

    #[test]
    fn labels_are_stable() {
        assert_eq!(HwAccel::Vaapi.label(), "vaapi");
        assert_eq!(HwAccel::Nvdec.label(), "nvdec");
    }
}
