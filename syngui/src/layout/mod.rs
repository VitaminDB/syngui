pub mod constraints;
pub mod flex;

pub use constraints::Constraints;
pub use flex::*;

use crate::core::Size;
use crate::widget::Element;

pub trait Layout {
    fn intrinsic_width(&self, height: f32) -> f32;

    fn intrinsic_height(&self, width: f32) -> f32;

    fn layout(&mut self, children: &mut [&mut dyn Element], constraints: Constraints) -> Size;
}
