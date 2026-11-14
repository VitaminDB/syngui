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

/// После перестройки дерева (закрыли группу, перенесли вкладку) новые группы
/// получают стили — фон-карточка не пропадает.
#[test]
fn groups_keep_styles_after_rebuild() {
    let state = DockState::new(DockSpec::row(
        DockSpec::tabs(Vec::<String>::new()).named("docs"),
        DockSpec::column(DockSpec::tabs(["ai"]), DockSpec::tabs(["term"]), 0.6),
        0.5,
    ));
    let w = DockArea::new(state).group_closable(|g| g == "docs").build();
    let mut h = TestHarness::new(Box::new(w));
    let engine = h.apply_mss(".dock-group { background-color: #ff0000; }");
    h.frame(Some(&engine), 800.0, 600.0);
    let styled = |h: &TestHarness| {
        let ids = h.find_by_class("dock-group");
        let ok = ids.iter().filter(|&&id| h.element_mss(id).and_then(|m| m.background_color).is_some()).count();
        (ids.len(), ok)
    };
    assert_eq!(styled(&h), (3, 3));
    state.close_group("docs");
    h.frame(Some(&engine), 800.0, 600.0);
    assert_eq!(styled(&h), (2, 2), "после закрытия группы");
    state.move_to_edge("term", syngui::widgets::DropZone::Right);
    h.frame(Some(&engine), 800.0, 600.0);
    assert_eq!(styled(&h), (2, 2), "после переноса к краю");
}
