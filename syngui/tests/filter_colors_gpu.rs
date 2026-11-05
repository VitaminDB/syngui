//! Цветовые фильтры MSS на GPU: `saturate()` и `contrast()` по отдельности и цепочкой
//! (один проход эффекта 24) не меняют оттенок — оранжевое остаётся оранжевым, а
//! не синеет (перестановка каналов, «негатив» из-за неверно прочитанного кода операции).
//! Рисуется настоящим wgpu (`shot`); без GPU-адаптера тест пропускается.

use syngui::prelude::*;

const MSS: &str = "
.tile { width: 60px; height: 60px; background: #d9822b; }
.sat { filter: saturate(125%); }
.con { filter: contrast(1.06); }
.both { filter: saturate(125%) contrast(1.06); }
.inv { filter: invert(100%); }
";

fn hue(r: f32, g: f32, b: f32) -> f32 {
    let v = r.max(g).max(b);
    let m = r.min(g).min(b);
    let d = v - m;
    if d < 1e-3 {
        return 0.0;
    }
    let h = if v == r {
        ((g - b) / d).rem_euclid(6.0)
    } else if v == g {
        (b - r) / d + 2.0
    } else {
        (r - g) / d + 4.0
    };
    h * 60.0
}

fn sat(r: f32, g: f32, b: f32) -> f32 {
    let v = r.max(g).max(b);
    if v <= 0.0 {
        0.0
    } else {
        (v - r.min(g).min(b)) / v
    }
}

#[test]
fn saturate_contrast_keep_hue() {
    let out = std::env::temp_dir().join(format!("syngui-filter-colors-{}.png", std::process::id()));
    let out_s = out.to_string_lossy().to_string();
    let classes = ["tile", "tile sat", "tile con", "tile both", "tile inv"];
    let r = syngui::shot::render_png(
        move || {
            let mut row = Row::new().gap(10.0);
            for c in classes {
                row = row.child(DecoratedBox::new().class(c));
            }
            Box::new(row)
        },
        MSS,
        (400, 80),
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
    let px = |i: u32| {
        let p = img.get_pixel(i * 70 + 30, 30);
        (p[0] as f32 / 255.0, p[1] as f32 / 255.0, p[2] as f32 / 255.0)
    };
    let (r0, g0, b0) = px(0);
    let h0 = hue(r0, g0, b0);
    let s0 = sat(r0, g0, b0);
    assert!((25.0..35.0).contains(&h0), "исходный оранжевый: тон {h0}");
    for (i, name) in [(1, "saturate"), (2, "contrast"), (3, "saturate+contrast")] {
        let (r, g, b) = px(i);
        let h = hue(r, g, b);
        assert!((h - h0).abs() < 4.0, "{name}: тон {h:.1}° вместо {h0:.1}° (rgb {r:.2} {g:.2} {b:.2})");
        assert!(r > b, "{name}: синий преобладает над красным (rgb {r:.2} {g:.2} {b:.2})");
        if name != "contrast" {
            assert!(sat(r, g, b) >= s0 - 0.01, "{name}: насыщенность упала {:.2} → {:.2}", s0, sat(r, g, b));
        }
    }
    // invert — проверка, что фильтры вообще применяются
    let (r, g, b) = px(4);
    assert!(b > r, "invert: ожидался голубой, rgb {r:.2} {g:.2} {b:.2}");
    let _ = std::fs::remove_file(&out);
}
