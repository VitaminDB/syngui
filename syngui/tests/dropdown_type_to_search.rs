use std::sync::{Arc, Mutex};

use syngui::core::Point;
use syngui::input::{Event, Key, MouseButton};
use syngui::testing::*;
use syngui::widgets::containers::Stack;
use syngui::widgets::input::{Dropdown, DropdownItem};

fn harness(chosen: Arc<Mutex<String>>) -> TestHarness {
    let items: Vec<DropdownItem> = ["Котельная №2", "ТЭЦ-1", "Котельная №3", "ТЭЦ-2", "Школа"]
        .iter()
        .map(|s| DropdownItem::simple(*s))
        .collect();
    let widget = Stack::new().child(
        Dropdown::new()
            .items(items)
            .width(240.0)
            .on_change(move |v: &str| *chosen.lock().unwrap() = v.to_string()),
    );
    let mut h = TestHarness::new(Box::new(widget));
    h.layout(800.0, 600.0);
    h
}

fn open(h: &mut TestHarness) {
    h.send_event(&Event::MouseDown {
        button: MouseButton::Left,
        position: Point::new(20.0, 20.0),
    });
    h.layout(800.0, 600.0);
}

#[test]
fn typing_filters_and_enter_picks_first_match() {
    let chosen = Arc::new(Mutex::new(String::new()));
    let mut h = harness(chosen.clone());
    open(&mut h);
    h.send_events(&type_text("тэц"));
    h.send_events(&press_key(Key::Down));
    h.send_events(&press_key(Key::Enter));
    assert_eq!(*chosen.lock().unwrap(), "ТЭЦ-2");
}

#[test]
fn backspace_widens_filter() {
    let chosen = Arc::new(Mutex::new(String::new()));
    let mut h = harness(chosen.clone());
    open(&mut h);
    h.send_events(&type_text("кот3"));
    h.send_events(&press_key(Key::Enter));
    assert_eq!(
        *chosen.lock().unwrap(),
        "",
        "нет совпадений — выбирать нечего"
    );
    h.send_events(&press_key(Key::Backspace));
    h.send_events(&press_key(Key::Enter));
    assert_eq!(*chosen.lock().unwrap(), "Котельная №2");
}

#[test]
fn escape_clears_query_before_closing() {
    let chosen = Arc::new(Mutex::new(String::new()));
    let mut h = harness(chosen.clone());
    open(&mut h);
    h.send_events(&type_text("шк"));
    h.send_events(&press_key(Key::Escape));
    h.send_events(&press_key(Key::Enter));
    assert_eq!(*chosen.lock().unwrap(), "Котельная №2");
}
