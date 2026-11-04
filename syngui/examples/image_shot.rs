//! Снимок виджета `Image` с картинками из файлов (headless wgpu): проверка
//! декодера — форматы, поворот, альфа (на шахматном фоне видна прозрачность).
//!
//! `cargo run --example image_shot --features heif -- a.heic b.avif c.jpg out.png`

use syngui::prelude::*;

fn main() {
    let mut args: Vec<String> = std::env::args().skip(1).collect();
    let out = args.pop().unwrap_or_else(|| "image_shot.png".into());
    let mss = r#"
        .root { background-color: #303840; padding: 12; }
        .cell { width: 220; height: 220; background-color: #8890a0; }
        .name { color: #ffffff; font-size: 11; }
    "#;
    let n = args.len().max(1) as u32;
    let size = syngui::shot::render_png_with(
        move || {
            let mut row = Row::new().gap(12.0);
            for path in &args {
                let name = std::path::Path::new(path)
                    .file_name()
                    .map(|s| s.to_string_lossy().into_owned())
                    .unwrap_or_default();
                row = row.child(
                    Column::new()
                        .gap(4.0)
                        .child(Image::new(path.clone()).fit(ImageFit::Contain).class("cell"))
                        .child(Text::new(name).class("name")),
                );
            }
            Box::new(DecoratedBox::new().class("root").child(row))
        },
        mss,
        (24 + n * 232, 260),
        1.0,
        &out,
        syngui::shot::ShotOptions {
            wait: std::time::Duration::from_millis(1500),
            ..Default::default()
        },
    )
    .expect("shot");
    eprintln!("снимок {}×{} → {out}", size.0, size.1);
}
