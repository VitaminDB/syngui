//! HEIF/HEIC и AVIF в общем декодере картинок ([`super::image_store`]).
//!
//! Камеры телефонов (Samsung, iPhone) по умолчанию снимают в HEIC, а
//! image-крейт его не читает. Формат узнаётся по сигнатуре (`ftyp` с брендом
//! HEIF/AVIF), дальше:
//! - Linux/Windows/macOS — libheif (фича `heif`, крейт `libheif-rs`); без фичи
//!   декодер возвращает понятную ошибку, она уходит в лог;
//! - Android — системный `ImageDecoder` через JNI (HEIF с API 28, AVIF с 31),
//!   libheif не нужен.
//!
//! Поворот и отражение (`irot`/`imir`) применяют оба декодера; EXIF-ориентацию
//! у HEIF по спецификации не учитывают — её роль играют эти свойства.
//! 10-битные картинки сводятся к 8 битам, альфа — неумноженная, как у прочих
//! форматов в [`super::ImageData`].

/// Готовая картинка: RGBA8 построчно, альфа неумноженная.
pub(crate) struct Rgba {
    pub width: u32,
    pub height: u32,
    pub data: Vec<u8>,
}

/// Разновидность контейнера.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum Kind {
    /// HEIF/HEIC (HEVC внутри).
    Heif,
    /// AVIF (AV1 внутри).
    Avif,
}

impl Kind {
    pub fn name(self) -> &'static str {
        match self {
            Kind::Heif => "HEIF/HEIC",
            Kind::Avif => "AVIF",
        }
    }
}

/// Узнать HEIF/AVIF по первому боксу `ftyp`: основной бренд или любой из
/// совместимых. `mif1`/`msf1` без конкретного кодека считаются HEIF.
pub(crate) fn sniff(bytes: &[u8]) -> Option<Kind> {
    if bytes.len() < 16 || &bytes[4..8] != b"ftyp" {
        return None;
    }
    let size = u32::from_be_bytes(bytes[0..4].try_into().ok()?) as usize;
    let end = size.clamp(16, bytes.len().min(4096));
    // Основной бренд, версия, затем список совместимых.
    let brands = std::iter::once(&bytes[8..12]).chain(
        bytes[16..end].chunks_exact(4),
    );
    let mut generic = false;
    for b in brands {
        match b {
            b"avif" | b"avis" => return Some(Kind::Avif),
            b"heic" | b"heix" | b"hevc" | b"hevx" | b"heim" | b"heis" | b"hevm" | b"hevs" => {
                return Some(Kind::Heif)
            }
            b"mif1" | b"msf1" | b"mif2" => generic = true,
            _ => {}
        }
    }
    generic.then_some(Kind::Heif)
}

/// Декодировать HEIF/AVIF в RGBA8 с поворотом. `max_side` (0 — без предела)
/// — подсказка декодеру Android уменьшить картинку сразу при декодировании.
pub(crate) fn decode(bytes: &[u8], kind: Kind, max_side: u32) -> Result<Rgba, String> {
    #[cfg(target_os = "android")]
    {
        let (width, height, mut data, premultiplied) =
            crate::platform::android::decode_image(bytes, max_side)
                .map_err(|e| format!("{}: {e}", kind.name()))?;
        if data.len() != width as usize * height as usize * 4 {
            return Err(format!("{}: неверный размер буфера", kind.name()));
        }
        if premultiplied {
            unpremultiply(&mut data);
        }
        return Ok(Rgba { width, height, data });
    }
    #[cfg(all(feature = "heif", not(target_os = "android")))]
    {
        let _ = max_side;
        return decode_libheif(bytes).map_err(|e| format!("{}: {e}", kind.name()));
    }
    #[allow(unreachable_code)]
    {
        let _ = (bytes, max_side);
        Err(format!(
            "{} не поддерживается этой сборкой: включите фичу `heif` у syngui (нужна системная libheif)",
            kind.name()
        ))
    }
}

#[cfg(all(feature = "heif", not(target_os = "android")))]
fn libheif() -> &'static libheif_rs::LibHeif {
    static LIB: std::sync::OnceLock<libheif_rs::LibHeif> = std::sync::OnceLock::new();
    LIB.get_or_init(libheif_rs::LibHeif::new)
}

#[cfg(all(feature = "heif", not(target_os = "android")))]
fn decode_libheif(bytes: &[u8]) -> Result<Rgba, String> {
    use libheif_rs::{ColorSpace, HeifContext, RgbChroma};

    let ctx = HeifContext::read_from_bytes(bytes).map_err(|e| e.to_string())?;
    let handle = ctx.primary_image_handle().map_err(|e| e.to_string())?;
    let deep = handle.luma_bits_per_pixel() > 8;
    let chroma = if deep { RgbChroma::HdrRgbaLe } else { RgbChroma::Rgba };
    // decode() применяет irot/imir/clap.
    let img = libheif()
        .decode(&handle, ColorSpace::Rgb(chroma), None)
        .map_err(|e| e.to_string())?;
    let (w, h) = (img.width(), img.height());
    let planes = img.planes();
    let plane = planes.interleaved.ok_or("нет RGBA-плоскости")?;
    let row = w as usize * 4;
    let mut data = Vec::with_capacity(row * h as usize);
    if deep {
        let bits = plane.bits_per_pixel.clamp(9, 16) as u32;
        let max = (1u32 << bits) - 1;
        for y in 0..h as usize {
            let src = &plane.data[y * plane.stride..][..row * 2];
            data.extend(src.chunks_exact(2).map(|v| {
                let v = (u16::from_le_bytes([v[0], v[1]]) as u32).min(max);
                ((v * 255 + max / 2) / max) as u8
            }));
        }
    } else {
        for y in 0..h as usize {
            data.extend_from_slice(&plane.data[y * plane.stride..][..row]);
        }
    }
    if img.is_premultiplied_alpha() {
        unpremultiply(&mut data);
    }
    Ok(Rgba { width: w, height: h, data })
}

/// Перевести RGBA с умноженной альфой в неумноженную.
#[allow(dead_code)]
pub(crate) fn unpremultiply(rgba: &mut [u8]) {
    for px in rgba.chunks_exact_mut(4) {
        let a = px[3] as u32;
        if a > 0 && a < 255 {
            for c in &mut px[..3] {
                *c = ((*c as u32 * 255 + a / 2) / a).min(255) as u8;
            }
        }
    }
}

/// Размер основной картинки по заголовку (свойство `ispe` с учётом `irot`),
/// без декодирования и без libheif — такой, каким её покажет декодер.
pub(crate) fn header_size(bytes: &[u8]) -> Option<(u32, u32)> {
    let meta = find_box(bytes, b"meta")?;
    let meta = meta.get(4..)?; // FullBox: версия + флаги
    let pitm = find_box(meta, b"pitm")?;
    let primary = if pitm[0] == 0 {
        u16::from_be_bytes(pitm.get(4..6)?.try_into().ok()?) as u32
    } else {
        u32::from_be_bytes(pitm.get(4..8)?.try_into().ok()?)
    };
    let iprp = find_box(meta, b"iprp")?;
    let ipco = find_box(iprp, b"ipco")?;
    let props: Vec<(&[u8; 4], &[u8])> = boxes(ipco).collect();
    let ipma = find_box(iprp, b"ipma")?;
    let (version, flags) = (ipma[0], ipma[3]);
    let count = u32::from_be_bytes(ipma.get(4..8)?.try_into().ok()?);
    let mut p = 8usize;
    let (mut size, mut crop, mut turned) = (None, None, false);
    for _ in 0..count {
        let id = if version < 1 {
            p += 2;
            u16::from_be_bytes(ipma.get(p - 2..p)?.try_into().ok()?) as u32
        } else {
            p += 4;
            u32::from_be_bytes(ipma.get(p - 4..p)?.try_into().ok()?)
        };
        let n = *ipma.get(p)? as usize;
        p += 1;
        for _ in 0..n {
            let index = if flags & 1 != 0 {
                p += 2;
                (u16::from_be_bytes(ipma.get(p - 2..p)?.try_into().ok()?) & 0x7fff) as usize
            } else {
                p += 1;
                (ipma[p - 1] & 0x7f) as usize
            };
            if id != primary || index == 0 {
                continue;
            }
            match props.get(index - 1) {
                Some((b"ispe", body)) => {
                    let w = u32::from_be_bytes(body.get(4..8)?.try_into().ok()?);
                    let h = u32::from_be_bytes(body.get(8..12)?.try_into().ok()?);
                    size = Some((w, h));
                }
                // Обрезка (кодеки дополняют до кратного блока: 64×48 → 64×64).
                Some((b"clap", body)) => {
                    let r = |i: usize| -> Option<f64> {
                        Some(u32::from_be_bytes(body.get(i..i + 4)?.try_into().ok()?) as f64)
                    };
                    let (wn, wd, hn, hd) = (r(0)?, r(4)?, r(8)?, r(12)?);
                    if wd > 0.0 && hd > 0.0 {
                        crop = Some(((wn / wd).round() as u32, (hn / hd).round() as u32));
                    }
                }
                Some((b"irot", body)) => turned = body.first()? & 1 == 1,
                _ => {}
            }
        }
    }
    let (w, h) = crop.or(size)?;
    Some(if turned { (h, w) } else { (w, h) })
}

/// Дочерние боксы ISOBMFF: (тип, содержимое без заголовка).
fn boxes(mut data: &[u8]) -> impl Iterator<Item = (&[u8; 4], &[u8])> {
    std::iter::from_fn(move || {
        if data.len() < 8 {
            return None;
        }
        let size32 = u32::from_be_bytes(data[0..4].try_into().ok()?) as u64;
        let kind: &[u8; 4] = data[4..8].try_into().ok()?;
        let (header, size) = match size32 {
            0 => (8, data.len() as u64),
            1 => (16, u64::from_be_bytes(data.get(8..16)?.try_into().ok()?)),
            s => (8, s),
        };
        if size < header as u64 || size > data.len() as u64 {
            return None;
        }
        let body = &data[header..size as usize];
        data = &data[size as usize..];
        Some((kind, body))
    })
}

fn find_box<'a>(data: &'a [u8], kind: &[u8; 4]) -> Option<&'a [u8]> {
    boxes(data).find(|(k, _)| *k == kind).map(|(_, b)| b)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn ftyp(major: &[u8; 4], compat: &[&[u8; 4]]) -> Vec<u8> {
        let mut v = Vec::new();
        v.extend_from_slice(&((16 + 4 * compat.len()) as u32).to_be_bytes());
        v.extend_from_slice(b"ftyp");
        v.extend_from_slice(major);
        v.extend_from_slice(&[0; 4]);
        for c in compat {
            v.extend_from_slice(*c);
        }
        v
    }

    #[test]
    fn sniff_brands() {
        assert_eq!(sniff(&ftyp(b"heic", &[b"mif1"])), Some(Kind::Heif));
        assert_eq!(sniff(&ftyp(b"mif1", &[b"heic"])), Some(Kind::Heif));
        assert_eq!(sniff(&ftyp(b"avif", &[b"mif1"])), Some(Kind::Avif));
        assert_eq!(sniff(&ftyp(b"mif1", &[b"miaf", b"avif"])), Some(Kind::Avif));
        assert_eq!(sniff(&ftyp(b"mif1", &[])), Some(Kind::Heif));
        // MP4/MOV — не картинка.
        assert_eq!(sniff(&ftyp(b"isom", &[b"mp41"])), None);
        assert_eq!(sniff(&ftyp(b"qt  ", &[])), None);
        assert_eq!(sniff(b"\x89PNG\r\n\x1a\n........"), None);
    }

    /// Без фичи `heif` (не Android) — ошибка с подсказкой, без паники.
    #[cfg(all(not(feature = "heif"), not(target_os = "android")))]
    #[test]
    fn without_feature_explains() {
        let bytes = ftyp(b"heic", &[b"mif1"]);
        let err = decode(&bytes, Kind::Heif, 0).err().unwrap();
        assert!(err.contains("`heif`"), "{err}");
    }
}
