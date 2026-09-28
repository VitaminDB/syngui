use std::time::Duration;
use syngui::prelude::*;
use syngui::testing::*;

const MSS: &str = "
.td-tile { width: 130px; height: 130px; border-radius: 18px; background: linear-gradient(135deg, #6a8dff, #b86bff);
  transition: rotate-y 700ms ease-out-back, rotate-x 400ms ease-out-cubic, translate-z 300ms ease-out-back; }
.td-tilt { perspective: 300px; }
.td-tilt:hover { rotate-x: 22deg; rotate-y: -24deg; }
";

#[test]
fn hover_starts_3d_transition() {
    let mut h = TestHarness::new(Box::new(DecoratedBox::new().class("td-tile td-tilt")));
    h.apply_mss(MSS);
    h.layout(400.0, 400.0);
    let id = h.find_by_class("td-tile")[0];
    h.send_event(&Event::MouseMove(Point::new(50.0, 50.0)));
    h.animate(Duration::from_millis(100));
    let mss = h.element_mss(id).unwrap();
    let t = mss.projected_layer_params(Rect::new(Point::zero(), Size::new(130.0, 130.0)));
    assert!(h.is_animating(id), "наведение должно запустить переход");
    assert!(t.is_some(), "должен быть 3D-слой при наведении");
}

/// Страница переключателя не пустеет после ухода предыдущей.
#[test]
fn switcher_keeps_content_after_exit() {
    let key = use_signal(1u64);
    let root = DecoratedBox::new().child(move || {
        let k = key.get();
        AnimatedSwitcher::new(k, move || Box::new(Text::new(format!("page {k}")).class("pg")))
            .duration_ms(100)
            .exit_duration_ms(100)
    });
    let mut h = TestHarness::new(Box::new(root));
    h.rebuild();
    h.layout(400.0, 400.0);
    assert_eq!(h.find_by_class("pg").len(), 1);
    key.set(2);
    h.rebuild();
    h.layout(400.0, 400.0);
    assert_eq!(h.find_by_class("pg").len(), 2, "во время перехода видны обе страницы");
    for _ in 0..10 {
        h.animate(Duration::from_millis(50));
        h.layout(400.0, 400.0);
    }
    assert_eq!(h.find_by_class("pg").len(), 1, "после ухода остаётся текущая страница");
}
