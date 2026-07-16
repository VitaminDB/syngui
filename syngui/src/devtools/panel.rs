use crate::core::{Color, Point, Rect, Size};
use crate::render::DisplayList;
use super::DevToolsTab;

pub const PANEL_MIN_WIDTH: f32 = 250.0;
pub const PANEL_MAX_WIDTH: f32 = 600.0;
pub const PANEL_DEFAULT_WIDTH: f32 = 350.0;
pub const TAB_BAR_HEIGHT: f32 = 32.0;
pub const RESIZE_HANDLE_WIDTH: f32 = 4.0;
pub const FONT_SIZE: f32 = 12.0;
pub const LINE_HEIGHT: f32 = 18.0;
pub const INDENT_SIZE: f32 = 16.0;
pub const PADDING: f32 = 8.0;
pub const SMALL_FONT_SIZE: f32 = 10.0;

pub const BG_COLOR: Color = Color::new(0.0, 0.0, 0.0, 0.85);
pub const TAB_BG: Color = Color::new(0.0, 0.0, 0.0, 0.9);
pub const TAB_ACTIVE_BG: Color = Color::new(0.1, 0.1, 0.1, 0.9);
pub const TAB_INDICATOR: Color = Color::new(0.098, 0.463, 0.824, 1.0);
pub const TEXT_PRIMARY: Color = Color::new(0.847, 0.847, 0.847, 1.0);
pub const TEXT_SECONDARY: Color = Color::new(0.600, 0.600, 0.600, 1.0);
pub const TEXT_KEYWORD: Color = Color::new(0.337, 0.612, 0.839, 1.0);
pub const TEXT_STRING: Color = Color::new(0.808, 0.596, 0.400, 1.0);
pub const TEXT_NUMBER: Color = Color::new(0.710, 0.808, 0.659, 1.0);
pub const HIGHLIGHT_BG: Color = Color::new(0.098, 0.463, 0.824, 0.15);
pub const HOVER_BG: Color = Color::new(1.0, 1.0, 1.0, 0.05);
pub const SEPARATOR: Color = Color::new(0.25, 0.25, 0.25, 1.0);
pub const RESIZE_HANDLE: Color = Color::new(0.098, 0.463, 0.824, 0.6);
pub const SELECTED_HIGHLIGHT: Color = Color::new(0.231, 0.510, 0.965, 0.15);
pub const HOVERED_HIGHLIGHT: Color = Color::new(0.976, 0.451, 0.086, 0.15);
pub const SELECTED_BORDER: Color = Color::new(0.231, 0.510, 0.965, 0.6);
pub const HOVERED_BORDER: Color = Color::new(0.976, 0.451, 0.086, 0.6);

pub const EVENT_HANDLED: Color = Color::new(0.298, 0.686, 0.314, 1.0);
pub const EVENT_CAPTURED: Color = Color::new(0.337, 0.612, 0.839, 1.0);
pub const EVENT_IGNORED: Color = Color::new(0.500, 0.500, 0.500, 1.0);

pub const PROF_LAYOUT: Color = Color::new(0.337, 0.612, 0.839, 0.8);
pub const PROF_DISPLAY_LIST: Color = Color::new(0.710, 0.808, 0.659, 0.8);
pub const PROF_RENDER: Color = Color::new(0.808, 0.596, 0.400, 0.8);
pub const PROF_LINE_60FPS: Color = Color::new(0.298, 0.686, 0.314, 0.5);

pub fn panel_rect(surface_size: Size, panel_width: f32) -> Rect {
    Rect::new(
        Point::new(surface_size.width - panel_width, 0.0),
        Size::new(panel_width, surface_size.height),
    )
}

pub fn content_rect(panel: Rect) -> Rect {
    Rect::new(
        Point::new(panel.origin.x + PADDING, panel.origin.y + TAB_BAR_HEIGHT + 1.0),
        Size::new(
            panel.size.width - PADDING * 2.0,
            panel.size.height - TAB_BAR_HEIGHT - 1.0,
        ),
    )
}

pub fn render_panel_background(list: &mut DisplayList, panel: Rect) {
    list.push_rect(panel, BG_COLOR, [0.0; 4]);
    let border = Rect::new(panel.origin, Size::new(1.0, panel.size.height));
    list.push_rect(border, SEPARATOR, [0.0; 4]);
}

pub fn render_tab_bar(list: &mut DisplayList, panel: Rect, active_tab: &DevToolsTab) {
    let tab_bar = Rect::new(panel.origin, Size::new(panel.size.width, TAB_BAR_HEIGHT));
    list.push_rect(tab_bar, TAB_BG, [0.0; 4]);

    let tabs = [
        (DevToolsTab::Inspector, "Elements"),
        (DevToolsTab::Styles, "Styles"),
        (DevToolsTab::Profiler, "Profiler"),
        (DevToolsTab::EventLog, "Events"),
    ];

    let tab_width = panel.size.width / tabs.len() as f32;
    for (i, (tab, label)) in tabs.iter().enumerate() {
        let x = panel.origin.x + i as f32 * tab_width;
        let tab_rect = Rect::new(Point::new(x, panel.origin.y), Size::new(tab_width, TAB_BAR_HEIGHT));

        if std::mem::discriminant(tab) == std::mem::discriminant(active_tab) {
            list.push_rect(tab_rect, TAB_ACTIVE_BG, [0.0; 4]);
            let indicator = Rect::new(
                Point::new(x, panel.origin.y + TAB_BAR_HEIGHT - 2.0),
                Size::new(tab_width, 2.0),
            );
            list.push_rect(indicator, TAB_INDICATOR, [0.0; 4]);
        }

        let text_rect = Rect::new(
            Point::new(x + 4.0, panel.origin.y + (TAB_BAR_HEIGHT - FONT_SIZE) / 2.0),
            Size::new(tab_width - 8.0, FONT_SIZE + 2.0),
        );
        let color = if std::mem::discriminant(tab) == std::mem::discriminant(active_tab) {
            TEXT_PRIMARY
        } else {
            TEXT_SECONDARY
        };
        list.push_text(label, text_rect, color, FONT_SIZE);
    }

    let sep = Rect::new(
        Point::new(panel.origin.x, panel.origin.y + TAB_BAR_HEIGHT),
        Size::new(panel.size.width, 1.0),
    );
    list.push_rect(sep, SEPARATOR, [0.0; 4]);
}

pub fn render_resize_handle(list: &mut DisplayList, panel: Rect, is_resizing: bool) {
    if is_resizing {
        let handle = Rect::new(
            Point::new(panel.origin.x - 1.0, panel.origin.y),
            Size::new(RESIZE_HANDLE_WIDTH, panel.size.height),
        );
        list.push_rect(handle, RESIZE_HANDLE, [0.0; 4]);
    }
}

pub fn hit_test_tab(pos: Point, panel: Rect) -> Option<DevToolsTab> {
    if pos.y < panel.origin.y || pos.y > panel.origin.y + TAB_BAR_HEIGHT {
        return None;
    }
    if pos.x < panel.origin.x || pos.x > panel.origin.x + panel.size.width {
        return None;
    }

    let tabs = [
        DevToolsTab::Inspector,
        DevToolsTab::Styles,
        DevToolsTab::Profiler,
        DevToolsTab::EventLog,
    ];
    let tab_width = panel.size.width / tabs.len() as f32;
    let rel_x = pos.x - panel.origin.x;
    let index = (rel_x / tab_width) as usize;
    tabs.get(index).cloned()
}
