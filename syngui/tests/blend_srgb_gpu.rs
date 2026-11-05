//! Смешение полупрозрачных цветов — как в CSS/браузерах (в гамма-пространстве sRGB),
//! а не в линейном: 50 % чёрного поверх белого — (128,128,128), а не (188,188,188).
//! Сплошные цвета не меняются. Рисуется настоящим wgpu (`shot`); без GPU — пропуск.

use syngui::prelude::*;

const MSS: &str = "
.root { background: #ffffff; padding: 10px; }
.half { width: 60px; height: 60px; background: #00000080; }
.solid { width: 60px; height: 60px; background: #d9822b; }
.veil { width: 60px; height: 60px; background: #F3F4EF; }
.veil-in { width: 60px; height: 60px; background: #006BA69E; }
";

#[test]
fn alpha_blends_in_srgb_like_css() {
    let out = std::env::temp_dir().join(format!("syngui-blend-{}.png", std::process::id()));
    let out_s = out.to_string_lossy().to_string();
    let r = syngui::shot::render_png(
        || {
            Box::new(
                Row::new()
                    .class("root")
                    .gap(10.0)
                    .child(DecoratedBox::new().class("half"))
                    .child(DecoratedBox::new().class("solid"))
                    .child(DecoratedBox::new().class("veil").child(DecoratedBox::new().class("veil-in"))),
            )
        },
        MSS,
        (300, 80),
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
    let px = |x: u32| {
        let p = img.get_pixel(x, 40);
        [p[0] as i32, p[1] as i32, p[2] as i32]
    };
    let near = |got: [i32; 3], want: [i32; 3], tol: i32| got.iter().zip(want).all(|(g, w)| (g - w).abs() <= tol);
    let half = px(40);
    assert!(near(half, [128, 128, 128], 2), "50 % #000 поверх #fff: {half:?}, по CSS (128,128,128)");
    let solid = px(110);
    assert!(near(solid, [217, 130, 43], 1), "сплошной #d9822b: {solid:?}");
    let veil = px(180);
    assert!(near(veil, [92, 159, 194], 3), "#006BA69E поверх #F3F4EF: {veil:?}, по CSS (92,159,194)");
    let _ = std::fs::remove_file(&out);
}
