//! Мигание значка: бесконечная keyframe-анимация opacity у Icon внутри строки с классом
//! состояния (hexspire: жёлтый ключ выполняющегося инструмента не мигал).

use syngui::prelude::*;
use syngui::testing::TestHarness;
use syngui::widgets::*;

const MSS: &str = r#"
@keyframes blink { 0% { opacity: 1; } 50% { opacity: 0.25; } 100% { opacity: 1; } }
.tool.running .tool-icon { color: #ffcc00; animation: blink 1000ms ease-in-out infinite; }
"#;

#[test]
fn icon_opacity_keyframes_animate() {
    let row = DecoratedBox::new().class("tool").class("running").child(Row::new().child(Icon::new("\u{e869}").class("tool-icon")).child(Text::new("run_rhai")));
    let mut h = TestHarness::new(Box::new(row));
    let engine = h.apply_mss(MSS);
    h.apply_styles(&engine);
    h.layout(300.0, 40.0);
    let mut seen = Vec::new();
    for _ in 0..40 {
        h.animate(std::time::Duration::from_millis(16));
        let id = h.find_by_class("tool-icon")[0];
        let o = h.element_mss(id).and_then(|m| m.keyframe_animation.as_ref().and_then(|a| a.current_values().opacity())).unwrap_or(1.0);
        seen.push(o);
    }
    let min = seen.iter().cloned().fold(1.0f32, f32::min);
    assert!(min < 0.6, "прозрачность значка не менялась: {seen:?}");
}

/// `@keyframes` у виджетов без своего тика (Text) и у контейнеров, где анимация не запускалась
/// (Row): раньше запуск был только у DecoratedBox — `animation:` на остальных молча не работал.
#[test]
fn text_and_row_keyframes_animate() {
    const CSS: &str = r#"
@keyframes blink { 0% { opacity: 1; } 50% { opacity: 0.2; } 100% { opacity: 1; } }
@keyframes slide { 0% { translate-x: 0px; } 50% { translate-x: 30px; } 100% { translate-x: 0px; } }
.t { animation: blink 600ms linear infinite; }
.r { animation: slide 600ms linear infinite; }
"#;
    let root = Column::new().child(Text::new("мигает").class("t")).child(Row::new().class("r").child(Text::new("едет")));
    let mut h = TestHarness::new(Box::new(root));
    let engine = h.apply_mss(CSS);
    h.apply_styles(&engine);
    h.layout(300.0, 100.0);
    let (mut min_o, mut max_x) = (1.0f32, 0.0f32);
    for _ in 0..30 {
        h.animate(std::time::Duration::from_millis(16));
        let t = h.find_by_class("t")[0];
        let o = h.element_mss(t).and_then(|m| m.keyframe_animation.as_ref().and_then(|a| a.current_values().opacity())).unwrap_or(1.0);
        min_o = min_o.min(o);
        let r = h.find_by_class("r")[0];
        let b = h.element_bounds(r);
        let x = h.element_mss(r).and_then(|m| m.compute_active_transform(b)).map(|t| t.m31).unwrap_or(0.0);
        max_x = max_x.max(x);
    }
    assert!(min_o < 0.6, "Text: opacity не анимируется (min {min_o})");
    assert!(max_x > 10.0, "Row: translate-x не анимируется (max {max_x})");
}
