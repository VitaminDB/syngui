//! Smoke-тест каркаса DocumentEditor (этап S2): read-only рендер —
//! построение детей по блокам, раскладка, отрисовка без паники.

use std::sync::Arc;

use syngui::core::{Point, Rect, Size};
use syngui::render::DisplayList;
use syngui::testing::TestHarness;
use syngui::widget::context::TextMeasure;
use syngui::widgets::input::document_editor::DocumentEditor;

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
