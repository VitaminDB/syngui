//! Стекло: полупрозрачный белый с backdrop-filter: blur() поверх однотонного фона
//! светлее фона (как в CSS), а не мутно-серое. GPU (`shot`); без GPU — пропуск.

use syngui::prelude::*;

const MSS: &str = "
.root { background: #f5d9a8; padding: 20px; width: 300px; height: 140px; }
.glass { width: 200px; height: 80px; border-radius: 30px; background-color: #FFFFFF80; backdrop-filter: blur(22px); }
";

#[test]
fn backdrop_glass_is_lighter_than_background() {
    let out = std::env::temp_dir().join(format!("syngui-glass-{}.png", std::process::id()));
    let out_s = out.to_string_lossy().to_string();
    let r = syngui::shot::render_png(
        || Box::new(DecoratedBox::new().class("root").child(DecoratedBox::new().class("glass"))),
        MSS,
        (300, 140),
        1.0,
        &out_s,
    );
    if let Err(e) = &r {
        if e.contains("GPU") {
            eprintln!("пропуск: {e}");
            return;
        }
    }
    r.unwrap();
    let img = image::open(&out).unwrap().to_rgba8();
    let p = |x, y| {
        let p = img.get_pixel(x, y);
        [p[0] as i32, p[1] as i32, p[2] as i32]
    };
    let bg = p(280, 130);
    let glass = p(120, 60);
    // по CSS: 50 % белого поверх размытого однотонного фона = (bg + 255) / 2
    let want: Vec<i32> = bg.iter().map(|c| (c + 255 + 1) / 2).collect();
    eprintln!("фон {bg:?}, стекло {glass:?}, по CSS {want:?}");
    for i in 0..3 {
        assert!((glass[i] - want[i]).abs() <= 3, "стекло {glass:?} вместо {want:?} (фон {bg:?})");
    }
    let _ = std::fs::remove_file(&out);
}
