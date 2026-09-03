//! Тесты DocumentEditor на headless-харнесе: read-only рендер (S2) и
//! редактирование — каретка, набор, выделение, IME (S3).

use std::sync::Arc;

use syngui::core::{Point, Rect, Size};
use syngui::input::{Event, Key, MouseButton};
use syngui::render::DisplayList;
use syngui::testing::TestHarness;
use syngui::widget::context::TextMeasure;
use syngui::widgets::input::document_editor::{
    parse_document, BlockKind, DocGrid, DocLayout, DocOp, DocumentEditor, DocumentEditorHandle,
    ShapeKind, SlashAction, TableOp,
};


/// Значение ключа геометрии/свойства блока `idx` из служебного блока.
fn geom_val(md: &str, idx: usize, key: &str) -> Option<f32> {
    let line = md.lines().find(|l| l.starts_with(&format!("{idx} ")))?;
    let inner = line.split_once('{')?.1.trim_end_matches('}');
    inner
        .split_whitespace()
        .find_map(|kv| kv.strip_prefix(&format!("{key}="))?.parse().ok())
}

/// Значение инлайн-атрибута `{k=v …}` из markdown (ключ — целиком, не
/// подстрокой: `x1` не должен находиться внутри `cx1`).
fn attr_val(md: &str, key: &str) -> Option<f32> {
    md.split(['{', '}', ' ', '\n'])
        .find_map(|tok| tok.strip_prefix(&format!("{key}="))?.parse().ok())
}

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

    // Регрессия: tight-layout контейнеров не должен терять высоту
    // (фоны callout'ов и хит-зона дропа рисуются по bounds).
    let root_bounds = h.element_bounds(h.root_id);
    assert!(root_bounds.size.height > 100.0, "корень сжался: {root_bounds:?}");

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

// ─── Автокомплит [[ и ссылки (S7) ───────────────────────────────────────────

struct StubLinks;
impl syngui::widgets::input::document_editor::DocLinkProvider for StubLinks {
    fn complete(
        &self,
        prefix: &str,
    ) -> Vec<syngui::widgets::input::document_editor::LinkCandidate> {
        use syngui::widgets::input::document_editor::LinkCandidate;
        ["Проект X", "Проект Y", "План"]
            .iter()
            .filter(|t| t.to_lowercase().contains(&prefix.to_lowercase()))
            .map(|t| LinkCandidate { target: t.to_string(), label: t.to_string() })
            .collect()
    }
    fn link_exists(&self, target: &str) -> bool {
        target != "Битая"
    }
}

#[test]
fn wiki_autocomplete_inserts_link() {
    let handle = DocumentEditorHandle::new();
    let mut h = TestHarness::new(Box::new(
        DocumentEditor::new()
            .markdown("см. \n")
            .handle(&handle)
            .links(Arc::new(StubLinks)),
    ));
    h.tree.text_measure = Some(Arc::new(Mono));
    h.rebuild();
    h.layout(800.0, 2000.0);
    // NB: хвостовой пробел срезан парсером; кликаем в конец «см.».
    let p = Point::new(X0 + 30.0, Y0 + 8.0);
    h.send_event(&Event::MouseDown { button: MouseButton::Left, position: p });
    h.send_event(&Event::MouseUp { button: MouseButton::Left, position: p });
    type_str(&mut h, " [[план");
    h.send_event(&Event::KeyDown(Key::Enter));
    settle(&mut h);
    assert_eq!(handle.serialize(), "см. [[План]]\n");
}

#[test]
fn wiki_escape_leaves_literal() {
    let handle = DocumentEditorHandle::new();
    let mut h = TestHarness::new(Box::new(
        DocumentEditor::new()
            .markdown("аб\n")
            .handle(&handle)
            .links(Arc::new(StubLinks)),
    ));
    h.tree.text_measure = Some(Arc::new(Mono));
    h.rebuild();
    h.layout(800.0, 2000.0);
    let p = Point::new(X0 + 40.0, Y0 + 8.0);
    h.send_event(&Event::MouseDown { button: MouseButton::Left, position: p });
    h.send_event(&Event::MouseUp { button: MouseButton::Left, position: p });
    type_str(&mut h, " [[x");
    h.send_event(&Event::KeyDown(Key::Escape));
    type_str(&mut h, "!");
    settle(&mut h);
    // Литеральный `[[x!` при сериализации экранируется, при парсе он
    // остаётся текстом (не ссылкой) — проверяем содержимое.
    let m = parse_document(&handle.serialize());
    let BlockKind::Paragraph(t) = &m.blocks[0].kind else { panic!() };
    assert_eq!(t.text(), "аб [[x!");
}

// ─── Дроп файлов и patch_media (S8) ─────────────────────────────────────────

#[test]
fn drop_file_inserts_pending_and_patch_resolves() {
    use std::sync::Mutex as StdMutex;
    let handle = DocumentEditorHandle::new();
    let dropped: Arc<StdMutex<Vec<(String, String)>>> = Arc::new(StdMutex::new(Vec::new()));
    let dropped_cb = dropped.clone();
    let mut h = TestHarness::new(Box::new(
        DocumentEditor::new()
            .markdown("абв\n")
            .handle(&handle)
            .on_drop_file(move |path, token| {
                dropped_cb.lock().unwrap().push((path.display().to_string(), token));
            }),
    ));
    h.tree.text_measure = Some(Arc::new(Mono));
    h.rebuild();
    h.layout(800.0, 2000.0);

    h.tree.dispatch_drag_event(&Event::Drop {
        position: Point::new(X0 + 20.0, Y0 + 40.0),
        data: syngui::input::DragData::external_file(std::path::Path::new("/tmp/демо.mp4")),
    });
    settle(&mut h);

    let calls = dropped.lock().unwrap().clone();
    assert_eq!(calls.len(), 1, "колбэк дропа должен вызваться");
    let token = calls[0].1.clone();
    assert!(handle.serialize().contains(&format!("pending:{token}")));

    // Хост «загрузил» файл и патчит url.
    assert!(handle.patch_media(&token, "blob:aa11.mp4"));
    settle(&mut h);
    assert!(handle.serialize().contains("blob:aa11.mp4"), "{}", handle.serialize());
    assert!(!handle.serialize().contains("pending:"));
}

// ─── Сохранение модели и свободная раскладка ────────────────────────────────

/// Регрессия: пересоздание элемента (размонтирование поддерева при смене
/// вкладки) не должно перепарсивать исходник хоста поверх правок в ручке.
#[test]
fn handle_keeps_edits_when_element_is_recreated() {
    let (mut h, handle) = editing_harness("аб\n", Point::new(X0 + 40.0, Y0 + 8.0));
    type_str(&mut h, "вг");
    settle(&mut h);
    assert_eq!(handle.serialize(), "абвг\n");

    // Новый элемент по тому же исходнику и той же ручке — как при
    // возврате на вкладку заметок.
    let mut again = TestHarness::new(Box::new(
        DocumentEditor::new().markdown("аб\n").handle(&handle),
    ));
    again.tree.text_measure = Some(Arc::new(Mono));
    again.rebuild();
    again.layout(800.0, 2000.0);
    assert_eq!(handle.serialize(), "абвг\n", "правки потерялись при пересборке элемента");

    // Другой исходник (перезагрузка страницы) модель всё ещё заменяет.
    let mut reload = TestHarness::new(Box::new(
        DocumentEditor::new().markdown("другое\n").handle(&handle),
    ));
    reload.tree.text_measure = Some(Arc::new(Mono));
    reload.rebuild();
    reload.layout(800.0, 2000.0);
    assert_eq!(handle.serialize(), "другое\n");
}

/// Свободная раскладка: блоки стоят по своим координатам, а не колонкой.
#[test]
fn free_layout_positions_blocks_by_coordinates() {
    let md = "Первый\n\nВторой\n\n~~~doc-layout\n0 40 300 200\n1 400 60 200\n~~~\n"
        .replace("~~~", "```");
    let handle = DocumentEditorHandle::new();
    let layout = DocLayout { free: true, grid: DocGrid::Dots, ..DocLayout::default() };
    let mut h = TestHarness::new(Box::new(
        DocumentEditor::new().markdown(&md).handle(&handle).layout(layout),
    ));
    h.tree.text_measure = Some(Arc::new(Mono));
    h.rebuild();
    let size = h.layout(900.0, 700.0);

    let rows = h.find_by_type_name("doc-text-row");
    assert_eq!(rows.len(), 2, "должны быть два параграфа");
    let a = h.element_bounds(rows[0]);
    let b = h.element_bounds(rows[1]);
    assert!((a.origin.x - 40.0).abs() < 1.0, "первый блок не по x=40: {:?}", a.origin);
    assert!((a.origin.y - 300.0).abs() < 1.0, "первый блок не по y=300: {:?}", a.origin);
    assert!((b.origin.x - 400.0).abs() < 1.0, "второй блок не по x=400: {:?}", b.origin);
    assert!((b.origin.y - 60.0).abs() < 1.0, "второй блок не по y=60: {:?}", b.origin);
    assert!(a.size.width <= 201.0, "ширина блока задаётся раскладкой: {:?}", a.size);
    assert!(size.height >= 700.0, "холст должен закрывать вьюпорт: {size:?}");

    // Геометрия переживает round-trip через markdown.
    let back = handle.serialize();
    assert_eq!(geom_val(&back, 0, "x"), Some(40.0), "геометрия не сохранилась:\n{back}");
    assert_eq!(geom_val(&back, 0, "y"), Some(300.0));
    assert_eq!(geom_val(&back, 0, "w"), Some(200.0));

    let mut list = DisplayList::new();
    h.tree.build_display_list(
        h.root_id,
        &mut list,
        Rect::new(Point::zero(), Size::new(900.0, 700.0)),
    );
}

/// Свободная раскладка не двигает то, чего не двигали: блок без координат
/// остаётся в колонке потока, координаты появляются только при переносе.
#[test]
fn free_layout_keeps_untouched_blocks_in_flow() {
    let handle = DocumentEditorHandle::new();
    let mut h = TestHarness::new(Box::new(
        DocumentEditor::new().markdown("Раз\n\nДва\n").handle(&handle),
    ));
    h.tree.text_measure = Some(Arc::new(Mono));
    h.rebuild();
    h.layout(800.0, 600.0);
    let flow_second = h.element_bounds(h.find_by_type_name("doc-text-row")[1]);

    h.update_widget(Box::new(
        DocumentEditor::new()
            .markdown("Раз\n\nДва\n")
            .handle(&handle)
            .layout(DocLayout { free: true, ..DocLayout::default() }),
    ));
    h.rebuild();
    h.layout(800.0, 600.0);
    let free_second = h.element_bounds(h.find_by_type_name("doc-text-row")[1]);
    assert!(
        (free_second.origin.y - flow_second.origin.y).abs() < 2.0,
        "блок прыгнул при переходе в свободную раскладку: {:?} → {:?}",
        flow_second.origin,
        free_second.origin
    );
    assert!(
        !handle.serialize().contains("doc-layout"),
        "нетронутый блок не должен получать координаты:\n{}",
        handle.serialize()
    );
}

/// Перенос за ручку ⋮⋮ закрепляет блок на холсте с привязкой к шагу.
#[test]
fn dragging_by_the_handle_pins_the_block() {
    let handle = DocumentEditorHandle::new();
    let layout = DocLayout { free: true, snap: true, snap_step: 5.0, ..DocLayout::default() };
    let mut h = TestHarness::new(Box::new(
        DocumentEditor::new().markdown("Раз\n\nДва\n").handle(&handle).layout(layout),
    ));
    h.tree.text_measure = Some(Arc::new(Mono));
    h.rebuild();
    h.layout(800.0, 600.0);

    // Фокус и наведение — ручка рисуется только у блока под курсором.
    let row = h.element_bounds(h.find_by_type_name("doc-text-row")[1]);
    let inside = Point::new(row.origin.x + 5.0, row.origin.y + 5.0);
    h.send_event(&Event::MouseDown { button: MouseButton::Left, position: inside });
    h.send_event(&Event::MouseUp { button: MouseButton::Left, position: inside });
    h.send_event(&Event::MouseMove(inside));
    let mut list = DisplayList::new();
    h.tree.build_display_list(
        h.root_id,
        &mut list,
        Rect::new(Point::zero(), Size::new(800.0, 600.0)),
    );

    let grip = Point::new(row.origin.x - 14.0, row.origin.y + 8.0);
    h.send_event(&Event::MouseDown { button: MouseButton::Left, position: grip });
    h.send_event(&Event::MouseMove(Point::new(grip.x + 103.0, grip.y + 47.0)));
    h.send_event(&Event::MouseUp { button: MouseButton::Left, position: grip });
    settle(&mut h);

    let md = handle.serialize();
    assert!(md.contains("doc-layout"), "перенос не закрепил блок:\n{md}");
    let x = geom_val(&md, 1, "x").expect("координаты второго блока");
    let y = geom_val(&md, 1, "y").expect("координаты второго блока");
    for v in [x, y] {
        assert!((v % 5.0).abs() < 0.01, "координата не по шагу привязки: {v} в\n{md}");
    }
}

/// Регрессия: закреплённый блок должен «числиться» там, где нарисован.
/// Дерево ставит Positioned-обёртку в начало холста и смещает только её
/// ребёнка — из-за этого блок перехватывал наведение у соседей сверху и
/// не тянулся за собственную ручку.
#[test]
fn pinned_block_is_registered_where_it_is_drawn() {
    let md = "Первый\n\nВторой\n\n~~~doc-layout\n1 520 300 200\n~~~\n".replace("~~~", "```");
    let handle = DocumentEditorHandle::new();
    let layout = DocLayout { free: true, snap: true, snap_step: 5.0, ..DocLayout::default() };
    let mut h = TestHarness::new(Box::new(
        DocumentEditor::new().markdown(&md).handle(&handle).layout(layout),
    ));
    h.tree.text_measure = Some(Arc::new(Mono));
    h.rebuild();
    h.layout(900.0, 700.0);

    // Второй блок закреплён; первый остался в потоке наверху.
    let rows = h.find_by_type_name("doc-text-row");
    let pinned = rows
        .iter()
        .map(|id| h.element_bounds(*id))
        .find(|b| b.origin.x > 400.0)
        .expect("закреплённый блок нарисован по своим координатам");

    let inside = Point::new(pinned.origin.x + 5.0, pinned.origin.y + 5.0);
    h.send_event(&Event::MouseDown { button: MouseButton::Left, position: inside });
    h.send_event(&Event::MouseUp { button: MouseButton::Left, position: inside });
    h.send_event(&Event::MouseMove(inside));
    let mut list = DisplayList::new();
    h.tree.build_display_list(
        h.root_id,
        &mut list,
        Rect::new(Point::zero(), Size::new(900.0, 700.0)),
    );

    // Ручка ⋮⋮ этого блока — слева от него; тянем вниз на 100 px.
    let grip = Point::new(pinned.origin.x - 14.0, pinned.origin.y + 8.0);
    h.send_event(&Event::MouseDown { button: MouseButton::Left, position: grip });
    h.send_event(&Event::MouseMove(Point::new(grip.x, grip.y + 100.0)));
    h.send_event(&Event::MouseUp { button: MouseButton::Left, position: grip });
    settle(&mut h);

    let md = handle.serialize();
    let x = geom_val(&md, 1, "x").expect("геометрия закреплённого блока");
    let y = geom_val(&md, 1, "y").expect("геометрия закреплённого блока");
    assert!((y - 400.0).abs() < 6.0, "блок не поехал за своей ручкой: {y}\n{md}");
    assert!((x - 520.0).abs() < 6.0, "блок уехал по горизонтали: {x}\n{md}");
}

/// Код-блок редактируется на месте: клик ставит каретку внутрь кода,
/// набор и Enter правят его текст, а не разрывают блок.
#[test]
fn code_block_is_editable_in_place() {
    let (mut h, handle) = editing_harness("```rust\nfn a() {}\n```\n", Point::new(X0, Y0));
    let code = h.find_by_type_name("doc-code-block");
    assert_eq!(code.len(), 1);
    let b = h.element_bounds(code[0]);

    // Клик в конец первой строки кода.
    let at = Point::new(b.origin.x + 400.0, b.origin.y + 14.0);
    h.send_event(&Event::MouseDown { button: MouseButton::Left, position: at });
    h.send_event(&Event::MouseUp { button: MouseButton::Left, position: at });
    type_str(&mut h, "!");
    settle(&mut h);
    assert!(
        handle.serialize().contains("fn a() {}!"),
        "набор не попал в код:\n{}",
        handle.serialize()
    );

    // Enter внутри кода — перевод строки, а не разрыв блока.
    h.send_event(&Event::KeyDown(Key::Enter));
    type_str(&mut h, "x");
    settle(&mut h);
    let md = handle.serialize();
    assert!(md.contains("fn a() {}!\nx"), "Enter не дал новую строку кода:\n{md}");
    assert_eq!(md.matches("```").count(), 2, "блок разорвался:\n{md}");
}

/// Выноска без заголовка тоже редактируется: строка заголовка строится
/// всегда, иначе каретке некуда встать.
#[test]
fn empty_callout_has_an_editable_row() {
    let (mut h, handle) = editing_harness("> [!note]\n>\n> Тело.\n", Point::new(X0, Y0));
    let rows = h.find_by_type_name("doc-text-row");
    assert!(rows.len() >= 2, "у выноски должна быть строка заголовка: {}", rows.len());
    let b = h.element_bounds(rows[0]);
    let at = Point::new(b.origin.x + 2.0, b.origin.y + 4.0);
    h.send_event(&Event::MouseDown { button: MouseButton::Left, position: at });
    h.send_event(&Event::MouseUp { button: MouseButton::Left, position: at });
    type_str(&mut h, "Тема");
    settle(&mut h);
    assert!(
        handle.serialize().contains("Тема"),
        "заголовок выноски не редактируется:\n{}",
        handle.serialize()
    );
}

/// Вставка правым кликом ставит в точку **сам блок**, а не пустой каркас:
/// таблица и код добавляют собственный блок рядом, и координаты должны
/// достаться ему.
#[test]
fn context_insert_pins_the_real_block() {
    for (action, expect) in [
        (SlashAction::Table, "|"),
        (SlashAction::CodeBlock, "```"),
        (SlashAction::Todo, "- [ ]"),
    ] {
        let handle = DocumentEditorHandle::new();
        let layout = DocLayout { free: true, snap: true, snap_step: 5.0, ..DocLayout::default() };
        let widget = |epoch: u64, handle: &DocumentEditorHandle| -> Box<dyn syngui::widget::Widget> {
            Box::new(
                DocumentEditor::new()
                    .markdown("Текст\n")
                    .handle(handle)
                    .layout(layout)
                    .model_epoch(epoch)
                    .on_context_menu(|_| {}),
            )
        };
        let mut h = TestHarness::new(widget(0, &handle));
        h.tree.text_measure = Some(Arc::new(Mono));
        h.rebuild();
        h.layout(900.0, 700.0);

        // Правый клик в пустое место → операция вставки из меню хоста.
        let at = Point::new(300.0, 400.0);
        h.send_event(&Event::MouseDown { button: MouseButton::Right, position: at });
        handle.queue_op(DocOp::InsertBlock(action.clone()));
        h.update_widget(widget(1, &handle));
        settle(&mut h);

        let md = handle.serialize();
        assert!(md.contains(expect), "{action:?}: блок не вставился:\n{md}");
        let geom: Vec<&str> = md
            .lines()
            .skip_while(|l| !l.starts_with("```doc-layout"))
            .skip(1)
            .take_while(|l| !l.starts_with("```"))
            .collect();
        assert_eq!(geom.len(), 1, "{action:?}: закреплён не ровно один блок:\n{md}");
        let idx: usize = geom[0].split_whitespace().next().unwrap().parse().unwrap();
        let x = geom_val(&md, idx, "x").unwrap();
        let y = geom_val(&md, idx, "y").unwrap();
        assert!(
            (x - 300.0).abs() < 40.0 && (y - 400.0).abs() < 40.0,
            "{action:?}: блок встал не в точку клика: {x},{y}\n{md}"
        );
    }
}

/// Дерево блоков видит и пустые блоки, а свойства блока меняют его вид.
#[test]
fn outline_and_block_props() {
    let handle = DocumentEditorHandle::new();
    let mut h = TestHarness::new(Box::new(
        DocumentEditor::new().markdown("Текст\n\n---\n\n| a | b |\n| --- | --- |\n| 1 | 2 |\n").handle(&handle),
    ));
    h.tree.text_measure = Some(Arc::new(Mono));
    h.rebuild();
    h.layout(800.0, 600.0);

    let outline = handle.outline();
    assert_eq!(outline.len(), 3, "в дереве должны быть все блоки: {outline:?}");
    assert_eq!(outline[1].kind, "divider", "невидимый блок обязан быть в дереве");
    assert_eq!(outline[2].kind, "table");

    // Свойство блока применяется к отрисовке: кегль заголовка из атрибута.
    let id = outline[0].id;
    let props = handle.block_props(id).expect("свойства блока");
    assert_eq!(props.kind, "paragraph");
    handle.queue_op(DocOp::SetAttr { block: id, key: "size".into(), value: Some("40".into()) });
    h.update_widget(Box::new(
        DocumentEditor::new()
            .markdown("Текст\n\n---\n\n| a | b |\n| --- | --- |\n| 1 | 2 |\n")
            .handle(&handle)
            .model_epoch(1),
    ));
    settle(&mut h);
    let row = h.element_bounds(h.find_by_type_name("doc-text-row")[0]);
    assert!(row.size.height > 40.0, "кегль из свойств не применился: {:?}", row.size);
    assert!(
        handle.serialize().contains("size=40"),
        "свойство не сохранилось:\n{}",
        handle.serialize()
    );

    // Колонка таблицы добавляется операцией панели свойств.
    let table = outline[2].id;
    handle.queue_op(DocOp::Table { block: table, op: TableOp::AddColumn });
    h.update_widget(Box::new(
        DocumentEditor::new()
            .markdown("Текст\n\n---\n\n| a | b |\n| --- | --- |\n| 1 | 2 |\n")
            .handle(&handle)
            .model_epoch(2),
    ));
    settle(&mut h);
    let props = handle.block_props(table).expect("свойства таблицы");
    assert_eq!(props.table, Some((2, 3)), "колонка не добавилась");
}

// ─── Векторные примитивы ────────────────────────────────────────────────────

/// Прогнать очередь операций хоста: элемент разбирает её в `update()`,
/// то есть при пересборке виджета (в приложении это делает Reactive).
fn pump(h: &mut TestHarness, handle: &DocumentEditorHandle, md: &str, layout: DocLayout) {
    h.update_widget(Box::new(
        DocumentEditor::new()
            .markdown(md)
            .handle(handle)
            .layout(layout)
            .on_context_menu(|_| {})
            .model_epoch(1),
    ));
    settle(h);
}

/// Вставка примитива из меню: блок появляется в точке правого клика со
/// своими умолчаниями размера и становится текущим (панель свойств хоста).
#[test]
fn inserting_a_shape_places_it_at_the_click() {
    let handle = DocumentEditorHandle::new();
    let layout = DocLayout { free: true, snap: false, ..DocLayout::default() };
    let mut h = TestHarness::new(Box::new(
        DocumentEditor::new()
            .markdown("Текст\n")
            .handle(&handle)
            .layout(layout)
            // Точку вставки запоминает обработчик контекстного меню.
            .on_context_menu(|_| {}),
    ));
    h.tree.text_measure = Some(Arc::new(Mono));
    h.rebuild();
    h.layout(900.0, 700.0);

    // Правый клик запоминает точку вставки, как в реальном меню.
    h.send_event(&Event::MouseDown {
        button: MouseButton::Right,
        position: Point::new(300.0, 240.0),
    });
    handle.queue_op(DocOp::InsertBlock(SlashAction::Shape(ShapeKind::Rect)));
    pump(&mut h, &handle, "Текст\n", layout);

    let md = handle.serialize();
    assert!(md.contains("![[shape:rect]]"), "фигура не вставилась:\n{md}");
    // Индекс фигуры среди верхнеуровневых блоков — по служебному блоку.
    let idx: usize = md
        .lines()
        .find(|l| l.contains("h=140"))
        .and_then(|l| l.split_whitespace().next())
        .and_then(|i| i.parse().ok())
        .unwrap_or_else(|| panic!("у фигуры нет геометрии:\n{md}"));
    assert!(geom_val(&md, idx, "x").is_some(), "фигура не встала в точку клика:\n{md}");
    assert_eq!(geom_val(&md, idx, "w"), Some(220.0), "ширина по умолчанию:\n{md}");
    assert!(h.find_by_type_name("doc-shape").len() == 1, "фигура не отрисована");
    assert_eq!(handle.selected().get_untracked().is_some(), true, "фигура не стала текущей");
}

/// Свойства фигуры правятся панелью хоста через `SetAttr` и переживают
/// сериализацию.
#[test]
fn shape_properties_go_through_attrs() {
    let handle = DocumentEditorHandle::new();
    let mut h = TestHarness::new(Box::new(
        DocumentEditor::new().markdown("![[shape:ellipse]]\n").handle(&handle),
    ));
    h.tree.text_measure = Some(Arc::new(Mono));
    h.rebuild();
    h.layout(800.0, 600.0);

    let block = handle.outline()[0].id;
    handle.queue_op(DocOp::SetAttr { block, key: "fill".into(), value: Some("#4f8cff".into()) });
    handle.queue_op(DocOp::SetAttr { block, key: "sw".into(), value: Some("4".into()) });
    pump(&mut h, &handle, "![[shape:ellipse]]\n", DocLayout::default());
    let md = handle.serialize();
    assert!(md.contains("fill=#4f8cff") && md.contains("sw=4"), "свойства не записались:\n{md}");

    // Пустое значение возвращает свойство к теме.
    handle.queue_op(DocOp::SetAttr { block, key: "fill".into(), value: None });
    pump(&mut h, &handle, "![[shape:ellipse]]\n", DocLayout::default());
    assert!(!handle.serialize().contains("fill="), "свойство не снялось");
}

/// «Превратить в» меняет вид фигуры, сохраняя её оформление и геометрию.
#[test]
fn turning_a_shape_keeps_its_look() {
    let handle = DocumentEditorHandle::new();
    let mut h = TestHarness::new(Box::new(
        DocumentEditor::new()
            .markdown("![[shape:rect]]{fill=#243149 sw=3}\n")
            .handle(&handle),
    ));
    h.tree.text_measure = Some(Arc::new(Mono));
    h.rebuild();
    h.layout(800.0, 600.0);

    let block = handle.outline()[0].id;
    handle.queue_op(DocOp::Select(block));
    handle.queue_op(DocOp::TurnInto(SlashAction::Shape(ShapeKind::Diamond)));
    pump(&mut h, &handle, "![[shape:rect]]{fill=#243149 sw=3}\n", DocLayout::default());

    let md = handle.serialize();
    assert!(md.contains("![[shape:diamond]]"), "вид не сменился:\n{md}");
    assert!(md.contains("fill=#243149") && md.contains("sw=3"), "оформление потерялось:\n{md}");
    assert_eq!(handle.outline().len(), 1, "вместо смены вида добавился блок");
}

/// Конец линии тянется мышью: концы пишутся в атрибуты, а рамка блока
/// подтягивается под них.
#[test]
fn dragging_a_line_endpoint_moves_it() {
    let md = "![[shape:arrow]]{x1=0 y1=0 x2=200 y2=0}\n\n~~~doc-layout\n0 {h=24 w=224 x=100 y=100}\n~~~\n"
        .replace("~~~", "```");
    let handle = DocumentEditorHandle::new();
    let layout = DocLayout { free: true, snap: false, ..DocLayout::default() };
    let mut h = TestHarness::new(Box::new(
        DocumentEditor::new().markdown(&md).handle(&handle).layout(layout),
    ));
    h.tree.text_measure = Some(Arc::new(Mono));
    h.rebuild();
    h.layout(900.0, 700.0);

    // Хваталки рисуются у блока под курсором — наводимся на линию.
    let shape = h.element_bounds(h.find_by_type_name("doc-shape")[0]);
    let on_line = Point::new(shape.origin.x + 60.0, shape.origin.y + 12.0);
    h.send_event(&Event::MouseMove(on_line));
    let mut list = DisplayList::new();
    h.tree.build_display_list(
        h.root_id,
        &mut list,
        Rect::new(Point::zero(), Size::new(900.0, 700.0)),
    );

    // Правый конец отрезка: он на 12 px (поле) от правого края рамки.
    let end = Point::new(shape.origin.x + shape.size.width - 12.0, shape.origin.y + 12.0);
    h.send_event(&Event::MouseDown { button: MouseButton::Left, position: end });
    h.send_event(&Event::MouseMove(Point::new(end.x + 60.0, end.y + 80.0)));
    h.send_event(&Event::MouseUp { button: MouseButton::Left, position: end });
    settle(&mut h);

    let out = handle.serialize();
    let val = |k: &str| attr_val(&out, k);
    assert!(val("x2").unwrap_or(0.0) > 240.0, "конец не уехал вправо:\n{out}");
    assert!(val("y2").unwrap_or(0.0) > 60.0, "конец не уехал вниз:\n{out}");
    assert!(geom_val(&out, 0, "h").unwrap_or(0.0) > 80.0, "рамка не подтянулась:\n{out}");
}

/// Клик по фигуре делает её текущим блоком — каретки внутри неё нет.
#[test]
fn clicking_a_shape_selects_it() {
    let md = "Текст\n\n![[shape:rect]]\n\n~~~doc-layout\n1 {h=120 w=200 x=300 y=300}\n~~~\n"
        .replace("~~~", "```");
    let handle = DocumentEditorHandle::new();
    let layout = DocLayout { free: true, ..DocLayout::default() };
    let mut h = TestHarness::new(Box::new(
        DocumentEditor::new().markdown(&md).handle(&handle).layout(layout),
    ));
    h.tree.text_measure = Some(Arc::new(Mono));
    h.rebuild();
    h.layout(900.0, 700.0);

    let shape = h.element_bounds(h.find_by_type_name("doc-shape")[0]);
    let inside = Point::new(shape.origin.x + 40.0, shape.origin.y + 40.0);
    h.send_event(&Event::MouseDown { button: MouseButton::Left, position: inside });
    h.send_event(&Event::MouseUp { button: MouseButton::Left, position: inside });

    let selected = handle.selected().get_untracked().expect("фигура должна стать текущей");
    let props = handle.block_props(selected).expect("свойства фигуры");
    assert_eq!(props.kind, "shape");
    assert_eq!(props.shape, Some(ShapeKind::Rect));
}

/// Направляющая кривой тянется отдельной хваталкой: концы остаются на
/// месте, а `cx/cy` уезжают за курсором.
#[test]
fn dragging_a_curve_control_bends_it() {
    let md = "![[shape:curve]]{x1=0 y1=0 x2=200 y2=0}\n\n~~~doc-layout\n0 {h=24 w=224 x=100 y=100}\n~~~\n"
        .replace("~~~", "```");
    let handle = DocumentEditorHandle::new();
    let layout = DocLayout { free: true, snap: false, ..DocLayout::default() };
    let mut h = TestHarness::new(Box::new(
        DocumentEditor::new().markdown(&md).handle(&handle).layout(layout),
    ));
    h.tree.text_measure = Some(Arc::new(Mono));
    h.rebuild();
    h.layout(900.0, 700.0);

    // Хваталки рисуются у блока под курсором.
    let shape = h.element_bounds(h.find_by_type_name("doc-shape")[0]);
    h.send_event(&Event::MouseMove(Point::new(
        shape.origin.x + shape.size.width / 2.0,
        shape.origin.y + shape.size.height / 2.0,
    )));
    let mut list = DisplayList::new();
    h.tree.build_display_list(
        h.root_id,
        &mut list,
        Rect::new(Point::zero(), Size::new(900.0, 700.0)),
    );

    // Первая направляющая по умолчанию — правее первого конца на 45% длины
    // (у горизонтальной кривой она лежит на той же высоте).
    let ctrl = Point::new(shape.origin.x + 12.0 + 90.0, shape.origin.y + 12.0);
    h.send_event(&Event::MouseDown { button: MouseButton::Left, position: ctrl });
    h.send_event(&Event::MouseMove(Point::new(ctrl.x, ctrl.y + 120.0)));
    h.send_event(&Event::MouseUp { button: MouseButton::Left, position: ctrl });
    settle(&mut h);

    let out = handle.serialize();
    // Ключ ищется целиком: `x1=` — подстрока `cx1=`, и наивный split_once
    // возвращал бы значение направляющей вместо конца.
    let val = |k: &str| attr_val(&out, k);
    assert!(val("cy1").unwrap_or(0.0) > 80.0, "направляющая не уехала вниз:\n{out}");
    assert_eq!(val("x1"), Some(0.0), "конец кривой сдвинулся:\n{out}");
    assert_eq!(val("x2"), Some(200.0), "второй конец сдвинулся:\n{out}");
    assert!(geom_val(&out, 0, "h").unwrap_or(0.0) > 100.0, "рамка не выросла:\n{out}");
}

/// Холст свободной раскладки шире видимой области, когда блок ушёл за её
/// правый край (страница прокручивается по горизонтали), а колонка потока
/// при этом держит ширину области, а не ужимается к своим строкам.
#[test]
fn free_layout_canvas_grows_past_the_viewport_width() {
    let md = "Поток\n\nДалеко\n\n~~~doc-layout\n1 1200 40 300\n~~~\n".replace("~~~", "```");
    let handle = DocumentEditorHandle::new();
    let mut h = TestHarness::new(Box::new(
        DocumentEditor::new()
            .markdown(&md)
            .handle(&handle)
            .layout(DocLayout { free: true, ..DocLayout::default() }),
    ));
    h.tree.text_measure = Some(Arc::new(Mono));
    h.rebuild();
    // Как под `Page::both()`: обе оси безграничны, containing block — вьюпорт.
    let viewport = Size::new(900.0, 700.0);
    let size = h.tree.layout(
        h.root_id,
        syngui::layout::Constraints {
            min_width: 0.0,
            max_width: f32::INFINITY,
            min_height: 0.0,
            max_height: f32::INFINITY,
            containing_block: viewport,
        },
    );
    assert!(size.width >= 1500.0, "холст должен вместить блок за правым краем: {size:?}");
    assert!(size.height >= 700.0, "холст не ниже вьюпорта: {size:?}");

    let rows = h.find_by_type_name("doc-text-row");
    assert_eq!(rows.len(), 2);
    let flow = h.element_bounds(rows[0]);
    let far = h.element_bounds(rows[1]);
    assert!((far.origin.x - 1200.0).abs() < 1.0, "закреплённый блок не по x=1200: {:?}", far.origin);
    assert!(flow.size.width > 700.0, "колонка потока ужалась при бесконечной ширине: {:?}", flow.size);
    assert!(flow.origin.x > 40.0, "колонка потока должна стоять по центру области: {:?}", flow.origin);
}

/// Виджеты внутри живой врезки получают мышь: клик по кнопке доски
/// должен дойти до неё, а не осесть в редакторе (выбор блока).
#[test]
fn embed_children_receive_clicks() {
    use std::sync::atomic::{AtomicUsize, Ordering};
    use syngui::prelude::*;
    use syngui::widget::Widget;
    use syngui::widgets::input::document_editor::{EmbedCtx, EmbedFactory};
    use syngui::widgets::GestureDetector;

    struct ClickyFactory(Arc<AtomicUsize>);
    impl EmbedFactory for ClickyFactory {
        fn build(&self, _target: &str, _ctx: &EmbedCtx) -> Option<Box<dyn Widget>> {
            let clicks = self.0.clone();
            Some(Box::new(
                GestureDetector::new()
                    .on_click(move || {
                        clicks.fetch_add(1, Ordering::SeqCst);
                    })
                    // Текст даёт виджету размер и без MSS-движка (харнес
                    // инлайн-стили не применяет).
                    .child(Text::new("Кнопка доски")),
            ))
        }
        fn has_own_height(&self, _target: &str) -> bool {
            true
        }
    }

    let clicks = Arc::new(AtomicUsize::new(0));
    let handle = DocumentEditorHandle::new();
    let mut h = TestHarness::new(Box::new(
        DocumentEditor::new()
            .markdown("Текст\n\n![[kanban:abc]]\n")
            .handle(&handle)
            .embeds(Arc::new(ClickyFactory(clicks.clone())))
            .layout(DocLayout { free: true, ..DocLayout::default() }),
    ));
    h.tree.text_measure = Some(Arc::new(Mono));
    h.rebuild();
    h.layout(800.0, 600.0);

    let gestures = h.find_by_type_name("GestureDetector");
    assert_eq!(gestures.len(), 1, "врезка не построилась");
    let b = h.element_bounds(gestures[0]);
    assert!(b.size.width > 0.0 && b.size.height > 0.0, "у виджета врезки нет размера: {b:?}");
    let inside = Point::new(b.origin.x + b.size.width / 2.0, b.origin.y + b.size.height / 2.0);
    h.send_event(&Event::MouseDown { button: MouseButton::Left, position: inside });
    h.send_event(&Event::MouseUp { button: MouseButton::Left, position: inside });
    assert_eq!(clicks.load(Ordering::SeqCst), 1, "клик не дошёл до виджета врезки");
}

/// Клик внутри врезки-объекта (доска) не оставляет каретку в блоке над
/// ней: приложению редактор над врезкой фокус не отдаёт
/// (`Element::text_input_hit`), а клик в её пустое место лишь делает
/// объект текущим и снимает фокус — набор после этого в заголовок не идёт.
#[test]
fn click_inside_embed_drops_caret_from_previous_block() {
    use syngui::prelude::*;
    use syngui::widget::Widget;
    use syngui::widgets::input::document_editor::{EmbedCtx, EmbedFactory};

    struct InertFactory;
    impl EmbedFactory for InertFactory {
        fn build(&self, _target: &str, _ctx: &EmbedCtx) -> Option<Box<dyn Widget>> {
            Some(Box::new(Text::new("Доска без кнопок")))
        }
        fn has_own_height(&self, _target: &str) -> bool {
            true
        }
    }

    let handle = DocumentEditorHandle::new();
    let mut h = TestHarness::new(Box::new(
        DocumentEditor::new()
            .markdown("# Заголовок\n\n![[kanban:abc]]{h=200}\n")
            .handle(&handle)
            .embeds(Arc::new(InertFactory))
            .layout(DocLayout { free: true, ..DocLayout::default() }),
    ));
    h.tree.text_measure = Some(Arc::new(Mono));
    h.rebuild();
    h.layout(800.0, 600.0);

    let texts = h.find_by_type_name("Text");
    assert_eq!(texts.len(), 1, "врезка не построилась");
    let b = h.element_bounds(texts[0]);
    let inside = Point::new(b.origin.x + b.size.width / 2.0, b.origin.y + b.size.height / 2.0);
    let heading = Point::new(X0 + 1.0, Y0 + 1.0);

    // Правило для приложения: над врезкой редактор фокус не берёт.
    let editors = h.find_by_type_name("document-editor");
    assert_eq!(editors.len(), 1);
    let editor = h.tree.get(editors[0]).unwrap();
    assert!(editor.text_input_hit(heading), "над заголовком фокус должен браться");
    assert!(!editor.text_input_hit(inside), "над врезкой фокус браться не должен");

    // Каретка в заголовке: набор попадает в него.
    h.send_event(&Event::MouseDown { button: MouseButton::Left, position: heading });
    h.send_event(&Event::MouseUp { button: MouseButton::Left, position: heading });
    h.send_event(&Event::CharInput('A'));
    assert!(handle.serialize().starts_with("# AЗаголовок"), "{}", handle.serialize());

    // Клик по пустому месту врезки: объект — текущий блок, каретки и
    // фокуса у редактора нет — буква в заголовок не попадает.
    h.send_event(&Event::MouseDown { button: MouseButton::Left, position: inside });
    h.send_event(&Event::MouseUp { button: MouseButton::Left, position: inside });
    let embed_id = handle.outline().iter().find(|b| b.kind == "embed").map(|b| b.id);
    assert!(embed_id.is_some());
    assert_eq!(handle.selected().get(), embed_id, "врезка должна стать текущим блоком");
    h.send_event(&Event::CharInput('B'));
    assert!(!handle.serialize().contains('B'), "набор ушёл в заголовок: {}", handle.serialize());

    // Клик обратно в текст возвращает каретку.
    h.send_event(&Event::MouseDown { button: MouseButton::Left, position: heading });
    h.send_event(&Event::MouseUp { button: MouseButton::Left, position: heading });
    h.send_event(&Event::CharInput('C'));
    assert!(handle.serialize().starts_with("# CAЗаголовок"), "{}", handle.serialize());
}

/// Draggable внутри врезки → drag дерева → Drop в DropArea той же врезки
/// (цепочка приложения: MouseDown, MouseMove, DragMove, Drop) — виджеты
/// живой врезки участвуют в переносе наравне с остальными.
#[test]
fn embed_drag_and_drop_reaches_drop_area() {
    use std::sync::atomic::{AtomicUsize, Ordering};
    use syngui::prelude::*;
    use syngui::widget::Widget;
    use syngui::widgets::input::document_editor::{EmbedCtx, EmbedFactory};
    use syngui::widgets::overlay::{Draggable, DropArea};

    struct DndFactory(Arc<AtomicUsize>);
    impl EmbedFactory for DndFactory {
        fn build(&self, _target: &str, _ctx: &EmbedCtx) -> Option<Box<dyn Widget>> {
            let drops = self.0.clone();
            Some(Box::new(
                Column::new()
                    .gap(20.0)
                    .child(Draggable::new("card", "b|k").child(Text::new("Карточка")))
                    .child(
                        DropArea::new()
                            .accept_types(vec!["card".to_string()])
                            .on_drop(move |_d| {
                                drops.fetch_add(1, Ordering::SeqCst);
                            })
                            .child(Text::new("Хвост колонки")),
                    ),
            ))
        }
        fn has_own_height(&self, _target: &str) -> bool {
            true
        }
    }

    let drops = Arc::new(AtomicUsize::new(0));
    let handle = DocumentEditorHandle::new();
    let mut h = TestHarness::new(Box::new(
        DocumentEditor::new()
            .markdown("Текст\n\n![[kanban:abc]]{h=300}\n")
            .handle(&handle)
            .embeds(Arc::new(DndFactory(drops.clone())))
            .layout(DocLayout { free: true, ..DocLayout::default() }),
    ));
    h.tree.text_measure = Some(Arc::new(Mono));
    h.rebuild();
    h.layout(800.0, 600.0);

    let drg = h.find_by_type_name("Draggable");
    let dra = h.find_by_type_name("DropArea");
    assert_eq!((drg.len(), dra.len()), (1, 1), "врезка не построилась");
    let bd = h.element_bounds(drg[0]);
    let ba = h.element_bounds(dra[0]);
    eprintln!("draggable {bd:?} droparea {ba:?}");
    assert!(bd.size.width > 0.0 && ba.size.height > 0.0);
    let from = Point::new(bd.origin.x + bd.size.width / 2.0, bd.origin.y + bd.size.height / 2.0);
    let to = Point::new(ba.origin.x + ba.size.width / 2.0, ba.origin.y + ba.size.height / 2.0);

    h.send_event(&Event::MouseDown { button: MouseButton::Left, position: from });
    h.send_event(&Event::MouseMove(Point::new(from.x + 12.0, from.y + 12.0)));
    assert!(h.tree.drag_state.is_some(), "drag не начался: MouseDown/MouseMove не дошли до Draggable");
    h.send_event(&Event::MouseMove(to));
    let data = h.tree.drag_state.as_ref().unwrap().data.clone();
    h.tree.dispatch_drag_event(&Event::DragMove { position: to, data: data.clone() });
    h.tree.dispatch_drag_event(&Event::Drop { position: to, data });
    h.send_event(&Event::DragEnd { cancelled: false });
    h.tree.drag_state = None;
    assert_eq!(drops.load(Ordering::SeqCst), 1, "Drop не дошёл до DropArea врезки");
}

/// Перенос блока за ⋮⋮ с `block_drag_type` — ещё и drag дерева: DropArea
/// врезки (доска на той же странице) получает дроп с id блока, хост
/// удаляет блок через `DocOp::DeleteBlock`, а редактор заканчивает жест
/// по `DragEnd` (MouseUp приложение при drag'е дерева не шлёт).
#[test]
fn block_drag_by_handle_drops_into_embed_drop_area() {
    use std::sync::Mutex;
    use syngui::prelude::*;
    use syngui::widget::Widget;
    use syngui::widgets::input::document_editor::{BlockId, EmbedCtx, EmbedFactory};
    use syngui::widgets::overlay::DropArea;

    struct SinkFactory(Arc<Mutex<Vec<String>>>);
    impl EmbedFactory for SinkFactory {
        fn build(&self, _target: &str, _ctx: &EmbedCtx) -> Option<Box<dyn Widget>> {
            let got = self.0.clone();
            Some(Box::new(
                DropArea::new()
                    .accept_types(vec!["doc-block".to_string()])
                    .on_drop(move |d| got.lock().unwrap().push(d.payload.clone()))
                    .child(Text::new("Колонка доски — сюда можно бросить блок")),
            ))
        }
        fn has_own_height(&self, _target: &str) -> bool {
            true
        }
    }

    let got = Arc::new(Mutex::new(Vec::new()));
    let handle = DocumentEditorHandle::new();
    let layout = DocLayout { free: true, ..DocLayout::default() };
    let mut h = TestHarness::new(Box::new(
        DocumentEditor::new()
            .markdown("Раз\n\nДва\n\n![[kanban:abc]]{h=120}\n")
            .handle(&handle)
            .embeds(Arc::new(SinkFactory(got.clone())))
            .block_drag_type("doc-block")
            .layout(layout),
    ));
    h.tree.text_measure = Some(Arc::new(Mono));
    h.rebuild();
    h.layout(800.0, 600.0);

    let areas = h.find_by_type_name("DropArea");
    assert_eq!(areas.len(), 1);
    let area = h.element_bounds(areas[0]);
    assert!(area.size.height > 0.0, "у DropArea врезки нет размера: {area:?}");

    // Наведение — ручка рисуется у блока под курсором.
    let row = h.element_bounds(h.find_by_type_name("doc-text-row")[1]);
    let inside = Point::new(row.origin.x + 5.0, row.origin.y + 5.0);
    h.send_event(&Event::MouseDown { button: MouseButton::Left, position: inside });
    h.send_event(&Event::MouseUp { button: MouseButton::Left, position: inside });
    h.send_event(&Event::MouseMove(inside));
    let mut list = DisplayList::new();
    h.tree.build_display_list(h.root_id, &mut list, Rect::new(Point::zero(), Size::new(800.0, 600.0)));

    let grip = Point::new(row.origin.x - 14.0, row.origin.y + 8.0);
    h.send_event(&Event::MouseDown { button: MouseButton::Left, position: grip });
    h.send_event(&Event::MouseMove(Point::new(grip.x + 40.0, grip.y + 30.0)));
    let drag = h.tree.drag_state.as_ref().expect("перенос блока должен объявить drag дерева");
    assert_eq!(drag.data.drag_type, "doc-block");
    assert!(!drag.data.ghost, "призрак не нужен — блок едет живьём");
    let block_id: u64 = drag.data.payload.parse().expect("payload — id блока");
    let data = drag.data.clone();

    // Как AppHandler: движение → DragMove целям, отпускание → Drop, DragEnd.
    let to = Point::new(area.origin.x + area.size.width / 2.0, area.origin.y + area.size.height / 2.0);
    h.send_event(&Event::MouseMove(to));
    h.tree.dispatch_drag_event(&Event::DragMove { position: to, data: data.clone() });
    h.tree.dispatch_drag_event(&Event::Drop { position: to, data });
    h.send_event(&Event::DragEnd { cancelled: false });
    h.tree.drag_state = None;

    assert_eq!(got.lock().unwrap().as_slice(), [block_id.to_string()], "DropArea должна получить id блока");
    // Хост забрал блок — удаляет его из документа.
    assert_eq!(handle.block_markdown(BlockId(block_id)).as_deref(), Some("Два\n"));
    handle.queue_op(DocOp::DeleteBlock(BlockId(block_id)));
    h.update_widget(Box::new(
        DocumentEditor::new()
            .markdown("Раз\n\nДва\n\n![[kanban:abc]]{h=120}\n")
            .handle(&handle)
            .embeds(Arc::new(SinkFactory(got.clone())))
            .block_drag_type("doc-block")
            .model_epoch(1)
            .layout(DocLayout { free: true, ..DocLayout::default() }),
    ));
    settle(&mut h);
    let md = handle.serialize();
    assert!(!md.contains("Два"), "блок должен уйти со страницы:\n{md}");
    assert!(md.contains("Раз"), "остальное на месте:\n{md}");
    // Жест закончен: движение мыши больше ничего не тащит.
    let before = handle.serialize();
    h.send_event(&Event::MouseMove(Point::new(to.x + 50.0, to.y + 50.0)));
    settle(&mut h);
    assert_eq!(handle.serialize(), before);
}

/// В потоке то же: с `block_drag_type` перенос объявляется drag'ом дерева, а
/// перестановка блоков доводится до конца по `DragEnd` вместо MouseUp.
#[test]
fn flow_block_drag_finishes_on_drag_end() {
    let handle = DocumentEditorHandle::new();
    let mut h = TestHarness::new(Box::new(
        DocumentEditor::new().markdown("раз\n\nдва\n\nтри\n").handle(&handle).block_drag_type("doc-block"),
    ));
    h.tree.text_measure = Some(Arc::new(Mono));
    h.rebuild();
    h.layout(800.0, 600.0);
    let click = Point::new(X0 + 10.0, Y0 + 8.0);
    h.send_event(&Event::MouseDown { button: MouseButton::Left, position: click });
    h.send_event(&Event::MouseUp { button: MouseButton::Left, position: click });
    h.send_event(&Event::MouseMove(click));
    let mut list = DisplayList::new();
    h.tree.build_display_list(h.root_id, &mut list, Rect::new(Point::zero(), Size::new(800.0, 2000.0)));
    let grab = Point::new(4.0, Y0 + 8.0);
    h.send_event(&Event::MouseDown { button: MouseButton::Left, position: grab });
    let drop = Point::new(X0 + 10.0, Y0 + 3.0 * 33.0);
    h.send_event(&Event::MouseMove(drop));
    assert!(h.tree.drag_state.is_some(), "перенос в потоке должен объявить drag дерева");
    h.send_event(&Event::DragEnd { cancelled: false });
    h.tree.drag_state = None;
    settle(&mut h);
    assert_eq!(handle.serialize(), "два\n\nтри\n\nраз\n");
}

/// Встроенный редактор карточки: `plain` (без ручки/подсветки) и
/// подсказки в пустом заголовке и абзаце рисуются без паники, а набор в
/// пустом заголовке их заменяет.
#[test]
fn plain_editor_with_placeholders_renders_and_edits() {
    let handle = DocumentEditorHandle::new();
    let mut h = TestHarness::new(Box::new(
        DocumentEditor::new()
            .markdown("## \n")
            .handle(&handle)
            .plain(true)
            .heading_placeholder("Заголовок")
            .placeholder("Описание")
            .autofocus(true),
    ));
    h.tree.text_measure = Some(Arc::new(Mono));
    h.rebuild();
    h.layout(300.0, 200.0);
    let mut list = DisplayList::new();
    h.tree.build_display_list(h.root_id, &mut list, Rect::new(Point::zero(), Size::new(300.0, 200.0)));
    // Клик в пустой заголовок (харнес фокус сам не переводит): набор +
    // Enter → абзац.
    let row = h.element_bounds(h.find_by_type_name("doc-text-row")[0]);
    let at = Point::new(row.origin.x + 4.0, row.origin.y + row.size.height / 2.0);
    h.send_event(&Event::MouseDown { button: MouseButton::Left, position: at });
    h.send_event(&Event::MouseUp { button: MouseButton::Left, position: at });
    h.send_events(&[Event::CharInput('З'), Event::CharInput('а'), Event::KeyDown(Key::Enter), Event::KeyUp(Key::Enter)]);
    settle(&mut h);
    h.send_events(&[Event::CharInput('т')]);
    settle(&mut h);
    assert_eq!(handle.serialize(), "## За\n\nт\n");
    let mut list = DisplayList::new();
    h.tree.build_display_list(h.root_id, &mut list, Rect::new(Point::zero(), Size::new(300.0, 200.0)));
}
