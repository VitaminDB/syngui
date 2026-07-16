use std::any::Any;
use hashbrown::HashMap;
use super::types::{A11yId, A11yNode, LiveRegion, NodeState};

pub trait PlatformAdapter: Send + Sync {
    fn tree_updated(&mut self, nodes: &HashMap<A11yId, A11yNode>, root: Option<A11yId>);

    fn as_any_mut(&mut self) -> &mut dyn Any;

    fn node_state_changed(&mut self, node_id: A11yId, state: &NodeState);

    fn focus_moved(&mut self, node_id: A11yId);

    fn value_changed(&mut self, node_id: A11yId, value: &str);

    fn announce(&mut self, message: &str, priority: LiveRegion);
}

pub struct LoggingAdapter;

impl PlatformAdapter for LoggingAdapter {
    fn tree_updated(&mut self, _nodes: &HashMap<A11yId, A11yNode>, _root: Option<A11yId>) {
    }

    fn node_state_changed(&mut self, _node_id: A11yId, _state: &NodeState) {
    }

    fn focus_moved(&mut self, _node_id: A11yId) {
    }

    fn value_changed(&mut self, _node_id: A11yId, _value: &str) {
    }

    fn announce(&mut self, _message: &str, _priority: LiveRegion) {
    }

    fn as_any_mut(&mut self) -> &mut dyn Any {
        self
    }
}

pub struct NullAdapter;

impl PlatformAdapter for NullAdapter {
    fn tree_updated(&mut self, _nodes: &HashMap<A11yId, A11yNode>, _root: Option<A11yId>) {}
    fn node_state_changed(&mut self, _node_id: A11yId, _state: &NodeState) {}
    fn focus_moved(&mut self, _node_id: A11yId) {}
    fn value_changed(&mut self, _node_id: A11yId, _value: &str) {}
    fn announce(&mut self, _message: &str, _priority: LiveRegion) {}
    fn as_any_mut(&mut self) -> &mut dyn Any {
        self
    }
}
