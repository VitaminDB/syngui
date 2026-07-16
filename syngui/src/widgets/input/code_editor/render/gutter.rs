use super::super::theme::Theme;
use crate::core::{Point, Rect, Size};
use crate::mss::{MssFields, TextAlign, TextDecoration};
use crate::render::DisplayList;

pub const GUTTER_WIDTH: f32 = 56.0;
pub const GUTTER_RIGHT_PADDING: f32 = 8.0;

pub fn paint_gutter_bg(
    list: &mut DisplayList,
    bounds: Rect,
    theme: &Theme,
    mss: &MssFields,
    show_line_numbers: bool,
) {
    if !show_line_numbers {
        return;
    }
    let gutter_rect = Rect::new(bounds.origin, Size::new(GUTTER_WIDTH, bounds.size.height));
    list.push_rect(gutter_rect, theme.gutter_bg(mss), [0.0; 4]);
}

pub fn paint_line_number(
    list: &mut DisplayList,
    line_idx: usize,
    bounds_x: f32,
    line_y: f32,
    line_height: f32,
    font_size: f32,
    theme: &Theme,
    mss: &MssFields,
) {
    let num_str = format!("{}", line_idx + 1);
    let rect = Rect::new(
        Point::new(bounds_x, line_y),
        Size::new(GUTTER_WIDTH - GUTTER_RIGHT_PADDING, line_height),
    );
    list.push_text_aligned(
        &num_str,
        rect,
        theme.gutter_fg(mss),
        font_size,
        TextAlign::RIGHT | TextAlign::VCENTER,
        TextDecoration::None,
        400,
    );
}
