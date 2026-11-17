//! Снимок виджета `Lottie` в настоящем дереве элементов (headless wgpu):
//! проверка раскладки, загрузки текстуры и тиков анимации.
//!
//! `cargo run -p syngui --example lottie_shot --features image -- file.json out.png`

use syngui::prelude::*;
use syngui::widgets::Lottie;

fn main() {
    let args: Vec<String> = std::env::args().collect();
    let path = args.get(1).cloned().unwrap_or_default();
    let out = args.get(2).cloned().unwrap_or_else(|| "lottie_shot.png".into());
    let mss = r#"
        .root { background-color: #202830; padding: 16; }
        .big { width: 240; height: 240; background-color: #ffffff20; }
        .small { width: 120; height: 80; }
    "#;
    let p1 = path.clone();
    let p2 = path.clone();
    let p3 = path.clone();
    let size = syngui::shot::render_png_with(
        move || {
            Box::new(
                DecoratedBox::new().class("root").child(
                    Row::new()
                        .gap(16.0)
                        .child(Lottie::from_file(&p1).class("big"))
                        .child(Lottie::from_file(&p2).fit(ImageFit::Cover).class("small"))
                        .child(Lottie::from_file(&p3).speed(0.0).progress_signal(use_signal(0.5)).class("small")),
                ),
            )
        },
        mss,
        (560, 300),
        2.0,
        &out,
        syngui::shot::ShotOptions {
            wait: std::time::Duration::from_millis(600),
            ..Default::default()
        },
    )
    .expect("shot");
    eprintln!("снимок {}×{} → {out}", size.0, size.1);
}
