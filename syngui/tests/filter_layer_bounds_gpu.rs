//! Фильтр слоя (gradient-map, duotone) меняет только свой элемент: прозрачная часть
//! слоя не должна прибавлять цвет ко всему кадру (премультиплицированный выход).
//! GPU (`shot`); без GPU — пропуск.

use syngui::prelude::*;

const MSS: &str = "
.page { background: #808080; width: 300px; height: 200px; padding: 50px; }
.thumb { width: 60px; height: 60px; background: #ffffff; filter: gradient-map(#ff0000, #00ff00); }
.duo { width: 60px; height: 60px; background: #000000; filter: duotone(#0000ff, #ffff00); }
";

#[test]
fn layer_filter_stays_inside_element() {
    let out = std::env::temp_dir().join(format!("syngui-layer-bounds-{}.png", std::process::id()));
    let out_s = out.to_string_lossy().to_string();
    let r = syngui::shot::render_png(
        || {
            Box::new(
                Row::new()
                    .class("page")
                    .gap(40.0)
                    .child(DecoratedBox::new().class("thumb"))
                    .child(DecoratedBox::new().class("duo")),
            )
        },
        MSS,
        (300, 200),
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
    let px = |x, y| {
        let p = img.get_pixel(x, y);
        [p[0] as i32, p[1] as i32, p[2] as i32]
    };
    for (x, y) in [(10, 10), (290, 190), (25, 150), (200, 30)] {
        let p = px(x, y);
        assert!(p.iter().all(|c| (c - 128).abs() <= 2), "фон вне элемента изменился в ({x},{y}): {p:?}");
    }
    let t = px(80, 80);
    assert!(t[1] > 200 && t[0] < 60, "белое под gradient-map → светлый край (зелёный): {t:?}");
    let d = px(180, 80);
    assert!(d[2] > 200 && d[0] < 60, "чёрное под duotone → тёмный край (синий): {d:?}");
    let _ = std::fs::remove_file(&out);
}
