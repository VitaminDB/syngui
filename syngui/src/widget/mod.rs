pub mod basic;
pub mod context;
pub mod dirty;
pub mod element;
pub mod selection;
pub mod styled;
pub mod tree;
pub mod visitor;
pub mod widget;

pub(crate) use basic::count_visual_lines_via_measure;
pub use basic::{Center, Elide, Text};
pub use context::{BuildContext, EventContext, UpdateContext};
pub use dirty::DirtyFlags;
pub use element::{BoxAlign, ChildHit, Element, EventContextExt, LayoutHint};
pub use styled::{StyledElement, StyledWidget, WidgetExt};
pub use tree::{DragState, ElementId, ElementTree, OverlayEntry, RenderHandle};
pub use visitor::ElementVisitor;
pub use widget::Widget;

/// Добавить классы из строки `"a b c"` — как `StyledWidget::class`: строка
/// делится по пробелам, повторы пропускаются. Раньше у большинства виджетов
/// `.class("a b")` становился одним классом `"a b"`, и селекторы `.a`/`.b`
/// его не видели.
pub fn push_classes(classes: &mut Vec<String>, input: String) {
    for c in input.split_whitespace() {
        if !classes.iter().any(|x| x == c) {
            classes.push(c.to_string());
        }
    }
}
