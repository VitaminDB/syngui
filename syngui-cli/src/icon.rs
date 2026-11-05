//! Значки из SVG (resvg): mipmap для Android, AppIcon для iOS.

use anyhow::{Context, Result};
use resvg::{tiny_skia, usvg};
use std::path::Path;

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
    fn letter_icon_renders() {
        let out = std::env::temp_dir().join("syngui-letter-test.png");
        render(&crate::android::letter_icon("Калькулятор"), 48, None, &out).unwrap();
        assert!(std::fs::metadata(&out).unwrap().len() > 100);
    }
}
