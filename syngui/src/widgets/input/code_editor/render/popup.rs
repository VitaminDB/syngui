//! Рисование подсказок языкового сервера: волнистое подчёркивание
//! диагностик, метка в gutter'е, панель всплывающего окна.

use super::super::theme::Theme;
use crate::core::{Color, Point, Rect, RectExt, Size};
use crate::mss::{MssFields, TextAlign, TextDecoration};
use crate::render::{Border, DisplayList};

/// Скругление всплывающих окон — как у панели поиска.
pub const POPUP_RADIUS: f32 = 6.0;

/// Волнистая линия от `x0` до `x1` около `y` (базовая линия подчёркивания).
pub fn paint_wavy_underline(list: &mut DisplayList, x0: f32, x1: f32, y: f32, color: Color) {
    let x1 = x1.max(x0 + 3.0);
    const STEP: f32 = 2.0;
    const AMP: f32 = 1.2;
    let mut pts = Vec::with_capacity(((x1 - x0) / STEP) as usize + 2);
    let mut x = x0;
    let mut up = true;
    while x < x1 {
        pts.push([x, if up { y - AMP } else { y + AMP }]);
        up = !up;
        x += STEP;
    }
    pts.push([x1, if up { y - AMP } else { y + AMP }]);
    list.push_line_strip(pts, color, 1.0);
}

/// Прямое подчёркивание (слово под мышью с зажатым Ctrl).
pub fn paint_straight_underline(list: &mut DisplayList, x0: f32, x1: f32, y: f32, color: Color) {
    if x1 <= x0 {
        return;
    }
    list.push_rect(Rect::new(Point::new(x0, y), Size::new(x1 - x0, 1.0)), color, [0.0; 4]);
}

/// Метка диагностики в gutter'е: точка у левого края строки.
pub fn paint_gutter_marker(list: &mut DisplayList, x: f32, line_y: f32, line_height: f32, color: Color) {
    let d = 6.0;
    let rect = Rect::new(Point::new(x + 5.0, line_y + (line_height - d) / 2.0), Size::new(d, d));
    list.push_rect(rect, color, [d / 2.0; 4]);
}

/// Фон всплывающего окна: тень, заливка, рамка.
pub fn paint_panel(list: &mut DisplayList, rect: Rect, theme: &Theme, mss: &MssFields) {
    list.push_shadow(
        rect,
        Color::new(0.0, 0.0, 0.0, 0.35),
        10.0,
        (0.0, 3.0),
        [POPUP_RADIUS; 4],
    );
    list.push_rect_bordered(
        rect,
        theme.popup_bg(mss),
        [POPUP_RADIUS; 4],
        Border::new(1.0, theme.popup_border(mss)),
    );
}

/// Текст одной строкой шрифтом редактора (моноширинным, если задан).
pub fn push_text(
    list: &mut DisplayList,
    text: &str,
    rect: Rect,
    color: Color,
    font_size: f32,
    font_family: Option<&str>,
) {
    if text.is_empty() {
        return;
    }
    let align = TextAlign::LEFT | TextAlign::VCENTER;
    match font_family {
        Some(f) => list.push_text_styled(
            text,
            rect,
            color,
            font_size,
            align,
            TextDecoration::None,
            400,
            Some(f.to_string()),
        ),
        None => list.push_text_aligned(text, rect, color, font_size, align, TextDecoration::None, 400),
    }
}

/// Вертикальный ползунок у правого края окна со списком.
pub fn paint_list_thumb(
    list: &mut DisplayList,
    rect: Rect,
    top: usize,
    visible: usize,
    total: usize,
    color: Color,
) {
    if total <= visible || total == 0 {
        return;
    }
    let track_h = rect.size.height - 8.0;
    let h = (track_h * visible as f32 / total as f32).max(12.0);
    let max_top = total - visible;
    let y = rect.y() + 4.0 + (track_h - h) * (top as f32 / max_top as f32);
    list.push_rect(
        Rect::new(Point::new(rect.x() + rect.size.width - 5.0, y), Size::new(3.0, h)),
        color,
        [1.5; 4],
    );
}
