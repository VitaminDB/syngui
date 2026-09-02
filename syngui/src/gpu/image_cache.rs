use super::image_store::{ImageData, ImageHandle, ImageStore};
use hashbrown::HashMap;

/// Сколько mip-уровней нужно стороне `max(w, h)` — вплоть до 1×1.
fn mip_level_count(w: u32, h: u32) -> u32 {
    32 - w.max(h).max(1).leading_zeros()
}

/// sRGB → линейный свет, таблицей на байт (powf на каждый тексел большого
/// фото — это уже сотни миллисекунд на цепочку мипов).
fn srgb_to_linear(v: u8) -> f32 {
    static LUT: std::sync::OnceLock<[f32; 256]> = std::sync::OnceLock::new();
    LUT.get_or_init(|| {
        std::array::from_fn(|i| {
            let x = i as f32 / 255.0;
            if x <= 0.04045 {
                x / 12.92
            } else {
                ((x + 0.055) / 1.055).powf(2.4)
            }
        })
    })[v as usize]
}

/// Линейный свет → sRGB-байт, обратной таблицей на 4096 корзин: для мипов
/// ошибка квантования ≤ 1/255 незаметна, а powf уходит из горячего цикла.
fn linear_to_srgb(x: f32) -> u8 {
    static LUT: std::sync::OnceLock<[u8; 4096]> = std::sync::OnceLock::new();
    let lut = LUT.get_or_init(|| {
        std::array::from_fn(|i| {
            let x = i as f32 / 4095.0;
            let y = if x <= 0.003_130_8 {
                x * 12.92
            } else {
                1.055 * x.powf(1.0 / 2.4) - 0.055
            };
            (y.clamp(0.0, 1.0) * 255.0).round() as u8
        })
    });
    lut[((x.clamp(0.0, 1.0) * 4095.0) as usize).min(4095)]
}

/// Следующий mip-уровень: 2×2-бокс предыдущего. Усреднение — в линейном
/// свете и с весом по альфе: среднее sRGB-байтов по прямой альфе даёт
/// грязные ореолы на прозрачных краях и тёмные полутона (рваный логотип
/// в рейле — минификация 512 → 30 px без мипов, а первая же попытка мипов
/// «в лоб» дала бы кайму). Нечётные размеры кламплю к последнему ряду.
fn downscale_half(w: u32, h: u32, src: &[u8]) -> (u32, u32, Vec<u8>) {
    let dw = (w / 2).max(1);
    let dh = (h / 2).max(1);
    let mut dst = vec![0u8; (dw * dh * 4) as usize];
    for dy in 0..dh {
        for dx in 0..dw {
            let sx0 = (dx * 2).min(w - 1);
            let sy0 = (dy * 2).min(h - 1);
            let sx1 = (sx0 + 1).min(w - 1);
            let sy1 = (sy0 + 1).min(h - 1);
            let (mut r, mut g, mut b, mut a) = (0.0f32, 0.0f32, 0.0f32, 0.0f32);
            for (sx, sy) in [(sx0, sy0), (sx1, sy0), (sx0, sy1), (sx1, sy1)] {
                let i = ((sy * w + sx) * 4) as usize;
                let pa = src[i + 3] as f32 / 255.0;
                r += srgb_to_linear(src[i]) * pa;
                g += srgb_to_linear(src[i + 1]) * pa;
                b += srgb_to_linear(src[i + 2]) * pa;
                a += pa;
            }
            let o = ((dy * dw + dx) * 4) as usize;
            if a > 0.0 {
                dst[o] = linear_to_srgb(r / a);
                dst[o + 1] = linear_to_srgb(g / a);
                dst[o + 2] = linear_to_srgb(b / a);
            }
            dst[o + 3] = (a / 4.0 * 255.0).round().clamp(0.0, 255.0) as u8;
        }
    }
    (dw, dh, dst)
}

/// Записывает уровень `level` размером `w×h` в текстуру.
fn write_level(
    queue: &wgpu::Queue,
    texture: &wgpu::Texture,
    level: u32,
    w: u32,
    h: u32,
    rgba: &[u8],
) {
    queue.write_texture(
        wgpu::TexelCopyTextureInfo {
            texture,
            mip_level: level,
            origin: wgpu::Origin3d::ZERO,
            aspect: wgpu::TextureAspect::All,
        },
        rgba,
        wgpu::TexelCopyBufferLayout {
            offset: 0,
            bytes_per_row: Some(4 * w),
            rows_per_image: Some(h),
        },
        wgpu::Extent3d {
            width: w,
            height: h,
            depth_or_array_layers: 1,
        },
    );
}

/// Заливает нулевой уровень и достраивает всю цепочку мипов CPU-боксом.
fn write_all_levels(queue: &wgpu::Queue, texture: &wgpu::Texture, data: &ImageData) {
    write_level(queue, texture, 0, data.width, data.height, &data.rgba);
    let levels = mip_level_count(data.width, data.height);
    let (mut w, mut h) = (data.width, data.height);
    let mut cur: Vec<u8> = data.rgba.to_vec();
    for level in 1..levels {
        let (nw, nh, next) = downscale_half(w, h, &cur);
        write_level(queue, texture, level, nw, nh, &next);
        w = nw;
        h = nh;
        cur = next;
    }
}

struct GpuImage {
    texture: wgpu::Texture,
    bind_group: wgpu::BindGroup,
}

pub struct ImageGpuCache {
    images: HashMap<u32, GpuImage>,
    sampler: wgpu::Sampler,
    bind_group_layout: wgpu::BindGroupLayout,
}

impl ImageGpuCache {
    pub fn new(device: &wgpu::Device) -> Self {
        let sampler = device.create_sampler(&wgpu::SamplerDescriptor {
            label: Some("Image Sampler"),
            address_mode_u: wgpu::AddressMode::ClampToEdge,
            address_mode_v: wgpu::AddressMode::ClampToEdge,
            address_mode_w: wgpu::AddressMode::ClampToEdge,
            mag_filter: wgpu::FilterMode::Linear,
            min_filter: wgpu::FilterMode::Linear,
            mipmap_filter: wgpu::MipmapFilterMode::Linear,
            ..Default::default()
        });

        let bind_group_layout = device.create_bind_group_layout(&wgpu::BindGroupLayoutDescriptor {
            label: Some("Image BGL"),
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

        Self {
            images: HashMap::new(),
            sampler,
            bind_group_layout,
        }
    }

    pub fn bind_group_layout(&self) -> &wgpu::BindGroupLayout {
        &self.bind_group_layout
    }

    pub fn upload(
        &mut self,
        device: &wgpu::Device,
        queue: &wgpu::Queue,
        handle: ImageHandle,
        data: &ImageData,
    ) {
        let same_size = self
            .images
            .get(&handle.0)
            .map(|img| {
                let s = img.texture.size();
                s.width == data.width && s.height == data.height
            })
            .unwrap_or(false);

        if same_size {
            // Перезапись содержимого обязана обновить и мипы: сэмплер
            // трилинейный, устаревшие уровни всплыли бы при минификации.
            let img = self.images.get(&handle.0).expect("checked above");
            write_all_levels(queue, &img.texture, data);
            return;
        }

        // Полная цепочка мипов: UI рисует картинки сильно меньше натурала
        // (SVG-логотип растеризуется в 512, а плитка в рейле — ~30 px), и
        // один уровень под трилинейным сэмплером давал рваные края.
        let texture = device.create_texture(&wgpu::TextureDescriptor {
            label: Some("Image Texture"),
            size: wgpu::Extent3d {
                width: data.width,
                height: data.height,
                depth_or_array_layers: 1,
            },
            mip_level_count: mip_level_count(data.width, data.height),
            sample_count: 1,
            dimension: wgpu::TextureDimension::D2,
            format: wgpu::TextureFormat::Rgba8UnormSrgb,
            usage: wgpu::TextureUsages::TEXTURE_BINDING | wgpu::TextureUsages::COPY_DST,
            view_formats: &[],
        });

        write_all_levels(queue, &texture, data);

        let texture_view = texture.create_view(&wgpu::TextureViewDescriptor::default());

        let bind_group = device.create_bind_group(&wgpu::BindGroupDescriptor {
            label: Some("Image BG"),
            layout: &self.bind_group_layout,
            entries: &[
                wgpu::BindGroupEntry {
                    binding: 0,
                    resource: wgpu::BindingResource::TextureView(&texture_view),
                },
                wgpu::BindGroupEntry {
                    binding: 1,
                    resource: wgpu::BindingResource::Sampler(&self.sampler),
                },
            ],
        });

        self.images.insert(
            handle.0,
            GpuImage {
                texture,
                bind_group,
            },
        );
    }

    pub fn process_uploads(
        &mut self,
        device: &wgpu::Device,
        queue: &wgpu::Queue,
        store: &mut ImageStore,
    ) {
        store.poll_bg();
        let uploads = store.take_pending_uploads();
        for (handle, data) in &uploads {
            self.upload(device, queue, *handle, data);
        }
    }

    pub fn get_bind_group(&self, handle_id: u32) -> Option<&wgpu::BindGroup> {
        self.images.get(&handle_id).map(|img| &img.bind_group)
    }
}

#[cfg(test)]
mod mip_tests {
    use super::{downscale_half, mip_level_count};

    #[test]
    fn level_count_reaches_one_by_one() {
        assert_eq!(mip_level_count(512, 512), 10);
        assert_eq!(mip_level_count(512, 32), 10);
        assert_eq!(mip_level_count(1, 1), 1);
        assert_eq!(mip_level_count(3, 3), 2);
    }

    /// Прозрачные текселы не тянут чёрный в цвет соседей: RGB усредняется с
    /// весом по альфе. Без этого на краях иконки над прозрачным фоном
    /// появлялась тёмная кайма.
    #[test]
    fn transparent_neighbours_do_not_darken_edges() {
        // 2×2: один красный непрозрачный + три полностью прозрачных чёрных.
        let src = [
            255, 0, 0, 255, /**/ 0, 0, 0, 0,
            0, 0, 0, 0, /*   */ 0, 0, 0, 0,
        ];
        let (w, h, out) = downscale_half(2, 2, &src);
        assert_eq!((w, h), (1, 1));
        assert_eq!(out[0], 255, "красный не должен темнеть: {out:?}");
        assert_eq!(out[3], 64, "альфа — среднее по четырём: {out:?}");
    }

    #[test]
    fn odd_sizes_clamp_to_last_row() {
        let src = vec![10u8; 3 * 1 * 4];
        let (w, h, out) = downscale_half(3, 1, &src);
        assert_eq!((w, h), (1, 1));
        assert_eq!(out.len(), 4);
    }
}
