use super::super::theme::Theme;
use crate::core::{Point, Rect, Size};
use crate::mss::MssFields;
use crate::render::{Border, DisplayList};

pub fn paint_current_line_bg(
    list: &mut DisplayList,
    bounds_x: f32,
    bounds_w: f32,
    line_y: f32,
    line_height: f32,
    theme: &Theme,
    mss: &MssFields,
) {
    let rect = Rect::new(
        Point::new(bounds_x, line_y),
        Size::new(bounds_w, line_height),
    );
    list.push_rect(rect, theme.current_line(mss), [0.0; 4]);
}

pub fn paint_selection_for_line(
    list: &mut DisplayList,
    text: &str,
    start_byte: usize,
    end_byte: usize,
    text_origin: Point,
    line_height: f32,
    font_size: f32,
    font_family: Option<&str>,
    theme: &Theme,
    mss: &MssFields,
) {
    if start_byte >= end_byte || text.is_empty() && start_byte == end_byte {
        return;
    }
    list.push_text_selection_styled(
        text,
        start_byte,
        end_byte,
        text_origin.x,
        text_origin.y,
        line_height,
        font_size,
        theme.selection(mss),
        font_family.map(|s| s.to_string()),
    );
}

pub fn paint_full_line_selection(
    list: &mut DisplayList,
    bounds_x: f32,
    bounds_w: f32,
    line_y: f32,
    line_height: f32,
    theme: &Theme,
    mss: &MssFields,
) {
    let rect = Rect::new(
        Point::new(bounds_x, line_y),
        Size::new(bounds_w, line_height),
    );
    list.push_rect(rect, theme.selection(mss), [0.0; 4]);
}

pub fn paint_find_match(
    list: &mut DisplayList,
    text: &str,
    start_byte: usize,
    end_byte: usize,
    text_origin: Point,
    line_height: f32,
    font_size: f32,
    font_family: Option<&str>,
    theme: &Theme,
    mss: &MssFields,
    is_current: bool,
) {
    if start_byte >= end_byte {
        return;
    }
    let color = if is_current {
        theme.find_current(mss)
    } else {
        theme.find_match(mss)
    };
    list.push_text_selection_styled(
        text,
        start_byte,
        end_byte,
        text_origin.x,
        text_origin.y,
        line_height,
        font_size,
        color,
        font_family.map(|s| s.to_string()),
    );
}

pub fn paint_indent_guides(
    list: &mut DisplayList,
    text_origin_x: f32,
    line_y: f32,
    line_height: f32,
    char_width: f32,
    tab_width: u8,
    level_count: usize,
    theme: &Theme,
    mss: &MssFields,
) {
    if level_count == 0 || char_width <= 0.0 {
        return;
    }
    let color = theme.indent_guide(mss);
    let step = char_width * tab_width as f32;
    for level in 1..=level_count {
        let x = text_origin_x + step * level as f32 - 0.5;
        let rect = Rect::new(Point::new(x, line_y), Size::new(1.0, line_height));
        list.push_rect(rect, color, [0.0; 4]);
    }
}

pub fn paint_bracket_highlight(
    list: &mut DisplayList,
    x: f32,
    y: f32,
    width: f32,
    height: f32,
    theme: &Theme,
    mss: &MssFields,
) {
    if width <= 0.0 || height <= 0.0 {
        return;
    }
    let color = theme.bracket_match(mss);
    let rect = Rect::new(Point::new(x, y), Size::new(width, height));
    list.push_rect_bordered(
        rect,
        crate::core::Color::new(0.0, 0.0, 0.0, 0.0),
        [2.0; 4],
        Border { width: 1.0, color },
    );
}

pub fn paint_cursor(
    list: &mut DisplayList,
    text: &str,
    cursor_byte_in_line: usize,
    text_origin: Point,
    line_height: f32,
    font_size: f32,
    font_family: Option<&str>,
    theme: &Theme,
    mss: &MssFields,
) {
    list.push_text_cursor_styled(
        text,
        cursor_byte_in_line,
        text_origin.x,
        text_origin.y,
        line_height,
        font_size,
        400,
        theme.cursor(mss),
        font_family.map(|s| s.to_string()),
    );
}
