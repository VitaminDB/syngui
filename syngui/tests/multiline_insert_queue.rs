//! `MultilineTextEdit::insert_queue`: хост кладёт строку в очередь и
//! пересобирает виджет — текст вставляется в позицию каретки (не в конец),
//! каретка встаёт за вставленным, `on_change` получает новый текст.

use std::sync::{Arc, Mutex};

use syngui::input::{Event, Key};
use syngui::prelude::*;
use syngui::testing::*;
use syngui::widgets::MultilineTextEdit;

fn editor(text: &str, queue: Arc<Mutex<Vec<String>>>, seen: Arc<Mutex<String>>) -> Box<dyn Widget> {
    let seen_c = seen.clone();
    Box::new(
        MultilineTextEdit::new()
            .text(text)
            .insert_queue(queue)
            .on_change(move |s| {
                if let Ok(mut v) = seen_c.lock() {
                    *v = s.to_string();
                }
            }),
    )
}

#[test]
fn queued_text_lands_at_the_caret() {
    let queue = Arc::new(Mutex::new(Vec::new()));
    let seen = Arc::new(Mutex::new(String::new()));
    // Каретка при создании — в конце текста.
    let mut h = TestHarness::new(editor("ab", queue.clone(), seen.clone()));
    h.layout(300.0, 100.0);

    queue.lock().unwrap().push("😀".to_string());
    h.update_widget(editor("ab", queue.clone(), seen.clone()));
    assert_eq!(seen.lock().unwrap().as_str(), "ab😀");

    // Хост синхронизировал text с тем, что видел в on_change; каретка за
    // эмодзи — следующая вставка идёт туда же.
    queue.lock().unwrap().push("!".to_string());
    h.update_widget(editor("ab😀", queue.clone(), seen.clone()));
    assert_eq!(seen.lock().unwrap().as_str(), "ab😀!");
    assert!(queue.lock().unwrap().is_empty(), "очередь вычерпана");
}

#[test]
fn queued_text_replaces_caret_position_after_home() {
    let queue = Arc::new(Mutex::new(Vec::new()));
    let seen = Arc::new(Mutex::new(String::new()));
    let mut h = TestHarness::new(editor("ab", queue.clone(), seen.clone()));
    h.layout(300.0, 100.0);
    // Фокус кликом, каретка в начало строки.
    h.send_events(&click_at(syngui::core::Point::new(10.0, 10.0)));
    h.send_event(&Event::KeyDown(Key::Home));
    queue.lock().unwrap().push("• ".to_string());
    h.update_widget(editor("ab", queue.clone(), seen.clone()));
    assert_eq!(seen.lock().unwrap().as_str(), "• ab");
}
