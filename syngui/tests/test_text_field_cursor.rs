//! Регрессия: текст поля подменили снаружи (нормализованное значение после
//! submit — «прототип,дизайн» → «прототип, дизайн»), а каретка осталась на
//! старом байтовом смещении внутри буквы. Следующий ввод/рендер паниковал
//! на срезе строки не по границе символа.

use std::sync::{Arc, Mutex};

use syngui::core::Point;
use syngui::input::{Event, MouseButton};
use syngui::testing::TestHarness;
use syngui::widgets::TextField;

#[test]
fn external_text_change_keeps_cursor_on_char_boundary() {
    let seen: Arc<Mutex<String>> = Arc::new(Mutex::new(String::new()));
    let field = |text: &str| {
        let seen = seen.clone();
        TextField::with_text(text).width(400.0).on_change(move |t| *seen.lock().unwrap() = t.to_string())
    };
    let mut h = TestHarness::new(Box::new(field("прототип,дизайн")));
    h.layout(500.0, 60.0);
    // Клик в конец текста — фокус и каретка после последнего символа.
    let at = Point::new(390.0, 20.0);
    h.send_event(&Event::MouseDown { button: MouseButton::Left, position: at });
    h.send_event(&Event::MouseUp { button: MouseButton::Left, position: at });
    // Хост пересобрал виджет с нормализованным текстом (на 1 байт длиннее).
    h.update_widget(Box::new(field("прототип, дизайн")));
    // Ввод не должен паниковать и обязан встать на границу символа.
    h.send_event(&Event::CharInput('!'));
    let text = seen.lock().unwrap().clone();
    assert!(text.contains('!'), "ввод потерян: {text:?}");
    assert!(text.starts_with("прототип, дизай"), "текст разорван: {text:?}");
}
