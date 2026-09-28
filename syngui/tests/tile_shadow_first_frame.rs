//! Тень плитки с `transition` на 3D-свойствах должна быть и в первом
//! кадре, а не появляться после первой перерисовки.
use std::time::Duration;
use syngui::prelude::*;
use syngui::render::DrawCommand;
use syngui::testing::*;

const MSS: &str = "
.td-tile { width: 130px; height: 130px; border-radius: 18px; background: linear-gradient(135deg, #6a8dff, #b86bff);
  box-shadow: 0 10px 26px #00000044;
  transition: rotate-y 700ms ease-out-back, rotate-x 400ms ease-out-cubic, translate-z 300ms ease-out-back; }
.td-tilt { perspective: 300px; }
.td-tilt:hover { rotate-x: 22deg; rotate-y: -24deg; }
";

fn shadows(list: &syngui::render::DisplayList) -> usize {
    list.commands().iter().filter(|c| matches!(c, DrawCommand::Shadow { .. })).count()
}

#[test]
fn shadow_present_in_first_frame() {
    let mut h = TestHarness::new(Box::new(
        Row::new().gap(24.0).child(DecoratedBox::new().class("td-tile td-flip")).child(DecoratedBox::new().class("td-tile td-tilt")),
    ));
    h.apply_mss(MSS);
    h.layout(600.0, 300.0);
    let first = shadows(&h.paint());
    h.animate(Duration::from_millis(16));
    let second = shadows(&h.paint());
    assert_eq!(first, 2, "в первом кадре у обеих плиток есть тень (потом: {second})");
    assert_eq!(second, 2);
}
