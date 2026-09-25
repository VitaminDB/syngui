//! Регрессия: правила с `:disabled` попадали в базовый слой — каскад не знал
//! этого псевдокласса и считал его «оконным» без флага, поэтому
//! `Button:disabled { color: … }` красил все кнопки, включённые тоже
//! (и перебивал `.class { color }` по специфичности).

use syngui::prelude::*;
use syngui::testing::*;

const MSS: &str = "
Button { color: #ffffff; background: #202020; &:disabled { color: #535a68; background: #101010; } }
.chip { color: #ff0000; }
";

fn tree() -> Box<dyn Widget> {
    Box::new(Column::new().child(Button::new("A").class("chip")).child(Button::new("B")))
}

fn check(h: &TestHarness) {
    let buttons = h.find_by_type_name("Button");
    assert_eq!(buttons.len(), 2);
    let chip = h.element_mss(buttons[0]).unwrap();
    assert_eq!(chip.color, Some(Color::from_hex("#ff0000")), ".chip не должен перебиваться :disabled");
    let plain = h.element_mss(buttons[1]).unwrap();
    assert_eq!(plain.color, Some(Color::from_hex("#ffffff")));
    assert_eq!(plain.background_color, Some(Color::from_hex("#202020")));
    // Слой :disabled сохранён отдельно.
    let d = plain.style_disabled.as_ref().expect("нет слоя :disabled");
    assert_eq!(d.color(), Some(Color::from_hex("#535a68")));
}

#[test]
fn disabled_rules_stay_out_of_base_full_cascade() {
    let mut h = TestHarness::new(tree());
    h.apply_mss(MSS);
    h.layout(400.0, 300.0);
    check(&h);
}

#[test]
fn disabled_rules_stay_out_of_base_dirty_path() {
    let mut h = TestHarness::new(tree());
    h.apply_mss_dirty(MSS);
    h.layout(400.0, 300.0);
    check(&h);
}
