//! Минимальная запись PDF: страницы заданного размера с картинками,
//! каждая — в своём прямоугольнике и обрезанная по своей рамке. Этого
//! хватает и для фото, и для страниц документа, отрисованных растром.
//!
//! Фото (много цветов) кладутся JPEG, графика и скриншоты — без потерь
//! (Flate). Прозрачность сводится на белый.

use std::io::Write;

use super::{PlacedImage, PrintImage};

/// Миллиметры → пункты PDF.
pub fn mm_to_pt(mm: f32) -> f32 {
    mm * 72.0 / 25.4
}

/// Страница: размер в мм и картинки на ней.
pub struct PdfPage<'a> {
    pub width_mm: f32,
    pub height_mm: f32,
    pub items: &'a [PlacedImage],
}

/// Поток картинки для PDF.
struct Encoded {
    width: u32,
    height: u32,
    gray: bool,
    filter: &'static str,
    data: Vec<u8>,
}

/// Много ли разных цветов (фото → JPEG). Смотрим выборку пикселей.
fn is_photo(img: &PrintImage) -> bool {
    let n = (img.width as usize) * (img.height as usize);
    if n == 0 {
        return false;
    }
    let step = (n / 4096).max(1);
    let mut seen = std::collections::HashSet::new();
    for i in (0..n).step_by(step) {
        let p = &img.rgba[i * 4..i * 4 + 4];
        seen.insert([p[0] >> 2, p[1] >> 2, p[2] >> 2]);
        if seen.len() > 512 {
            return true;
        }
    }
    false
}

/// RGBA → RGB (или серый) на белом.
fn flatten(img: &PrintImage, gray: bool) -> Vec<u8> {
    let n = (img.width as usize) * (img.height as usize);
    let mut out = Vec::with_capacity(n * if gray { 1 } else { 3 });
    for p in img.rgba.chunks_exact(4).take(n) {
        let a = p[3] as u32;
        let mix = |c: u8| ((c as u32 * a + 255 * (255 - a) + 127) / 255) as u8;
        let (r, g, b) = (mix(p[0]), mix(p[1]), mix(p[2]));
        if gray {
            out.push(luma(r, g, b));
        } else {
            out.extend_from_slice(&[r, g, b]);
        }
    }
    out
}

pub(crate) fn luma(r: u8, g: u8, b: u8) -> u8 {
    ((r as u32 * 299 + g as u32 * 587 + b as u32 * 114 + 500) / 1000) as u8
}

fn encode(img: &PrintImage, gray: bool) -> Result<Encoded, String> {
    let raw = flatten(img, gray);
    if is_photo(img) {
        let mut data = Vec::new();
        let color = if gray { image::ExtendedColorType::L8 } else { image::ExtendedColorType::Rgb8 };
        image::codecs::jpeg::JpegEncoder::new_with_quality(&mut data, 93)
            .encode(&raw, img.width, img.height, color)
            .map_err(|e| e.to_string())?;
        return Ok(Encoded { width: img.width, height: img.height, gray, filter: "DCTDecode", data });
    }
    let data = miniz_oxide::deflate::compress_to_vec_zlib(&raw, 6);
    Ok(Encoded { width: img.width, height: img.height, gray, filter: "FlateDecode", data })
}

/// Записать PDF. `gray` — всё в оттенках серого.
pub fn write(out: &mut impl Write, title: &str, pages: &[PdfPage], gray: bool) -> Result<(), String> {
    let mut buf: Vec<u8> = Vec::new();
    let mut offsets: Vec<usize> = Vec::new();
    // Номера объектов: 1 — каталог, 2 — дерево страниц, 3 — сведения, дальше по порядку.
    let mut next = 4usize;
    buf.extend_from_slice(b"%PDF-1.4\n%\xE2\xE3\xCF\xD3\n");
    let mut objects: Vec<(usize, Vec<u8>)> = Vec::new();
    let mut kids = Vec::new();
    for page in pages {
        let page_id = next;
        let content_id = next + 1;
        next += 2;
        kids.push(page_id);
        let (pw, ph) = (mm_to_pt(page.width_mm), mm_to_pt(page.height_mm));
        let mut content = String::new();
        let mut xobjects = String::new();
        for (i, item) in page.items.iter().enumerate() {
            let enc = encode(&item.image, gray)?;
            let img_id = next;
            next += 1;
            let mut obj = format!(
                "<< /Type /XObject /Subtype /Image /Width {} /Height {} /ColorSpace /{} /BitsPerComponent 8 /Filter /{} /Length {} >>\nstream\n",
                enc.width,
                enc.height,
                if enc.gray { "DeviceGray" } else { "DeviceRGB" },
                enc.filter,
                enc.data.len()
            )
            .into_bytes();
            obj.extend_from_slice(&enc.data);
            obj.extend_from_slice(b"\nendstream");
            objects.push((img_id, obj));
            xobjects.push_str(&format!("/Im{i} {img_id} 0 R "));
            // Рамка (обрезка) и картинка; у PDF начало снизу.
            let c = item.clip;
            let (cx, cy, cw, ch) = (mm_to_pt(c.x), ph - mm_to_pt(c.y + c.h), mm_to_pt(c.w), mm_to_pt(c.h));
            let r = item.rect;
            let (x, y, w, h) = (mm_to_pt(r.x), ph - mm_to_pt(r.y + r.h), mm_to_pt(r.w), mm_to_pt(r.h));
            content.push_str(&format!("q {cx:.3} {cy:.3} {cw:.3} {ch:.3} re W n {w:.3} 0 0 {h:.3} {x:.3} {y:.3} cm /Im{i} Do Q\n"));
        }
        let page_obj = format!(
            "<< /Type /Page /Parent 2 0 R /MediaBox [0 0 {pw:.3} {ph:.3}] /Resources << /XObject << {xobjects}>> >> /Contents {content_id} 0 R >>"
        );
        objects.push((page_id, page_obj.into_bytes()));
        let mut c = format!("<< /Length {} >>\nstream\n", content.len()).into_bytes();
        c.extend_from_slice(content.as_bytes());
        c.extend_from_slice(b"endstream");
        objects.push((content_id, c));
    }
    objects.push((1, b"<< /Type /Catalog /Pages 2 0 R >>".to_vec()));
    let kids_s: Vec<String> = kids.iter().map(|k| format!("{k} 0 R")).collect();
    objects.push((2, format!("<< /Type /Pages /Kids [{}] /Count {} >>", kids_s.join(" "), kids.len()).into_bytes()));
    objects.push((3, format!("<< /Title {} /Producer (syngui) >>", pdf_text(title)).into_bytes()));
    objects.sort_by_key(|(id, _)| *id);
    offsets.resize(next, 0);
    for (id, body) in &objects {
        offsets[*id] = buf.len();
        buf.extend_from_slice(format!("{id} 0 obj\n").as_bytes());
        buf.extend_from_slice(body);
        buf.extend_from_slice(b"\nendobj\n");
    }
    let xref = buf.len();
    buf.extend_from_slice(format!("xref\n0 {next}\n0000000000 65535 f \n").as_bytes());
    for off in offsets.iter().skip(1) {
        buf.extend_from_slice(format!("{off:010} 00000 n \n").as_bytes());
    }
    buf.extend_from_slice(format!("trailer\n<< /Size {next} /Root 1 0 R /Info 3 0 R >>\nstartxref\n{xref}\n%%EOF\n").as_bytes());
    out.write_all(&buf).map_err(|e| e.to_string())
}

/// Строка PDF в UTF-16BE (`<FEFF…>`): кириллица в заголовке.
fn pdf_text(s: &str) -> String {
    let mut out = String::from("<FEFF");
    for u in s.encode_utf16() {
        out.push_str(&format!("{u:04X}"));
    }
    out.push('>');
    out
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::print::RectMm;
    use std::sync::Arc;

    fn img(w: u32, h: u32, photo: bool) -> PrintImage {
        let mut rgba = Vec::new();
        for y in 0..h {
            for x in 0..w {
                if photo {
                    rgba.extend_from_slice(&[(x * 7 % 256) as u8, (y * 13 % 256) as u8, ((x * y) % 256) as u8, 255]);
                } else {
                    rgba.extend_from_slice(&[255, 0, 0, if x < w / 2 { 255 } else { 0 }]);
                }
            }
        }
        PrintImage { width: w, height: h, rgba: Arc::new(rgba) }
    }

    #[test]
    fn writes_valid_structure() {
        let r = RectMm { x: 10.0, y: 10.0, w: 100.0, h: 50.0 };
        let items = vec![
            PlacedImage { image: img(64, 32, true), rect: r, clip: r },
            PlacedImage { image: img(16, 16, false), rect: RectMm { x: 0.0, y: 0.0, w: 50.0, h: 50.0 }, clip: r },
        ];
        let mut out = Vec::new();
        write(&mut out, "Тест", &[PdfPage { width_mm: 210.0, height_mm: 297.0, items: &items }], false).unwrap();
        let s = String::from_utf8_lossy(&out);
        assert!(s.starts_with("%PDF-1.4"));
        assert!(s.contains("/DCTDecode") && s.contains("/FlateDecode"));
        assert!(s.contains("/MediaBox [0 0 595.276 841.890]"));
        assert!(s.contains("/Count 1"));
        // Таблица ссылок указывает на начала объектов.
        // Смещения — в байтах файла (в нём двоичные потоки).
        let find = |pat: &[u8]| out.windows(pat.len()).rposition(|w| w == pat).unwrap();
        let xref = find(b"startxref\n");
        let tail = String::from_utf8_lossy(&out[xref + 10..]);
        let at: usize = tail.lines().next().unwrap().parse().unwrap();
        assert!(out[at..].starts_with(b"xref"));
        let table = String::from_utf8_lossy(&out[at..]);
        let first = table.lines().nth(3).unwrap();
        let off: usize = first[..10].parse().unwrap();
        assert!(out[off..].starts_with(b"1 0 obj"));
    }

    #[test]
    fn alpha_flattens_on_white_and_gray() {
        let i = img(4, 1, false);
        assert_eq!(flatten(&i, false), vec![255, 0, 0, 255, 0, 0, 255, 255, 255, 255, 255, 255]);
        assert_eq!(flatten(&i, true)[0], luma(255, 0, 0));
        assert_eq!(flatten(&i, true)[3], 255);
    }
}
