//! Регрессия: текст MarkdownView пропадал в прокрученной ленте.
//!
//! С отсечением блоков (`a7de5f4`) MarkdownView пропускает блоки вне `clip`,
//! сравнивая их с координатами своей раскладки. А дерево отдавало элементу
//! экранный `clip`, не сдвинутый на прокрутку контейнера, — хотя детей само
//! отсекает по сдвинутому (`cull_clip`). В ленте чата, прокрученной ниже
//! первого экрана, сообщения, «Размышления» и аргументы вызовов рисовались
//! пустыми подложками; клик в текст ставил выделение, отсечение выключалось —
//! и текст появлялся.

use syngui::core::Point;
use syngui::prelude::*;
use syngui::testing::*;
use syngui::widgets::containers::{VirtualList, VirtualRow};
use syngui::widgets::visual::MarkdownView;
use syngui::widgets::ScrollView;

const NEEDLE: &str = "ИГОЛКА";

fn painted_texts(h: &mut TestHarness) -> Vec<String> {
    h.paint()
        .commands()
        .into_iter()
        .filter_map(|cmd| match cmd {
            DrawCommand::Text { text, .. } => Some(text.to_string()),
            _ => None,
        })
        .collect()
}

/// Лента внутри обычного контейнера, как в приложении: корневой ScrollView
/// дерево обходит отдельной веткой, и прокрутку корня она не учитывает.
fn feed(md: String) -> TestHarness {
    let feed = ScrollView::new().vertical().child(
        Column::new()
            .child(
                DecoratedBox::new()
                    .style("width", 100.0_f32)
                    .style("height", 2000.0_f32),
            )
            .child(MarkdownView::new(md)),
    );
    let mut h = TestHarness::new(Box::new(DecoratedBox::new().child(feed)));
    let engine = h.apply_mss("");
    h.apply_styles(&engine);
    h.layout(400.0, 600.0);
    h
}

/// Лента прокручена к низу: сообщение под высокой распоркой на экране, и
/// его текст обязан попасть в display list.
#[test]
fn markdown_below_the_first_screen_is_painted_when_scrolled() {
    let md = format!("Первый абзац сообщения.\n\nВторой абзац, в нём {NEEDLE}.\n\nТретий абзац.");
    let mut h = feed(md);
    let wheel = Event::MouseWheel {
        delta: -5000.0,
        delta_x: 0.0,
        position: Point::new(200.0, 300.0),
    };
    assert_eq!(
        h.send_event(&wheel),
        EventResult::Handled,
        "лента не прокрутилась"
    );
    h.layout(400.0, 600.0);

    let texts = painted_texts(&mut h);
    assert!(
        texts.iter().any(|t| t.contains(NEEDLE)),
        "видимое сообщение в прокрученной ленте не нарисовано; текст в display list: {texts:?}"
    );
}

/// Без прокрутки сообщение под распоркой за экраном и не рисуется, а
/// сообщение наверху — рисуется.
#[test]
fn markdown_is_painted_only_where_visible_without_scroll() {
    let mut h = feed(format!("Абзац с {NEEDLE}."));
    let texts = painted_texts(&mut h);
    assert!(
        !texts.iter().any(|t| t.contains(NEEDLE)),
        "сообщение за экраном не должно рисоваться: {texts:?}"
    );

    let top = ScrollView::new()
        .vertical()
        .child(Column::new().child(MarkdownView::new(format!("Абзац с {NEEDLE}."))));
    let mut h = TestHarness::new(Box::new(DecoratedBox::new().child(top)));
    let engine = h.apply_mss("");
    h.apply_styles(&engine);
    h.layout(400.0, 600.0);
    let texts = painted_texts(&mut h);
    assert!(texts.iter().any(|t| t.contains(NEEDLE)), "текст: {texts:?}");
}

/// То же в `VirtualList` — на нём лента чата. Его сдвиг прокрутки свой
/// (`shift` построенного окна), но в дерево приходит тем же `scroll_offset`.
#[test]
fn markdown_in_scrolled_virtual_list_is_painted() {
    let md = format!("Первый абзац сообщения.\n\nВторой абзац, в нём {NEEDLE}.\n\nТретий абзац.");
    let rows = vec![
        VirtualRow::new(1, 1, || {
            Box::new(
                DecoratedBox::new()
                    .style("width", 100.0_f32)
                    .style("height", 2000.0_f32),
            ) as Box<dyn Widget>
        }),
        VirtualRow::new(2, 1, move || {
            Box::new(MarkdownView::new(md.clone())) as Box<dyn Widget>
        }),
    ];
    let list = VirtualList::new(rows).estimated_row_height(100.0);
    let mut h = TestHarness::new(Box::new(DecoratedBox::new().child(list)));
    let engine = h.apply_mss("");
    // Строки окна VirtualList появляются в дереве только на `rebuild()`, а
    // само окно уточняется замером высот — поэтому несколько проходов.
    let settle = |h: &mut TestHarness| {
        for _ in 0..3 {
            h.rebuild();
            h.apply_styles(&engine);
            h.layout(400.0, 600.0);
        }
    };
    settle(&mut h);
    let wheel = Event::MouseWheel {
        delta: -5000.0,
        delta_x: 0.0,
        position: Point::new(200.0, 300.0),
    };
    assert_eq!(
        h.send_event(&wheel),
        EventResult::Handled,
        "список не прокрутился"
    );
    settle(&mut h);

    let texts = painted_texts(&mut h);
    assert!(
        texts.iter().any(|t| t.contains(NEEDLE)),
        "видимое сообщение в прокрученном VirtualList не нарисовано; текст: {texts:?}"
    );
}
