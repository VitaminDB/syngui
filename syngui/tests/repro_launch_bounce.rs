//! Прыжок значка дока при запуске: класс с бесконечной keyframe-анимацией
//! появляется у значка при перестройке реактивного ряда. Правило задано
//! сокращением `animation: имя …` — имя бралось только из `animation-name`.

use syngui::prelude::*;
use syngui::testing::TestHarness;

const MSS: &str = r#"
.item { transition: translate-y 220ms ease-out-back, background-color 150ms ease-out; }
.bounce .item-launching { animation: bounce-up 0.72s ease-in-out infinite; }
@keyframes bounce-up {
    from { translate-y: 0px; }
    35% { translate-y: -22px; }
    60% { translate-y: 0px; }
    to { translate-y: 0px; }
}
"#;

#[test]
fn launching_class_after_rebuild_bounces() {
    let launching = use_signal(false);
    let row = DecoratedBox::new().class("bounce").child(move || {
        let cls = if launching.get() { "item item-launching" } else { "item" };
        DecoratedBox::new().class(cls).style("width", 40.0_f32).style("height", 40.0_f32)
    });
    let mut h = TestHarness::new(Box::new(row));
    let engine = h.apply_mss(MSS);
    h.apply_styles(&engine);
    h.layout(200.0, 60.0);
    for _ in 0..3 {
        h.animate(std::time::Duration::from_millis(16));
    }

    launching.set(true);
    h.rebuild();
    h.apply_styles_dirty(&engine);
    h.layout(200.0, 60.0);
    let mut max_dy = 0.0f32;
    for _ in 0..30 {
        h.animate(std::time::Duration::from_millis(16));
        let id = h.find_by_class("item-launching")[0];
        let b = h.element_bounds(id);
        let ty = h.element_mss(id).and_then(|m| m.compute_active_transform(b)).map(|t| t.m32).unwrap_or(0.0);
        max_dy = max_dy.min(ty);
    }
    assert!(max_dy < -10.0, "значок должен подпрыгнуть, сдвиг {max_dy}");
}
