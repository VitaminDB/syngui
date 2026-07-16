use super::super::find::FindState;
use super::super::theme::Theme;
use crate::core::{Point, Rect, RectExt, Size};
use crate::mss::{MssFields, TextAlign, TextDecoration};
use crate::render::DisplayList;

const TOOLBAR_WIDTH: f32 = 360.0;
const TOOLBAR_HEIGHT: f32 = 36.0;
const TOOLBAR_MARGIN: f32 = 8.0;
const TOOLBAR_PADDING_X: f32 = 12.0;

pub fn paint_find_toolbar(
    list: &mut DisplayList,
    bounds: Rect,
    find: &FindState,
    theme: &Theme,
    mss: &MssFields,
    font_size: f32,
) {
    if !find.visible {
        return;
    }
    let toolbar_x = bounds.x() + bounds.size.width - TOOLBAR_WIDTH - TOOLBAR_MARGIN;
    let toolbar_y = bounds.y() + TOOLBAR_MARGIN;
    let toolbar_rect = Rect::new(
        Point::new(toolbar_x, toolbar_y),
        Size::new(TOOLBAR_WIDTH, TOOLBAR_HEIGHT),
    );
    let bg = theme.gutter_bg(mss);
    list.push_rect(toolbar_rect, bg, [6.0; 4]);

    let label = if find.query.is_empty() {
        "Find:".to_string()
    } else {
        format!("Find: {}", find.query)
    };
    let label_rect = Rect::new(
        Point::new(toolbar_x + TOOLBAR_PADDING_X, toolbar_y),
        Size::new(TOOLBAR_WIDTH - TOOLBAR_PADDING_X * 2.0 - 80.0, TOOLBAR_HEIGHT),
    );
    list.push_text_aligned(
        &label,
        label_rect,
        theme.fg(mss),
        font_size,
        TextAlign::LEFT | TextAlign::VCENTER,
        TextDecoration::None,
        400,
    );

    let counter = if find.matches.is_empty() {
        if find.query.is_empty() {
            String::new()
        } else {
            "0".to_string()
        }
    } else {
        format!(
            "{} / {}",
            find.current.map(|i| i + 1).unwrap_or(0),
            find.matches.len()
        )
    };
    let counter_rect = Rect::new(
        Point::new(
            toolbar_x + TOOLBAR_WIDTH - TOOLBAR_PADDING_X - 80.0,
            toolbar_y,
        ),
        Size::new(80.0, TOOLBAR_HEIGHT),
    );
    list.push_text_aligned(
        &counter,
        counter_rect,
        theme.gutter_fg(mss),
        font_size,
        TextAlign::RIGHT | TextAlign::VCENTER,
        TextDecoration::None,
        400,
    );
}

pub fn paint_goto_toolbar(
    list: &mut DisplayList,
    bounds: Rect,
    buffer: &str,
    total_lines: usize,
    theme: &Theme,
    mss: &MssFields,
    font_size: f32,
) {
    let toolbar_x = bounds.x() + bounds.size.width - TOOLBAR_WIDTH - TOOLBAR_MARGIN;
    let toolbar_y = bounds.y() + TOOLBAR_MARGIN;
    let toolbar_rect = Rect::new(
        Point::new(toolbar_x, toolbar_y),
        Size::new(TOOLBAR_WIDTH, TOOLBAR_HEIGHT),
    );
    let bg = theme.gutter_bg(mss);
    list.push_rect(toolbar_rect, bg, [6.0; 4]);

    let label = if buffer.is_empty() {
        format!("Go to line (1..{}):", total_lines)
    } else {
        format!("Go to line: {}", buffer)
    };
    let label_rect = Rect::new(
        Point::new(toolbar_x + TOOLBAR_PADDING_X, toolbar_y),
        Size::new(TOOLBAR_WIDTH - TOOLBAR_PADDING_X * 2.0, TOOLBAR_HEIGHT),
    );
    list.push_text_aligned(
        &label,
        label_rect,
        theme.fg(mss),
        font_size,
        TextAlign::LEFT | TextAlign::VCENTER,
        TextDecoration::None,
        400,
    );
}
