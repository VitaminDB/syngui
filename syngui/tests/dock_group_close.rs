//! Пустую именованную группу можно закрыть крестиком в заголовке.

use syngui::testing::*;
use syngui::widgets::{DockArea, DockSpec, DockState};

#[test]
fn empty_named_group_has_close_button() {
    let state = DockState::new(DockSpec::row(DockSpec::tabs(Vec::<String>::new()).named("docs"), DockSpec::tabs(["ai"]), 0.5));
    let w = DockArea::new(state).group_closable(|g| g == "docs").build();
    let mut h = TestHarness::new(Box::new(w));
    h.rebuild();
    h.layout(800.0, 600.0);
    eprintln!("tabs={} headers={} drag={}", h.find_by_class("dock-tab").len(), h.find_by_class("dock-header").len(), h.find_by_type_name("Draggable").len());
    let closes = h.find_by_class("dock-group-close");
    assert_eq!(closes.len(), 1, "крестик у пустой области");
    let b = h.element_bounds(closes[0]);
    assert!(b.size.width > 0.0 && b.size.height > 0.0, "{b:?}");
    h.send_events(&click_at(syngui::core::Point::new(b.origin.x + b.size.width / 2.0, b.origin.y + b.size.height / 2.0)));
    h.rebuild();
    assert_eq!(state.spec(), Some(DockSpec::tabs(["ai"])));
}
