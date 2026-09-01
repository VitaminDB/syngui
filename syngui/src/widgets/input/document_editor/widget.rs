//! Корневой виджет DocumentEditor.
//!
//! Этап S2: контейнер, который парсит markdown в [`DocModel`], строит
//! по-блочные дочерние элементы (см. [`super::build`]) и стилизуется через
//! MSS (`document-editor`, переменные `--doc-*`). Редактирование, каретка
//! и выделение добавляются следующими этапами.

use std::any::Any;
use std::collections::hash_map::DefaultHasher;
use std::hash::{Hash, Hasher};
use std::sync::Arc;
use std::time::Duration;

use crate::core::{Point, Rect, Size};
use crate::input::{Event, EventResult};
use crate::layout::{Constraints, CrossAxisAlignment, MainAxisAlignment};
use crate::mss::ComputedStyle;
use crate::render::DisplayList;
use crate::widget::context::{EventContext, UpdateContext};
use crate::widget::{
    DirtyFlags, Element, ElementId, ElementTree, LayoutHint, StyledElement, Widget,
};

use super::build::block_widget;
use super::model::DocModel;
use super::parse::parse_document;
use super::style::DocStyle;

pub struct DocumentEditor {
    source: String,
    read_only: bool,
    classes: Vec<String>,
}

impl DocumentEditor {
    pub fn new() -> Self {
        Self { source: String::new(), read_only: false, classes: Vec::new() }
    }

    /// Markdown-исходник документа.
    pub fn markdown(mut self, source: impl Into<String>) -> Self {
        self.source = source.into();
        self
    }

    /// Пока редактирование не реализовано, влияет только на будущие этапы.
    pub fn read_only(mut self, read_only: bool) -> Self {
        self.read_only = read_only;
        self
    }

    pub fn class(mut self, class: impl Into<String>) -> Self {
        self.classes.push(class.into());
        self
    }
}

impl Default for DocumentEditor {
    fn default() -> Self {
        Self::new()
    }
}

fn fingerprint(s: &str) -> u64 {
    let mut h = DefaultHasher::new();
    s.hash(&mut h);
    h.finish()
}

impl Widget for DocumentEditor {
    fn create_element(&self) -> Box<dyn Element> {
        Box::new(DocumentEditorElement {
            id: ElementId::new(),
            bounds: Rect::zero(),
            dirty: DirtyFlags::LAYOUT | DirtyFlags::RENDER,
            classes: self.classes.clone(),
            model: parse_document(&self.source),
            style: Arc::new(DocStyle::default()),
            source_fp: fingerprint(&self.source),
            read_only: self.read_only,
            rebuild: true,
        })
    }

    fn can_update(&self, other: &dyn Any) -> bool {
        other.is::<Self>()
    }

    fn as_any(&self) -> &dyn Any {
        self
    }

    fn as_any_mut(&mut self) -> &mut dyn Any {
        self
    }

    fn mount(&self, _tree: &mut ElementTree, _parent_id: ElementId) {}

    fn widget_classes(&self) -> &[String] {
        &self.classes
    }
}

pub struct DocumentEditorElement {
    id: ElementId,
    bounds: Rect,
    dirty: DirtyFlags,
    classes: Vec<String>,
    model: DocModel,
    style: Arc<DocStyle>,
    source_fp: u64,
    read_only: bool,
    rebuild: bool,
}

impl Element for DocumentEditorElement {
    fn update(&mut self, widget: &dyn Widget, ctx: &mut UpdateContext) {
        let Some(w) = widget.as_any().downcast_ref::<DocumentEditor>() else { return };
        self.read_only = w.read_only;
        let fp = fingerprint(&w.source);
        if fp != self.source_fp {
            self.source_fp = fp;
            self.model = parse_document(&w.source);
            self.rebuild = true;
            self.mark_dirty(DirtyFlags::LAYOUT | DirtyFlags::RENDER);
            ctx.mark_layout_dirty();
        }
    }

    fn mount(&mut self, _tree: &mut ElementTree) {}

    fn layout(&mut self, constraints: Constraints) -> Size {
        // Вызывается только для пустого документа (без детей).
        let width = if constraints.max_width.is_finite() { constraints.max_width } else { 0.0 };
        let height = self.style.doc_padding * 2.0 + self.style.line_h(self.style.text_size);
        self.bounds.size = Size::new(width, height);
        self.bounds.size
    }

    fn layout_hint(&self) -> LayoutHint {
        let s = &self.style;
        // Колонка как в Notion: листья сами ограничивают свою ширину
        // max_content_width (см. rows::clamp_width), а корень центрирует их.
        LayoutHint::Column {
            gap: s.block_spacing,
            cross_align: CrossAxisAlignment::Center,
            main_align: MainAxisAlignment::Start,
            padding_left: s.doc_padding,
            padding_top: s.doc_padding,
            padding_right: s.doc_padding,
            padding_bottom: s.doc_padding,
            expand: false,
        }
    }

    fn manages_own_children(&self) -> bool {
        true
    }

    fn needs_rebuild(&self) -> bool {
        self.rebuild
    }

    fn build_children(&self) -> Vec<Box<dyn Widget>> {
        self.model.blocks.iter().map(|b| block_widget(b, &self.style)).collect()
    }

    fn clear_rebuild(&mut self) {
        self.rebuild = false;
    }

    fn build_display_list(&self, _list: &mut DisplayList, _clip: Rect) {}

    fn handle_event(&mut self, _event: &Event, _ctx: &mut EventContext) -> EventResult {
        EventResult::Ignored
    }

    fn animate(&mut self, _dt: Duration) -> bool {
        false
    }

    fn element_type_name(&self) -> &str {
        "document-editor"
    }

    fn apply_computed_style(&mut self, style: &ComputedStyle) {
        self.apply_style(style);
    }

    fn get_classes(&self) -> &[String] {
        &self.classes
    }

    fn set_classes(&mut self, classes: Vec<String>) {
        self.classes = classes;
    }

    fn id(&self) -> ElementId {
        self.id
    }
    fn set_id(&mut self, id: ElementId) {
        self.id = id;
    }
    fn bounds(&self) -> Rect {
        self.bounds
    }
    fn set_position(&mut self, pos: Point) {
        self.bounds.origin = pos;
    }
    fn children(&self) -> &[ElementId] {
        &[]
    }
    fn mark_dirty(&mut self, flags: DirtyFlags) {
        self.dirty |= flags;
    }
    fn clear_dirty(&mut self, flags: DirtyFlags) {
        self.dirty.remove(flags);
    }
    fn is_dirty(&self, flags: DirtyFlags) -> bool {
        self.dirty.contains(flags)
    }
}

impl StyledElement for DocumentEditorElement {
    fn apply_style(&mut self, style: &ComputedStyle) {
        let mut doc_style = DocStyle::default();
        doc_style.apply(style);
        // Дети перестраиваются только при реальном изменении стиля.
        if doc_style != *self.style {
            self.style = Arc::new(doc_style);
            self.rebuild = true;
            self.mark_dirty(DirtyFlags::LAYOUT | DirtyFlags::RENDER);
        }
    }

    fn classes(&self) -> &[String] {
        &self.classes
    }

    fn set_classes(&mut self, classes: Vec<String>) {
        self.classes = classes;
    }
}
