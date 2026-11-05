//! Удалённый экран (LiveView): двойной щелчок доходит до гостя как два нажатия и два отпускания.
//! syngui присылает второе нажатие как `Event::DoubleClick` — раньше LiveView его терял.

use std::sync::{Arc, Mutex};

use syngui::prelude::*;
use syngui::testing::TestHarness;
use syngui::widgets::visual::{LiveFrame, LiveInput, LiveView};

#[test]
fn double_click_reaches_guest_as_two_presses() {
    let got: Arc<Mutex<Vec<String>>> = Arc::default();
    let g = got.clone();
    let view = LiveView::new(LiveFrame::new()).on_input(move |e| {
        let s = match e {
            LiveInput::Down { .. } => "down",
            LiveInput::Up { .. } => "up",
            _ => return,
        };
        g.lock().unwrap().push(s.into());
    });
    let mut h = TestHarness::new(Box::new(view));
    h.layout(400.0, 300.0);
    let p = Point::new(100.0, 100.0);
    h.send_events(&[
        Event::MouseDown { button: MouseButton::Left, position: p },
        Event::MouseUp { button: MouseButton::Left, position: p },
        Event::DoubleClick { button: MouseButton::Left, position: p },
        Event::MouseUp { button: MouseButton::Left, position: p },
    ]);
    assert_eq!(*got.lock().unwrap(), ["down", "up", "down", "up"]);
}
