use crate::core::{Color, Point, Rect, RectExt, Size};
use crate::input::{Event, EventResult, MouseButton};
use crate::layout::Constraints;
use crate::mss::{ComputedStyle, Dimension};
use crate::mss::MssFields;
use crate::render::{Border, DisplayList};
use crate::widget::context::{EventContext, EventContextExt};
use crate::widget::{DirtyFlags, Element, ElementId, ElementTree, StyledElement, UpdateContext, Widget};
use std::any::Any;
use std::cell::Cell;
use std::sync::Arc;
use crate::core::sync::Mutex;
use crate::signal::{RwSignal, use_signal};

/// Локализованные подписи кнопок встроенных диалогов: (подтвердить, отмена).
static DIALOG_LABELS: std::sync::Mutex<Option<(String, String)>> = std::sync::Mutex::new(None);

/// Задать локализованные подписи кнопок для `ConfirmDialog`/`AlertDialog`.
/// Приложение вызывает это один раз (и при смене языка). По умолчанию — «OK» / «Cancel».
pub fn set_dialog_labels(confirm: impl Into<String>, cancel: impl Into<String>) {
    if let Ok(mut g) = DIALOG_LABELS.lock() {
        *g = Some((confirm.into(), cancel.into()));
    }
}

fn dialog_labels() -> (String, String) {
    DIALOG_LABELS
        .lock()
        .ok()
        .and_then(|g| g.clone())
        .unwrap_or_else(|| (crate::i18n::builtin("dialog.ok", "OK"), crate::i18n::builtin("dialog.cancel", "Cancel")))
}

#[derive(Clone)]
pub struct DialogAction {
    pub label: String,
    pub primary: bool,
    pub on_click: Arc<Mutex<dyn FnMut() + Send>>,
}

impl DialogAction {
    pub fn new(label: impl Into<String>, callback: impl FnMut() + Send + 'static) -> Self {
        Self {
            label: label.into(),
            primary: false,
            on_click: Arc::new(Mutex::new(callback)),
        }
    }

    pub fn primary(mut self) -> Self {
        self.primary = true;
        self
    }
}

impl std::fmt::Debug for DialogAction {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("DialogAction")
            .field("label", &self.label)
            .field("primary", &self.primary)
            .finish()
    }
}

pub struct Dialog {
    pub title: String,
    pub icon: Option<String>,
    pub body: String,
    pub actions: Vec<DialogAction>,
    pub is_open: RwSignal<bool>,
    pub width: Dimension,
    pub on_close: Option<Arc<Mutex<dyn FnMut() + Send>>>,
}

impl Dialog {
    pub fn new(title: impl Into<String>) -> Self {
        Self {
            title: title.into(),
            icon: None,
            body: String::new(),
            actions: Vec::new(),
            is_open: use_signal(false),
            width: Dimension::Px(400.0),
            on_close: None,
        }
    }

    pub fn icon(mut self, icon: impl Into<String>) -> Self {
        self.icon = Some(icon.into());
        self
    }

    pub fn body(mut self, body: impl Into<String>) -> Self {
        self.body = body.into();
        self
    }

    pub fn action(mut self, action: DialogAction) -> Self {
        self.actions.push(action);
        self
    }

    pub fn is_open(mut self, state: RwSignal<bool>) -> Self {
        self.is_open = state;
        self
    }

    pub fn width(mut self, width: f32) -> Self {
        self.width = Dimension::Px(width);
        self
    }

    pub fn on_close(mut self, callback: impl FnMut() + Send + 'static) -> Self {
        self.on_close = Some(Arc::new(Mutex::new(callback)));
        self
    }
}

impl Widget for Dialog {
    fn create_element(&self) -> Box<dyn Element> {
        Box::new(DialogElement {
            id: ElementId::new(),
            title: self.title.clone(),
            icon: self.icon.clone(),
            body: self.body.clone(),
            actions: self.actions.clone(),
            is_open: self.is_open,
            width: self.width,
            on_close: self.on_close.clone(),
            bounds: Rect::zero(),
            viewport_size: Cell::new(Size::zero()),
            hover_action: None,
            // Класс по умолчанию — чтобы приложение могло темизировать все диалоги
            // одним правилом `.dialog { ... }` в своих стилях.
            classes: vec!["dialog".to_string()],
            dirty_flags: DirtyFlags::LAYOUT | DirtyFlags::RENDER,
            overlay_registered: false,
            mss: MssFields::new(),
        })
    }

    fn can_update(&self, other: &dyn Any) -> bool {
        other.is::<Self>()
    }

    fn as_any(&self) -> &dyn Any { self }
    fn as_any_mut(&mut self) -> &mut dyn Any { self }
    fn mount(&self, _tree: &mut ElementTree, _parent_id: ElementId) {}
}

struct DialogElement {
    id: ElementId,
    title: String,
    icon: Option<String>,
    body: String,
    actions: Vec<DialogAction>,
    is_open: RwSignal<bool>,
    width: Dimension,
    on_close: Option<Arc<Mutex<dyn FnMut() + Send>>>,
    bounds: Rect,
    viewport_size: Cell<Size>,
    hover_action: Option<usize>,
    classes: Vec<String>,
    dirty_flags: DirtyFlags,
    overlay_registered: bool,
    mss: MssFields,
}

impl DialogElement {
    fn dialog_rect_for(&self, viewport: Size) -> Rect {
        let resolved_width = self.mss.width
            .map(|d| d.resolve(viewport.width))
            .unwrap_or_else(|| self.width.resolve(viewport.width));
        let title_height = 48.0;
        let body_height = if self.body.is_empty() { 0.0 } else { 60.0 };
        let actions_height = if self.actions.is_empty() { 0.0 } else { 56.0 };
        let dialog_height = title_height + body_height + actions_height + 16.0;

        let x = (viewport.width - resolved_width) / 2.0;
        let y = (viewport.height - dialog_height) / 2.0;
        Rect::new(Point::new(x, y), Size::new(resolved_width, dialog_height))
    }

    fn dialog_rect(&self) -> Rect {
        self.dialog_rect_for(self.viewport_size.get())
    }

    fn action_rects(&self) -> Vec<Rect> {
        let dialog = self.dialog_rect();
        let actions_y = dialog.y() + dialog.size.height - 56.0;
        let button_width = 80.0;
        let gap = 8.0;
        let total_width = self.actions.len() as f32 * button_width + (self.actions.len() as f32 - 1.0) * gap;
        let start_x = dialog.x() + dialog.size.width - 16.0 - total_width;

        self.actions.iter().enumerate().map(|(i, _)| {
            Rect::new(
                Point::new(start_x + i as f32 * (button_width + gap), actions_y + 8.0),
                Size::new(button_width, 36.0),
            )
        }).collect()
    }

    fn close(&mut self, ctx: &mut EventContext) {
        self.is_open.set(false);
        if self.overlay_registered {
            ctx.unregister_overlay();
            self.overlay_registered = false;
        }
        if let Some(ref cb) = self.on_close {
            if let Ok(mut f) = cb.lock() { f(); }
        }
        ctx.request_paint();
    }
}

impl Element for DialogElement {
    fn update(&mut self, widget: &dyn Widget, _ctx: &mut UpdateContext) {
        if let Some(d) = widget.as_any().downcast_ref::<Dialog>() {
            self.title = d.title.clone();
            self.icon = d.icon.clone();
            self.body = d.body.clone();
            self.actions = d.actions.clone();
            self.is_open = d.is_open;
            self.width = d.width;
            self.on_close = d.on_close.clone();
            self.mark_dirty(DirtyFlags::RENDER);
        }
    }

    fn layout(&mut self, constraints: Constraints) -> Size {
        let w = if constraints.max_width.is_finite() { constraints.max_width } else { 0.0 };
        let h = if constraints.max_height.is_finite() { constraints.max_height } else { 0.0 };
        self.bounds = Rect::new(Point::zero(), Size::new(w, h));
        if w > 0.0 && h > 0.0 {
            self.viewport_size.set(Size::new(w, h));
        }
        Size::zero()
    }

    fn build_display_list(&self, list: &mut DisplayList, _clip: Rect) {
        if !self.is_open.get_untracked() {
            return;
        }

        list.begin_overlay();

        let viewport = list.surface_size();
        self.viewport_size.set(viewport);

        let bg = self.mss.background_color.unwrap_or(Color::WHITE);
        let fg = self.mss.color.unwrap_or(Color::from_hex("#111827"));
        let border = self.mss.border_color.unwrap_or(Color::from_hex("#E5E7EB"));
        let primary = self.mss.accent_color.unwrap_or(Color::from_hex("#3B82F6"));

        let backdrop_rect = Rect::new(Point::zero(), viewport);
        list.push_rect(backdrop_rect, Color::new(0.0, 0.0, 0.0, 0.4), [0.0; 4]);

        let dialog = self.dialog_rect_for(viewport);

        list.push_shadow(
            dialog,
            Color::new(0.0, 0.0, 0.0, 0.2),
            24.0,
            (0.0, 8.0),
            [12.0; 4],
        );

        list.push_rect_bordered(
            dialog,
            bg,
            [12.0; 4],
            Border { width: 1.0, color: border },
        );

        let title_x_offset = if let Some(ref icon) = self.icon {
            let icon_size = 22.0;
            let icon_rect = Rect::new(
                Point::new(dialog.x() + 24.0, dialog.y() + 13.0),
                Size::new(icon_size, icon_size),
            );
            list.push_text_centered(icon, icon_rect, fg, icon_size);
            icon_size + 10.0
        } else {
            0.0
        };
        let title_rect = Rect::new(
            Point::new(dialog.x() + 24.0 + title_x_offset, dialog.y() + 8.0),
            Size::new(dialog.size.width - 48.0 - title_x_offset, 40.0),
        );
        list.push_text(&self.title, title_rect, fg, 18.0);

        if !self.body.is_empty() {
            let body_rect = Rect::new(
                Point::new(dialog.x() + 24.0, dialog.y() + 52.0),
                Size::new(dialog.size.width - 48.0, 48.0),
            );
            list.push_text(&self.body, body_rect, fg.with_alpha(0.7), 14.0);
        }

        let action_rects = self.action_rects();
        for (i, action) in self.actions.iter().enumerate() {
            if i < action_rects.len() {
                let rect = action_rects[i];
                let is_hover = self.hover_action == Some(i);
                let (btn_bg, text_color) = if action.primary {
                    if is_hover { (primary.darken(0.1), Color::WHITE) } else { (primary, Color::WHITE) }
                } else {
                    if is_hover { (bg.darken(0.05), fg.with_alpha(0.7)) } else { (bg, fg.with_alpha(0.7)) }
                };
                list.push_rect_bordered(rect, btn_bg, [6.0; 4], Border { width: 1.0, color: if action.primary { primary } else { border } });
                list.push_text_centered(&action.label, rect, text_color, 14.0);
            }
        }

        list.end_overlay();
    }

    fn handle_event(&mut self, event: &Event, ctx: &mut EventContext) -> EventResult {
        let is_open = self.is_open.get_untracked();

        if is_open && !self.overlay_registered {
            let overlay_bounds = Rect::new(Point::zero(), self.viewport_size.get());
            ctx.register_overlay(overlay_bounds, true);
            self.overlay_registered = true;
            ctx.request_paint();
        } else if !is_open && self.overlay_registered {
            ctx.unregister_overlay();
            self.overlay_registered = false;
        }

        if !is_open {
            return EventResult::Ignored;
        }

        match event {
            Event::MouseMove(pos) => {
                let action_rects = self.action_rects();
                let mut new_hover = None;
                for (i, rect) in action_rects.iter().enumerate() {
                    if rect.contains(*pos) {
                        new_hover = Some(i);
                        break;
                    }
                }
                if new_hover.is_some() {
                    ctx.set_cursor(crate::input::CursorIcon::Pointer);
                }
                if new_hover != self.hover_action {
                    self.hover_action = new_hover;
                    ctx.request_paint();
                    return EventResult::Handled;
                }
                EventResult::Handled
            }
            Event::MouseDown { button, position } => {
                if *button == MouseButton::Left {
                    let action_rects = self.action_rects();
                    for (i, rect) in action_rects.iter().enumerate() {
                        if rect.contains(*position) {
                            if let Ok(mut cb) = self.actions[i].on_click.lock() {
                                cb();
                            }
                            self.close(ctx);
                            return EventResult::Handled;
                        }
                    }

                    let dialog = self.dialog_rect();
                    if !dialog.contains(*position) {
                        self.close(ctx);
                        return EventResult::Handled;
                    }
                }
                EventResult::Handled
            }
            Event::KeyDown(crate::input::Key::Escape) | Event::BackPressed => {
                self.close(ctx);
                EventResult::Handled
            }
            _ => EventResult::Handled,
        }
    }

    fn children(&self) -> &[ElementId] { &[] }
    fn bounds(&self) -> Rect { self.bounds }

    fn hit_test(&self, _point: Point) -> bool {
        self.is_open.get_untracked()
    }

    fn overlay_request(&self) -> Option<(Rect, bool)> {
        if self.is_open.get_untracked() {
            Some((Rect::new(Point::zero(), self.viewport_size.get()), true))
        } else {
            None
        }
    }
    fn set_position(&mut self, pos: Point) { self.bounds.origin = pos; }
    fn mark_dirty(&mut self, flags: DirtyFlags) { self.dirty_flags |= flags; }
    fn clear_dirty(&mut self, flags: DirtyFlags) { self.dirty_flags.remove(flags); }
    fn is_dirty(&self, flags: DirtyFlags) -> bool { self.dirty_flags.contains(flags) }
    fn id(&self) -> ElementId { self.id }
    fn set_id(&mut self, id: ElementId) { self.id = id; }
    fn mount(&mut self, _tree: &mut ElementTree) {}
    fn set_classes(&mut self, classes: Vec<String>) { self.classes = classes; self.mark_dirty(DirtyFlags::RENDER); }
    fn get_classes(&self) -> &[String] { &self.classes }
    fn element_type_name(&self) -> &str { "Dialog" }
    fn reset_mss_styles(&mut self) { self.mss.reset(); }
    fn mss(&self) -> Option<&crate::mss::MssFields> { Some(&self.mss) }
    fn apply_computed_style(&mut self, style: &ComputedStyle) {
        self.mss.apply(style);
        if let Some(d) = style.width() { self.width = d; }
        self.mark_dirty(DirtyFlags::RENDER | DirtyFlags::LAYOUT);
    }

    fn apply_transition_styles(
        &mut self,
        base: &ComputedStyle,
        hover: Option<&ComputedStyle>,
        active: Option<&ComputedStyle>,
        focus: Option<&ComputedStyle>,
        selected: Option<&ComputedStyle>,
        _checked: Option<&ComputedStyle>,
    ) {
        self.mss.apply_transitions(base, hover, active, focus, selected);
    }

    fn accessibility_info(&self) -> Option<crate::a11y::AccessibilityInfo> {
        Some(crate::a11y::AccessibilityInfo {
            role: crate::a11y::Role::Group,
            state: crate::a11y::NodeState {
                hidden: !self.is_open.get_untracked(),
                ..Default::default()
            },
            properties: crate::a11y::NodeProperties {
                label: Some(self.title.clone()),
                description: if self.body.is_empty() { None } else { Some(self.body.clone()) },
                ..Default::default()
            },
        })
    }

    fn set_content_size(&mut self, size: Size) {
        if size.width > 0.0 && size.height > 0.0 {
            self.viewport_size.set(size);
        }
    }
}

impl StyledElement for DialogElement {
    fn apply_style(&mut self, _style: &ComputedStyle) { self.mark_dirty(DirtyFlags::RENDER | DirtyFlags::LAYOUT); }
    fn classes(&self) -> &[String] { &self.classes }
    fn set_classes(&mut self, classes: Vec<String>) { self.classes = classes; self.mark_dirty(DirtyFlags::RENDER); }
}

pub struct AlertDialog;

impl AlertDialog {
    pub fn new(title: impl Into<String>, message: impl Into<String>, is_open: RwSignal<bool>) -> Dialog {
        let (ok_label, _) = dialog_labels();
        Dialog::new(title)
            .body(message)
            .is_open(is_open)
            .action(DialogAction::new(ok_label, move || {
                is_open.set(false);
            }).primary())
    }
}

pub struct ConfirmDialog;

impl ConfirmDialog {
    pub fn new(
        title: impl Into<String>,
        message: impl Into<String>,
        is_open: RwSignal<bool>,
        on_confirm: impl FnMut(bool) + Send + 'static,
    ) -> Dialog {
        let on_confirm = Arc::new(Mutex::new(on_confirm));
        let on_confirm_ok = on_confirm.clone();
        let on_confirm_cancel = on_confirm;
        let (ok_label, cancel_label) = dialog_labels();

        Dialog::new(title)
            .body(message)
            .is_open(is_open)
            .action(DialogAction::new(cancel_label, move || {
                is_open.set(false);
                if let Ok(mut cb) = on_confirm_cancel.lock() { cb(false); }
            }))
            .action(DialogAction::new(ok_label, move || {
                is_open.set(false);
                if let Ok(mut cb) = on_confirm_ok.lock() { cb(true); }
            }).primary())
    }
}
