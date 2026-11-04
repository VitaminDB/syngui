//! Кнопка с иконкой и подписью: место под иконку не отдаётся тексту при
//! подсчёте строк, а подпись, помещающаяся в строку, рисуется без переноса —
//! на Android (Roboto, дробный масштаб) ширина шейпинга расходилась с
//! измеренной на доли пикселя, и «Чат с разработчиком» ломался на две строки.
use std::sync::Arc;

use syngui::prelude::*;
use syngui::testing::*;
use syngui::widget::context::TextMeasure;

/// Моноширинная метрика: 10px на символ.
struct Mono;
impl TextMeasure for Mono {
    fn measure_text_width(&self, _t: &str, _fs: f32, chars: usize) -> f32 {
        chars as f32 * 10.0
    }
    fn hit_test_char(&self, text: &str, _fs: f32, x: f32) -> usize {
        ((x / 10.0).round() as usize).min(text.chars().count())
    }
}

const LABEL: &str = "Чат с разработчиком"; // 19 символов = 190 px

fn button_size(max_w: f32) -> (Size, Vec<bool>) {
    // В «обнимающем» Row кнопка по размеру подписи; узкая — в Column
    // (он ограничивает ширину детей).
    let root: Box<dyn Widget> = if max_w >= 1000.0 {
        Box::new(Row::new().child(Button::new(LABEL).icon("chat")))
    } else {
        Box::new(Column::new().child(Button::new(LABEL).icon("chat")))
    };
    let mut h = TestHarness::with_text_measure(root, Arc::new(Mono));
    h.layout_loose(max_w, 400.0);
    let id = h.find_by_type_name("Button")[0];
    let size = h.element_bounds(id).size;
    let no_wrap = h
        .paint()
        .commands()
        .into_iter()
        .filter_map(|c| match c {
            DrawCommand::Text { text, no_wrap, .. } if text.as_str() == LABEL => Some(no_wrap),
            _ => None,
        })
        .collect();
    (size, no_wrap)
}

#[test]
fn hugging_button_keeps_label_on_one_line() {
    let (size, no_wrap) = button_size(1000.0);
    // 16 + 18 + 8 + 190 + 16
    assert_eq!(size.width, 248.0);
    assert!((size.height - (14.0 * 1.2 + 16.0)).abs() < 0.01, "высота {}", size.height);
    assert_eq!(no_wrap, vec![true], "однострочная подпись рисуется без переноса");
}

#[test]
fn narrow_button_counts_icon_space() {
    // Без иконки 220 - 32 = 188 < 190 — и так две строки; берём ширину,
    // где текст влез бы, только если забыть про иконку: 230 - 32 = 198 ≥ 190,
    // но 198 - 26 = 172 < 190.
    let (size, no_wrap) = button_size(230.0);
    assert_eq!(size.width, 230.0);
    assert!((size.height - (2.0 * 14.0 * 1.2 + 16.0)).abs() < 0.01, "высота {}", size.height);
    assert_eq!(no_wrap, vec![false], "две строки — с переносом");
}
