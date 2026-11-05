//! box-shadow как в CSS (Chromium): σ = blur/2, у края коробки половина плотности,
//! тень тянется на ~1,5·blur. Эталон — профиль, снятый с Chromium (headless) на той же
//! странице: белая коробка 200×150, radius 24, на #F0EAE0,
//! box-shadow: 0 16px 34px #60463842, 0 2px 6px #60463824. GPU (`shot`); без GPU — пропуск.

use syngui::prelude::*;

const MSS: &str = "
.page { background: #F0EAE0; width: 400px; height: 330px; padding: 60px 100px 120px 100px; }
.card { width: 200px; height: 150px; border-radius: 24px; background: #ffffff;
        box-shadow: 0 16px 34px #60463842, 0 2px 6px #60463824; }
";

/// Профиль Chromium: (x, y, r, g, b).
const CHROME: &[(u32, u32, [i32; 3])] = &[
    (99, 135, [214, 204, 192]),
    (97, 135, [219, 210, 199]),
    (95, 135, [223, 215, 204]),
    (92, 135, [228, 219, 209]),
    (88, 135, [231, 223, 213]),
    (84, 135, [233, 226, 216]),
    (80, 135, [235, 228, 218]),
    (75, 135, [237, 231, 220]),
    (70, 135, [238, 232, 222]),
    (60, 135, [240, 234, 224]),
    (200, 211, [200, 188, 177]),
    (200, 212, [203, 192, 180]),
    (200, 215, [209, 199, 188]),
    (200, 220, [216, 207, 196]),
    (200, 225, [221, 212, 202]),
    (200, 230, [224, 217, 206]),
    (200, 240, [232, 226, 215]),
    (200, 250, [237, 231, 220]),
    (200, 260, [239, 233, 223]),
    (200, 270, [240, 234, 224]),
    (200, 59, [229, 221, 211]),
    (200, 57, [233, 225, 215]),
    (200, 55, [235, 229, 218]),
    (200, 52, [237, 230, 220]),
    (200, 48, [238, 231, 221]),
    (200, 44, [239, 233, 223]),
    (100, 135, [255, 255, 255]),
    (200, 60, [255, 255, 255]),
    (150, 100, [255, 255, 255]),
];

pub fn render(out: &str) -> std::result::Result<(u32, u32), String> {
    syngui::shot::render_png(
        || Box::new(DecoratedBox::new().class("page").child(DecoratedBox::new().class("card"))),
        MSS,
        (400, 330),
        1.0,
        out,
    )
}

#[test]
fn shadow_matches_chromium() {
    let out = std::env::var("SYNGUI_SHADOW_OUT")
        .unwrap_or_else(|_| std::env::temp_dir().join(format!("syngui-shadow-{}.png", std::process::id())).to_string_lossy().into());
    if let Err(e) = render(&out) {
        if e.contains("GPU") {
            eprintln!("пропуск: {e}");
            return;
        }
        panic!("{e}");
    }
    let img = image::open(&out).unwrap().to_rgba8();
    for &(x, y, want) in CHROME {
        let p = img.get_pixel(x, y);
        let got = [p[0] as i32, p[1] as i32, p[2] as i32];
        assert!(got.iter().zip(want).all(|(g, w)| (g - w).abs() <= 6), "({x},{y}): {got:?}, Chromium {want:?}");
    }
}
