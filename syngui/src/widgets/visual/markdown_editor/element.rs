use std::any::Any;
use std::sync::Arc;

use crate::core::{Point, Rect, Size};
use crate::core::sync::Mutex;
use crate::input::{Event, EventResult};
use crate::layout::Constraints;
use crate::render::DisplayList;
use crate::signal::{self, RwSignal};
use crate::widget::{
    DirtyFlags, Element, ElementId, ElementTree, LayoutHint, UpdateContext, Widget,
};
use crate::widget::styled::WidgetExt;
use crate::widgets::containers::{Column, SplitView, SplitDirection};
use crate::widgets::input::MultilineTextEdit;
use crate::widgets::scroll::ScrollView;
use crate::widgets::visual::MarkdownView;

use super::toolbar::build_toolbar;
use super::widget::{EditorMode, MarkdownEditor};

pub(crate) struct MarkdownEditorElement {
    id: ElementId,
    bounds: Rect,
    child_ids: Vec<ElementId>,
    dirty_flags: DirtyFlags,
    mounted: bool,
    needs_child_rebuild: bool,

    text: RwSignal<String>,
    mode: RwSignal<EditorMode>,
    show_toolbar: bool,
    syntax_highlight: bool,
    copy_code: bool,
    line_numbers: bool,
    rows: usize,
    split_ratio: f32,
    on_change: Option<Arc<Mutex<dyn FnMut(&str) + Send>>>,
}

impl MarkdownEditorElement {
    pub(crate) fn new(
        text: RwSignal<String>,
        mode: RwSignal<EditorMode>,
        show_toolbar: bool,
        syntax_highlight: bool,
        copy_code: bool,
        line_numbers: bool,
        rows: usize,
        split_ratio: f32,
        on_change: Option<Arc<Mutex<dyn FnMut(&str) + Send>>>,
    ) -> Self {
        Self {
            id: ElementId::new(),
            bounds: Rect::zero(),
            child_ids: Vec::new(),
            dirty_flags: DirtyFlags::LAYOUT | DirtyFlags::RENDER,
            mounted: false,
            needs_child_rebuild: false,
            text,
            mode,
            show_toolbar,
            syntax_highlight,
            copy_code,
            line_numbers,
            rows,
            split_ratio,
            on_change,
        }
    }

    fn build_editor_pane(&self) -> Box<dyn Widget> {
        let text_sig = self.text;
        let on_change = self.on_change.clone();
        Box::new(
            MultilineTextEdit::new()
                .text(text_sig.get())
                .soft_wrap(true)
                .show_line_numbers(self.line_numbers)
                .rows(self.rows)
                .on_change(move |v| {
                    text_sig.set(v.to_string());
                    if let Some(cb) = on_change.as_ref() {
                        if let Ok(mut f) = cb.lock() {
                            f(v);
                        }
                    }
                })
                .class("editor-pane"),
        )
    }

    fn build_preview_pane(&self) -> Box<dyn Widget> {
        let body = MarkdownView::new(self.text.get())
            .with_syntax_highlight(self.syntax_highlight)
            .with_copy_code(self.copy_code)
            .class("preview-md");
        Box::new(
            ScrollView::new()
                .vertical()
                .class("preview-pane")
                .child(body),
        )
    }

    fn build_body(&self) -> Box<dyn Widget> {
        match self.mode.get() {
            EditorMode::Edit => self.build_editor_pane(),
            EditorMode::Preview => self.build_preview_pane(),
            EditorMode::Split => {
                let editor = self.build_editor_pane();
                let preview = self.build_preview_pane();
                Box::new(
                    SplitView::new(BoxedWidget(editor), BoxedWidget(preview))
                        .direction(SplitDirection::Horizontal)
                        .initial_ratio(self.split_ratio)
                        .class("split-pane"),
                )
            }
        }
    }
}

struct BoxedWidget(Box<dyn Widget>);

impl Widget for BoxedWidget {
    fn create_element(&self) -> Box<dyn Element> { self.0.create_element() }
    fn can_update(&self, other: &dyn Any) -> bool { self.0.can_update(other) }
    fn as_any(&self) -> &dyn Any { self.0.as_any() }
    fn as_any_mut(&mut self) -> &mut dyn Any { self.0.as_any_mut() }
    fn mount(&self, tree: &mut ElementTree, parent_id: ElementId) {
        self.0.mount(tree, parent_id);
    }
}

impl Element for MarkdownEditorElement {
    fn update(&mut self, widget: &dyn Widget, _ctx: &mut UpdateContext) {
        if let Some(w) = widget.as_any().downcast_ref::<MarkdownEditor>() {
            let changed = self.show_toolbar != w.show_toolbar
                || self.syntax_highlight != w.syntax_highlight
                || self.copy_code != w.copy_code
                || self.line_numbers != w.line_numbers
                || self.rows != w.rows
                || (self.split_ratio - w.split_ratio).abs() > f32::EPSILON;
            self.show_toolbar = w.show_toolbar;
            self.syntax_highlight = w.syntax_highlight;
            self.copy_code = w.copy_code;
            self.line_numbers = w.line_numbers;
            self.rows = w.rows;
            self.split_ratio = w.split_ratio;
            self.text = w.text;
            self.mode = w.mode;
            self.on_change = w.on_change.clone();
            if changed {
                self.needs_child_rebuild = true;
                self.mark_dirty(DirtyFlags::LAYOUT | DirtyFlags::RENDER);
            }
        }
    }

    fn layout(&mut self, constraints: Constraints) -> Size {
        let w = if constraints.max_width.is_finite() {
            constraints.max_width
        } else {
            constraints.min_width
        };
        let h = if constraints.max_height.is_finite() {
            constraints.max_height
        } else {
            constraints.min_height
        };
        self.bounds = Rect::new(Point::zero(), Size::new(w, h));
        Size::new(w, h)
    }

    fn layout_hint(&self) -> LayoutHint {
        LayoutHint::Loose
    }

    fn build_display_list(&self, _list: &mut DisplayList, _clip: Rect) {}

    fn handle_event(&mut self, _event: &Event, _ctx: &mut crate::widget::context::EventContext) -> EventResult {
        EventResult::Ignored
    }

    fn children(&self) -> &[ElementId] { &self.child_ids }
    fn bounds(&self) -> Rect { self.bounds }
    fn set_position(&mut self, pos: Point) { self.bounds.origin = pos; }
    fn mark_dirty(&mut self, flags: DirtyFlags) { self.dirty_flags |= flags; }
    fn clear_dirty(&mut self, flags: DirtyFlags) { self.dirty_flags.remove(flags); }
    fn is_dirty(&self, flags: DirtyFlags) -> bool { self.dirty_flags.contains(flags) }
    fn id(&self) -> ElementId { self.id }
    fn set_id(&mut self, id: ElementId) { self.id = id; }
    fn mount(&mut self, _tree: &mut ElementTree) {}
    fn element_type_name(&self) -> &str { "MarkdownEditor" }

    fn manages_own_children(&self) -> bool { true }

    fn needs_rebuild(&self) -> bool {
        !self.mounted || self.needs_child_rebuild || signal::is_element_dirty(self.id)
    }

    fn build_children(&self) -> Vec<Box<dyn Widget>> {
        signal::begin_tracking(self.id);
        signal::begin_element_scope(self.id);

        let mut col = Column::new()
            .gap(8.0)
            .class("markdown-editor")
            .expand();

        if self.show_toolbar {
            col = col.children(std::iter::once(build_toolbar(self.mode)));
        }
        col = col.children(std::iter::once(self.build_body()));

        let result: Vec<Box<dyn Widget>> = vec![Box::new(col)];

        signal::end_element_scope();
        signal::end_tracking();

        result
    }

    fn clear_rebuild(&mut self) {
        self.mounted = true;
        self.needs_child_rebuild = false;
        signal::clear_element_dirty(self.id);
    }
}

impl Drop for MarkdownEditorElement {
    fn drop(&mut self) {
        signal::cleanup_element(self.id);
    }
}
