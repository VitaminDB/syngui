use crate::core::Size;
use crate::layout::{Constraints, Layout};
use crate::widget::Element;

#[derive(Clone, Copy, Debug, PartialEq, Default)]
pub enum FlexDirection {
    #[default]
    Row,
    Column,
}

#[derive(Clone, Copy, Debug, PartialEq, Default)]
pub enum MainAxisAlignment {
    #[default]
    Start,
    End,
    Center,
    SpaceBetween,
    SpaceAround,
    SpaceEvenly,
}

#[derive(Clone, Copy, Debug, PartialEq, Default)]
pub enum CrossAxisAlignment {
    #[default]
    Start,
    End,
    Center,
    Stretch,
    Baseline,
}

#[derive(Clone, Debug)]
pub struct FlexLayout {
    pub direction: FlexDirection,
    pub main_alignment: MainAxisAlignment,
    pub cross_alignment: CrossAxisAlignment,
    pub gap: f32,
    pub wrap: bool,
}

impl Default for FlexLayout {
    fn default() -> Self {
        Self {
            direction: FlexDirection::Row,
            main_alignment: MainAxisAlignment::Start,
            cross_alignment: CrossAxisAlignment::Start,
            gap: 0.0,
            wrap: false,
        }
    }
}

impl Layout for FlexLayout {
    fn intrinsic_width(&self, _height: f32) -> f32 {
        0.0
    }

    fn intrinsic_height(&self, _width: f32) -> f32 {
        0.0
    }

    fn layout(&mut self, children: &mut [&mut dyn Element], constraints: Constraints) -> Size {
        match self.direction {
            FlexDirection::Row => self.layout_row(children, constraints),
            FlexDirection::Column => self.layout_column(children, constraints),
        }
    }
}

impl FlexLayout {
    fn layout_row(&mut self, children: &mut [&mut dyn Element], constraints: Constraints) -> Size {
        let n = children.len();
        if n == 0 {
            return Size::zero();
        }

        let mut total_fixed = 0.0f32;

        for child in children.iter_mut() {
            let child_constraints = Constraints {
                min_width: 0.0,
                max_width: f32::INFINITY,
                min_height: if self.cross_alignment == CrossAxisAlignment::Stretch {
                    constraints.max_height
                } else {
                    0.0
                },
                max_height: constraints.max_height,
                containing_block: constraints.containing_block,
            };
            let size = child.layout(child_constraints);
            total_fixed += size.width;
        }

        let total_gap = self.gap * (n.saturating_sub(1)) as f32;
        let total_width = total_fixed + total_gap;
        let max_height = constraints.max_height;

        Size::new(
            constraints.constrain_width(total_width),
            constraints.constrain_height(max_height),
        )
    }

    fn layout_column(&mut self, children: &mut [&mut dyn Element], constraints: Constraints) -> Size {
        let n = children.len();
        if n == 0 {
            return Size::zero();
        }

        let mut total_fixed = 0.0f32;

        for child in children.iter_mut() {
            let child_constraints = Constraints {
                min_width: if self.cross_alignment == CrossAxisAlignment::Stretch {
                    constraints.max_width
                } else {
                    0.0
                },
                max_width: constraints.max_width,
                min_height: 0.0,
                max_height: f32::INFINITY,
                containing_block: constraints.containing_block,
            };
            let size = child.layout(child_constraints);
            total_fixed += size.height;
        }

        let total_gap = self.gap * (n.saturating_sub(1)) as f32;
        let total_height = total_fixed + total_gap;
        let max_width = constraints.max_width;

        Size::new(
            constraints.constrain_width(max_width),
            constraints.constrain_height(total_height),
        )
    }
}
