#[cfg(feature = "accessibility")]
mod inner {
    use crate::a11y::platform::PlatformAdapter;
    use crate::a11y::types::*;
    use hashbrown::HashMap;
    use std::any::Any;

    fn map_role(role: &Role) -> accesskit::Role {
        match role {
            Role::Document => accesskit::Role::Document,
            Role::Application => accesskit::Role::Application,
            Role::Group => accesskit::Role::Group,
            Role::Button => accesskit::Role::Button,
            Role::CheckBox => accesskit::Role::CheckBox,
            Role::RadioButton => accesskit::Role::RadioButton,
            Role::TextField => accesskit::Role::TextInput,
            Role::Slider => accesskit::Role::Slider,
            Role::ScrollBar => accesskit::Role::ScrollBar,
            Role::ProgressBar => accesskit::Role::ProgressIndicator,
            Role::ComboBox => accesskit::Role::ComboBox,
            Role::ListBox => accesskit::Role::ListBox,
            Role::Menu => accesskit::Role::Menu,
            Role::MenuBar => accesskit::Role::MenuBar,
            Role::Tree => accesskit::Role::Tree,
            Role::TabList => accesskit::Role::TabList,
            Role::StaticText => accesskit::Role::TextRun,
            Role::Heading(_) => accesskit::Role::Heading,
            Role::Paragraph => accesskit::Role::Paragraph,
            Role::Link => accesskit::Role::Link,
            Role::Image => accesskit::Role::Image,
            Role::Terminal => accesskit::Role::Terminal,
            Role::None | Role::Presentation => accesskit::Role::Unknown,
        }
    }

    fn to_node_id(id: A11yId) -> accesskit::NodeId {
        accesskit::NodeId(id.0)
    }

    fn build_accesskit_node(node: &A11yNode) -> accesskit::Node {
        let mut ak_node = accesskit::Node::new(map_role(&node.role));

        if let Some(ref label) = node.properties.label {
            ak_node.set_label(label.as_str());
        }

        if let Some(ref value) = node.properties.value {
            ak_node.set_value(value.as_str());
        }

        if let Some(ref desc) = node.properties.description {
            ak_node.set_description(desc.as_str());
        }

        if node.state.disabled {
            ak_node.set_disabled();
        }

        if node.state.hidden {
            ak_node.set_hidden();
        }

        if let Some(checked) = node.state.checked {
            ak_node.set_toggled(if checked {
                accesskit::Toggled::True
            } else {
                accesskit::Toggled::False
            });
        }

        if let Some(expanded) = node.state.expanded {
            ak_node.set_expanded(expanded);
        }

        if node.state.selected {
            ak_node.set_selected(true);
        }

        let b = node.bounds;
        ak_node.set_bounds(accesskit::Rect::new(
            b.origin.x as f64,
            b.origin.y as f64,
            (b.origin.x + b.size.width) as f64,
            (b.origin.y + b.size.height) as f64,
        ));

        let children: Vec<accesskit::NodeId> =
            node.children.iter().map(|id| to_node_id(*id)).collect();
        ak_node.set_children(children);

        // Что скринридер может сделать с узлом: без объявленных действий он
        // не предлагает «активировать» даже кнопку.
        match node.role {
            Role::Button | Role::CheckBox | Role::RadioButton | Role::Link | Role::ComboBox => {
                ak_node.add_action(accesskit::Action::Click);
                ak_node.add_action(accesskit::Action::Focus);
            }
            Role::TextField | Role::Terminal => {
                ak_node.add_action(accesskit::Action::Focus);
                ak_node.add_action(accesskit::Action::ReplaceSelectedText);
            }
            Role::Slider | Role::ListBox | Role::Tree | Role::TabList => {
                ak_node.add_action(accesskit::Action::Focus);
            }
            _ => {}
        }

        if let Some(ref shortcut) = node.properties.keyboard_shortcut {
            ak_node.set_keyboard_shortcut(shortcut.as_str());
        }

        ak_node
    }

    #[cfg(test)]
    pub(super) fn build_accesskit_node_for_test(node: &A11yNode) -> accesskit::Node {
        build_accesskit_node(node)
    }

    pub struct AccessKitAdapter {
        pending_update: Option<accesskit::TreeUpdate>,
        focused_id: Option<accesskit::NodeId>,
        root_id: Option<accesskit::NodeId>,
    }

    impl AccessKitAdapter {
        pub fn new() -> Self {
            Self {
                pending_update: None,
                focused_id: None,
                root_id: None,
            }
        }

        pub fn take_pending_update(&mut self) -> Option<accesskit::TreeUpdate> {
            self.pending_update.take()
        }

        pub fn build_initial_tree(&self) -> Option<accesskit::TreeUpdate> {
            self.pending_update.clone()
        }

        pub fn root_node_id(&self) -> Option<accesskit::NodeId> {
            self.root_id
        }
    }

    impl PlatformAdapter for AccessKitAdapter {
        fn tree_updated(&mut self, nodes: &HashMap<A11yId, A11yNode>, root: Option<A11yId>) {
            let mut ak_nodes = Vec::with_capacity(nodes.len());

            for (a11y_id, node) in nodes.iter() {
                let ak_id = to_node_id(*a11y_id);
                let ak_node = build_accesskit_node(node);
                ak_nodes.push((ak_id, ak_node));
            }

            let root_id = root.map(to_node_id);
            self.root_id = root_id;

            if let Some(root_id) = root_id {
                let focus = self.focused_id.unwrap_or(root_id);
                let update = accesskit::TreeUpdate {
                    nodes: ak_nodes,
                    tree: Some(accesskit::Tree::new(root_id)),
                    tree_id: accesskit::TreeId::ROOT,
                    focus,
                };
                self.pending_update = Some(update);
            }
        }

        fn node_state_changed(&mut self, _node_id: A11yId, _state: &NodeState) {}

        fn focus_moved(&mut self, node_id: A11yId) {
            let ak_id = to_node_id(node_id);
            self.focused_id = Some(ak_id);
        }

        fn value_changed(&mut self, _node_id: A11yId, _value: &str) {}

        fn announce(&mut self, _message: &str, _priority: LiveRegion) {}

        fn as_any_mut(&mut self) -> &mut dyn Any {
            self
        }
    }
}

#[cfg(feature = "accessibility")]
pub use inner::AccessKitAdapter;

#[cfg(all(test, feature = "accessibility"))]
mod tests {
    use crate::a11y::types::*;

    fn node(role: Role) -> A11yNode {
        A11yNode {
            id: A11yId(5),
            role,
            state: NodeState::default(),
            properties: NodeProperties::default(),
            parent: None,
            children: vec![],
            element_id: crate::widget::ElementId(5),
            bounds: crate::core::Rect::default(),
        }
    }

    #[test]
    fn buttons_and_fields_declare_actions() {
        use accesskit::Action;
        let b = super::inner::build_accesskit_node_for_test(&node(Role::Button));
        assert!(b.supports_action(Action::Click) && b.supports_action(Action::Focus));
        let t = super::inner::build_accesskit_node_for_test(&node(Role::TextField));
        assert!(t.supports_action(Action::ReplaceSelectedText) && !t.supports_action(Action::Click));
        let g = super::inner::build_accesskit_node_for_test(&node(Role::Group));
        assert!(!g.supports_action(Action::Click));
    }

    #[test]
    fn a11y_id_is_stable_element_id() {
        let e = crate::widget::ElementId(42);
        assert_eq!(A11yId::for_element(e), A11yId::for_element(e));
        assert_eq!(A11yId::for_element(e).0, 42);
    }
}
