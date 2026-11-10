use super::image_store::{ImageData, ImageHandle, ImageStore, YuvFrame, YuvLayout};
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
/// sRGB-байт → линейный свет в 12-битном целом (0..=4095): для непрозрачных
/// блоков среднее считается без float и деления.
fn srgb_to_linear_q12(v: u8) -> u16 {
    static LUT: std::sync::OnceLock<[u16; 256]> = std::sync::OnceLock::new();
    LUT.get_or_init(|| std::array::from_fn(|i| (srgb_to_linear(i as u8) * 4095.0).round() as u16))
        [v as usize]
}

fn linear_q12_to_srgb(x: u16) -> u8 {
    static LUT: std::sync::OnceLock<[u8; 4096]> = std::sync::OnceLock::new();
    LUT.get_or_init(|| std::array::from_fn(|i| linear_to_srgb(i as f32 / 4095.0)))
        [(x as usize).min(4095)]
}

/// Вся цепочка мипов 1..n для картинки `w×h` (уровень 0 — сама картинка).
pub fn build_mips(w: u32, h: u32, rgba: &[u8]) -> Vec<super::image_store::MipLevel> {
    let levels = mip_level_count(w, h);
    let mut out = Vec::with_capacity(levels.saturating_sub(1) as usize);
    let (mut cw, mut ch) = (w, h);
    let mut cur: std::borrow::Cow<[u8]> = std::borrow::Cow::Borrowed(rgba);
    for _ in 1..levels {
        let (nw, nh, next) = downscale_half(cw, ch, &cur);
        let arc: std::sync::Arc<[u8]> = std::sync::Arc::from(next.into_boxed_slice());
        out.push(super::image_store::MipLevel {
            width: nw,
            height: nh,
            rgba: arc.clone(),
        });
        cw = nw;
        ch = nh;
        cur = std::borrow::Cow::Owned(arc.to_vec());
    }
    out
}

pub fn downscale_half(w: u32, h: u32, src: &[u8]) -> (u32, u32, Vec<u8>) {
    let dw = (w / 2).max(1);
    let dh = (h / 2).max(1);
    let mut dst = vec![0u8; (dw * dh * 4) as usize];
    for dy in 0..dh {
        for dx in 0..dw {
            let sx0 = (dx * 2).min(w - 1);
            let sy0 = (dy * 2).min(h - 1);
            let sx1 = (sx0 + 1).min(w - 1);
            let sy1 = (sy0 + 1).min(h - 1);
            // Быстрый путь: все четыре текселя непрозрачны (фото, постеры) —
            // целочисленное среднее в линейном свете без деления на альфу.
            let i00 = ((sy0 * w + sx0) * 4) as usize;
            let i10 = ((sy0 * w + sx1) * 4) as usize;
            let i01 = ((sy1 * w + sx0) * 4) as usize;
            let i11 = ((sy1 * w + sx1) * 4) as usize;
            if src[i00 + 3] == 255
                && src[i10 + 3] == 255
                && src[i01 + 3] == 255
                && src[i11 + 3] == 255
            {
                let o = ((dy * dw + dx) * 4) as usize;
                for c in 0..3 {
                    let sum = srgb_to_linear_q12(src[i00 + c]) as u32
                        + srgb_to_linear_q12(src[i10 + c]) as u32
                        + srgb_to_linear_q12(src[i01 + c]) as u32
                        + srgb_to_linear_q12(src[i11 + c]) as u32;
                    dst[o + c] = linear_q12_to_srgb(((sum + 2) >> 2) as u16);
                }
                dst[o + 3] = 255;
                continue;
            }
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
    // Буфер может быть длиннее кадра (в кадрах видео за последней строкой
    // остаётся запас под SIMD-хвост swscale) — берём ровно `w×h`.
    let needed = (4 * w as usize) * h as usize;
    let Some(rgba) = rgba.get(..needed) else {
        log::warn!("image: буфер {} байт меньше кадра {w}×{h}", rgba.len());
        return;
    };
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

/// Места частичной записи, обрезанные по кадру; `None` — выгоднее целиком
/// (мест много или они покрывают больше половины кадра).
fn fit_regions(rects: Vec<[u32; 4]>, w: u32, h: u32) -> Option<Vec<[u32; 4]>> {
    let mut out: Vec<[u32; 4]> = rects
        .into_iter()
        .filter_map(|[x, y, rw, rh]| {
            let (x1, y1) = ((x + rw).min(w), (y + rh).min(h));
            (x < x1 && y < y1).then_some([x, y, x1 - x, y1 - y])
        })
        .collect();
    if out.len() > 32 {
        // Много мелких мест — одной рамкой вокруг всех.
        let (x0, y0) = (
            out.iter().map(|r| r[0]).min()?,
            out.iter().map(|r| r[1]).min()?,
        );
        let (x1, y1) = (
            out.iter().map(|r| r[0] + r[2]).max()?,
            out.iter().map(|r| r[1] + r[3]).max()?,
        );
        out = vec![[x0, y0, x1 - x0, y1 - y0]];
    }
    let area: u64 = out.iter().map(|r| r[2] as u64 * r[3] as u64).sum();
    (area * 2 <= w as u64 * h as u64).then_some(out)
}

/// Записывает место `[x, y, w, h]` кадра шириной `fw` в нулевой уровень.
fn write_region(
    queue: &wgpu::Queue,
    texture: &wgpu::Texture,
    fw: u32,
    rgba: &[u8],
    [x, y, w, h]: [u32; 4],
) {
    let start = ((y as usize * fw as usize) + x as usize) * 4;
    let end = ((y + h - 1) as usize * fw as usize + (x + w) as usize) * 4;
    let Some(src) = rgba.get(start..end) else {
        return;
    };
    queue.write_texture(
        wgpu::TexelCopyTextureInfo {
            texture,
            mip_level: 0,
            origin: wgpu::Origin3d { x, y, z: 0 },
            aspect: wgpu::TextureAspect::All,
        },
        src,
        wgpu::TexelCopyBufferLayout {
            offset: 0,
            bytes_per_row: Some(4 * fw),
            rows_per_image: Some(h),
        },
        wgpu::Extent3d {
            width: w,
            height: h,
            depth_or_array_layers: 1,
        },
    );
}

/// Сколько уровней нужно текстуре под эти данные.
fn levels_for(data: &ImageData) -> u32 {
    if data.single_level {
        1
    } else {
        mip_level_count(data.width, data.height)
    }
}

/// Заливает нулевой уровень и мипы: готовые (`data.mips`, посчитаны в потоке
/// декодирования), а недостающие достраивает CPU-боксом.
fn write_all_levels(queue: &wgpu::Queue, texture: &wgpu::Texture, data: &ImageData) {
    write_level(queue, texture, 0, data.width, data.height, &data.rgba);
    let levels = levels_for(data).min(texture.mip_level_count());
    let mut level = 1u32;
    for mip in &data.mips {
        if level >= levels {
            break;
        }
        write_level(queue, texture, level, mip.width, mip.height, &mip.rgba);
        level += 1;
    }
    if level >= levels {
        return;
    }
    let (mut w, mut h, mut cur): (u32, u32, Vec<u8>) = match data.mips.last() {
        Some(m) => (m.width, m.height, m.rgba.to_vec()),
        None => (data.width, data.height, data.rgba.to_vec()),
    };
    while level < levels {
        let (nw, nh, next) = downscale_half(w, h, &cur);
        write_level(queue, texture, level, nw, nh, &next);
        w = nw;
        h = nh;
        cur = next;
        level += 1;
    }
}

/// Та же картинка вдвое меньше: готовый первый мип или CPU-бокс.
/// `None` — меньше некуда.
fn smaller(d: &ImageData) -> Option<ImageData> {
    if d.width.max(d.height) <= 1 {
        return None;
    }
    let mut out = d.clone();
    match d.mips.first() {
        Some(m) => {
            out.width = m.width;
            out.height = m.height;
            out.rgba = m.rgba.clone();
            out.mips = d.mips[1..].to_vec();
        }
        None => {
            let (w, h, px) = downscale_half(d.width, d.height, &d.rgba);
            out.width = w;
            out.height = h;
            out.rgba = px.into();
        }
    }
    out.natural = d.natural.or(Some((d.width, d.height)));
    out.regions = None;
    Some(out)
}

/// Сколько байт картинок заливать в GPU за кадр (но не меньше двух картинок).
const UPLOAD_BYTES_PER_FRAME: usize = 4 << 20;

struct GpuImage {
    /// Для статичных картинок — одна текстура. Для потоковых кадров
    /// (`ImageData::single_level`) — кольцо из [`STREAM_RING`] текстур: запись
    /// идёт в ту, которую GPU уже не читает. Иначе драйвер (Mali) делает
    /// copy-on-write всей текстуры на каждый кадр, и второй поток
    /// mali-utility-worker съедает ядро.
    ring: Vec<(wgpu::Texture, wgpu::BindGroup)>,
    cur: usize,
    /// Места, изменённые последними заливками (`None` — кадр целиком), —
    /// новейшие в конце: текстура кольца отстала на столько кадров, и
    /// частичная запись в неё обязана догнать их все.
    recent: std::collections::VecDeque<Option<Vec<[u32; 4]>>>,
    /// Текстуры кольца годятся как цель отрисовки (кадры из внешних изображений).
    rt: bool,
}

impl GpuImage {
    fn texture(&self) -> &wgpu::Texture {
        &self.ring[self.cur].0
    }
    fn bind_group(&self) -> &wgpu::BindGroup {
        &self.ring[self.cur].1
    }
}

const STREAM_RING: usize = 3;

/// Текстуры кадра YUV: яркость (R8), цветность (NV12 — RG8, I420 — две R8)
/// и униформа с матрицей; цвет переводит `yuv.wgsl`.
struct YuvSlot {
    y: wgpu::Texture,
    c1: wgpu::Texture,
    c2: Option<wgpu::Texture>,
    uniform: wgpu::Buffer,
    bind_group: wgpu::BindGroup,
}

struct GpuYuv {
    ring: Vec<YuvSlot>,
    cur: usize,
    width: u32,
    height: u32,
    layout: YuvLayout,
    /// Кадры из dma-buf: текстуры, импортированные по буферу декодера (`YuvDmaBuf::id`).
    imported: HashMap<u64, YuvSlot>,
    /// Показывается импортированный буфер (иначе — `ring[cur]`).
    cur_import: Option<u64>,
    /// Последние показанные кадры из dma-buf: GPU может ещё читать их буферы —
    /// декодеру они вернутся, только когда выйдут отсюда.
    held: std::collections::VecDeque<std::sync::Arc<YuvFrame>>,
}

/// Импорт внешних изображений и проход, переписывающий их в текстуры картинок.
#[cfg(all(target_os = "linux", not(target_arch = "wasm32")))]
#[derive(Default)]
struct External {
    /// Импортированные изображения (`ExternalImage::id`) → текстура и bind group прохода.
    imports: HashMap<u64, (wgpu::Texture, wgpu::BindGroup)>,
    /// Порядок импорта — старые выбрасываются сверх [`EXTERNAL_IMPORTS`].
    order: std::collections::VecDeque<u64>,
    pass: Option<(wgpu::RenderPipeline, wgpu::BindGroupLayout)>,
}

/// Сколько импортированных внешних изображений помнить (кольца нескольких источников).
#[cfg(all(target_os = "linux", not(target_arch = "wasm32")))]
const EXTERNAL_IMPORTS: usize = 16;

/// Проход: тексел внешнего изображения в текстуру картинки 1:1, непрозрачным
/// (источник может не заполнять альфу — XRGB).
#[cfg(all(target_os = "linux", not(target_arch = "wasm32")))]
const EXTERNAL_WGSL: &str = r#"
@group(0) @binding(0) var src: texture_2d<f32>;
@vertex fn vs(@builtin(vertex_index) i: u32) -> @builtin(position) vec4<f32> {
    let uv = vec2<f32>(f32((i << 1u) & 2u), f32(i & 2u));
    return vec4<f32>(uv * vec2<f32>(2.0, -2.0) + vec2<f32>(-1.0, 1.0), 0.0, 1.0);
}
@fragment fn fs(@builtin(position) p: vec4<f32>) -> @location(0) vec4<f32> {
    return vec4<f32>(textureLoad(src, vec2<i32>(p.xy), 0).rgb, 1.0);
}
"#;

pub struct ImageGpuCache {
    #[cfg(all(target_os = "linux", not(target_arch = "wasm32")))]
    external: External,
    images: HashMap<u32, GpuImage>,
    yuv: HashMap<u32, GpuYuv>,
    sampler: wgpu::Sampler,
    bind_group_layout: wgpu::BindGroupLayout,
    yuv_layout: wgpu::BindGroupLayout,
}

fn write_plane(
    queue: &wgpu::Queue,
    texture: &wgpu::Texture,
    bpp: u32,
    w: u32,
    h: u32,
    data: &[u8],
) {
    queue.write_texture(
        wgpu::TexelCopyTextureInfo {
            texture,
            mip_level: 0,
            origin: wgpu::Origin3d::ZERO,
            aspect: wgpu::TextureAspect::All,
        },
        data,
        wgpu::TexelCopyBufferLayout {
            offset: 0,
            bytes_per_row: Some(bpp * w),
            rows_per_image: Some(h),
        },
        wgpu::Extent3d {
            width: w,
            height: h,
            depth_or_array_layers: 1,
        },
    );
}

fn plane_texture(
    device: &wgpu::Device,
    format: wgpu::TextureFormat,
    w: u32,
    h: u32,
) -> wgpu::Texture {
    device.create_texture(&wgpu::TextureDescriptor {
        label: Some("YUV Plane"),
        size: wgpu::Extent3d {
            width: w,
            height: h,
            depth_or_array_layers: 1,
        },
        mip_level_count: 1,
        sample_count: 1,
        dimension: wgpu::TextureDimension::D2,
        format,
        usage: wgpu::TextureUsages::TEXTURE_BINDING | wgpu::TextureUsages::COPY_DST,
        view_formats: &[],
    })
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

        let tex = |binding| wgpu::BindGroupLayoutEntry {
            binding,
            visibility: wgpu::ShaderStages::FRAGMENT,
            ty: wgpu::BindingType::Texture {
                sample_type: wgpu::TextureSampleType::Float { filterable: true },
                view_dimension: wgpu::TextureViewDimension::D2,
                multisampled: false,
            },
            count: None,
        };
        let yuv_layout = device.create_bind_group_layout(&wgpu::BindGroupLayoutDescriptor {
            label: Some("YUV BGL"),
            entries: &[
                tex(0),
                tex(1),
                tex(2),
                wgpu::BindGroupLayoutEntry {
                    binding: 3,
                    visibility: wgpu::ShaderStages::FRAGMENT,
                    ty: wgpu::BindingType::Sampler(wgpu::SamplerBindingType::Filtering),
                    count: None,
                },
                wgpu::BindGroupLayoutEntry {
                    binding: 4,
                    visibility: wgpu::ShaderStages::FRAGMENT,
                    ty: wgpu::BindingType::Buffer {
                        ty: wgpu::BufferBindingType::Uniform,
                        has_dynamic_offset: false,
                        min_binding_size: None,
                    },
                    count: None,
                },
            ],
        });

        #[cfg(all(target_os = "linux", not(target_arch = "wasm32")))]
        super::dmabuf::detect(device);
        #[cfg(all(target_os = "linux", not(target_arch = "wasm32")))]
        super::external::detect(device);
        Self {
            #[cfg(all(target_os = "linux", not(target_arch = "wasm32")))]
            external: External::default(),
            images: HashMap::new(),
            yuv: HashMap::new(),
            sampler,
            bind_group_layout,
            yuv_layout,
        }
    }

    pub fn yuv_bind_group_layout(&self) -> &wgpu::BindGroupLayout {
        &self.yuv_layout
    }

    /// Bind group кадра YUV (`yuv.wgsl`), если у handle сейчас кадр в YUV.
    pub fn get_yuv_bind_group(&self, handle_id: u32) -> Option<&wgpu::BindGroup> {
        self.yuv
            .get(&handle_id)
            .map(|y| match y.cur_import.and_then(|id| y.imported.get(&id)) {
                Some(s) => &s.bind_group,
                None => &y.ring[y.cur].bind_group,
            })
    }

    fn yuv_bind_group(
        &self,
        device: &wgpu::Device,
        y: &wgpu::Texture,
        c1: &wgpu::Texture,
        c2: Option<&wgpu::Texture>,
        uniform: &wgpu::Buffer,
    ) -> wgpu::BindGroup {
        let yv = y.create_view(&Default::default());
        let c1v = c1.create_view(&Default::default());
        // NV12: вторая плоскость не нужна шейдеру, но слот занят — та же UV.
        let c2v = c2.unwrap_or(c1).create_view(&Default::default());
        device.create_bind_group(&wgpu::BindGroupDescriptor {
            label: Some("YUV BG"),
            layout: &self.yuv_layout,
            entries: &[
                wgpu::BindGroupEntry {
                    binding: 0,
                    resource: wgpu::BindingResource::TextureView(&yv),
                },
                wgpu::BindGroupEntry {
                    binding: 1,
                    resource: wgpu::BindingResource::TextureView(&c1v),
                },
                wgpu::BindGroupEntry {
                    binding: 2,
                    resource: wgpu::BindingResource::TextureView(&c2v),
                },
                wgpu::BindGroupEntry {
                    binding: 3,
                    resource: wgpu::BindingResource::Sampler(&self.sampler),
                },
                wgpu::BindGroupEntry {
                    binding: 4,
                    resource: uniform.as_entire_binding(),
                },
            ],
        })
    }

    /// Кадр NV12 из dma-buf: плоскости импортируются (один раз на буфер декодера)
    /// и показываются без копии. `false` — импорт не удался, кадр надо копировать.
    #[cfg(all(target_os = "linux", not(target_arch = "wasm32")))]
    fn show_dmabuf(
        &mut self,
        device: &wgpu::Device,
        queue: &wgpu::Queue,
        handle: ImageHandle,
        f: &std::sync::Arc<YuvFrame>,
    ) -> bool {
        use super::dmabuf::{import_plane, FOURCC_GR88, FOURCC_R8};
        let Some(d) = f.dmabuf.as_ref() else {
            return false;
        };
        if !super::dmabuf::import_supported() {
            return false;
        }
        self.images.remove(&handle.0);
        let fits = self
            .yuv
            .get(&handle.0)
            .is_some_and(|g| g.width == f.width && g.height == f.height && g.layout == f.layout);
        if !fits {
            self.yuv.remove(&handle.0);
        }
        let need = !self
            .yuv
            .get(&handle.0)
            .is_some_and(|g| g.imported.contains_key(&d.id));
        if need {
            let (cw, ch) = f.chroma_size();
            let y = import_plane(
                device,
                d.fd,
                d.y_offset,
                d.y_pitch,
                f.width,
                f.height,
                FOURCC_R8,
                wgpu::TextureFormat::R8Unorm,
            );
            let uv = import_plane(
                device,
                d.fd,
                d.uv_offset,
                d.uv_pitch,
                cw,
                ch,
                FOURCC_GR88,
                wgpu::TextureFormat::Rg8Unorm,
            );
            let (Some(y), Some(uv)) = (y, uv) else {
                return false;
            };
            let uniform = device.create_buffer(&wgpu::BufferDescriptor {
                label: Some("YUV Uniform"),
                size: (super::image_store::YUV_UNIFORM_FLOATS * 4) as u64,
                usage: wgpu::BufferUsages::UNIFORM | wgpu::BufferUsages::COPY_DST,
                mapped_at_creation: false,
            });
            let bind_group = self.yuv_bind_group(device, &y, &uv, None, &uniform);
            let g = self.yuv.entry(handle.0).or_insert_with(|| GpuYuv {
                ring: Vec::new(),
                cur: 0,
                width: f.width,
                height: f.height,
                layout: f.layout,
                imported: HashMap::new(),
                cur_import: None,
                held: Default::default(),
            });
            // Буферы прошлых декодеров (перемотка пересоздаёт декодер) — не копить.
            if g.imported.len() >= 32 {
                let keep = g.cur_import;
                g.imported.retain(|id, _| Some(*id) == keep);
            }
            g.imported.insert(
                d.id,
                YuvSlot {
                    y,
                    c1: uv,
                    c2: None,
                    uniform,
                    bind_group,
                },
            );
        }
        let g = self.yuv.get_mut(&handle.0).expect("создано выше");
        let slot = &g.imported[&d.id];
        let u = f.shader_uniform();
        let bytes: Vec<u8> = u.iter().flat_map(|v| v.to_le_bytes()).collect();
        queue.write_buffer(&slot.uniform, 0, &bytes);
        g.cur_import = Some(d.id);
        g.held.push_back(f.clone());
        while g.held.len() > 3 {
            g.held.pop_front();
        }
        true
    }

    fn upload_yuv(
        &mut self,
        device: &wgpu::Device,
        queue: &wgpu::Queue,
        handle: ImageHandle,
        f: &std::sync::Arc<YuvFrame>,
    ) {
        #[cfg(all(target_os = "linux", not(target_arch = "wasm32")))]
        if f.dmabuf.is_some() {
            if self.show_dmabuf(device, queue, handle, f) {
                return;
            }
            // GPU не импортирует: та же картинка, скопированная в память.
            let d = f.dmabuf.as_ref().expect("проверено");
            let copy = YuvFrame {
                data: (d.copy)(),
                dmabuf: None,
                ..(**f).clone()
            };
            return self.upload_yuv(device, queue, handle, &std::sync::Arc::new(copy));
        }
        self.images.remove(&handle.0);
        let (cw, ch) = f.chroma_size();
        let fits = self.yuv.get(&handle.0).is_some_and(|g| {
            g.width == f.width && g.height == f.height && g.layout == f.layout && !g.ring.is_empty()
        });
        if !fits {
            let mut ring = Vec::with_capacity(STREAM_RING);
            for _ in 0..STREAM_RING {
                let y_fmt = if f.layout == YuvLayout::P010 {
                    wgpu::TextureFormat::R16Unorm
                } else {
                    wgpu::TextureFormat::R8Unorm
                };
                let y = plane_texture(device, y_fmt, f.width, f.height);
                let (c1, c2) = match f.layout {
                    YuvLayout::Nv12 => (
                        plane_texture(device, wgpu::TextureFormat::Rg8Unorm, cw, ch),
                        None,
                    ),
                    YuvLayout::P010 => (
                        plane_texture(device, wgpu::TextureFormat::Rg16Unorm, cw, ch),
                        None,
                    ),
                    YuvLayout::I420 => (
                        plane_texture(device, wgpu::TextureFormat::R8Unorm, cw, ch),
                        Some(plane_texture(device, wgpu::TextureFormat::R8Unorm, cw, ch)),
                    ),
                };
                let uniform = device.create_buffer(&wgpu::BufferDescriptor {
                    label: Some("YUV Uniform"),
                    size: (super::image_store::YUV_UNIFORM_FLOATS * 4) as u64,
                    usage: wgpu::BufferUsages::UNIFORM | wgpu::BufferUsages::COPY_DST,
                    mapped_at_creation: false,
                });
                let yv = y.create_view(&Default::default());
                let c1v = c1.create_view(&Default::default());
                // NV12: вторая плоскость не нужна шейдеру, но слот занят — та же UV.
                let c2v = c2.as_ref().unwrap_or(&c1).create_view(&Default::default());
                let bind_group = device.create_bind_group(&wgpu::BindGroupDescriptor {
                    label: Some("YUV BG"),
                    layout: &self.yuv_layout,
                    entries: &[
                        wgpu::BindGroupEntry {
                            binding: 0,
                            resource: wgpu::BindingResource::TextureView(&yv),
                        },
                        wgpu::BindGroupEntry {
                            binding: 1,
                            resource: wgpu::BindingResource::TextureView(&c1v),
                        },
                        wgpu::BindGroupEntry {
                            binding: 2,
                            resource: wgpu::BindingResource::TextureView(&c2v),
                        },
                        wgpu::BindGroupEntry {
                            binding: 3,
                            resource: wgpu::BindingResource::Sampler(&self.sampler),
                        },
                        wgpu::BindGroupEntry {
                            binding: 4,
                            resource: uniform.as_entire_binding(),
                        },
                    ],
                });
                ring.push(YuvSlot {
                    y,
                    c1,
                    c2,
                    uniform,
                    bind_group,
                });
            }
            self.yuv.insert(
                handle.0,
                GpuYuv {
                    ring,
                    cur: STREAM_RING - 1,
                    width: f.width,
                    height: f.height,
                    layout: f.layout,
                    imported: HashMap::new(),
                    cur_import: None,
                    held: Default::default(),
                },
            );
        }
        let g = self.yuv.get_mut(&handle.0).expect("создано выше");
        g.cur_import = None;
        g.held.clear();
        g.cur = (g.cur + 1) % g.ring.len();
        let slot = &g.ring[g.cur];
        let sb = f.layout.sample_bytes() as u32;
        write_plane(queue, &slot.y, sb, f.width, f.height, f.y_plane());
        let (c1, c2) = f.chroma_planes();
        match (&slot.c2, c2) {
            (Some(t2), Some(c2)) => {
                write_plane(queue, &slot.c1, sb, cw, ch, c1);
                write_plane(queue, t2, sb, cw, ch, c2);
            }
            _ => write_plane(queue, &slot.c1, 2 * sb, cw, ch, c1),
        }
        let u = f.shader_uniform();
        let bytes: Vec<u8> = u.iter().flat_map(|v| v.to_le_bytes()).collect();
        queue.write_buffer(&slot.uniform, 0, &bytes);
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
        if let Some(f) = &data.yuv {
            self.upload_yuv(device, queue, handle, f);
            return;
        }
        #[cfg(all(target_os = "linux", not(target_arch = "wasm32")))]
        if let Some(f) = &data.external {
            self.upload_external(device, queue, handle, f);
            return;
        }
        self.yuv.remove(&handle.0);
        let ring_len = if data.single_level { STREAM_RING } else { 1 };
        let same_size = self
            .images
            .get(&handle.0)
            .map(|img| {
                let s = img.texture().size();
                s.width == data.width
                    && s.height == data.height
                    && img.texture().mip_level_count() == levels_for(data)
                    && img.ring.len() == ring_len
            })
            .unwrap_or(false);

        if same_size {
            // Перезапись содержимого обязана обновить и мипы: сэмплер
            // трилинейный, устаревшие уровни всплыли бы при минификации.
            let img = self.images.get_mut(&handle.0).expect("checked above");
            img.cur = (img.cur + 1) % img.ring.len();
            // Частичная запись: эта текстура кольца видела кадр ring_len назад —
            // догоняем места всех кадров с тех пор.
            let lag = img.ring.len();
            let mut rects: Option<Vec<[u32; 4]>> = data
                .regions
                .clone()
                .filter(|_| data.single_level && img.recent.len() + 1 >= lag);
            for prev in img.recent.iter().rev().take(lag - 1) {
                rects = match (rects, prev) {
                    (Some(mut a), Some(b)) => {
                        a.extend_from_slice(b);
                        Some(a)
                    }
                    _ => None,
                };
            }
            match rects.and_then(|r| fit_regions(r, data.width, data.height)) {
                Some(r) => {
                    for rect in &r {
                        write_region(queue, img.texture(), data.width, &data.rgba, *rect);
                    }
                }
                None => write_all_levels(queue, img.texture(), data),
            }
            img.recent.push_back(data.regions.clone());
            while img.recent.len() > lag {
                img.recent.pop_front();
            }
            return;
        }

        // Фото с телефона (4592×8160 ≈ 200 МБ с мипами) может не влезть:
        // видеопамять занята нейросетями или сторона больше предела карты.
        // Тогда — уменьшенная вдвое копия, пока не влезет, а не паника.
        let limit = device.limits().max_texture_dimension_2d;
        let mut cur = std::borrow::Cow::Borrowed(data);
        loop {
            if cur.width.max(cur.height) <= limit {
                if let Some(ring) = self.create_ring(device, queue, &cur, ring_len) {
                    if cur.width != data.width {
                        log::warn!("картинка {}×{} загружена в GPU уменьшенной до {}×{}", data.width, data.height, cur.width, cur.height);
                    }
                    self.images.insert(
                        handle.0,
                        GpuImage {
                            ring,
                            cur: 0,
                            recent: std::collections::VecDeque::new(),
                            rt: false,
                        },
                    );
                    return;
                }
            }
            match smaller(&cur) {
                Some(next) => cur = std::borrow::Cow::Owned(next),
                None => {
                    log::error!("картинка {}×{} не поместилась в видеопамять", data.width, data.height);
                    self.images.remove(&handle.0);
                    return;
                }
            }
        }
    }

    /// Текстуры под картинку (кольцо из `ring_len`) с залитыми уровнями;
    /// `None` — видеопамяти не хватило.
    fn create_ring(
        &self,
        device: &wgpu::Device,
        queue: &wgpu::Queue,
        data: &ImageData,
        ring_len: usize,
    ) -> Option<Vec<(wgpu::Texture, wgpu::BindGroup)>> {
        #[cfg(not(target_arch = "wasm32"))]
        let scope = device.push_error_scope(wgpu::ErrorFilter::OutOfMemory);
        // Полная цепочка мипов: UI рисует картинки сильно меньше натурала
        // (SVG-логотип растеризуется в 512, а плитка в рейле — ~30 px), и
        // один уровень под трилинейным сэмплером давал рваные края.
        let mut ring = Vec::with_capacity(ring_len);
        for _ in 0..ring_len {
            let texture = device.create_texture(&wgpu::TextureDescriptor {
                label: Some("Image Texture"),
                size: wgpu::Extent3d {
                    width: data.width,
                    height: data.height,
                    depth_or_array_layers: 1,
                },
                mip_level_count: levels_for(data),
                sample_count: 1,
                dimension: wgpu::TextureDimension::D2,
                format: wgpu::TextureFormat::Rgba8UnormSrgb,
                usage: wgpu::TextureUsages::TEXTURE_BINDING | wgpu::TextureUsages::COPY_DST,
                view_formats: &[],
            });
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
            ring.push((texture, bind_group));
        }
        write_all_levels(queue, &ring[0].0, data);
        #[cfg(not(target_arch = "wasm32"))]
        if let Some(e) = pollster::block_on(scope.pop()) {
            log::warn!("картинка {}×{}: {e}", data.width, data.height);
            return None;
        }
        Some(ring)
    }

    /// Кадр из внешнего изображения: импорт (один раз на изображение), затем проход на GPU в
    /// текстуру кольца картинки. Источнику кадр возвращается, когда GPU закончил проход.
    #[cfg(all(target_os = "linux", not(target_arch = "wasm32")))]
    fn upload_external(
        &mut self,
        device: &wgpu::Device,
        queue: &wgpu::Queue,
        handle: ImageHandle,
        frame: &std::sync::Arc<super::external::ExternalFrame>,
    ) {
        let img = &frame.image;
        let (w, h) = (img.width, img.height);
        if w == 0 || h == 0 {
            return;
        }
        if self.external.pass.is_none() {
            let bgl = device.create_bind_group_layout(&wgpu::BindGroupLayoutDescriptor {
                label: Some("external BGL"),
                entries: &[wgpu::BindGroupLayoutEntry {
                    binding: 0,
                    visibility: wgpu::ShaderStages::FRAGMENT,
                    ty: wgpu::BindingType::Texture {
                        sample_type: wgpu::TextureSampleType::Float { filterable: false },
                        view_dimension: wgpu::TextureViewDimension::D2,
                        multisampled: false,
                    },
                    count: None,
                }],
            });
            let module = device.create_shader_module(wgpu::ShaderModuleDescriptor {
                label: Some("external"),
                source: wgpu::ShaderSource::Wgsl(EXTERNAL_WGSL.into()),
            });
            let layout = device.create_pipeline_layout(&wgpu::PipelineLayoutDescriptor {
                label: Some("external"),
                bind_group_layouts: &[&bgl],
                immediate_size: 0,
            });
            let pipeline = device.create_render_pipeline(&wgpu::RenderPipelineDescriptor {
                label: Some("external"),
                layout: Some(&layout),
                vertex: wgpu::VertexState {
                    module: &module,
                    entry_point: Some("vs"),
                    compilation_options: Default::default(),
                    buffers: &[],
                },
                fragment: Some(wgpu::FragmentState {
                    module: &module,
                    entry_point: Some("fs"),
                    compilation_options: Default::default(),
                    targets: &[Some(wgpu::ColorTargetState {
                        format: wgpu::TextureFormat::Rgba8UnormSrgb,
                        blend: None,
                        write_mask: wgpu::ColorWrites::ALL,
                    })],
                }),
                primitive: Default::default(),
                depth_stencil: None,
                multisample: Default::default(),
                multiview_mask: None,
                cache: None,
            });
            self.external.pass = Some((pipeline, bgl));
        }
        if !self.external.imports.contains_key(&img.id) {
            let Some(tex) = super::external::import(device, img) else {
                frame.fail();
                return;
            };
            let view_format = super::external::formats(img.format).map(|f| f.1);
            let view = tex.create_view(&wgpu::TextureViewDescriptor { format: view_format, ..Default::default() });
            let (_, bgl) = self.external.pass.as_ref().expect("создан выше");
            let bg = device.create_bind_group(&wgpu::BindGroupDescriptor {
                label: Some("external BG"),
                layout: bgl,
                entries: &[wgpu::BindGroupEntry { binding: 0, resource: wgpu::BindingResource::TextureView(&view) }],
            });
            self.external.imports.insert(img.id, (tex, bg));
            self.external.order.push_back(img.id);
            while self.external.order.len() > EXTERNAL_IMPORTS {
                if let Some(old) = self.external.order.pop_front() {
                    self.external.imports.remove(&old);
                }
            }
        }
        self.yuv.remove(&handle.0);
        let fits = self.images.get(&handle.0).is_some_and(|g| {
            let s = g.texture().size();
            g.rt && s.width == w && s.height == h && g.ring.len() == STREAM_RING
        });
        if !fits {
            let ring = (0..STREAM_RING)
                .map(|_| {
                    let texture = device.create_texture(&wgpu::TextureDescriptor {
                        label: Some("Image Texture (external)"),
                        size: wgpu::Extent3d { width: w, height: h, depth_or_array_layers: 1 },
                        mip_level_count: 1,
                        sample_count: 1,
                        dimension: wgpu::TextureDimension::D2,
                        format: wgpu::TextureFormat::Rgba8UnormSrgb,
                        usage: wgpu::TextureUsages::TEXTURE_BINDING | wgpu::TextureUsages::COPY_DST | wgpu::TextureUsages::RENDER_ATTACHMENT,
                        view_formats: &[],
                    });
                    let view = texture.create_view(&Default::default());
                    let bind_group = device.create_bind_group(&wgpu::BindGroupDescriptor {
                        label: Some("Image BG"),
                        layout: &self.bind_group_layout,
                        entries: &[
                            wgpu::BindGroupEntry { binding: 0, resource: wgpu::BindingResource::TextureView(&view) },
                            wgpu::BindGroupEntry { binding: 1, resource: wgpu::BindingResource::Sampler(&self.sampler) },
                        ],
                    });
                    (texture, bind_group)
                })
                .collect();
            self.images.insert(handle.0, GpuImage { ring, cur: 0, recent: Default::default(), rt: true });
        }
        let g = self.images.get_mut(&handle.0).expect("создано выше");
        g.cur = (g.cur + 1) % g.ring.len();
        // Вся текстура перезаписана — частичным заливкам догонять нечего.
        g.recent.clear();
        g.recent.push_back(None);
        let target = g.ring[g.cur].0.create_view(&Default::default());
        let (pipeline, _) = self.external.pass.as_ref().expect("создан выше");
        let (_, bg) = &self.external.imports[&img.id];
        let mut enc = device.create_command_encoder(&wgpu::CommandEncoderDescriptor { label: Some("external") });
        {
            let mut rp = enc.begin_render_pass(&wgpu::RenderPassDescriptor {
                label: Some("external"),
                color_attachments: &[Some(wgpu::RenderPassColorAttachment {
                    view: &target,
                    depth_slice: None,
                    resolve_target: None,
                    ops: wgpu::Operations { load: wgpu::LoadOp::Clear(wgpu::Color::BLACK), store: wgpu::StoreOp::Store },
                })],
                depth_stencil_attachment: None,
                timestamp_writes: None,
                occlusion_query_set: None,
                multiview_mask: None,
            });
            rp.set_pipeline(pipeline);
            rp.set_bind_group(0, bg, &[]);
            rp.draw(0..3, 0..1);
        }
        queue.submit(std::iter::once(enc.finish()));
        // Изображение источника свободно, когда GPU выполнил проход.
        let hold = frame.clone();
        queue.on_submitted_work_done(move || drop(hold));
    }

    pub fn process_uploads(
        &mut self,
        device: &wgpu::Device,
        queue: &wgpu::Queue,
        store: &mut ImageStore,
    ) {
        store.poll_bg();
        for handle in store.take_pending_frees() {
            self.images.remove(&handle.0);
            self.yuv.remove(&handle.0);
        }
        // Бюджет на кадр в байтах: одна загрузка большого постера с мипами —
        // единицы миллисекунд на слабом GPU, пачка из десятка — заметный
        // рывок; а значки по ~200 КБ идут десятками (по две штуки за кадр
        // меню приложений проявлялось секунду). Остаток ждёт следующего
        // кадра (`ImageStore::has_pending_uploads` держит цикл кадров живым).
        let uploads = store.take_pending_uploads_budget(UPLOAD_BYTES_PER_FRAME);
        for (handle, data) in &uploads {
            self.upload(device, queue, *handle, data);
        }
    }

    pub fn get_bind_group(&self, handle_id: u32) -> Option<&wgpu::BindGroup> {
        self.images.get(&handle_id).map(|img| img.bind_group())
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
            255, 0, 0, 255, /**/ 0, 0, 0, 0, 0, 0, 0, 0, /*   */ 0, 0, 0, 0,
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

#[cfg(test)]
mod region_tests {
    use super::fit_regions;

    #[test]
    fn regions_are_clipped_and_large_ones_go_whole() {
        assert_eq!(
            fit_regions(vec![[90, 90, 20, 20]], 100, 100),
            Some(vec![[90, 90, 10, 10]])
        );
        assert_eq!(fit_regions(vec![[0, 0, 100, 60]], 100, 100), None);
        let many: Vec<[u32; 4]> = (0..40).map(|i| [i, 0, 1, 1]).collect();
        assert_eq!(fit_regions(many, 100, 100), Some(vec![[0, 0, 40, 1]]));
    }
}
