#[cfg(feature = "accessibility")]
use super::accesskit_adapter::AccessKitAdapter;
use super::platform::PlatformAdapter;
use super::types::*;
use crate::widget::{ElementId, ElementTree};
use hashbrown::HashMap;

/// Перевод рамок элемента на экран: `экран = рамка × k + o`, плюс видимая
/// область (пересечение обрезающих и прокручиваемых предков) в экранных координатах.
#[derive(Clone, Copy)]
struct Xf {
    k: f32,
    ox: f32,
    oy: f32,
    clip: Option<crate::core::Rect>,
}

impl Xf {
    const IDENTITY: Xf = Xf { k: 1.0, ox: 0.0, oy: 0.0, clip: None };

    fn map(&self, r: crate::core::Rect) -> crate::core::Rect {
        crate::core::Rect::new(
            crate::core::Point::new(r.origin.x * self.k + self.ox, r.origin.y * self.k + self.oy),
            crate::core::Size::new(r.size.width * self.k, r.size.height * self.k),
        )
    }

    /// Видимая часть рамки на экране (None — целиком скрыта).
    fn visible(&self, r: crate::core::Rect) -> Option<crate::core::Rect> {
        let m = self.map(r);
        match self.clip {
            None => Some(m),
            // пустые рамки (контейнеры без размера) не отсекаем по площади
            Some(c) if m.size.width <= 0.0 || m.size.height <= 0.0 => {
                let p = m.origin;
                (p.x >= c.origin.x - 0.5 && p.y >= c.origin.y - 0.5 && p.x <= c.origin.x + c.size.width + 0.5 && p.y <= c.origin.y + c.size.height + 0.5).then_some(m)
            }
            Some(c) => c.intersection(&m).filter(|i| i.size.width >= 1.0 && i.size.height >= 1.0),
        }
    }

    /// Перевод для детей элемента: его прокрутка и масштаб, его обрезка.
    fn child(&self, e: &dyn crate::widget::Element) -> Xf {
        let mut clip = self.clip;
        if e.clip_content() || e.is_scroll_container() {
            let own = self.map(e.bounds());
            clip = Some(match clip {
                Some(c) => c.intersection(&own).unwrap_or(crate::core::Rect::new(own.origin, crate::core::Size::new(0.0, 0.0))),
                None => own,
            });
        }
        let s = e.scroll_offset();
        let kp = e.event_scale().max(f32::EPSILON);
        Xf { k: self.k * kp, ox: self.ox - self.k * s.x, oy: self.oy - self.k * s.y, clip }
    }
}

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
        self.build_node(element_tree, root_id, None, &mut top_level, Xf::IDENTITY);

        self.root = if top_level.len() == 1 {
            Some(top_level[0])
        } else if top_level.len() > 1 {
            let root_a11y = A11yId::SYNTHETIC_ROOT;
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
                offscreen: false,
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
        xf: Xf,
    ) {
        let node_entry = match element_tree.elements.get(&element_id) {
            Some(n) => n,
            None => return,
        };
        let element = &node_entry.element;

        if !element.is_visible() {
            return;
        }

        // Рамка на экране: прокрутка и масштаб предков (как в hit-test);
        // вне видимой области прокрутки/обрезки предков — узел помечается
        // `offscreen`: агент не должен «нажимать» строку, которой не видно.
        let vis = xf.visible(element.bounds());
        let offscreen = vis.is_none();
        let bounds = vis.unwrap_or_else(|| xf.map(element.bounds()));
        let child_xf = xf.child(element);

        let info = element.accessibility_info();
        let children_ids: Vec<ElementId> = node_entry.children.clone();

        match &info {
            Some(info) if info.role != Role::None && info.role != Role::Presentation => {
                let a11y_id = A11yId::for_element(element_id);

                let mut a11y_children = Vec::new();
                for child_id in &children_ids {
                    self.build_node(element_tree, *child_id, Some(a11y_id), &mut a11y_children, child_xf);
                }
                self.push_virtual(element.as_ref(), element_id, Some(a11y_id), &mut a11y_children, xf);

                let node = A11yNode {
                    id: a11y_id,
                    role: info.role,
                    state: info.state.clone(),
                    properties: info.properties.clone(),
                    parent,
                    children: a11y_children,
                    element_id,
                    bounds,
                    offscreen,
                };

                self.nodes.insert(a11y_id, node);
                self.element_to_a11y.insert(element_id, a11y_id);

                out.push(a11y_id);
            }
            _ => {
                for child_id in &children_ids {
                    self.build_node(element_tree, *child_id, parent, out, child_xf);
                }
                self.push_virtual(element.as_ref(), element_id, parent, out, xf);
            }
        }
    }

    /// Узлы частей рисованного элемента ([`crate::widget::Element::accessibility_children`]).
    fn push_virtual(&mut self, element: &dyn crate::widget::Element, element_id: ElementId, parent: Option<A11yId>, out: &mut Vec<A11yId>, xf: Xf) {
        for (i, (info, bounds)) in element.accessibility_children().into_iter().enumerate() {
            // части элемента — в его координатах; за краем прокрутки — offscreen
            let vis = xf.visible(bounds);
            let offscreen = vis.is_none();
            let bounds = vis.unwrap_or_else(|| xf.map(bounds));
            let id = A11yId::virtual_child(element_id, i);
            self.nodes.insert(id, A11yNode { id, role: info.role, state: info.state, properties: info.properties, parent, children: Vec::new(), element_id, bounds, offscreen });
            out.push(id);
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

    pub fn announce_state_change(
        &mut self,
        node_id: A11yId,
        old_state: &NodeState,
        new_state: &NodeState,
    ) {
        let label = self
            .nodes
            .get(&node_id)
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
                    let bounds = if node_id.is_virtual() { self.nodes.get(&node_id).map(|n| n.bounds).unwrap_or_default() } else { elem.bounds() };
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
    /// Скринридер подключился/отключился: без него узлы AccessKit не строятся.
    pub fn set_accesskit_active(&mut self, on: bool) {
        if let Some(ak) = self
            .platform
            .as_any_mut()
            .downcast_mut::<AccessKitAdapter>()
        {
            ak.set_active(on);
        }
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

#[cfg(test)]
mod tests {
    use super::*;
    use crate::a11y::NullAdapter;
    use crate::testing::TestHarness;
    use crate::widgets::containers::named::scroll_to_named;
    use crate::widgets::{Button, Column, Named, ScrollView};

    fn labels(t: &A11yTree) -> Vec<(String, crate::core::Rect)> {
        let mut v: Vec<(String, crate::core::Rect)> = t.nodes.values().filter(|n| !n.offscreen).filter_map(|n| n.properties.label.clone().map(|l| (l, n.bounds))).collect();
        v.sort_by(|a, b| a.1.origin.y.total_cmp(&b.1.origin.y));
        v
    }

    /// Строки за краем прокрутки не попадают в дерево, рамки видимых — экранные.
    #[test]
    fn scrolled_out_rows_are_hidden_and_bounds_on_screen() {
        let mut col = Column::new();
        for i in 0..20 {
            col = col.child(Named::new(format!("row{i}"), Column::new().height(100.0).child(Button::new(format!("Строка {i}")))));
        }
        let mut h = TestHarness::new(Box::new(ScrollView::new().vertical().child(col)));
        h.layout(300.0, 400.0);
        let mut t = A11yTree::new(Box::new(NullAdapter));
        t.sync(&h.tree, h.root_id);
        let names: Vec<String> = labels(&t).into_iter().map(|x| x.0).filter(|l| l.starts_with("Строка")).collect();
        assert!(names.contains(&"Строка 0".to_string()) && !names.contains(&"Строка 5".to_string()), "{names:?}");

        scroll_to_named("row10");
        h.layout(300.0, 400.0);
        for _ in 0..60 {
            h.animate(std::time::Duration::from_millis(16));
        }
        h.layout(300.0, 400.0);
        t.sync(&h.tree, h.root_id);
        let rows: Vec<(String, crate::core::Rect)> = labels(&t).into_iter().filter(|l| l.0.starts_with("Строка")).collect();
        let names: Vec<&str> = rows.iter().map(|x| x.0.as_str()).collect();
        assert!(!names.contains(&"Строка 0") && !names.contains(&"Строка 9") && names.contains(&"Строка 10") && !names.contains(&"Строка 14"), "{names:?}");
        // для скринридера скрытые строки остаются в дереве (навигация), но помечены
        let all = t.nodes.values().filter(|n| n.properties.label.as_deref().is_some_and(|l| l.starts_with("Строка"))).count();
        assert_eq!(all, 20);
        // осмотр снаружи не отдаёт скрытых
        crate::a11y::inspect::publish(77, 1, (300.0, 400.0), &t);
        let snap = crate::a11y::inspect::snapshot_json(77).unwrap_or_default();
        assert!(snap.contains("Строка 10") && !snap.contains("Строка 0\"") && !snap.contains("Строка 15"), "{snap}");
        let r10 = rows.iter().find(|x| x.0 == "Строка 10").unwrap().1;
        assert!(r10.origin.y >= -0.5 && r10.origin.y < 100.0, "{r10:?}");
        for (_, b) in &rows {
            assert!(b.origin.y >= -0.5 && b.origin.y + b.size.height <= 400.5, "{b:?}");
        }
    }
}
