//! Регрессия: точечный реестр анимаций не должен «терять» виджеты, которые
//! в `animate()` не только анимируют, но и выполняют отложенную работу —
//! Terminal (вывод PTY, команды Copy/Paste/Clear из контекстного меню) и
//! MarkdownView (действия его собственного контекстного меню).

use std::time::Duration;

use syngui::core::Point;
use syngui::input::{Event, MouseButton};
use syngui::testing::*;
use syngui::widgets::containers::Stack;
use syngui::widgets::visual::MarkdownView;

#[test]
fn terminal_gets_animate_ticks() {
    use syngui::widgets::visual::Terminal;

    let widget = Stack::new().child(Terminal::new());
    let mut harness = TestHarness::new(Box::new(widget));
    harness.layout(800.0, 600.0);
    harness.rebuild();
    harness.layout(800.0, 600.0);

    let ids = harness.find_by_type_name("Terminal");
    assert_eq!(ids.len(), 1);
    assert!(
        harness.is_animating(ids[0]),
        "терминал без тиков не показывает вывод PTY и не выполняет команды меню"
    );
}

#[test]
fn markdown_view_context_menu_copies_all() {
    let widget = Stack::new().child(MarkdownView::new("Первая строка\n\nвторая"));
    let mut harness = TestHarness::new(Box::new(widget));
    harness.layout(800.0, 600.0);
    harness.rebuild();
    harness.layout(800.0, 600.0);

    let ids = harness.find_by_type_name("MarkdownView");
    assert_eq!(ids.len(), 1);
    assert!(
        !harness.is_animating(ids[0]),
        "в покое markdown-виджет не должен требовать кадров"
    );

    // Правый клик открывает меню — с этого момента виджету нужны тики:
    // выбор пункта приходит отложенно, через сигнал menu_action.
    let at = Point::new(60.0, 20.0);
    harness.send_event(&Event::MouseDown { button: MouseButton::Right, position: at });
    harness.layout(800.0, 600.0);
    // Меню узнаёт размер поверхности при отрисовке — до неё оно не попадает
    // в overlay-стек и кликов не получает.
    harness.paint();
    assert!(
        harness.is_animating(ids[0]),
        "с открытым меню виджет обязан стоять в реестре анимаций"
    );

    // Системный буфер трогаем аккуратно: возвращаем прежнее содержимое.
    let saved = syngui::clipboard::paste();

    // Третий пункт меню — «Копировать всё»: padding 4 + два пункта по 32.
    let item = Point::new(at.x + 20.0, at.y + 4.0 + 64.0 + 16.0);
    harness.send_event(&Event::MouseDown { button: MouseButton::Left, position: item });
    harness.animate(Duration::from_millis(16));

    // В headless-среде без дисплея буфер может быть недоступен — тогда
    // проверять нечего, регрессию ловит ассерт про реестр выше.
    let copied = syngui::clipboard::paste();
    if let Some(prev) = saved {
        syngui::clipboard::copy(&prev);
    }
    if let Some(text) = copied {
        assert!(
            text.contains("Первая строка"),
            "«Копировать всё» из контекстного меню не сработало: {text:?}"
        );
    }
}

#[test]
fn text_field_context_menu_pastes() {
    use syngui::widgets::input::TextField;

    let saved = syngui::clipboard::paste();
    syngui::clipboard::copy("вставленное");

    let widget = Stack::new().child(TextField::new().text("было"));
    let mut harness = TestHarness::new(Box::new(widget));
    harness.layout(800.0, 600.0);
    harness.rebuild();
    harness.layout(800.0, 600.0);

    let ids = harness.find_by_type_name("TextField");
    assert_eq!(ids.len(), 1);

    let at = Point::new(40.0, 20.0);
    harness.send_event(&Event::MouseDown { button: MouseButton::Right, position: at });
    harness.rebuild();
    harness.layout(800.0, 600.0);
    harness.paint();
    assert!(
        harness.is_animating(ids[0]),
        "с открытым меню полю нужны кадры — иначе выбор пункта не разберётся"
    );

    // Пункты поля для чтения-записи без выделения: Вырезать (выкл),
    // Копировать (выкл), Вставить, ─, Выделить всё. Третий — «Вставить».
    let item = Point::new(at.x + 20.0, at.y + 4.0 + 64.0 + 16.0);
    harness.send_event(&Event::MouseDown { button: MouseButton::Left, position: item });
    harness.animate(Duration::from_millis(16));

    let text = harness.tree.get(ids[0]).and_then(|e| {
        e.accessibility_info().and_then(|i| i.properties.value)
    });

    if let Some(prev) = saved {
        syngui::clipboard::copy(&prev);
    }
    // Каретку правый клик поставил по позиции клика — вставка идёт туда.
    let text = text.expect("поле должно отдавать своё значение через a11y");
    assert!(
        text.contains("вставленное") && text.len() > "было".len(),
        "«Вставить» из контекстного меню поля не сработало: {text:?}"
    );
}

#[test]
fn multiline_edit_context_menu_pastes() {
    use syngui::widgets::MultilineTextEdit;

    let saved = syngui::clipboard::paste();
    syngui::clipboard::copy("из буфера");

    let widget = Stack::new().child(MultilineTextEdit::new().text("строка"));
    let mut harness = TestHarness::new(Box::new(widget));
    harness.layout(800.0, 600.0);
    harness.rebuild();
    harness.layout(800.0, 600.0);

    let ids = harness.find_by_type_name("MultilineTextEdit");
    assert_eq!(ids.len(), 1);

    let at = Point::new(30.0, 20.0);
    harness.send_event(&Event::MouseDown { button: MouseButton::Right, position: at });
    harness.rebuild();
    harness.layout(800.0, 600.0);
    harness.paint();
    assert!(harness.is_animating(ids[0]));

    // Третий пункт — «Вставить».
    let item = Point::new(at.x + 20.0, at.y + 4.0 + 64.0 + 16.0);
    harness.send_event(&Event::MouseDown { button: MouseButton::Left, position: item });
    harness.animate(Duration::from_millis(16));

    let text = harness
        .tree
        .get(ids[0])
        .and_then(|e| e.accessibility_info().and_then(|i| i.properties.value));

    if let Some(prev) = saved {
        syngui::clipboard::copy(&prev);
    }
    let text = text.expect("поле должно отдавать своё значение через a11y");
    assert!(
        text.contains("из буфера"),
        "«Вставить» из контекстного меню многострочного поля не сработало: {text:?}"
    );
}
