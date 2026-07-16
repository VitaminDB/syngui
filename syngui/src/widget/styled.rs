use super::{Element, Widget, super::ElementTree, super::ElementId};
use crate::mss::{StyleContext, ComputedStyle, StyleValue};
use std::any::Any;

pub trait WidgetExt: Widget + Sized + 'static {
    fn class(self, class: impl Into<String>) -> StyledWidget<Self> {
        StyledWidget::new(self).class(class)
    }

    fn classes(self, classes: Vec<String>) -> StyledWidget<Self> {
        let mut styled = StyledWidget::new(self);
        for class in classes {
            styled = styled.class(class);
        }
        styled
    }

    fn style(self, prop: impl Into<String>, value: impl Into<StyleValue>) -> StyledWidget<Self> {
        StyledWidget::new(self).style(prop, value)
    }

    fn named(self, name: impl Into<String>) -> crate::widgets::containers::named::Named {
        crate::widgets::containers::named::Named::new(name, self)
    }
}

impl<T: Widget + Sized + 'static> WidgetExt for T {}

pub struct StyledWidget<W: Widget> {
    inner: W,
    classes: Vec<String>,
    id: Option<String>,
    inline_styles: Vec<(String, StyleValue)>,
}

impl<W: Widget> StyledWidget<W> {
    pub fn new(inner: W) -> Self {
        Self {
            inner,
            classes: Vec::new(),
            id: None,
            inline_styles: Vec::new(),
        }
    }

    pub fn class(mut self, class: impl Into<String>) -> Self {
        let input = class.into();
        for c in input.split_whitespace() {
            let s = c.to_string();
            if !self.classes.contains(&s) {
                self.classes.push(s);
            }
        }
        self
    }

    pub fn id(mut self, id: impl Into<String>) -> Self {
        self.id = Some(id.into());
        self
    }

    pub fn style(mut self, prop: impl Into<String>, value: impl Into<StyleValue>) -> Self {
        self.inline_styles.push((prop.into(), value.into()));
        self
    }

    pub fn get_classes(&self) -> &[String] {
        &self.classes
    }

    pub fn get_id(&self) -> Option<&str> {
        self.id.as_deref()
    }

    pub fn style_context(&self, element_type: &str) -> StyleContext {
        let mut ctx = StyleContext::default()
            .with_element_type(element_type);
        
        if let Some(id) = &self.id {
            ctx = ctx.with_id(id.clone());
        }
        
        for class in &self.classes {
            ctx.add_class(class.clone());
        }
        
        ctx
    }
}

impl<W: Widget> Widget for StyledWidget<W> {
    fn create_element(&self) -> Box<dyn Element> {
        let mut element = self.inner.create_element();
        if !self.classes.is_empty() {
            element.set_classes(self.classes.clone());
        }
        if !self.inline_styles.is_empty() {
            element.set_inline_styles(self.inline_styles.clone());
        }
        element
    }

    fn can_update(&self, other: &dyn Any) -> bool {
        if let Some(other) = other.downcast_ref::<Self>() {
            self.inner.can_update(&other.inner)
        } else {
            self.inner.can_update(other)
        }
    }

    fn as_any(&self) -> &dyn Any {
        self.inner.as_any()
    }

    fn as_any_mut(&mut self) -> &mut dyn Any {
        self.inner.as_any_mut()
    }

    fn mount(&self, tree: &mut ElementTree, parent_id: ElementId) {
        if !self.inline_styles.is_empty() {
            tree.set_node_inline_styles(parent_id, self.inline_styles.clone());
        }
        self.inner.mount(tree, parent_id);
    }

    fn child_widgets(&self) -> Vec<&dyn Widget> {
        self.inner.child_widgets()
    }

    fn widget_classes(&self) -> &[String] {
        &self.classes
    }

    fn widget_inline_styles(&self) -> &[(String, StyleValue)] {
        &self.inline_styles
    }
}

impl StyledWidget<crate::widgets::containers::DecoratedBox> {
    pub fn child<M>(mut self, child: impl crate::widgets::containers::IntoWidget<M>) -> Self {
        self.inner = self.inner.child(child);
        self
    }

    pub fn clip(mut self, c: bool) -> Self {
        self.inner = self.inner.clip(c);
        self
    }
}

pub trait StyledElement {
    fn apply_style(&mut self, style: &ComputedStyle);

    fn classes(&self) -> &[String];

    fn set_classes(&mut self, classes: Vec<String>);
}

pub fn apply_computed_style(element: &mut dyn StyledElement, style: &ComputedStyle) {
    element.apply_style(style);
}
