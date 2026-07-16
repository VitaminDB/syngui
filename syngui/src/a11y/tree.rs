use hashbrown::HashMap;
use crate::widget::{ElementId, ElementTree};
use super::types::*;
use super::platform::PlatformAdapter;
#[cfg(feature = "accessibility")]
use super::accesskit_adapter::AccessKitAdapter;

pub struct A11yTree {
    nodes: HashMap<A11yId, A11yNode>,
    root: Option<A11yId>,
    focused_node: Option<A11yId>,
    element_to_a11y: HashMap<ElementId, A11yId>,
    platform: Box<dyn PlatformAdapter>,
}

impl A11yTree {
    pub fn new(platform: Box<dyn PlatformAdapter>) -> Self {
        Self {
            nodes: HashMap::new(),
            root: None,
            focused_node: None,
            element_to_a11y: HashMap::new(),
            platform,
        }
    }

    pub fn sync(&mut self, element_tree: &ElementTree, root_id: ElementId) {
        self.nodes.clear();
        self.element_to_a11y.clear();

        let mut top_level = Vec::new();
        self.build_node(element_tree, root_id, None, &mut top_level);

        self.root = if top_level.len() == 1 {
            Some(top_level[0])
        } else if top_level.len() > 1 {
            let root_a11y = A11yId::new();
            for &child_id in &top_level {
                if let Some(node) = self.nodes.get_mut(&child_id) {
                    node.parent = Some(root_a11y);
                }
            }
            let root_node = A11yNode {
                id: root_a11y,
                role: Role::Application,
                state: NodeState::default(),
                properties: NodeProperties::default(),
                parent: None,
                children: top_level,
                element_id: root_id,
                bounds: crate::core::Rect::default(),
            };
            self.nodes.insert(root_a11y, root_node);
            Some(root_a11y)
        } else {
            None
        };

        if let Some(focused_a11y) = self.focused_node {
            if !self.nodes.contains_key(&focused_a11y) {
                self.focused_node = None;
            }
        }

        self.platform.tree_updated(&self.nodes, self.root);
    }

    fn build_node(
        &mut self,
        element_tree: &ElementTree,
        element_id: ElementId,
        parent: Option<A11yId>,
        out: &mut Vec<A11yId>,
    ) {
        let node_entry = match element_tree.elements.get(&element_id) {
            Some(n) => n,
            None => return,
        };
        let element = &node_entry.element;

        if !element.is_visible() {
            return;
        }

        let info = element.accessibility_info();
        let children_ids: Vec<ElementId> = node_entry.children.clone();

        match &info {
            Some(info) if info.role != Role::None && info.role != Role::Presentation => {
                let a11y_id = A11yId::new();

                let mut a11y_children = Vec::new();
                for child_id in &children_ids {
                    self.build_node(element_tree, *child_id, Some(a11y_id), &mut a11y_children);
                }

                let node = A11yNode {
                    id: a11y_id,
                    role: info.role,
                    state: info.state.clone(),
                    properties: info.properties.clone(),
                    parent,
                    children: a11y_children,
                    element_id,
                    bounds: element.bounds(),
                };

                self.nodes.insert(a11y_id, node);
                self.element_to_a11y.insert(element_id, a11y_id);

                out.push(a11y_id);
            }
            _ => {
                for child_id in &children_ids {
                    self.build_node(element_tree, *child_id, parent, out);
                }
            }
        }
    }

    pub fn update_focus(&mut self, element_id: ElementId) {
        if let Some(old_a11y) = self.focused_node {
            if let Some(node) = self.nodes.get_mut(&old_a11y) {
                node.state.focused = false;
            }
        }

        if let Some(&a11y_id) = self.element_to_a11y.get(&element_id) {
            if let Some(node) = self.nodes.get_mut(&a11y_id) {
                node.state.focused = true;
            }
            self.focused_node = Some(a11y_id);
            self.platform.focus_moved(a11y_id);
        }
    }

    pub fn find_node_by_element(&self, element_id: ElementId) -> Option<A11yId> {
        self.element_to_a11y.get(&element_id).copied()
    }

    pub fn announce(&mut self, message: impl Into<String>, priority: LiveRegion) {
        let msg = message.into();
        if !msg.is_empty() {
            self.platform.announce(&msg, priority);
        }
    }

    pub fn announce_state_change(&mut self, node_id: A11yId, old_state: &NodeState, new_state: &NodeState) {
        let label = self.nodes.get(&node_id)
            .and_then(|n| n.properties.label.clone())
            .unwrap_or_default();

        if old_state.checked != new_state.checked {
            let msg = if new_state.checked.unwrap_or(false) {
                format!("{} checked", label)
            } else {
                format!("{} unchecked", label)
            };
            self.announce(msg, LiveRegion::Polite);
        }

        if old_state.expanded != new_state.expanded {
            let msg = if new_state.expanded.unwrap_or(false) {
                format!("{} expanded", label)
            } else {
                format!("{} collapsed", label)
            };
            self.announce(msg, LiveRegion::Polite);
        }

        if old_state.disabled != new_state.disabled {
            let msg = if new_state.disabled {
                format!("{} disabled", label)
            } else {
                format!("{} enabled", label)
            };
            self.announce(msg, LiveRegion::Polite);
        }
    }

    pub fn on_action(&mut self, node_id: A11yId, action: Action, tree: &mut ElementTree) {
        let element_id = match self.nodes.get(&node_id) {
            Some(node) => node.element_id,
            None => return,
        };

        let root_id = match tree.root_id {
            Some(id) => id,
            None => return,
        };

        match action {
            Action::Click => {
                if let Some(elem) = tree.get(element_id) {
                    let bounds = elem.bounds();
                    let center = crate::core::Point::new(
                        bounds.origin.x + bounds.size.width / 2.0,
                        bounds.origin.y + bounds.size.height / 2.0,
                    );
                    let down = crate::input::Event::MouseDown {
                        button: crate::input::MouseButton::Left,
                        position: center,
                    };
                    let up = crate::input::Event::MouseUp {
                        button: crate::input::MouseButton::Left,
                        position: center,
                    };
                    tree.handle_event(root_id, &down);
                    tree.handle_event(root_id, &up);
                }
            }
            Action::Focus => {
                self.platform.focus_moved(node_id);
            }
            Action::SetValue(ref value) => {
                for ch in value.chars() {
                    let event = crate::input::Event::CharInput(ch);
                    tree.handle_event(root_id, &event);
                }
            }
            Action::Increment => {
                let event = crate::input::Event::KeyDown(crate::input::Key::Up);
                tree.handle_event(root_id, &event);
            }
            Action::Decrement => {
                let event = crate::input::Event::KeyDown(crate::input::Key::Down);
                tree.handle_event(root_id, &event);
            }
            Action::Expand | Action::Collapse => {
                if let Some(elem) = tree.get(element_id) {
                    let bounds = elem.bounds();
                    let center = crate::core::Point::new(
                        bounds.origin.x + bounds.size.width / 2.0,
                        bounds.origin.y + bounds.size.height / 2.0,
                    );
                    let down = crate::input::Event::MouseDown {
                        button: crate::input::MouseButton::Left,
                        position: center,
                    };
                    let up = crate::input::Event::MouseUp {
                        button: crate::input::MouseButton::Left,
                        position: center,
                    };
                    tree.handle_event(root_id, &down);
                    tree.handle_event(root_id, &up);
                }
            }
        }
    }

    pub fn root(&self) -> Option<A11yId> {
        self.root
    }

    pub fn get_node(&self, id: A11yId) -> Option<&A11yNode> {
        self.nodes.get(&id)
    }

    pub fn node_count(&self) -> usize {
        self.nodes.len()
    }

    #[cfg(feature = "accessibility")]
    pub fn take_accesskit_update(&mut self) -> Option<accesskit::TreeUpdate> {
        let adapter = self.platform.as_any_mut();
        if let Some(ak_adapter) = adapter.downcast_mut::<AccessKitAdapter>() {
            ak_adapter.take_pending_update()
        } else {
            None
        }
    }
}
