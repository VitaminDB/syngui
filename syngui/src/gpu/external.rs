//! Кадры из изображений Vulkan другого процесса на той же видеокарте, без копии через процессор:
//! источник (например, паравиртуальная видеокарта ВМ) экспортирует память изображения по
//! `OPAQUE_FD`, syngui создаёт `VkImage` с теми же параметрами и импортирует память. Кадр
//! переписывается в обычную текстуру картинки на GPU (с принудительной непрозрачностью), и
//! источник получает буфер обратно, как только GPU закончил чтение.
//!
//! Работает только на бэкенде Vulkan; [`device_uuids`] — UUID видеокарты окна (источник
//! сверяет их со своими), `None` — импорт недоступен, источник должен отдавать кадры в памяти.

use std::os::fd::{AsRawFd, OwnedFd};
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, Mutex, OnceLock};

use ash::vk;

static UUIDS: OnceLock<([u8; 16], [u8; 16])> = OnceLock::new();

/// UUID видеокарты и драйвера окна (`VkPhysicalDeviceIDProperties`) — когда рендерер на
/// Vulkan и импорт `OPAQUE_FD` включён. Появляется после создания GPU окна.
pub fn device_uuids() -> Option<([u8; 16], [u8; 16])> {
    if std::env::var_os("SYNGUI_NO_EXTERNAL_IMAGE").is_some() {
        return None;
    }
    UUIDS.get().copied()
}

/// Запомнить UUID карты, если рендерер может импортировать внешние изображения.
pub fn detect(device: &wgpu::Device) {
    // SAFETY: только чтение сведений об устройстве; устройство живо.
    let ids = unsafe {
        device.as_hal::<wgpu::hal::api::Vulkan>().and_then(|hal| {
            if !hal.enabled_device_extensions().contains(&ash::khr::external_memory_fd::NAME) {
                return None;
            }
            let inst = hal.shared_instance().raw_instance();
            let mut id = vk::PhysicalDeviceIDProperties::default();
            let mut p2 = vk::PhysicalDeviceProperties2::default().push_next(&mut id);
            inst.get_physical_device_properties2(hal.raw_physical_device(), &mut p2);
            Some((id.device_uuid, id.driver_uuid))
        })
    };
    if let Some(ids) = ids {
        let _ = UUIDS.set(ids);
    }
    log::info!("gpu: внешние изображения Vulkan (OPAQUE_FD) — {}", if ids.is_some() { "да" } else { "нет" });
}

/// Изображение источника: параметры создания и память. Импортируется один раз (по `id`).
pub struct ExternalImage {
    /// Свой у каждого изображения источника — по нему кэшируется импорт.
    pub id: u64,
    pub fd: OwnedFd,
    /// `VkFormat` (поддерживаются B8G8R8A8 и R8G8B8A8, UNORM и SRGB).
    pub format: i32,
    pub width: u32,
    pub height: u32,
    /// `VkImageCreateFlags` (должен быть `MUTABLE_FORMAT`, чтобы читать как sRGB).
    pub flags: u32,
    /// `VkImageUsageFlags` (с `SAMPLED`).
    pub usage: u32,
    /// `allocationSize` памяти.
    pub size: u64,
}

impl std::fmt::Debug for ExternalImage {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("ExternalImage").field("id", &self.id).field("format", &self.format).field("width", &self.width).field("height", &self.height).finish_non_exhaustive()
    }
}

type DoneFn = Box<dyn FnOnce(bool) + Send + Sync>;

/// Кадр в изображении источника. Пока жив, источник не пишет в изображение; при освобождении
/// вызывается `done(true)` (GPU прочитал или кадр не понадобился) или `done(false)` (импорт не
/// удался — источнику пора отдавать кадры в памяти).
pub struct ExternalFrame {
    pub image: Arc<ExternalImage>,
    done: Mutex<Option<DoneFn>>,
    failed: AtomicBool,
}

impl ExternalFrame {
    pub fn new(image: Arc<ExternalImage>, done: impl FnOnce(bool) + Send + Sync + 'static) -> Arc<Self> {
        Arc::new(Self { image, done: Mutex::new(Some(Box::new(done))), failed: AtomicBool::new(false) })
    }

    pub(crate) fn fail(&self) {
        self.failed.store(true, Ordering::Relaxed);
    }
}

impl Drop for ExternalFrame {
    fn drop(&mut self) {
        if let Some(f) = self.done.lock().unwrap_or_else(|e| e.into_inner()).take() {
            f(!self.failed.load(Ordering::Relaxed));
        }
    }
}

impl std::fmt::Debug for ExternalFrame {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("ExternalFrame").field("image", &self.image).finish_non_exhaustive()
    }
}

/// Формат текстуры wgpu и вид в sRGB для `VkFormat` источника.
pub(crate) fn formats(vk_format: i32) -> Option<(wgpu::TextureFormat, wgpu::TextureFormat)> {
    use wgpu::TextureFormat as F;
    match vk::Format::from_raw(vk_format) {
        vk::Format::B8G8R8A8_UNORM => Some((F::Bgra8Unorm, F::Bgra8UnormSrgb)),
        vk::Format::B8G8R8A8_SRGB => Some((F::Bgra8UnormSrgb, F::Bgra8UnormSrgb)),
        vk::Format::R8G8B8A8_UNORM => Some((F::Rgba8Unorm, F::Rgba8UnormSrgb)),
        vk::Format::R8G8B8A8_SRGB => Some((F::Rgba8UnormSrgb, F::Rgba8UnormSrgb)),
        _ => None,
    }
}

/// Импорт изображения источника в текстуру wgpu (только чтение). `None` — не удалось (причина в лог).
pub(crate) fn import(device: &wgpu::Device, img: &ExternalImage) -> Option<wgpu::Texture> {
    let Some((format, view)) = formats(img.format) else {
        log::warn!("gpu: внешнее изображение: формат {} не поддерживается", img.format);
        return None;
    };
    let usage = vk::ImageUsageFlags::from_raw(img.usage);
    if !usage.contains(vk::ImageUsageFlags::SAMPLED) || (view != format && img.flags & vk::ImageCreateFlags::MUTABLE_FORMAT.as_raw() == 0) {
        log::warn!("gpu: внешнее изображение: нет SAMPLED или MUTABLE_FORMAT");
        return None;
    }
    // SAFETY: изображение и память создаются на устройстве wgpu и отдаются ему же (освобождает
    // wgpu-hal: Dedicated); fd дублируется — Vulkan забирает копию при успешном импорте.
    unsafe {
        let hal = device.as_hal::<wgpu::hal::api::Vulkan>()?;
        let dev = hal.raw_device();
        let inst = hal.shared_instance().raw_instance();
        let ht = vk::ExternalMemoryHandleTypeFlags::OPAQUE_FD;
        let mut ext = vk::ExternalMemoryImageCreateInfo::default().handle_types(ht);
        let ici = vk::ImageCreateInfo::default()
            .flags(vk::ImageCreateFlags::from_raw(img.flags))
            .image_type(vk::ImageType::TYPE_2D)
            .format(vk::Format::from_raw(img.format))
            .extent(vk::Extent3D { width: img.width, height: img.height, depth: 1 })
            .mip_levels(1)
            .array_layers(1)
            .samples(vk::SampleCountFlags::TYPE_1)
            .tiling(vk::ImageTiling::OPTIMAL)
            .usage(usage)
            .sharing_mode(vk::SharingMode::EXCLUSIVE)
            .initial_layout(vk::ImageLayout::UNDEFINED)
            .push_next(&mut ext);
        let image = match dev.create_image(&ici, None) {
            Ok(i) => i,
            Err(e) => {
                log::warn!("gpu: внешнее изображение: vkCreateImage: {e}");
                return None;
            }
        };
        let req = dev.get_image_memory_requirements(image);
        let mp = inst.get_physical_device_memory_properties(hal.raw_physical_device());
        let ty = (0..mp.memory_type_count).find(|&i| {
            req.memory_type_bits & (1 << i) != 0 && mp.memory_types[i as usize].property_flags.contains(vk::MemoryPropertyFlags::DEVICE_LOCAL)
        });
        let Some(ty) = ty else {
            dev.destroy_image(image, None);
            log::warn!("gpu: внешнее изображение: нет типа памяти DEVICE_LOCAL");
            return None;
        };
        let fd = libc::dup(img.fd.as_raw_fd());
        if fd < 0 {
            dev.destroy_image(image, None);
            return None;
        }
        let mut ded = vk::MemoryDedicatedAllocateInfo::default().image(image);
        let mut imp = vk::ImportMemoryFdInfoKHR::default().handle_type(ht).fd(fd);
        let ai = vk::MemoryAllocateInfo::default().allocation_size(img.size).memory_type_index(ty).push_next(&mut ded).push_next(&mut imp);
        let memory = match dev.allocate_memory(&ai, None) {
            Ok(m) => m,
            Err(e) => {
                libc::close(fd);
                dev.destroy_image(image, None);
                log::warn!("gpu: внешнее изображение: импорт памяти: {e}");
                return None;
            }
        };
        if let Err(e) = dev.bind_image_memory(image, memory, 0) {
            dev.destroy_image(image, None);
            dev.free_memory(memory, None);
            log::warn!("gpu: внешнее изображение: vkBindImageMemory: {e}");
            return None;
        }
        let size = wgpu::Extent3d { width: img.width, height: img.height, depth_or_array_layers: 1 };
        let views: Vec<wgpu::TextureFormat> = if view != format { vec![view] } else { vec![] };
        let hal_desc = wgpu::hal::TextureDescriptor {
            label: Some("external image"),
            size,
            mip_level_count: 1,
            sample_count: 1,
            dimension: wgpu::TextureDimension::D2,
            format,
            usage: wgpu::TextureUses::RESOURCE,
            memory_flags: wgpu::hal::MemoryFlags::empty(),
            view_formats: views.clone(),
        };
        let raw = hal.texture_from_raw(image, &hal_desc, None, wgpu::hal::vulkan::TextureMemory::Dedicated(memory));
        drop(hal);
        Some(device.create_texture_from_hal::<wgpu::hal::api::Vulkan>(
            raw,
            &wgpu::TextureDescriptor {
                label: Some("external image"),
                size,
                mip_level_count: 1,
                sample_count: 1,
                dimension: wgpu::TextureDimension::D2,
                format,
                usage: wgpu::TextureUsages::TEXTURE_BINDING,
                view_formats: &views,
            },
        ))
    }
}
