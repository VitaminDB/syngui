//! Плавная смена темы: после замены таблицы стилей цвета перетекают у
//! элементов без собственного `transition` (фон карточки, цвет текста), а
//! не меняются скачком.

use std::time::Duration;
use syngui::animation::Easing;
use syngui::prelude::*;
use syngui::testing::*;

const LIGHT: &str = "
.card { width: 80px; height: 40px; background: #ffffff; }
.label { color: #000000; }
";
const DARK: &str = "
.card { width: 80px; height: 40px; background: #000000; }
.label { color: #ffffff; }
";

fn tree() -> Box<dyn Widget> {
    Box::new(DecoratedBox::new().child(Text::new("x").class("label")).class("card"))
}

#[test]
fn colors_flow_after_stylesheet_swap() {
    let mut h = TestHarness::new(tree());
    let light = h.apply_mss(LIGHT);
    h.layout(200.0, 200.0);
    let _ = light;

    let card = h.find_by_class("card")[0];
    let label = h.find_by_class("label")[0];

    h.restyle(DARK, Some((Duration::from_millis(300), Easing::Linear)));
    assert!(h.is_animating(card), "фон карточки должен перетекать");
    assert!(h.is_animating(label), "цвет текста должен перетекать");

    h.animate(Duration::from_millis(150));
    let mid = h.element_mss(card).and_then(|m| m.transition.background_color()).expect("промежуточный фон");
    let lum = mid.r + mid.g + mid.b;
    assert!(lum > 0.05 && lum < 2.95, "на середине перехода фон серый, а не чёрный/белый: {mid:?}");

    assert!(h.animate(Duration::from_millis(400)));
    assert!(!h.is_animating(card));
    assert!(!h.is_animating(label));
    assert!(h.element_mss(card).and_then(|m| m.transition.background_color()).is_none());
}

#[test]
fn swap_without_transition_is_instant() {
    let mut h = TestHarness::new(tree());
    h.apply_mss(LIGHT);
    h.layout(200.0, 200.0);
    let card = h.find_by_class("card")[0];
    h.restyle(DARK, None);
    assert!(!h.is_animating(card));
}
