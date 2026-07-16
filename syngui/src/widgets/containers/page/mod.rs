mod element;

use super::IntoWidget;
use crate::widget::Widget;
use crate::widgets::scroll::ScrollDirection;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum ScrollbarPolicy {
    Auto,
    Always,
    Never,
}

impl Default for ScrollbarPolicy {
    fn default() -> Self {
        ScrollbarPolicy::Auto
    }
}

#[derive(Clone, Copy, Debug)]
pub struct ScrollPhysics {
    pub friction: f32,
    pub min_velocity: f32,
    pub max_overscroll: f32,
    pub bounce_stiffness: f32,
    pub bounce_damping: f32,
}

impl Default for ScrollPhysics {
    fn default() -> Self {
        Self {
            friction: 0.98,
            min_velocity: 0.5,
            max_overscroll: 100.0,
            bounce_stiffness: 300.0,
            bounce_damping: 25.0,
        }
    }
}

#[derive(Clone, Copy, Debug)]
pub enum ScrollTarget {
    Top,
    Bottom,
    Offset(f32),
}

pub struct Page {
    child: Option<Box<dyn Widget>>,
    direction: ScrollDirection,
    scrollbar_policy: ScrollbarPolicy,
    scrollbar_width: f32,
    physics: ScrollPhysics,
    scroll_to: Option<ScrollTarget>,
}

impl Page {
    pub fn new() -> Self {
        Self {
            child: None,
            direction: ScrollDirection::Vertical,
            scrollbar_policy: ScrollbarPolicy::default(),
            scrollbar_width: 8.0,
            physics: ScrollPhysics::default(),
            scroll_to: None,
        }
    }

    pub fn child<M>(mut self, child: impl IntoWidget<M>) -> Self {
        self.child = Some(child.into_widget());
        self
    }

    pub fn direction(mut self, direction: ScrollDirection) -> Self {
        self.direction = direction;
        self
    }

    pub fn vertical(self) -> Self {
        self.direction(ScrollDirection::Vertical)
    }

    pub fn horizontal(self) -> Self {
        self.direction(ScrollDirection::Horizontal)
    }

    pub fn both(self) -> Self {
        self.direction(ScrollDirection::Both)
    }

    pub fn scrollbar_policy(mut self, policy: ScrollbarPolicy) -> Self {
        self.scrollbar_policy = policy;
        self
    }

    pub fn scrollbar_width(mut self, width: f32) -> Self {
        self.scrollbar_width = width;
        self
    }

    pub fn physics(mut self, physics: ScrollPhysics) -> Self {
        self.physics = physics;
        self
    }

    pub fn scroll_to(mut self, target: ScrollTarget) -> Self {
        self.scroll_to = Some(target);
        self
    }

    pub fn scroll_to_top(self) -> Self {
        self.scroll_to(ScrollTarget::Top)
    }

    pub fn scroll_to_bottom(self) -> Self {
        self.scroll_to(ScrollTarget::Bottom)
    }

}

impl Default for Page {
    fn default() -> Self {
        Self::new()
    }
}
