use std::collections::VecDeque;
use crate::core::{Point, Rect, Size};
use crate::render::DisplayList;
use super::panel;

#[derive(Clone, Debug)]
pub struct EventLogEntry {
    pub timestamp_ms: f64,
    pub event_type: String,
    pub result: String,
}

pub fn render_event_log(
    list: &mut DisplayList,
    content_rect: Rect,
    entries: &VecDeque<EventLogEntry>,
    scroll_offset: f32,
    paused: bool,
) {
    list.push_clip(content_rect);

    let x = content_rect.origin.x;
    let w = content_rect.size.width;
    let mut y = content_rect.origin.y;

    let pause_rect = Rect::new(
        Point::new(x, y),
        Size::new(w, panel::LINE_HEIGHT),
    );
    if paused {
        list.push_rect(pause_rect, crate::core::Color::new(0.6, 0.2, 0.2, 0.3), [0.0; 4]);
        let text_rect = Rect::new(
            Point::new(x + 4.0, y + 2.0),
            Size::new(w - 8.0, panel::FONT_SIZE + 2.0),
        );
        list.push_text("PAUSED (click to resume)", text_rect, crate::core::Color::new(1.0, 0.4, 0.4, 1.0), panel::FONT_SIZE);
    } else {
        list.push_rect(pause_rect, crate::core::Color::new(0.2, 0.4, 0.2, 0.3), [0.0; 4]);
        let text_rect = Rect::new(
            Point::new(x + 4.0, y + 2.0),
            Size::new(w - 8.0, panel::FONT_SIZE + 2.0),
        );
        list.push_text("RECORDING (click to pause)", text_rect, crate::core::Color::new(0.4, 0.8, 0.4, 1.0), panel::FONT_SIZE);
    }
    y += panel::LINE_HEIGHT + 2.0;

    let sep = Rect::new(Point::new(x, y), Size::new(w, 1.0));
    list.push_rect(sep, panel::SEPARATOR, [0.0; 4]);
    y += 3.0;

    let entries_start_y = y;
    y -= scroll_offset;

    for entry in entries.iter().rev() {
        if y + panel::LINE_HEIGHT < entries_start_y {
            y += panel::LINE_HEIGHT;
            continue;
        }
        if y > content_rect.origin.y + content_rect.size.height {
            break;
        }

        if y + panel::LINE_HEIGHT > entries_start_y {
            let time_str = format!("[{:.2}s]", entry.timestamp_ms / 1000.0);
            let time_rect = Rect::new(
                Point::new(x + 2.0, y + 2.0),
                Size::new(60.0, panel::FONT_SIZE + 2.0),
            );
            list.push_text(&time_str, time_rect, panel::TEXT_SECONDARY, panel::SMALL_FONT_SIZE);

            let type_rect = Rect::new(
                Point::new(x + 62.0, y + 2.0),
                Size::new(w - 130.0, panel::FONT_SIZE + 2.0),
            );
            list.push_text(&entry.event_type, type_rect, panel::TEXT_PRIMARY, panel::FONT_SIZE);

            let result_color = match entry.result.as_str() {
                "Handled" => panel::EVENT_HANDLED,
                "Captured" => panel::EVENT_CAPTURED,
                _ => panel::EVENT_IGNORED,
            };
            let result_rect = Rect::new(
                Point::new(x + w - 66.0, y + 2.0),
                Size::new(62.0, panel::FONT_SIZE + 2.0),
            );
            list.push_text(&entry.result, result_rect, result_color, panel::SMALL_FONT_SIZE);
        }

        y += panel::LINE_HEIGHT;
    }

    list.pop_clip();
}

pub fn compute_content_height(entries: &VecDeque<EventLogEntry>) -> f32 {
    panel::LINE_HEIGHT + 5.0 + entries.len() as f32 * panel::LINE_HEIGHT
}
