use crate::widget::Element;
use std::any::Any;

pub trait Widget: Any {
    fn create_element(&self) -> Box<dyn Element>;

    fn can_update(&self, other: &dyn Any) -> bool;

    fn as_any(&self) -> &dyn Any;

    fn as_any_mut(&mut self) -> &mut dyn Any;

    fn mount(&self, tree: &mut super::ElementTree, parent_id: super::ElementId);

    fn child_widgets(&self) -> Vec<&dyn Widget> { vec![] }

    fn widget_classes(&self) -> &[String] {
        static EMPTY: &[String] = &[];
        EMPTY
    }

    fn widget_inline_styles(&self) -> &[(String, crate::mss::StyleValue)] {
        &[]
    }
}
