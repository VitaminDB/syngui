//! Регрессия: `Text` сбрасывает MSS-стили, когда правило перестало совпадать.
//!
//! `TextElement` хранит стили не в `MssFields`, а плоскими полями
//! (`mss_padding_*`, `mss_letter_spacing`, …) — из-за этого у него долго
//! не было `reset_mss_styles`, и каскад применял новое правило поверх
//! старых значений. На практике это выглядело так: `Text::new(msg)
//! .class(if err { "err" } else { "ok" })` оставался красным и после того,
//! как ошибка ушла, потому что в `.ok` про `color` ничего не сказано.
//!
//! Проверяем оба состояния: правило сменилось на другое и правил не стало
//! вовсе, — это разные ветки в `cascade::apply_styles_dirty`.

use syngui::prelude::*;
use syngui::testing::*;
use syngui::mss::cascade::{apply_styles_dirty, mark_subtree_styles_dirty};

/// Что реально уехало в отрисовку: цвет, кегль и насыщенность подписи.
fn painted(h: &mut TestHarness, needle: &str) -> (Color, f32, u16) {
    for cmd in h.paint().commands() {
        if let DrawCommand::Text { text, color, font_size, font_weight, .. } = cmd {
            if text.as_str() == needle {
                return (color, font_size, font_weight);
            }
        }
    }
    panic!("в display list нет текста {needle:?}");
}

fn switch_class(h: &mut TestHarness, class: &str, engine: &StyleEngine) {
    let id = *h.find_by_type_name("Text").first().expect("Text в дереве");
    h.set_classes(id, vec![class.to_string()]);
    mark_subtree_styles_dirty(&mut h.tree, id);
    apply_styles_dirty(&mut h.tree, engine);
    h.layout(800.0, 600.0);
}

#[test]
fn color_does_not_stick_after_class_change() {
    let widget: Box<dyn Widget> = Box::new(Text::new("hello").class("err"));
    let mut h = TestHarness::new(widget);
    let engine = h.apply_mss_dirty(".err { color: #FF0000; } .ok { font-weight: 700; }");
    h.layout(800.0, 600.0);

    let (color, _, _) = painted(&mut h, "hello");
    assert!(color.r > 0.9 && color.g < 0.1, "предусловие: .err красит текст, получили {color:?}");

    switch_class(&mut h, "ok", &engine);

    let (color, _, weight) = painted(&mut h, "hello");
    assert!(
        color.r < 0.05 && color.g < 0.05 && color.b < 0.05,
        "после .err → .ok цвет должен вернуться к дефолтному, получили {color:?}"
    );
    assert_eq!(weight, 700, "стиль нового класса при этом должен примениться");
}

#[test]
fn padding_does_not_stick_after_rules_disappear() {
    let widget: Box<dyn Widget> = Box::new(Text::new("hello").class("pad"));
    let mut h = TestHarness::new(widget);
    let engine = h.apply_mss_dirty(".pad { padding: 8; }");
    h.layout(800.0, 600.0);

    let id = *h.find_by_type_name("Text").first().expect("Text в дереве");
    let padded = h.element_bounds(id).size.height;

    // Класс, под который правил нет вовсе, — ветка «правил не осталось».
    switch_class(&mut h, "no-rules", &engine);

    let bare = h.element_bounds(id).size.height;
    assert!(
        (padded - bare - 16.0).abs() < 0.5,
        "padding: 8 должен уйти вместе с правилом: было {padded}, стало {bare}"
    );
}

#[test]
fn builder_values_survive_the_reset() {
    // Сброс возвращает не «нули», а то, что задал сам виджет: иначе снятое
    // MSS-правило унесло бы с собой `Text::color()` и `bold()`.
    let blue = Color::rgb(0.0, 0.0, 1.0);
    let widget: Box<dyn Widget> = Box::new(
        Text::new("hello").color(blue).bold().class("err")
    );
    let mut h = TestHarness::new(widget);
    let engine = h.apply_mss_dirty(".err { color: #FF0000; font-weight: 400; }");
    h.layout(800.0, 600.0);

    let (color, _, weight) = painted(&mut h, "hello");
    assert!(color.r > 0.9, "предусловие: MSS перебивает цвет билдера, получили {color:?}");
    assert_eq!(weight, 400, "предусловие: MSS перебивает font-weight билдера");

    switch_class(&mut h, "no-rules", &engine);

    let (color, _, weight) = painted(&mut h, "hello");
    assert!(
        color.b > 0.9 && color.r < 0.05,
        "после сброса должен вернуться цвет из Text::color(), получили {color:?}"
    );
    assert_eq!(weight, 700, "и насыщенность из Text::bold()");
}

#[test]
fn inherited_color_survives_the_reset() {
    // Сброс не должен съедать наследование: у Text своих правил нет, цвет
    // приходит от родителя через `parent_inh` в каскаде, и применяется он
    // тем же вызовом `apply_computed_style`, что идёт сразу после сброса.
    let widget: Box<dyn Widget> = Box::new(
        Column::new().class("panel").child(Text::new("hello"))
    );
    let mut h = TestHarness::new(widget);
    h.apply_mss(".panel { color: #00FF00; }");
    h.layout(800.0, 600.0);

    let (color, _, _) = painted(&mut h, "hello");
    assert!(
        color.g > 0.9 && color.r < 0.05,
        "унаследованный от .panel цвет должен дойти до Text, получили {color:?}"
    );
}
