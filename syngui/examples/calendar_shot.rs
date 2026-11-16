//! Снимки `Calendar` в рамке заданного размера (`fit`): крупный месяц 600×600 со
//! своим шрифтом, средний с номерами недель, формы выделения дня
//! (`--cal-day-shape`), без дней соседних месяцев (`--cal-outside-color: transparent`).
//!
//! `cargo run --example calendar_shot --features image -- каталог`

use syngui::mss::{MssColor, StyleValue};
use syngui::prelude::*;
use syngui::widgets::{Calendar, Date};

const MSS: &str = r#"
    .frame { background-color: #1b2233; align-items: center; justify-content: center; }
    Calendar { color: #e8ecf4; accent-color: #4fc3f7; background-color: #00000000; border-color: #00000000; }
"#;

fn shot(dir: &str, name: &str, size: (u32, u32), week: bool, styles: Vec<(&'static str, StyleValue)>) {
    let out = format!("{dir}/{name}.png");
    let r = syngui::shot::render_png(
        move || {
            let mut cal = Calendar::new().selected(Date::new(2026, 11, 16)).show_week_numbers(week).fit(true).class("cal");
            for (k, v) in styles {
                cal = cal.style(k, v);
            }
            Box::new(
                DecoratedBox::new()
                    .class("frame")
                    .style("width", StyleValue::px(size.0 as f32))
                    .style("height", StyleValue::px(size.1 as f32))
                    .child(cal),
            )
        },
        MSS,
        size,
        1.0,
        &out,
    );
    println!("{out}: {r:?}");
}

fn main() {
    let dir = std::env::args().nth(1).unwrap_or_else(|| ".".into());
    let s = |v: &str| StyleValue::String(v.into());
    shot(&dir, "big", (600, 600), false, vec![("font-family", s("Noto Serif")), ("font-weight", StyleValue::Number(300.0)), ("--cal-day-shape", s("rounded"))]);
    shot(&dir, "weeks", (440, 380), true, vec![("--cal-day-shape", s("square")), ("--cal-weekend-color", StyleValue::Color(MssColor::rgb(255, 138, 128)))]);
    shot(&dir, "no_outside", (330, 330), false, vec![("--cal-outside-color", StyleValue::Color(MssColor::rgba(0, 0, 0, 0)))]);
    shot(&dir, "font_fixed", (600, 600), false, vec![("--cal-font-size", StyleValue::px(14.0))]);
}
