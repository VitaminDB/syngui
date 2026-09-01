//! Тесты DocumentEditor на headless-харнесе: read-only рендер (S2) и
//! редактирование — каретка, набор, выделение, IME (S3).

use std::sync::Arc;

use syngui::core::{Point, Rect, Size};
use syngui::input::{Event, Key, MouseButton};
use syngui::render::DisplayList;
use syngui::testing::TestHarness;
use syngui::widget::context::TextMeasure;
use syngui::widgets::input::document_editor::{DocumentEditor, DocumentEditorHandle};

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

const SAMPLE: &str = "\
# Заголовок {icon=🚀}

Параграф с **жирным**, `кодом` и [[Ссылкой]].

- пункт раз
- пункт два
  - вложенный
- [x] задача

> [!warning]{color=#e0a030} Внимание
>
> Тело callout'а.

```rust
fn main() {}
```

| A | B |
| --- | --- |
| 1 | 2 |

![голос](blob:aa11.wav)

![[Доска]]

---

Конец.
";

fn harness(md: &str) -> TestHarness {
    let mut h = TestHarness::new(Box::new(DocumentEditor::new().markdown(md)));
    h.tree.text_measure = Some(Arc::new(Mono));
    h.rebuild();
    h
}

#[test]
fn builds_expected_elements() {
    let mut h = harness(SAMPLE);
    h.layout(800.0, 2000.0);

    let rows = h.find_by_type_name("doc-text-row");
    // Заголовок, параграф, 4 пункта + вложенный, заголовок callout'а,
    // тело callout'а, финальный параграф.
    assert!(rows.len() >= 9, "мало текстовых строк: {}", rows.len());
    assert_eq!(h.find_by_type_name("doc-code-block").len(), 1);
    assert_eq!(h.find_by_type_name("doc-table").len(), 1);
    assert_eq!(h.find_by_type_name("doc-media-card").len(), 1);
    assert_eq!(h.find_by_type_name("doc-embed-card").len(), 1);
    assert_eq!(h.find_by_type_name("doc-divider").len(), 1);
    assert!(!h.find_by_type_name("doc-chrome").is_empty());
}

#[test]
fn layout_gives_heights_and_paint_does_not_panic() {
    let mut h = harness(SAMPLE);
    let size = h.layout(800.0, 4000.0);
    assert!(size.height > 100.0, "документ должен иметь высоту: {size:?}");

    for id in h.find_by_type_name("doc-text-row") {
        let b = h.element_bounds(id);
        assert!(b.size.height > 0.0, "строка без высоты: {b:?}");
        assert!(b.size.width > 0.0, "строка без ширины: {b:?}");
    }

    let mut list = DisplayList::new();
    let clip = Rect::new(Point::zero(), Size::new(800.0, 4000.0));
    h.tree.build_display_list(h.root_id, &mut list, clip);
}

#[test]
fn empty_document_renders() {
    let mut h = harness("");
    let size = h.layout(400.0, 300.0);
    assert!(size.height > 0.0);
}

#[test]
fn collapsed_toggle_hides_children() {
    let open = harness("> [!toggle]{open} Секция\n>\n> Внутри.\n");
    let closed = harness("> [!toggle] Секция\n>\n> Внутри.\n");
    let count = |h: &TestHarness| h.find_by_type_name("doc-text-row").len();
    assert!(
        count(&open) > count(&closed),
        "свёрнутый toggle не должен строить детей: open={}, closed={}",
        count(&open),
        count(&closed)
    );
}

// ─── Редактирование (S3) ────────────────────────────────────────────────────

/// Харнес с ручкой: клик уже сделан, каретка стоит в точке `click`.
fn editing_harness(md: &str, click: Point) -> (TestHarness, DocumentEditorHandle) {
    let handle = DocumentEditorHandle::new();
    let mut h = TestHarness::new(Box::new(DocumentEditor::new().markdown(md).handle(&handle)));
    h.tree.text_measure = Some(Arc::new(Mono));
    h.rebuild();
    h.layout(800.0, 2000.0);
    h.send_event(&Event::MouseDown { button: MouseButton::Left, position: click });
    h.send_event(&Event::MouseUp { button: MouseButton::Left, position: click });
    (h, handle)
}

/// Прогоняет цикл «правка → перестройка → раскладка».
fn settle(h: &mut TestHarness) {
    h.rebuild();
    h.layout(800.0, 2000.0);
}

fn type_str(h: &mut TestHarness, s: &str) {
    for c in s.chars() {
        h.send_event(&Event::CharInput(c));
    }
}

// Контент начинается при ширине 800: колонка 760, поля (768-760)/2=4,
// итого x0 = 16 + 4 = 20; первая строка на y = 16.
const X0: f32 = 20.0;
const Y0: f32 = 16.0;

#[test]
fn click_places_caret_and_typing_inserts() {
    // Клик за концом «абв» (3 симв. × 10px) — каретка в конец.
    let (mut h, handle) = editing_harness("абв\n", Point::new(X0 + 60.0, Y0 + 8.0));
    type_str(&mut h, "гд");
    settle(&mut h);
    assert_eq!(handle.serialize(), "абвгд\n");
    assert!(handle.revision().get_untracked() >= 2);
}

#[test]
fn click_mid_text_inserts_at_position() {
    // Клик между «а» и «б» (x = 10px от начала текста).
    let (mut h, handle) = editing_harness("абв\n", Point::new(X0 + 10.0, Y0 + 8.0));
    type_str(&mut h, "X");
    settle(&mut h);
    assert_eq!(handle.serialize(), "аXбв\n");
}

#[test]
fn enter_splits_and_backspace_merges() {
    let (mut h, handle) = editing_harness("абв\n", Point::new(X0 + 20.0, Y0 + 8.0));
    h.send_event(&Event::KeyDown(Key::Enter));
    settle(&mut h);
    assert_eq!(handle.serialize(), "аб\n\nв\n");
    // Каретка в начале «в» — Backspace склеивает обратно.
    h.send_event(&Event::KeyDown(Key::Backspace));
    settle(&mut h);
    assert_eq!(handle.serialize(), "абв\n");
}

#[test]
fn backspace_deletes_char() {
    let (mut h, handle) = editing_harness("абв\n", Point::new(X0 + 60.0, Y0 + 8.0));
    h.send_event(&Event::KeyDown(Key::Backspace));
    settle(&mut h);
    assert_eq!(handle.serialize(), "аб\n");
}

#[test]
fn drag_selection_delete() {
    // Тянем выделение от «б» до конца, Backspace удаляет диапазон.
    let (mut h, handle) = editing_harness("абвгд\n", Point::new(X0 + 10.0, Y0 + 8.0));
    h.send_event(&Event::MouseDown {
        button: MouseButton::Left,
        position: Point::new(X0 + 10.0, Y0 + 8.0),
    });
    h.send_event(&Event::MouseMove(Point::new(X0 + 50.0, Y0 + 8.0)));
    h.send_event(&Event::MouseUp {
        button: MouseButton::Left,
        position: Point::new(X0 + 50.0, Y0 + 8.0),
    });
    h.send_event(&Event::KeyDown(Key::Backspace));
    settle(&mut h);
    assert_eq!(handle.serialize(), "аде\n".replace("де", "")); // «а» + пусто
    assert_eq!(handle.serialize(), "а\n");
}

#[test]
fn arrows_navigate_between_blocks() {
    let (mut h, handle) = editing_harness("аб\n\nвг\n", Point::new(X0 + 40.0, Y0 + 8.0));
    // Каретка в конце «аб»; вправо — начало «вг»; печать попадает во 2-й блок.
    h.send_event(&Event::KeyDown(Key::Right));
    type_str(&mut h, "X");
    settle(&mut h);
    assert_eq!(handle.serialize(), "аб\n\nXвг\n");
}

#[test]
fn ime_commit_inserts() {
    let (mut h, handle) = editing_harness("аб\n", Point::new(X0 + 40.0, Y0 + 8.0));
    h.send_event(&Event::ImePreedit { text: "ねこ".to_string(), cursor: None });
    h.send_event(&Event::ImeCommit("猫".to_string()));
    settle(&mut h);
    assert_eq!(handle.serialize(), "аб猫\n");
}

#[test]
fn gutter_click_toggles_todo() {
    // Гаттер задачи: indent 26px, чекбокс в его середине.
    let (mut h, handle) = editing_harness("- [ ] задача\n", Point::new(X0 + 500.0, Y0 + 300.0));
    h.send_event(&Event::MouseDown {
        button: MouseButton::Left,
        position: Point::new(X0 + 13.0, Y0 + 10.0),
    });
    settle(&mut h);
    assert_eq!(handle.serialize(), "- [x] задача\n");
}

#[test]
fn enter_in_list_creates_item() {
    let (mut h, handle) = editing_harness("- раз\n", Point::new(X0 + 26.0 + 60.0, Y0 + 8.0));
    h.send_event(&Event::KeyDown(Key::Enter));
    type_str(&mut h, "два");
    settle(&mut h);
    assert_eq!(handle.serialize(), "- раз\n- два\n");
}

#[test]
fn wide_layout_centers_content_column() {
    let mut h = harness("Параграф.\n");
    h.layout(2000.0, 600.0);
    // При ширине больше max_content_width строки не растягиваются на всю
    // ширину: колонка ограничена и центрируется полями.
    let rows = h.find_by_type_name("doc-text-row");
    assert!(!rows.is_empty());
    let b = h.element_bounds(rows[0]);
    assert!(
        b.size.width < 1000.0,
        "контент должен быть ограничен колонкой: {:?}",
        b.size
    );
    assert!(b.origin.x > 100.0, "колонка должна центрироваться: {:?}", b.origin);
}

// ─── Undo/redo и отступы (S4) ───────────────────────────────────────────────

#[test]
fn undo_redo_typing() {
    let (mut h, handle) = editing_harness("аб\n", Point::new(X0 + 40.0, Y0 + 8.0));
    type_str(&mut h, "вг");
    settle(&mut h);
    assert_eq!(handle.serialize(), "абвг\n");
    // Набор сгруппирован — один Ctrl+Z откатывает всё слово.
    h.tree.modifiers.ctrl = true;
    h.send_event(&Event::KeyDown(Key::Z));
    h.tree.modifiers.ctrl = false;
    settle(&mut h);
    assert_eq!(handle.serialize(), "аб\n");
    h.tree.modifiers.ctrl = true;
    h.send_event(&Event::KeyDown(Key::Y));
    h.tree.modifiers.ctrl = false;
    settle(&mut h);
    assert_eq!(handle.serialize(), "абвг\n");
}

#[test]
fn undo_structure_steps_separate() {
    let (mut h, handle) = editing_harness("аб\n", Point::new(X0 + 40.0, Y0 + 8.0));
    h.send_event(&Event::KeyDown(Key::Enter));
    settle(&mut h);
    type_str(&mut h, "в");
    settle(&mut h);
    assert_eq!(handle.serialize(), "аб\n\nв\n");
    h.tree.modifiers.ctrl = true;
    h.send_event(&Event::KeyDown(Key::Z)); // откат набора
    h.send_event(&Event::KeyDown(Key::Z)); // откат Enter
    h.tree.modifiers.ctrl = false;
    settle(&mut h);
    assert_eq!(handle.serialize(), "аб\n");
}

#[test]
fn tab_indents_list_item() {
    // Каретка во втором пункте.
    let (mut h, handle) = editing_harness("- раз\n- два\n", Point::new(X0 + 26.0 + 20.0, Y0 + 23.0 * 1.0 + 8.0));
    // Уточняем позицию клика: вторая строка ниже первой на line_h (15*1.55 ≈ 23.25) + gap.
    let _ = &handle;
    h.send_event(&Event::KeyDown(Key::Tab));
    settle(&mut h);
    assert_eq!(handle.serialize(), "- раз\n  - два\n");
    h.tree.modifiers.shift = true;
    h.send_event(&Event::KeyDown(Key::Tab));
    h.tree.modifiers.shift = false;
    settle(&mut h);
    assert_eq!(handle.serialize(), "- раз\n- два\n");
}

// ─── Шорткаты, slash-меню, инлайн-стили (S5) ────────────────────────────────

#[test]
fn hash_space_makes_heading() {
    let (mut h, handle) = editing_harness("\n", Point::new(X0 + 5.0, Y0 + 8.0));
    // Пустой документ → пустой параграф? Пустой md не имеет блоков; берём
    // документ с одним параграфом и печатаем префикс в его начало.
    let _ = (h, handle);
    let (mut h, handle) = editing_harness("текст\n", Point::new(X0, Y0 + 8.0));
    type_str(&mut h, "## ");
    settle(&mut h);
    assert_eq!(handle.serialize(), "## текст\n");
}

#[test]
fn dash_space_makes_bullet() {
    let (mut h, handle) = editing_harness("пункт\n", Point::new(X0, Y0 + 8.0));
    type_str(&mut h, "- ");
    settle(&mut h);
    assert_eq!(handle.serialize(), "- пункт\n");
}

#[test]
fn checkbox_shortcut() {
    let (mut h, handle) = editing_harness("дело\n", Point::new(X0, Y0 + 8.0));
    type_str(&mut h, "[] ");
    settle(&mut h);
    assert_eq!(handle.serialize(), "- [ ] дело\n");
}

#[test]
fn inline_bold_shortcut() {
    // NB: pulldown срезает хвостовой пробел параграфа при парсе.
    let (mut h, handle) = editing_harness("см.\n", Point::new(X0 + 30.0, Y0 + 8.0));
    type_str(&mut h, " **вот**");
    settle(&mut h);
    assert_eq!(handle.serialize(), "см. **вот**\n");
    // Проверяем, что это именно стиль, а не литеральные звёздочки:
    // literals сериализовались бы как \*\*вот\*\*.
    assert!(!handle.serialize().contains("\\*"));
}

#[test]
fn slash_menu_turns_into_heading() {
    // «/h1» в начале параграфа, Enter — выбор Heading 1.
    let (mut h, handle) = editing_harness("абв\n", Point::new(X0, Y0 + 8.0));
    type_str(&mut h, "/h1");
    h.send_event(&Event::KeyDown(Key::Enter));
    settle(&mut h);
    assert_eq!(handle.serialize(), "# абв\n");
}

#[test]
fn slash_escape_keeps_text() {
    let (mut h, handle) = editing_harness("аб\n", Point::new(X0 + 40.0, Y0 + 8.0));
    type_str(&mut h, " /код");
    h.send_event(&Event::KeyDown(Key::Escape));
    // Esc оставляет набранный текст как есть, ввод продолжается.
    type_str(&mut h, "!");
    settle(&mut h);
    assert_eq!(handle.serialize(), "аб /код!\n");
}

#[test]
fn ctrl_b_bolds_selection() {
    let (mut h, handle) = editing_harness("абвгд\n", Point::new(X0 + 10.0, Y0 + 8.0));
    // Выделяем «бв» драгом.
    h.send_event(&Event::MouseDown {
        button: MouseButton::Left,
        position: Point::new(X0 + 10.0, Y0 + 8.0),
    });
    h.send_event(&Event::MouseMove(Point::new(X0 + 30.0, Y0 + 8.0)));
    h.send_event(&Event::MouseUp {
        button: MouseButton::Left,
        position: Point::new(X0 + 30.0, Y0 + 8.0),
    });
    h.tree.modifiers.ctrl = true;
    h.send_event(&Event::KeyDown(Key::B));
    h.tree.modifiers.ctrl = false;
    settle(&mut h);
    assert_eq!(handle.serialize(), "а**бв**гд\n");
    // Повторный Ctrl+B снимает стиль.
    h.tree.modifiers.ctrl = true;
    h.send_event(&Event::KeyDown(Key::B));
    h.tree.modifiers.ctrl = false;
    settle(&mut h);
    assert_eq!(handle.serialize(), "абвгд\n");
}

#[test]
fn divider_shortcut_inserts_divider() {
    let (mut h, handle) = editing_harness("x\n", Point::new(X0, Y0 + 8.0));
    // Backspace-ом стираем «x», получаем пустой параграф? Пустой блок
    // не переживает сериализацию, поэтому просто печатаем --- в начало.
    type_str(&mut h, "---");
    settle(&mut h);
    assert!(handle.serialize().starts_with("---\n"), "{}", handle.serialize());
}

// ─── Перетаскивание блоков за ручку (S6) ────────────────────────────────────

#[test]
fn drag_handle_reorders_blocks() {
    let (mut h, handle) = editing_harness("раз\n\nдва\n\nтри\n", Point::new(X0 + 10.0, Y0 + 8.0));
    // Наводимся на первую строку — появляется ручка.
    h.send_event(&Event::MouseMove(Point::new(X0 + 10.0, Y0 + 8.0)));
    // Перерисовка post-списка вычисляет хит-зону ручки.
    let mut list = DisplayList::new();
    h.tree.build_display_list(h.root_id, &mut list, Rect::new(Point::zero(), Size::new(800.0, 2000.0)));
    // Хватаем ручку (она слева от контента, кламп к краю контейнера).
    let grab = Point::new(4.0, Y0 + 8.0);
    h.send_event(&Event::MouseDown { button: MouseButton::Left, position: grab });
    // Тянем ниже третьего блока (строки ~23px + spacing 10).
    let drop = Point::new(X0 + 10.0, Y0 + 3.0 * 33.0);
    h.send_event(&Event::MouseMove(drop));
    h.send_event(&Event::MouseUp { button: MouseButton::Left, position: drop });
    settle(&mut h);
    assert_eq!(handle.serialize(), "два\n\nтри\n\nраз\n");
}

#[test]
fn move_block_unit() {
    use syngui::widgets::input::document_editor::*;
    let mut m = parse_document("а\n\nб\n\nв\n");
    let ids: Vec<_> = m.blocks.iter().map(|b| b.id).collect();
    assert!(edit::move_block(&mut m, ids[0], ids[2], false));
    assert_eq!(serialize_document(&m), "б\n\nв\n\nа\n");
    // В собственное поддерево нельзя.
    let mut m2 = parse_document("- родитель\n  - ребёнок\n");
    let parent = m2.blocks[0].id;
    let child = m2.blocks[0].kind.children().unwrap()[0].id;
    assert!(!edit::move_block(&mut m2, parent, child, true));
}
