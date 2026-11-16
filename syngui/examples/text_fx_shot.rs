//! Снимок эффектов текста: слои `text-shadow`, `text-glow`, `text-gradient`,
//! полые буквы (`color` прозрачный + `text-stroke`), `text-animation` — на
//! тёмном и светлом фоне, крупно (120 px) и мелко (14 px).
//!
//! `cargo run --example text_fx_shot -- out_dir [кадров]` → `out_dir/dark.png`,
//! `out_dir/light.png`; с числом кадров — ещё серия `anim_{n}.png` (анимации).

use syngui::mss::StyleValue;
use syngui::prelude::*;

const MSS: &str = r#"
    .dark { background-color: #1b2233; padding: 16; }
    .light { background-color: #f4f1ea; padding: 16; }
    .dark Text { color: #ffffff; }
    .light Text { color: #1d2433; }
    .big { font-size: 120px; font-weight: 700; }
    .small { font-size: 14px; }
    .shadows { text-shadow: 0 0 8 #4fc3f7, 0 0 24 #4fc3f780, 0 2 6 #00000080; }
    .glow { text-glow: 16px #4fc3f7; }
    .glow-self { text-glow: 12px; }
    .grad { text-gradient: linear-gradient(90deg, #4fc3f7, #ab47bc); }
    .grad3 { text-gradient: linear-gradient(135deg, #ff5252 0%, #ffd740 50%, #69f0ae 100%); text-glow: 10px #ffd74080; text-stroke: 2px #00000060; }
    .dark .hollow, .light .hollow { color: #00000000; text-stroke: 2px #66bb6a; }
    .pulse { text-glow: 18px #4fc3f7; text-animation: pulse 2s; }
    .shimmer { text-animation: shimmer 2s; }
    .gmove { text-gradient: linear-gradient(90deg, #4fc3f7, #ab47bc); text-animation: gradient 3s; }
    .rainbow { text-glow: 10px; text-animation: rainbow 4s; }
"#;

fn panel(theme: &'static str, classes: &[&'static str]) -> Box<dyn Widget> {
    let mut col = Column::new().gap(4.0);
    for c in classes {
        col = col.child(Text::new("12:34").class(format!("big {c}")));
    }
    let mut row = Row::new().gap(14.0);
    for c in classes {
        row = row.child(Text::new(format!("{c} 12:34")).class(format!("small {c}")));
    }
    Box::new(DecoratedBox::new().class(theme).child(col.child(row)))
}

fn main() {
    let args: Vec<String> = std::env::args().skip(1).collect();
    let dir = args.first().cloned().unwrap_or_else(|| ".".into());
    let frames: u32 = args.get(1).and_then(|s| s.parse().ok()).unwrap_or(0);
    let statics = ["shadows", "glow", "grad", "grad3", "hollow"];
    for theme in ["dark", "light"] {
        let out = format!("{dir}/{theme}.png");
        let r = syngui::shot::render_png(move || panel(theme, &statics), MSS, (760, 840), 1.0, &out);
        println!("{out}: {r:?}");
    }
    // Полые буквы стилями в коде (прозрачный `color` + `text-stroke`).
    let out = format!("{dir}/hollow_inline.png");
    let r = syngui::shot::render_png(
        || {
            Box::new(
                DecoratedBox::new().class("hollow-bg").child(
                    Text::new("11:24")
                        .style("font-size", StyleValue::px(120.0))
                        .style("color", StyleValue::Color(syngui::mss::MssColor::rgba(0, 0, 0, 0)))
                        .style("text-stroke", StyleValue::String("2px #66bb6a".into())),
                ),
            )
        },
        ".hollow-bg { background-color: #2b3a55; padding: 16; }",
        (360, 180),
        1.0,
        &out,
    );
    println!("{out}: {r:?}");
    if frames > 1 {
        let anims = ["pulse", "shimmer", "gmove", "rainbow", "glow-self"];
        let out = format!("{dir}/anim_{{n}}.png");
        let opts = syngui::shot::ShotOptions { frames, interval: std::time::Duration::from_millis(250), ..Default::default() };
        let r = syngui::shot::render_png_with(move || panel("dark", &anims), MSS, (760, 840), 1.0, &out, opts);
        println!("{out}: {r:?}");
    }
}
