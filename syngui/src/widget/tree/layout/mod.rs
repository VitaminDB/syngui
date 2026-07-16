mod measure;
mod position;

use crate::core::Size;
use crate::layout::Constraints;
use super::{ElementId, ElementTree};

pub(crate) fn clamp_finite(size: Size, constraints: Constraints) -> Size {
    let width = if size.width.is_finite() {
        size.width
    } else {
        if constraints.max_width.is_finite() { constraints.max_width } else { 100.0 }
    };
    let height = if size.height.is_finite() {
        size.height
    } else {
        if constraints.max_height.is_finite() { constraints.max_height } else { 40.0 }
    };
    let clamped_w = width.clamp(constraints.min_width.min(constraints.max_width), constraints.max_width);
    let clamped_h = height.clamp(constraints.min_height.min(constraints.max_height), constraints.max_height);
    Size::new(
        if clamped_w.is_finite() { clamped_w } else { width },
        if clamped_h.is_finite() { clamped_h } else { height },
    )
}

pub(crate) fn clamp_finite_explicit(size: Size, constraints: Constraints, explicit: (Option<f32>, Option<f32>)) -> Size {
    let width = if size.width.is_finite() { size.width }
        else if constraints.max_width.is_finite() { constraints.max_width } else { 100.0 };
    let height = if size.height.is_finite() { size.height }
        else if constraints.max_height.is_finite() { constraints.max_height } else { 40.0 };

    let clamped_w = if explicit.0.is_some() { width } else { width.clamp(constraints.min_width.min(constraints.max_width), constraints.max_width) };
    let clamped_h = if explicit.1.is_some() { height } else { height.clamp(constraints.min_height.min(constraints.max_height), constraints.max_height) };
    Size::new(
        if clamped_w.is_finite() { clamped_w } else { width },
        if clamped_h.is_finite() { clamped_h } else { height },
    )
}
