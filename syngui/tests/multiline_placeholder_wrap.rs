//! Плейсхолдер MultilineTextEdit, не влезающий в ширину поля (узкий экран
//! телефона), переносится по словам и рисуется построчно сверху вниз —
//! раньше весь текст центрировался в строке высотой 20 px, и первая строка
//! уходила выше клипа поля.
use std::sync::Arc;

use syngui::prelude::*;
use syngui::testing::*;
use syngui::widget::context::TextMeasure;
use syngui::widgets::input::MultilineTextEdit;

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

fn placeholder_rows(width: f32) -> Vec<(String, Rect, Rect)> {
    let edit = MultilineTextEdit::new()
        .rows(5)
        .placeholder("Первая строка подсказки длинная\nвторая");
    let mut h = TestHarness::with_text_measure(Box::new(edit), Arc::new(Mono));
    h.layout_loose(width, 400.0);
    let list = h.paint();
    let mut clip = Rect::zero();
    list.commands()
        .into_iter()
        .filter_map(|c| match c {
            DrawCommand::PushClip { rect, .. } => {
                clip = rect;
                None
            }
            DrawCommand::Text { text, rect, .. } => Some((text.to_string(), rect, clip)),
            _ => None,
        })
        .collect()
}

#[test]
fn narrow_placeholder_wraps_top_down() {
    let rows = placeholder_rows(200.0);
    eprintln!("{rows:#?}");
    assert!(rows.len() >= 3, "ожидался перенос: {rows:?}");
    assert_eq!(rows[0].0, "Первая строка");
    assert_eq!(rows.last().unwrap().0, "вторая");
    for (i, (text, rect, clip)) in rows.iter().enumerate() {
        assert!(rect.min_y() >= clip.min_y(), "строка {i} «{text}» выше клипа: {rect:?} / {clip:?}");
        if i > 0 {
            assert!(rect.min_y() > rows[i - 1].1.min_y(), "строки идут сверху вниз");
        }
    }
}

#[test]
fn wide_placeholder_stays_on_first_row() {
    let rows = placeholder_rows(1000.0);
    assert_eq!(rows.len(), 2);
    assert_eq!(rows[0].0, "Первая строка подсказки длинная");
    // Первая строка — там же, где первая строка набранного текста.
    assert_eq!(rows[0].1.size.height, 20.0);
}
