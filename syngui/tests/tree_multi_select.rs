//! Дерево с `SelectionMode::Multiple` выделяет как файловый менеджер:
//! щелчок — одна строка, Ctrl+щелчок — добавить/убрать, Shift+щелчок —
//! диапазон, Ctrl+A — всё; правый щелчок по выделенному выделение не сбрасывает.

use std::sync::{Arc, Mutex};

use syngui::core::Point;
use syngui::input::{Event, Key, Modifiers, MouseButton};
use syngui::testing::*;
use syngui::widgets::containers::Stack;
use syngui::widgets::data::{SelectionMode, TreeNode, TreeView};

fn click(h: &mut TestHarness, row: usize, m: Modifiers, button: MouseButton) {
    h.tree.modifiers = m;
    let p = Point::new(60.0, 16.0 + 32.0 * row as f32);
    h.send_event(&Event::MouseMove(p));
    h.send_event(&Event::MouseDown { button, position: p });
    h.send_event(&Event::MouseUp { button, position: p });
    h.tree.modifiers = Modifiers::default();
}

#[test]
fn file_manager_style_selection() {
    let sel: Arc<Mutex<Vec<String>>> = Arc::new(Mutex::new(Vec::new()));
    let opened: Arc<Mutex<Vec<String>>> = Arc::new(Mutex::new(Vec::new()));
    let (s2, o2) = (sel.clone(), opened.clone());
    let tv = TreeView::new(["a", "b", "c", "d"].iter().map(|n| TreeNode::leaf(*n, *n)).collect())
        .selection_mode(SelectionMode::Multiple)
        .on_selection_change(move |ids| *s2.lock().unwrap() = ids.to_vec())
        .on_select(move |id| o2.lock().unwrap().push(id.to_string()))
        .on_context(|_| {});
    let mut h = TestHarness::new(Box::new(Stack::new().child(tv)));
    h.frame(None, 300.0, 400.0);
    let ctrl = Modifiers { ctrl: true, ..Default::default() };
    let shift = Modifiers { shift: true, ..Default::default() };
    let get = || sel.lock().unwrap().clone();

    click(&mut h, 0, Modifiers::default(), MouseButton::Left);
    assert_eq!(get(), ["a"]);
    click(&mut h, 2, ctrl, MouseButton::Left);
    assert_eq!(get(), ["a", "c"]);
    click(&mut h, 3, shift, MouseButton::Left);
    assert_eq!(get(), ["c", "d"], "Shift — диапазон от последнего щелчка");
    click(&mut h, 3, ctrl, MouseButton::Left);
    assert_eq!(get(), ["c"], "Ctrl+щелчок убирает выделенное");
    click(&mut h, 2, shift, MouseButton::Left);
    assert_eq!(get(), ["c", "d"], "диапазон — от строки последнего Ctrl+щелчка");
    // правый щелчок по выделенному — выделение то же
    click(&mut h, 3, Modifiers::default(), MouseButton::Right);
    assert_eq!(get(), ["c", "d"]);
    click(&mut h, 1, Modifiers::default(), MouseButton::Left);
    assert_eq!(get(), ["b"], "обычный щелчок — одна строка");
    h.tree.modifiers = ctrl;
    h.send_event(&Event::KeyDown(Key::A));
    h.tree.modifiers = Modifiers::default();
    assert_eq!(get(), ["a", "b", "c", "d"]);
    // открытие (on_select) — только от обычных щелчков
    assert_eq!(*opened.lock().unwrap(), ["a", "b"]);
}
