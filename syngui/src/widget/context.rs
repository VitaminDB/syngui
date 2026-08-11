use crate::core::{Rect, Size};
use crate::input::{CursorIcon, DragData};
use crate::widget::{ElementId, DirtyFlags};
use std::sync::Arc;
#[cfg(feature = "clipboard")]
use crate::core::sync::Mutex;

pub trait TextMeasure: Send + Sync {
    fn measure_text_width(&self, text: &str, font_size: f32, char_count: usize) -> f32;

    fn measure_text_width_styled(&self, text: &str, font_size: f32, char_count: usize, bold: bool, font_family: Option<&str>) -> f32 {
        let _ = (bold, font_family);
        self.measure_text_width(text, font_size, char_count)
    }

    fn measure_text_width_styled_ls(
        &self,
        text: &str,
        font_size: f32,
        char_count: usize,
        bold: bool,
        font_family: Option<&str>,
        letter_spacing: f32,
    ) -> f32 {
        let _ = letter_spacing;
        self.measure_text_width_styled(text, font_size, char_count, bold, font_family)
    }

    fn hit_test_char(&self, text: &str, font_size: f32, x_offset: f32) -> usize;

    fn hit_test_char_styled(&self, text: &str, font_size: f32, x_offset: f32, font_family: Option<&str>) -> usize {
        let _ = font_family;
        self.hit_test_char(text, font_size, x_offset)
    }
}

#[derive(Debug)]
pub struct UpdateContext {
    pub element_id: ElementId,
    pub needs_layout: bool,
    pub needs_render: bool,
}

impl UpdateContext {
    pub fn new(element_id: ElementId) -> Self {
        Self {
            element_id,
            needs_layout: false,
            needs_render: false,
        }
    }

    pub fn mark_layout_dirty(&mut self) {
        self.needs_layout = true;
    }

    pub fn mark_render_dirty(&mut self) {
        self.needs_render = true;
    }
}

pub struct EventContext {
    pub element_id: ElementId,
    pub cursor_position: crate::core::Point,
    pub modifiers: crate::input::Modifiers,
    pub captured: bool,
    needs_paint: bool,
    needs_layout: bool,
    dirty_flags: DirtyFlags,
    pub(crate) overlay_register: Option<(Rect, bool)>,
    pub(crate) overlay_unregister: bool,
    pub(crate) start_drag: Option<DragData>,
    pub(crate) cursor_icon: Option<CursorIcon>,
    pub(crate) text_measure: Option<Arc<dyn TextMeasure>>,
    #[cfg(feature = "clipboard")]
    pub(crate) clipboard: Option<Arc<Mutex<arboard::Clipboard>>>,
    viewport_size: Size,
    pub(crate) show_virtual_keyboard: Option<bool>,
    pub(crate) numeric_keyboard: Option<bool>,
    pub(crate) focused_text: Option<String>,
    pub(crate) scroll_into_view_request: Option<crate::core::Rect>,
    pub(crate) start_window_drag: bool,
    pub(crate) close_window: bool,
    pub(crate) minimize_window: bool,
    pub(crate) toggle_maximize_window: bool,
    pub(crate) toggle_fullscreen_window: bool,
    pub(crate) hide_window: bool,
    pub(crate) show_window: bool,
    pub(crate) toggle_window_visibility: bool,
    window_flags: u8,
}

impl EventContext {
    pub fn new(element_id: ElementId) -> Self {
        Self {
            element_id,
            cursor_position: crate::core::Point::zero(),
            modifiers: crate::input::Modifiers::empty(),
            captured: false,
            needs_paint: false,
            needs_layout: false,
            dirty_flags: DirtyFlags::empty(),
            overlay_register: None,
            overlay_unregister: false,
            start_drag: None,
            cursor_icon: None,
            text_measure: None,
            #[cfg(feature = "clipboard")]
            clipboard: None,
            viewport_size: Size::new(1280.0, 720.0),
            show_virtual_keyboard: None,
            numeric_keyboard: None,
            focused_text: None,
            scroll_into_view_request: None,
            start_window_drag: false,
            close_window: false,
            minimize_window: false,
            toggle_maximize_window: false,
            toggle_fullscreen_window: false,
            hide_window: false,
            show_window: false,
            toggle_window_visibility: false,
            window_flags: 0,
        }
    }

    pub fn capture(&mut self) {
        self.captured = true;
    }

    pub(crate) fn take_dirty_flags(&mut self) -> DirtyFlags {
        let flags = self.dirty_flags;
        self.dirty_flags = DirtyFlags::empty();
        flags
    }

    pub(crate) fn has_side_effects(&self) -> bool {
        self.overlay_register.is_some()
            || self.overlay_unregister
            || self.start_drag.is_some()
            || self.cursor_icon.is_some()
            || self.show_virtual_keyboard.is_some()
            || self.numeric_keyboard.is_some()
            || self.focused_text.is_some()
            || self.scroll_into_view_request.is_some()
            || self.start_window_drag
            || self.close_window
            || self.minimize_window
            || self.toggle_maximize_window
            || self.toggle_fullscreen_window
            || self.hide_window
            || self.show_window
            || self.toggle_window_visibility
    }

    pub fn register_overlay(&mut self, bounds: Rect, modal: bool) {
        self.overlay_register = Some((bounds, modal));
    }

    pub fn unregister_overlay(&mut self) {
        self.overlay_unregister = true;
    }

    pub fn start_drag(&mut self, data: DragData) {
        self.start_drag = Some(data);
    }

    pub fn start_window_drag(&mut self) {
        self.start_window_drag = true;
    }

    pub fn close_window(&mut self) {
        self.close_window = true;
    }

    pub fn minimize_window(&mut self) {
        self.minimize_window = true;
    }

    pub fn toggle_maximize_window(&mut self) {
        self.toggle_maximize_window = true;
    }

    pub fn toggle_fullscreen_window(&mut self) {
        self.toggle_fullscreen_window = true;
    }

    pub fn hide_window(&mut self) {
        self.hide_window = true;
    }

    pub fn show_window(&mut self) {
        self.show_window = true;
    }

    pub fn toggle_window_visibility(&mut self) {
        self.toggle_window_visibility = true;
    }

    pub fn set_cursor(&mut self, icon: CursorIcon) {
        self.cursor_icon = Some(icon);
    }

    pub fn set_virtual_keyboard_visible(&mut self, visible: bool) {
        self.show_virtual_keyboard = Some(visible);
    }

    /// Тип экранной клавиатуры: `true` — цифровая, `false` — обычная (текст).
    pub fn set_numeric_keyboard(&mut self, numeric: bool) {
        self.numeric_keyboard = Some(numeric);
    }

    pub fn set_focused_text(&mut self, text: String) {
        self.focused_text = Some(text);
    }

    pub fn scroll_into_view(&mut self, rect: crate::core::Rect) {
        self.scroll_into_view_request = Some(rect);
    }

    pub fn set_text_measure(&mut self, tm: Arc<dyn TextMeasure>) {
        self.text_measure = Some(tm);
    }

    #[cfg(feature = "clipboard")]
    pub fn copy_to_clipboard(&self, text: &str) {
        if let Some(ref cb) = self.clipboard {
            if let Ok(mut clipboard) = cb.lock() {
                let _ = clipboard.set_text(text.to_string());
            }
        }
    }

    #[cfg(feature = "clipboard")]
    pub fn paste_from_clipboard(&self) -> Option<String> {
        self.clipboard.as_ref().and_then(|cb| {
            cb.lock().ok().and_then(|mut clipboard| clipboard.get_text().ok())
        })
    }

    #[cfg(not(feature = "clipboard"))]
    pub fn copy_to_clipboard(&self, _text: &str) {}

    #[cfg(not(feature = "clipboard"))]
    pub fn paste_from_clipboard(&self) -> Option<String> { None }

    pub fn set_viewport_size(&mut self, size: Size) {
        self.viewport_size = size;
    }

    pub fn viewport_size(&self) -> Size {
        self.viewport_size
    }

    pub fn set_window_flags(&mut self, flags: u8) {
        self.window_flags = flags;
    }

    pub fn window_flags(&self) -> u8 {
        self.window_flags
    }

    pub fn is_window_maximized(&self) -> bool {
        self.window_flags & crate::mss::window_flags::MAXIMIZED != 0
    }

    pub fn is_window_fullscreen(&self) -> bool {
        self.window_flags & crate::mss::window_flags::FULLSCREEN != 0
    }

    pub fn is_window_focused(&self) -> bool {
        self.window_flags & crate::mss::window_flags::FOCUSED != 0
    }

    pub fn measure_text_width(&self, text: &str, font_size: f32, char_count: usize) -> Option<f32> {
        self.text_measure.as_ref().map(|tm| tm.measure_text_width(text, font_size, char_count))
    }

    pub fn measure_text_width_ls(
        &self,
        text: &str,
        font_size: f32,
        char_count: usize,
        bold: bool,
        font_family: Option<&str>,
        letter_spacing: f32,
    ) -> Option<f32> {
        self.text_measure.as_ref().map(|tm| {
            tm.measure_text_width_styled_ls(text, font_size, char_count, bold, font_family, letter_spacing)
        })
    }

    pub fn hit_test_char(&self, text: &str, font_size: f32, x_offset: f32) -> Option<usize> {
        self.text_measure.as_ref().map(|tm| tm.hit_test_char(text, font_size, x_offset))
    }
}

pub trait EventContextExt {
    fn request_paint(&mut self);
    
    fn request_layout(&mut self);
    
    fn mark_dirty(&mut self, flags: DirtyFlags);
    
    fn needs_paint(&self) -> bool;
    
    fn needs_layout(&self) -> bool;
}

impl EventContextExt for EventContext {
    fn request_paint(&mut self) {
        self.needs_paint = true;
        self.dirty_flags |= DirtyFlags::RENDER;
    }
    
    fn request_layout(&mut self) {
        self.needs_layout = true;
        self.dirty_flags |= DirtyFlags::LAYOUT;
    }
    
    fn mark_dirty(&mut self, flags: DirtyFlags) {
        self.dirty_flags |= flags;
        if flags.contains(DirtyFlags::RENDER) {
            self.needs_paint = true;
        }
        if flags.contains(DirtyFlags::LAYOUT) {
            self.needs_layout = true;
        }
    }
    
    fn needs_paint(&self) -> bool {
        self.needs_paint
    }
    
    fn needs_layout(&self) -> bool {
        self.needs_layout
    }
}

#[derive(Debug)]
pub struct BuildContext {
    pub parent_id: Option<ElementId>,
    pub depth: u32,
}

impl BuildContext {
    pub fn root() -> Self {
        Self {
            parent_id: None,
            depth: 0,
        }
    }

    pub fn child(&self) -> Self {
        Self {
            parent_id: Some(self.parent_id.unwrap_or(ElementId(0))),
            depth: self.depth + 1,
        }
    }
}

#[derive(Debug)]
pub struct RenderContext {
    pub element_id: ElementId,
    pub clip_rect: crate::core::Rect,
}

impl RenderContext {
    pub fn new(element_id: ElementId, clip_rect: crate::core::Rect) -> Self {
        Self {
            element_id,
            clip_rect,
        }
    }

    pub fn clip_rect(&self) -> crate::core::Rect {
        self.clip_rect
    }
}

#[derive(Debug)]
pub struct AnimationContext {
    pub element_id: ElementId,
    pub needs_repaint: bool,
}

impl AnimationContext {
    pub fn new(element_id: ElementId) -> Self {
        Self {
            element_id,
            needs_repaint: false,
        }
    }

    pub fn request_repaint(&mut self) {
        self.needs_repaint = true;
    }
}
