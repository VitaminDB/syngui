//! Значки из SVG (resvg): mipmap для Android, AppIcon для iOS.

use anyhow::{Context, Result};
use resvg::{tiny_skia, usvg};
use std::path::Path;

/// Значок PNG (а не SVG)?
pub fn is_raster(path: &Path) -> bool {
    path.extension().and_then(|e| e.to_str()).is_some_and(|e| e.eq_ignore_ascii_case("png"))
}

/// Размер PNG по заголовку IHDR.
pub fn png_size(path: &Path) -> Result<(u32, u32)> {
    let b = std::fs::read(path).with_context(|| format!("значок {}", path.display()))?;
    anyhow::ensure!(b.len() >= 24 && b.starts_with(b"\x89PNG\r\n\x1a\n") && &b[12..16] == b"IHDR", "{} — не PNG", path.display());
    let be = |i: usize| u32::from_be_bytes([b[i], b[i + 1], b[i + 2], b[i + 3]]);
    Ok((be(16), be(20)))
}

/// Текст SVG значка: сам SVG или PNG, обёрнутый в `<image>` (resvg масштабирует растр).
pub fn load(path: &Path) -> Result<String> {
    if is_raster(path) {
        let (w, h) = png_size(path)?;
        let abs = std::fs::canonicalize(path).with_context(|| format!("значок {}", path.display()))?;
        return Ok(format!(
            r#"<svg xmlns="http://www.w3.org/2000/svg" xmlns:xlink="http://www.w3.org/1999/xlink" width="{w}" height="{h}" viewBox="0 0 {w} {h}"><image width="{w}" height="{h}" xlink:href="{}"/></svg>"#,
            crate::util::xml(&abs.to_string_lossy())
        ));
    }
    std::fs::read_to_string(path).with_context(|| format!("значок {}", path.display()))
}

/// Отрисовать SVG (текст) в PNG size×size; `background` — заливка под значком (iOS не любит прозрачность).
pub fn render(svg: &str, size: u32, background: Option<[u8; 3]>, out: &Path) -> Result<()> {
    let mut opt = usvg::Options::default();
    opt.fontdb_mut().load_system_fonts();
    let tree = usvg::Tree::from_str(svg, &opt).context("разбор SVG значка")?;
    let mut pix = tiny_skia::Pixmap::new(size, size).context("размер значка")?;
    if let Some([r, g, b]) = background {
        pix.fill(tiny_skia::Color::from_rgba8(r, g, b, 255));
    }
    let s = tree.size();
    let k = (size as f32 / s.width()).min(size as f32 / s.height());
    let dx = (size as f32 - s.width() * k) / 2.0;
    let dy = (size as f32 - s.height() * k) / 2.0;
    resvg::render(&tree, tiny_skia::Transform::from_row(k, 0.0, 0.0, k, dx, dy), &mut pix.as_mut());
    if let Some(d) = out.parent() {
        std::fs::create_dir_all(d)?;
    }
    let png = pix.encode_png().context("PNG")?;
    crate::util::write_if_changed(out, png)
}

/// Подменить viewBox (обрезать поля) и убрать скругление фона.
pub fn square(svg: &str, viewbox: Option<&str>, unround: Option<&str>) -> String {
    let mut s = svg.to_string();
    if let Some(vb) = viewbox {
        if let Some(i) = s.find("viewBox=\"") {
            let start = i + "viewBox=\"".len();
            if let Some(len) = s[start..].find('"') {
                s.replace_range(start..start + len, vb);
            }
        }
    }
    if let Some(u) = unround {
        if let Some((k, _)) = u.split_once('=') {
            s = s.replacen(u, &format!("{k}=\"0\""), 1);
        }
    }
    s
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn square_replaces_viewbox_and_radius() {
        let svg = r##"<svg xmlns="http://www.w3.org/2000/svg" viewBox="0 0 128 128"><rect x="8" y="8" width="112" height="112" rx="30" fill="#f00"/></svg>"##;
        let s = square(svg, Some("8 8 112 112"), Some(r#"rx="30""#));
        assert!(s.contains(r#"viewBox="8 8 112 112""#));
        assert!(s.contains(r#"rx="0""#));
        // без полей и скругления угол значка залит
        let out = std::env::temp_dir().join("syngui-icon-test.png");
        render(&s, 64, Some([255, 255, 255]), &out).unwrap();
        let png = std::fs::read(&out).unwrap();
        let pix = tiny_skia::Pixmap::decode_png(&png).unwrap();
        let p = pix.pixel(0, 0).unwrap();
        assert_eq!((p.red(), p.green(), p.blue()), (255, 0, 0));
    }

    #[test]
    fn png_icon_renders_scaled() {
        let src = std::env::temp_dir().join("syngui-icon-src.png");
        let mut pix = tiny_skia::Pixmap::new(32, 32).unwrap();
        pix.fill(tiny_skia::Color::from_rgba8(0, 128, 255, 255));
        std::fs::write(&src, pix.encode_png().unwrap()).unwrap();
        assert_eq!(png_size(&src).unwrap(), (32, 32));
        let out = std::env::temp_dir().join("syngui-icon-png-test.png");
        render(&load(&src).unwrap(), 96, None, &out).unwrap();
        let got = tiny_skia::Pixmap::decode_png(&std::fs::read(&out).unwrap()).unwrap();
        assert_eq!(got.width(), 96);
        let p = got.pixel(48, 48).unwrap();
        assert_eq!((p.red(), p.green(), p.blue(), p.alpha()), (0, 128, 255, 255));
    }

    #[test]
    fn letter_icon_renders() {
        let out = std::env::temp_dir().join("syngui-letter-test.png");
        render(&crate::android::letter_icon("Калькулятор"), 48, None, &out).unwrap();
        assert!(std::fs::metadata(&out).unwrap().len() > 100);
    }
}
