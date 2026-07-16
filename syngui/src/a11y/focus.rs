use crate::widget::{ElementId, ElementTree};
use super::types::Role;

#[derive(Clone, Debug)]
struct FocusScope {
    saved_index: Option<usize>,
    saved_tab_order: Vec<ElementId>,
}

pub struct FocusManager {
    tab_order: Vec<ElementId>,
    current_index: Option<usize>,
    scope_stack: Vec<FocusScope>,
}

impl FocusManager {
    pub fn new() -> Self {
        Self {
            tab_order: Vec::new(),
            current_index: None,
            scope_stack: Vec::new(),
        }
    }

    pub fn rebuild_tab_order(&mut self, element_tree: &ElementTree, root_id: ElementId) {
        let prev_focus_id = self
            .current_index
            .and_then(|idx| self.tab_order.get(idx).copied());

        if self.scope_stack.is_empty() {
            self.tab_order.clear();
            self.collect_focusable(element_tree, root_id);
        }

        self.current_index = prev_focus_id
            .and_then(|id| self.tab_order.iter().position(|&t| t == id));
    }

    pub fn push_focus_scope(&mut self, element_tree: &ElementTree, scope_root: ElementId) {
        let saved = FocusScope {
            saved_index: self.current_index,
            saved_tab_order: self.tab_order.clone(),
        };
        self.scope_stack.push(saved);

        self.tab_order.clear();
        self.collect_focusable(element_tree, scope_root);

        self.current_index = if self.tab_order.is_empty() { None } else { Some(0) };
    }

    pub fn pop_focus_scope(
        &mut self,
        element_tree: &ElementTree,
        root_id: ElementId,
    ) -> Option<ElementId> {
        if let Some(scope) = self.scope_stack.pop() {
            self.tab_order = scope.saved_tab_order;
            self.current_index = scope.saved_index;
            if self.scope_stack.is_empty() {
                self.rebuild_tab_order(element_tree, root_id);
            }
            self.current_focus()
        } else {
            None
        }
    }

    pub fn has_focus_scope(&self) -> bool {
        !self.scope_stack.is_empty()
    }

    fn collect_focusable(&mut self, element_tree: &ElementTree, element_id: ElementId) {
        let node = match element_tree.elements.get(&element_id) {
            Some(n) => n,
            None => return,
        };

        if !node.element.is_visible() {
            return;
        }

        if let Some(info) = node.element.accessibility_info() {
            if Self::is_focusable(&info.role) && !info.state.disabled && !info.state.hidden {
                self.tab_order.push(element_id);
            }
        }

        let children: Vec<ElementId> = node.children.clone();
        for child_id in children {
            self.collect_focusable(element_tree, child_id);
        }
    }

    pub fn next_focus(&mut self) -> Option<ElementId> {
        if self.tab_order.is_empty() {
            return None;
        }

        let next = match self.current_index {
            Some(idx) => (idx + 1) % self.tab_order.len(),
            None => 0,
        };
        self.current_index = Some(next);
        Some(self.tab_order[next])
    }

    pub fn previous_focus(&mut self) -> Option<ElementId> {
        if self.tab_order.is_empty() {
            return None;
        }

        let prev = match self.current_index {
            Some(idx) => {
                if idx == 0 {
                    self.tab_order.len() - 1
                } else {
                    idx - 1
                }
            }
            None => self.tab_order.len() - 1,
        };
        self.current_index = Some(prev);
        Some(self.tab_order[prev])
    }

    pub fn current_focus(&self) -> Option<ElementId> {
        self.current_index.map(|idx| self.tab_order[idx])
    }

    pub fn set_focus(&mut self, element_id: ElementId) -> bool {
        if let Some(idx) = self.tab_order.iter().position(|&id| id == element_id) {
            self.current_index = Some(idx);
            true
        } else {
            false
        }
    }

    pub fn clear_focus(&mut self) {
        self.current_index = None;
    }

    pub fn is_focusable(role: &Role) -> bool {
        matches!(
            role,
            Role::Button
                | Role::CheckBox
                | Role::RadioButton
                | Role::TextField
                | Role::Slider
                | Role::ComboBox
                | Role::Link
                | Role::TabList
                | Role::Tree
                | Role::ListBox
                | Role::Menu
                | Role::Terminal
        )
    }

    pub fn focusable_count(&self) -> usize {
        self.tab_order.len()
    }

    pub fn first_focus(&self) -> Option<ElementId> {
        self.tab_order.first().copied()
    }
}
