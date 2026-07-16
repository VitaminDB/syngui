use crate::core::{Color, Point, Rect, Size};
use crate::render::DisplayList;

pub use crate::widgets::containers::page::ScrollbarPolicy;

#[derive(Clone, Copy, Debug, PartialEq)]
pub struct ScrollbarStyle {
    pub width: f32,
    pub thumb_color: Color,
    pub thumb_hover_color: Color,
    pub track_color: Color,
    pub corner_radius: f32,
    pub fade_delay: f32,
    pub fade_rate: f32,
    pub min_thumb_length: f32,
    pub policy: ScrollbarPolicy,
}

impl Default for ScrollbarStyle {
    fn default() -> Self {
        Self {
            width: 8.0,
            thumb_color: Color::from_hex("#9CA3AF"),
            thumb_hover_color: Color::from_hex("#9CA3AF").darken(0.3),
            track_color: Color::from_hex("#808080").with_alpha(0.0),
            corner_radius: 4.0,
            fade_delay: 1.5,
            fade_rate: 3.0,
            min_thumb_length: 30.0,
            policy: ScrollbarPolicy::Auto,
        }
    }
}

impl ScrollbarStyle {
    pub fn with_foreground(fg: Color) -> Self {
        Self {
            thumb_color: fg.with_alpha(0.4),
            thumb_hover_color: fg.with_alpha(0.7),
            track_color: fg.with_alpha(0.0),
            ..Self::default()
        }
    }
}

#[derive(Clone, Copy, Debug)]
pub struct ScrollbarFader {
    pub opacity: f32,
    pub idle_time: f32,
    pub hovered: bool,
    pub dragging: bool,
}

impl Default for ScrollbarFader {
    fn default() -> Self {
        Self {
            opacity: 0.0,
            idle_time: 0.0,
            hovered: false,
            dragging: false,
        }
    }
}

impl ScrollbarFader {
    pub fn flash(&mut self) {
        self.opacity = 1.0;
        self.idle_time = 0.0;
    }

    pub fn tick(&mut self, dt: f32, style: &ScrollbarStyle) -> bool {
        match style.policy {
            ScrollbarPolicy::Always => {
                if (self.opacity - 1.0).abs() > 1e-3 {
                    self.opacity = 1.0;
                    return true;
                }
                false
            }
            ScrollbarPolicy::Never => {
                if self.opacity > 1e-3 {
                    self.opacity = 0.0;
                    return true;
                }
                false
            }
            ScrollbarPolicy::Auto => {
                if self.opacity <= 0.0 {
                    return false;
                }
                if self.hovered || self.dragging {
                    self.idle_time = 0.0;
                    return false;
                }
                self.idle_time += dt;
                if self.idle_time > style.fade_delay {
                    self.opacity = (self.opacity - style.fade_rate * dt).max(0.0);
                    return true;
                }
                false
            }
        }
    }
}

pub fn effective_opacity(fader: &ScrollbarFader, style: &ScrollbarStyle) -> f32 {
    match style.policy {
        ScrollbarPolicy::Always => 1.0,
        ScrollbarPolicy::Never => 0.0,
        ScrollbarPolicy::Auto => fader.opacity,
    }
}

pub fn show_vertical(viewport: Rect, content_h: f32) -> bool {
    content_h > viewport.size.height + 0.5
}

pub fn show_horizontal(viewport: Rect, content_w: f32) -> bool {
    content_w > viewport.size.width + 0.5
}

pub fn vertical_thumb_rect(
    viewport: Rect,
    content_h: f32,
    scroll_y: f32,
    style: &ScrollbarStyle,
) -> Rect {
    let track_h = viewport.size.height.max(0.0);
    let visible = viewport.size.height.max(1.0);
    let ratio = (visible / content_h.max(visible)).clamp(0.0, 1.0);
    let thumb_h = (track_h * ratio).max(style.min_thumb_length).min(track_h);
    let max_scroll = (content_h - visible).max(0.0);
    let pos_ratio = if max_scroll > 0.0 { (scroll_y / max_scroll).clamp(0.0, 1.0) } else { 0.0 };
    let thumb_y = viewport.origin.y + (track_h - thumb_h) * pos_ratio;
    let thumb_x = viewport.origin.x + viewport.size.width - style.width;
    Rect::new(Point::new(thumb_x, thumb_y), Size::new(style.width, thumb_h))
}

pub fn horizontal_thumb_rect(
    viewport: Rect,
    content_w: f32,
    scroll_x: f32,
    style: &ScrollbarStyle,
) -> Rect {
    let track_w = viewport.size.width.max(0.0);
    let visible = viewport.size.width.max(1.0);
    let ratio = (visible / content_w.max(visible)).clamp(0.0, 1.0);
    let thumb_w = (track_w * ratio).max(style.min_thumb_length).min(track_w);
    let max_scroll = (content_w - visible).max(0.0);
    let pos_ratio = if max_scroll > 0.0 { (scroll_x / max_scroll).clamp(0.0, 1.0) } else { 0.0 };
    let thumb_x = viewport.origin.x + (track_w - thumb_w) * pos_ratio;
    let thumb_y = viewport.origin.y + viewport.size.height - style.width;
    Rect::new(Point::new(thumb_x, thumb_y), Size::new(thumb_w, style.width))
}

pub fn vertical_track_rect(viewport: Rect, style: &ScrollbarStyle) -> Rect {
    Rect::new(
        Point::new(viewport.origin.x + viewport.size.width - style.width, viewport.origin.y),
        Size::new(style.width, viewport.size.height),
    )
}

pub fn horizontal_track_rect(viewport: Rect, style: &ScrollbarStyle) -> Rect {
    Rect::new(
        Point::new(viewport.origin.x, viewport.origin.y + viewport.size.height - style.width),
        Size::new(viewport.size.width, style.width),
    )
}

pub fn render_vertical(
    list: &mut DisplayList,
    viewport: Rect,
    content_h: f32,
    scroll_y: f32,
    style: &ScrollbarStyle,
    fader: &ScrollbarFader,
    opacity: f32,
) {
    if opacity <= 0.0 || !show_vertical(viewport, content_h) {
        return;
    }
    let radius = [style.corner_radius; 4];

    let track_alpha_base = if fader.hovered || fader.dragging { style.track_color.a * opacity } else { 0.0 };
    if track_alpha_base > 0.001 {
        let track = vertical_track_rect(viewport, style);
        list.push_rect(track, style.track_color.with_alpha(track_alpha_base), radius);
    }

    let thumb = vertical_thumb_rect(viewport, content_h, scroll_y, style);
    let color = pick_thumb_color(style, fader, opacity);
    list.push_rect(thumb, color, radius);
}

pub fn render_horizontal(
    list: &mut DisplayList,
    viewport: Rect,
    content_w: f32,
    scroll_x: f32,
    style: &ScrollbarStyle,
    fader: &ScrollbarFader,
    opacity: f32,
) {
    if opacity <= 0.0 || !show_horizontal(viewport, content_w) {
        return;
    }
    let radius = [style.corner_radius; 4];

    let track_alpha_base = if fader.hovered || fader.dragging { style.track_color.a * opacity } else { 0.0 };
    if track_alpha_base > 0.001 {
        let track = horizontal_track_rect(viewport, style);
        list.push_rect(track, style.track_color.with_alpha(track_alpha_base), radius);
    }

    let thumb = horizontal_thumb_rect(viewport, content_w, scroll_x, style);
    let color = pick_thumb_color(style, fader, opacity);
    list.push_rect(thumb, color, radius);
}

fn pick_thumb_color(style: &ScrollbarStyle, fader: &ScrollbarFader, opacity: f32) -> Color {
    let base = if fader.dragging || fader.hovered { style.thumb_hover_color } else { style.thumb_color };
    base.with_alpha(base.a * opacity)
}

pub const SCROLLBAR_HIT_MARGIN: f32 = 12.0;

#[derive(Clone, Copy, Debug)]
pub struct ScrollbarGeom {
    pub viewport: Rect,
    pub content_w: f32,
    pub content_h: f32,
    pub scroll_x: f32,
    pub scroll_y: f32,
}

impl ScrollbarGeom {
    pub fn max_scroll_y(&self) -> f32 {
        (self.content_h - self.viewport.size.height).max(0.0)
    }

    pub fn max_scroll_x(&self) -> f32 {
        (self.content_w - self.viewport.size.width).max(0.0)
    }

    pub fn show_v(&self) -> bool {
        show_vertical(self.viewport, self.content_h)
    }

    pub fn show_h(&self) -> bool {
        show_horizontal(self.viewport, self.content_w)
    }
}

#[derive(Default, Clone, Copy, Debug)]
pub struct ScrollbarInteraction {
    dragging_v: bool,
    dragging_h: bool,
    hover_thumb_v: bool,
    hover_thumb_h: bool,
    hover_area: bool,
    grab_offset_v: f32,
    grab_offset_h: f32,
}

impl ScrollbarInteraction {
    pub fn dragging(&self) -> bool { self.dragging_v || self.dragging_h }
    pub fn dragging_vertical(&self) -> bool { self.dragging_v }
    pub fn dragging_horizontal(&self) -> bool { self.dragging_h }
    pub fn hover_thumb_v(&self) -> bool { self.hover_thumb_v }
    pub fn hover_thumb_h(&self) -> bool { self.hover_thumb_h }
    pub fn hover_area(&self) -> bool { self.hover_area }

    pub fn try_begin_drag(
        &mut self,
        fader: &mut ScrollbarFader,
        geom: &ScrollbarGeom,
        style: &ScrollbarStyle,
        pos: Point,
    ) -> bool {
        if geom.show_v() {
            let thumb = vertical_thumb_rect(geom.viewport, geom.content_h, geom.scroll_y, style);
            if thumb.contains(pos) {
                self.dragging_v = true;
                self.grab_offset_v = pos.y - thumb.origin.y;
                fader.dragging = true;
                fader.flash();
                return true;
            }
        }
        if geom.show_h() {
            let thumb = horizontal_thumb_rect(geom.viewport, geom.content_w, geom.scroll_x, style);
            if thumb.contains(pos) {
                self.dragging_h = true;
                self.grab_offset_h = pos.x - thumb.origin.x;
                fader.dragging = true;
                fader.flash();
                return true;
            }
        }
        false
    }

    pub fn update_drag(
        &mut self,
        fader: &mut ScrollbarFader,
        geom: &ScrollbarGeom,
        style: &ScrollbarStyle,
        pos: Point,
    ) -> Option<(f32, f32)> {
        if !self.dragging() {
            return None;
        }
        let mut new_y = geom.scroll_y;
        let mut new_x = geom.scroll_x;

        if self.dragging_v {
            let track_h = geom.viewport.size.height.max(0.0);
            let visible = geom.viewport.size.height.max(1.0);
            let ratio = (visible / geom.content_h.max(visible)).clamp(0.0, 1.0);
            let thumb_h = (track_h * ratio).max(style.min_thumb_length).min(track_h);
            let max_y = geom.max_scroll_y();
            let track_remain = (track_h - thumb_h).max(0.0);
            if track_remain > 0.5 {
                let thumb_top = (pos.y - geom.viewport.origin.y - self.grab_offset_v)
                    .clamp(0.0, track_remain);
                let pos_ratio = thumb_top / track_remain;
                new_y = (pos_ratio * max_y).clamp(0.0, max_y);
            }
        }
        if self.dragging_h {
            let track_w = geom.viewport.size.width.max(0.0);
            let visible = geom.viewport.size.width.max(1.0);
            let ratio = (visible / geom.content_w.max(visible)).clamp(0.0, 1.0);
            let thumb_w = (track_w * ratio).max(style.min_thumb_length).min(track_w);
            let max_x = geom.max_scroll_x();
            let track_remain = (track_w - thumb_w).max(0.0);
            if track_remain > 0.5 {
                let thumb_left = (pos.x - geom.viewport.origin.x - self.grab_offset_h)
                    .clamp(0.0, track_remain);
                let pos_ratio = thumb_left / track_remain;
                new_x = (pos_ratio * max_x).clamp(0.0, max_x);
            }
        }

        fader.flash();
        Some((new_y, new_x))
    }

    pub fn end_drag(&mut self, fader: &mut ScrollbarFader) -> bool {
        if !self.dragging() {
            return false;
        }
        self.dragging_v = false;
        self.dragging_h = false;
        fader.dragging = false;
        true
    }

    pub fn update_hover(
        &mut self,
        fader: &mut ScrollbarFader,
        geom: &ScrollbarGeom,
        style: &ScrollbarStyle,
        pos: Point,
        hit_margin: f32,
    ) -> bool {
        let was_thumb_v = self.hover_thumb_v;
        let was_thumb_h = self.hover_thumb_h;
        let was_area = self.hover_area;
        let was_fader_hovered = fader.hovered;

        let mut new_thumb_v = false;
        let mut new_thumb_h = false;
        let mut new_area = false;

        let vp = geom.viewport;
        let inside_vp = pos.x >= vp.origin.x
            && pos.x <= vp.origin.x + vp.size.width
            && pos.y >= vp.origin.y
            && pos.y <= vp.origin.y + vp.size.height;

        if geom.show_v() {
            let thumb = vertical_thumb_rect(vp, geom.content_h, geom.scroll_y, style);
            new_thumb_v = thumb.contains(pos);
            if inside_vp {
                let area_x_start = vp.origin.x + vp.size.width - style.width - hit_margin;
                if pos.x >= area_x_start && pos.x <= vp.origin.x + vp.size.width {
                    new_area = true;
                }
            }
        }
        if geom.show_h() {
            let thumb = horizontal_thumb_rect(vp, geom.content_w, geom.scroll_x, style);
            new_thumb_h = thumb.contains(pos);
            if inside_vp {
                let area_y_start = vp.origin.y + vp.size.height - style.width - hit_margin;
                if pos.y >= area_y_start && pos.y <= vp.origin.y + vp.size.height {
                    new_area = true;
                }
            }
        }

        self.hover_thumb_v = new_thumb_v;
        self.hover_thumb_h = new_thumb_h;
        self.hover_area = new_area;
        let fader_hovered = new_thumb_v || new_thumb_h || new_area;
        fader.hovered = fader_hovered;

        let changed = was_thumb_v != new_thumb_v
            || was_thumb_h != new_thumb_h
            || was_area != new_area
            || was_fader_hovered != fader_hovered;
        if fader_hovered && !was_fader_hovered {
            fader.flash();
        }
        changed
    }

    pub fn clear_hover(&mut self, fader: &mut ScrollbarFader) -> bool {
        let was = self.hover_thumb_v || self.hover_thumb_h || self.hover_area || fader.hovered;
        self.hover_thumb_v = false;
        self.hover_thumb_h = false;
        self.hover_area = false;
        fader.hovered = false;
        was
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn vp(w: f32, h: f32) -> Rect { Rect::new(Point::zero(), Size::new(w, h)) }

    #[test]
    fn vertical_thumb_clamps_to_track_when_content_smaller() {
        let style = ScrollbarStyle::default();
        let thumb = vertical_thumb_rect(vp(100.0, 200.0), 100.0, 0.0, &style);
        assert!(thumb.size.height >= style.min_thumb_length);
        assert!(thumb.size.height <= 200.0);
    }

    #[test]
    fn vertical_thumb_at_max_scroll_touches_bottom() {
        let style = ScrollbarStyle::default();
        let viewport = vp(100.0, 100.0);
        let content_h = 400.0;
        let max_scroll = content_h - viewport.size.height;
        let thumb = vertical_thumb_rect(viewport, content_h, max_scroll, &style);
        let thumb_bottom = thumb.origin.y + thumb.size.height;
        let track_bottom = viewport.origin.y + viewport.size.height;
        assert!((thumb_bottom - track_bottom).abs() < 0.5,
            "thumb_bottom={} track_bottom={}", thumb_bottom, track_bottom);
    }

    #[test]
    fn fader_auto_fades_after_delay() {
        let mut style = ScrollbarStyle::default();
        style.policy = ScrollbarPolicy::Auto;
        style.fade_delay = 0.1;
        style.fade_rate = 1.0;
        let mut fader = ScrollbarFader::default();
        fader.flash();
        assert_eq!(fader.opacity, 1.0);

        let _ = fader.tick(0.05, &style);
        assert_eq!(fader.opacity, 1.0);

        for _ in 0..20 {
            fader.tick(0.05, &style);
        }
        assert!(fader.opacity < 1.0, "opacity={} (must decay)", fader.opacity);
    }

    #[test]
    fn fader_always_keeps_visible() {
        let mut style = ScrollbarStyle::default();
        style.policy = ScrollbarPolicy::Always;
        let mut fader = ScrollbarFader::default();
        fader.tick(1.0, &style);
        assert_eq!(fader.opacity, 1.0);
    }

    #[test]
    fn fader_never_hides() {
        let mut style = ScrollbarStyle::default();
        style.policy = ScrollbarPolicy::Never;
        let mut fader = ScrollbarFader::default();
        fader.flash();
        fader.tick(0.0, &style);
        assert_eq!(fader.opacity, 0.0);
    }

    #[test]
    fn hovered_resets_idle_timer() {
        let mut style = ScrollbarStyle::default();
        style.policy = ScrollbarPolicy::Auto;
        style.fade_delay = 0.1;
        let mut fader = ScrollbarFader::default();
        fader.flash();
        fader.hovered = true;
        for _ in 0..100 {
            fader.tick(0.1, &style);
        }
        assert_eq!(fader.opacity, 1.0, "hovered scrollbar must not fade");
    }

    fn geom(content_h: f32, scroll_y: f32) -> ScrollbarGeom {
        ScrollbarGeom {
            viewport: vp(100.0, 100.0),
            content_w: 100.0,
            content_h,
            scroll_x: 0.0,
            scroll_y,
        }
    }

    #[test]
    fn interaction_begin_drag_hits_thumb() {
        let style = ScrollbarStyle::default();
        let mut fader = ScrollbarFader::default();
        let mut it = ScrollbarInteraction::default();
        let g = geom(400.0, 0.0);
        let pos = Point::new(95.0, 5.0);
        assert!(it.try_begin_drag(&mut fader, &g, &style, pos));
        assert!(it.dragging());
        assert!(fader.dragging);
        assert!(fader.opacity > 0.99);
    }

    #[test]
    fn interaction_begin_drag_misses_outside_thumb() {
        let style = ScrollbarStyle::default();
        let mut fader = ScrollbarFader::default();
        let mut it = ScrollbarInteraction::default();
        let g = geom(400.0, 0.0);
        let pos = Point::new(20.0, 50.0);
        assert!(!it.try_begin_drag(&mut fader, &g, &style, pos));
        assert!(!it.dragging());
        assert!(!fader.dragging);
    }

    #[test]
    fn interaction_drag_updates_scroll_proportionally() {
        let style = ScrollbarStyle::default();
        let mut fader = ScrollbarFader::default();
        let mut it = ScrollbarInteraction::default();
        let g = geom(400.0, 0.0);
        assert!(it.try_begin_drag(&mut fader, &g, &style, Point::new(95.0, 15.0)));
        let res = it.update_drag(&mut fader, &g, &style, Point::new(95.0, 50.0));
        let (new_y, _) = res.expect("drag active");
        assert!((new_y - 150.0).abs() < 1.0, "new_y={new_y}, expected ≈150");
    }

    #[test]
    fn interaction_drag_clamps_to_max() {
        let style = ScrollbarStyle::default();
        let mut fader = ScrollbarFader::default();
        let mut it = ScrollbarInteraction::default();
        let g = geom(400.0, 0.0);
        assert!(it.try_begin_drag(&mut fader, &g, &style, Point::new(95.0, 15.0)));
        let (new_y, _) = it.update_drag(&mut fader, &g, &style, Point::new(95.0, 9999.0)).unwrap();
        assert!((new_y - 300.0).abs() < 1.0, "new_y={new_y} should clamp to 300");
        let (new_y, _) = it.update_drag(&mut fader, &g, &style, Point::new(95.0, -9999.0)).unwrap();
        assert!(new_y.abs() < 1.0, "new_y={new_y} should clamp to 0");
    }

    #[test]
    fn interaction_end_drag_resets_state() {
        let style = ScrollbarStyle::default();
        let mut fader = ScrollbarFader::default();
        let mut it = ScrollbarInteraction::default();
        let g = geom(400.0, 0.0);
        it.try_begin_drag(&mut fader, &g, &style, Point::new(95.0, 5.0));
        assert!(it.end_drag(&mut fader));
        assert!(!it.dragging());
        assert!(!fader.dragging);
        assert!(!it.end_drag(&mut fader));
    }

    #[test]
    fn interaction_update_drag_returns_none_when_idle() {
        let style = ScrollbarStyle::default();
        let mut fader = ScrollbarFader::default();
        let mut it = ScrollbarInteraction::default();
        let g = geom(400.0, 0.0);
        assert!(it.update_drag(&mut fader, &g, &style, Point::new(50.0, 50.0)).is_none());
    }

    #[test]
    fn interaction_hover_detects_thumb_and_area() {
        let style = ScrollbarStyle::default();
        let mut fader = ScrollbarFader::default();
        let mut it = ScrollbarInteraction::default();
        let g = geom(400.0, 0.0);
        assert!(it.update_hover(&mut fader, &g, &style, Point::new(95.0, 5.0), 12.0));
        assert!(it.hover_thumb_v());
        assert!(it.hover_area());
        assert!(fader.hovered);
        assert!(it.update_hover(&mut fader, &g, &style, Point::new(85.0, 50.0), 12.0));
        assert!(!it.hover_thumb_v());
        assert!(it.hover_area());
        assert!(fader.hovered);
        let _ = it.update_hover(&mut fader, &g, &style, Point::new(50.0, 50.0), 12.0);
        assert!(!it.hover_thumb_v());
        assert!(!it.hover_area());
        assert!(!fader.hovered);
    }

    #[test]
    fn interaction_clear_hover_resets_state() {
        let style = ScrollbarStyle::default();
        let mut fader = ScrollbarFader::default();
        let mut it = ScrollbarInteraction::default();
        let g = geom(400.0, 0.0);
        it.update_hover(&mut fader, &g, &style, Point::new(95.0, 5.0), 12.0);
        assert!(it.clear_hover(&mut fader));
        assert!(!it.hover_thumb_v());
        assert!(!it.hover_area());
        assert!(!fader.hovered);
        assert!(!it.clear_hover(&mut fader));
    }
}
