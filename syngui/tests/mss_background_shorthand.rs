//! Шорткат `background: <цвет>` перекрывает `background-color` из менее
//! специфичного/более раннего правила, как в CSS (и наоборот — поздний
//! `background-color` перекрывает цвет из `background`).
use syngui::prelude::*;
use syngui::render::DrawCommand;
use syngui::testing::*;

fn fill(mss: &str, class: &str) -> Option<[u8; 3]> {
    let mut h = TestHarness::new(Box::new(DecoratedBox::new().class(class)));
    h.apply_mss(mss);
    h.layout(200.0, 100.0);
    let list = h.paint();
    list.commands().iter().find_map(|c| match c {
        DrawCommand::Rect { color, .. } => Some(color.to_srgb_u8()),
        _ => None,
    })
}

const BASE: &str = ":root { --accent: #00ff00; } .btn { width: 50px; height: 20px; background-color: #ff0000; }";

#[test]
fn later_background_shorthand_wins() {
    let mss = format!("{BASE} .btn.fab {{ background: var(--accent); }}");
    assert_eq!(fill(&mss, "btn fab"), Some([0, 255, 0]));
}

#[test]
fn later_background_color_wins_over_shorthand() {
    let mss = format!("{BASE} .btn {{ background: #0000ff; }} .btn.fab {{ background-color: #ff00ff; }}");
    assert_eq!(fill(&mss, "btn fab"), Some([255, 0, 255]));
}

#[test]
fn same_rule_order_matters() {
    let a = ".x { width: 10px; height: 10px; background-color: #ff0000; background: #00ff00; }";
    assert_eq!(fill(a, "x"), Some([0, 255, 0]));
    let b = ".x { width: 10px; height: 10px; background: #00ff00; background-color: #ff0000; }";
    assert_eq!(fill(b, "x"), Some([255, 0, 0]));
}

#[test]
fn button_takes_later_background_shorthand() {
    let mss = format!("{BASE} .btn.fab {{ background: var(--accent); }}");
    let mut h = TestHarness::new(Box::new(Button::new("+").class("btn fab")));
    h.apply_mss(&mss);
    h.layout(200.0, 100.0);
    let list = h.paint();
    let colors: Vec<[u8; 3]> = list
        .commands()
        .iter()
        .filter_map(|c| match c {
            DrawCommand::Rect { color, .. } if color.a > 0.5 => Some(color.to_srgb_u8()),
            _ => None,
        })
        .collect();
    assert!(colors.contains(&[0, 255, 0]), "кнопка: {colors:?}");
    assert!(!colors.contains(&[255, 0, 0]), "цвет из .btn не должен остаться: {colors:?}");
}
