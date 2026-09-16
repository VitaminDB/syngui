//! Кадр, на котором переход завершился, всё равно нужно нарисовать.
//!
//! После простоя `dt` большой, и переход в 160 мс заканчивается на первом же
//! тике. Если такой тик не просит перерисовку, на экране остаётся значение,
//! с которого переход начинался: подсветка меню «залипала» на прежнем пункте.

use std::time::Duration;
use syngui::prelude::*;
use syngui::testing::*;

const MSS: &str = "
.box { width: 50px; height: 50px; background: rgba(255, 92, 77, 0); transition: background 160ms ease-out; }
.box-on { background: rgba(255, 92, 77, 1); }
";

#[test]
fn finished_transition_still_asks_for_a_frame() {
    let mut h = TestHarness::new(Box::new(DecoratedBox::new().class("box")));
    let engine = h.apply_mss(MSS);
    h.layout(200.0, 200.0);

    let id = h.find_by_class("box")[0];
    h.set_classes(id, vec!["box".to_string(), "box-on".to_string()]);
    h.apply_styles_dirty(&engine);
    assert!(h.is_animating(id), "смена класса должна запустить переход");

    assert!(
        h.animate(Duration::from_millis(500)),
        "тик, доигравший переход до конца, обязан запросить кадр"
    );
    assert!(!h.is_animating(id), "переход закончился — реестр его отпускает");
}
