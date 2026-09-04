use crate::animation::transition::mss_color_to_core;
use crate::core::sync::Mutex;
use crate::core::{Color, Point, Rect, Size};
use crate::gpu::image_store::{ImageLoadState, ImageSource, ImageStore};
use crate::input::{CursorIcon, Event, EventResult, Key, MouseButton};
use crate::layout::Constraints;
use crate::mss::{ComputedStyle, Dimension, TextAlign, TextDecoration};
use crate::render::DisplayList;
use crate::signal::{use_signal, RwSignal};
use crate::widget::{DirtyFlags, Element, ElementId, ElementTree, EventContextExt, StyledElement, UpdateContext, Widget};
use crate::widget::context::{EventContext, TextMeasure};
use crate::widgets::overlay::menu::{MenuItem, PopupMenu};
use hashbrown::HashMap;
use std::any::Any;
use std::sync::Arc;
use std::time::Duration;
use web_time::Instant;

use super::highlight::CodeHighlighter;
#[cfg(feature = "markdown-syntax")]
use super::highlight::SyntectHighlighter;
use super::model::{MdBlock, MdInline};
use super::parser::parse_markdown;
use super::plain_text;
use super::resolve::{resolve_link, resolve_ref, ResolvedRef};
use super::renderer::{
    measure_blocks, measure_natural_width, MdImageEntry, MdImageProbe, MdRenderer, MdStyle,
};
use super::selection_map::{
    extract_selection_text, hit_test, select_all_pos, word_boundaries_in_run, SelPos,
    SelectableRun,
};

const ICON_CONTENT_COPY: &str = "\u{E14D}";
const ICON_CHECK: &str = "\u{E5CA}";
const ICON_SELECT_ALL: &str = "\u{E162}";
const COPY_FLASH_DURATION: Duration = Duration::from_millis(1100);

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum MdMenuAction {
    CopySelection,
    SelectAll,
    CopyAll,
}

const DEFAULT_SELECTION_COLOR: Color = Color::new(0.231, 0.510, 0.965, 0.30);
const MULTI_CLICK_WINDOW_MS: u128 = 300;

pub struct MarkdownView {
    blocks: Vec<MdBlock>,
    max_width: Option<Dimension>,
    copy_code: bool,
    syntax_highlight: bool,
    syntax_theme: Option<String>,
    highlighter: Option<Arc<dyn CodeHighlighter>>,
    selectable: bool,

    menu_open: RwSignal<bool>,
    menu_pos: RwSignal<Point>,
    menu_action: RwSignal<Option<MdMenuAction>>,

    on_link_click: Option<Arc<dyn Fn(&str) + Send + Sync>>,
    base_url: Option<String>,
}

impl MarkdownView {
    pub fn new(source: impl Into<String>) -> Self {
        let source = source.into();
        let blocks = parse_markdown(&source);
        Self {
            blocks,
            max_width: None,
            copy_code: false,
            syntax_highlight: false,
            syntax_theme: None,
            highlighter: None,
            selectable: true,
            menu_open: use_signal(false),
            menu_pos: use_signal(Point::zero()),
            menu_action: use_signal(None),
            on_link_click: None,
            base_url: None,
        }
    }

    pub fn on_link_click<F: Fn(&str) + Send + Sync + 'static>(mut self, cb: F) -> Self {
        self.on_link_click = Some(Arc::new(cb));
        self
    }

    pub fn base_url(mut self, url: impl Into<String>) -> Self {
        let url = url.into();
        self.base_url = if url.is_empty() { None } else { Some(url) };
        self
    }

    pub fn selectable(mut self, on: bool) -> Self {
        self.selectable = on;
        self
    }

    pub fn max_width(mut self, w: f32) -> Self {
        self.max_width = Some(Dimension::Px(w));
        self
    }

    pub fn with_copy_code(mut self, on: bool) -> Self {
        self.copy_code = on;
        self
    }

    pub fn with_syntax_highlight(mut self, on: bool) -> Self {
        self.syntax_highlight = on;
        self
    }

    pub fn with_syntax_theme(mut self, theme: impl Into<String>) -> Self {
        self.syntax_theme = Some(theme.into());
        self.syntax_highlight = true;
        self
    }

    pub fn with_highlighter(mut self, h: Arc<dyn CodeHighlighter>) -> Self {
        self.highlighter = Some(h);
        self.syntax_highlight = true;
        self
    }

    pub fn with_editable(self, on: bool) -> Box<dyn Widget> {
        if !on {
            return Box::new(self);
        }
        let initial = inlines_to_source(&self.blocks);
        let text = crate::signal::use_signal(initial);
        Box::new(
            super::super::MarkdownEditor::new(text)
                .syntax_highlight(self.syntax_highlight)
                .copy_code(self.copy_code),
        )
    }
}

fn inlines_to_source(blocks: &[MdBlock]) -> String {
    let mut out = String::new();
    for (i, b) in blocks.iter().enumerate() {
        if i > 0 {
            out.push_str("\n\n");
        }
        block_to_source(b, &mut out);
    }
    out
}

fn block_to_source(b: &MdBlock, out: &mut String) {
    match b {
        MdBlock::Heading { level, inlines, .. } => {
            for _ in 0..*level {
                out.push('#');
            }
            out.push(' ');
            inlines_to_source_inner(inlines, out);
        }
        MdBlock::Paragraph { inlines } => inlines_to_source_inner(inlines, out),
        MdBlock::CodeBlock { language, code } => {
            out.push_str("```");
            if let Some(l) = language {
                out.push_str(l);
            }
            out.push('\n');
            out.push_str(code);
            if !code.ends_with('\n') {
                out.push('\n');
            }
            out.push_str("```");
        }
        MdBlock::BlockQuote { blocks } => {
            let inner = inlines_to_source(blocks);
            for line in inner.lines() {
                out.push_str("> ");
                out.push_str(line);
                out.push('\n');
            }
        }
        MdBlock::UnorderedList { items } => {
            for (i, it) in items.iter().enumerate() {
                if i > 0 { out.push('\n'); }
                out.push_str("- ");
                let inner = inlines_to_source(&it.blocks);
                out.push_str(&inner.replace('\n', "\n  "));
            }
        }
        MdBlock::OrderedList { start, items } => {
            for (i, it) in items.iter().enumerate() {
                if i > 0 { out.push('\n'); }
                out.push_str(&format!("{}. ", *start + i as u64));
                let inner = inlines_to_source(&it.blocks);
                out.push_str(&inner.replace('\n', "\n   "));
            }
        }
        MdBlock::TaskList { items } => {
            for (i, it) in items.iter().enumerate() {
                if i > 0 { out.push('\n'); }
                out.push_str(if it.checked { "- [x] " } else { "- [ ] " });
                inlines_to_source_inner(&it.inlines, out);
            }
        }
        MdBlock::Table { headers, rows, .. } => {
            for c in headers {
                out.push_str("| ");
                inlines_to_source_inner(&c.inlines, out);
                out.push(' ');
            }
            out.push_str("|\n");
            for _ in 0..headers.len() {
                out.push_str("| --- ");
            }
            out.push('|');
            for row in rows {
                out.push('\n');
                for c in row {
                    out.push_str("| ");
                    inlines_to_source_inner(&c.inlines, out);
                    out.push(' ');
                }
                out.push('|');
            }
        }
        MdBlock::HorizontalRule => out.push_str("---"),
        MdBlock::FootnoteDefinition { label, blocks } => {
            out.push_str(&format!("[^{label}]: "));
            out.push_str(&inlines_to_source(blocks).replace('\n', "\n    "));
        }
    }
}

fn inlines_to_source_inner(inlines: &[MdInline], out: &mut String) {
    for i in inlines {
        match i {
            MdInline::Text(t) => out.push_str(t),
            MdInline::Bold(c) => {
                out.push_str("**");
                inlines_to_source_inner(c, out);
                out.push_str("**");
            }
            MdInline::Italic(c) => {
                out.push('*');
                inlines_to_source_inner(c, out);
                out.push('*');
            }
            MdInline::Strikethrough(c) => {
                out.push_str("~~");
                inlines_to_source_inner(c, out);
                out.push_str("~~");
            }
            MdInline::Code(t) => {
                out.push('`');
                out.push_str(t);
                out.push('`');
            }
            MdInline::Link { children, url } => {
                out.push('[');
                inlines_to_source_inner(children, out);
                out.push_str("](");
                out.push_str(url);
                out.push(')');
            }
            MdInline::Image { alt, url } => {
                out.push_str("![");
                out.push_str(alt);
                out.push_str("](");
                out.push_str(url);
                out.push(')');
            }
            MdInline::SoftBreak => out.push('\n'),
            MdInline::HardBreak => out.push_str("  \n"),
            MdInline::FootnoteRef(label) => {
                out.push_str(&format!("[^{label}]"));
            }
        }
    }
}

impl Widget for MarkdownView {
    fn create_element(&self) -> Box<dyn Element> {
        Box::new(MarkdownViewElement {
            id: ElementId::new(),
            blocks: self.blocks.clone(),
            style: MdStyle::default(),
            max_width: self.max_width,
            bounds: Rect::zero(),
            content_height: 0.0,
            classes: Vec::new(),
            dirty_flags: DirtyFlags::LAYOUT | DirtyFlags::RENDER,
            text_measure: None,
            image_store: None,
            images: HashMap::new(),
            copy_code: self.copy_code,
            syntax_highlight: self.syntax_highlight,
            syntax_theme: self.syntax_theme.clone(),
            highlighter: build_highlighter(
                self.syntax_highlight,
                self.syntax_theme.as_deref(),
                self.highlighter.clone(),
            ),
            copy_hotspots: Mutex::new(Vec::new()),
            hover_hotspot: None,
            flash_until: None,
            flashed_hotspot: None,
            selectable: self.selectable,
            selection_anchor: None,
            selection_focus: SelPos::ZERO,
            mouse_selecting: false,
            selectable_runs: Mutex::new(Vec::new()),
            plain_text: plain_text::linearize(&self.blocks),
            selection_color: DEFAULT_SELECTION_COLOR,
            last_click_at: None,
            click_count: 0,
            last_click_run: None,
            menu_open: self.menu_open,
            menu_pos: self.menu_pos,
            menu_action: self.menu_action,
            menu_mounted: false,
            on_link_click: self.on_link_click.clone(),
            pending_link: None,
            base_url: self.base_url.clone(),
        })
    }

    fn can_update(&self, other: &dyn Any) -> bool {
        other.is::<Self>()
    }

    fn as_any(&self) -> &dyn Any { self }
    fn as_any_mut(&mut self) -> &mut dyn Any { self }

    fn mount(&self, _tree: &mut ElementTree, _parent_id: ElementId) {}
}

fn context_menu(
    menu_open: RwSignal<bool>,
    menu_pos: RwSignal<Point>,
    menu_action: RwSignal<Option<MdMenuAction>>,
) -> PopupMenu {
    PopupMenu::new()
        .items(vec![
            MenuItem::new("copy", crate::i18n::builtin("markdown_view.copy", "Copy"))
                .icon(ICON_CONTENT_COPY)
                .shortcut("Ctrl+C"),
            MenuItem::new("select_all", crate::i18n::builtin("markdown_view.select_all", "Select all"))
                .icon(ICON_SELECT_ALL)
                .shortcut("Ctrl+A"),
            MenuItem::new("copy_all", crate::i18n::builtin("markdown_view.copy_all", "Copy all"))
                .icon(ICON_CONTENT_COPY),
        ])
        .is_open(menu_open)
        .position(menu_pos)
        .on_select(move |id| {
            let action = match id {
                "copy" => Some(MdMenuAction::CopySelection),
                "select_all" => Some(MdMenuAction::SelectAll),
                "copy_all" => Some(MdMenuAction::CopyAll),
                _ => None,
            };
            if let Some(a) = action {
                menu_action.set(Some(a));
            }
        })
}

fn build_highlighter(
    enabled: bool,
    theme: Option<&str>,
    explicit: Option<Arc<dyn CodeHighlighter>>,
) -> Option<Arc<dyn CodeHighlighter>> {
    if let Some(h) = explicit {
        return Some(h);
    }
    if !enabled {
        return None;
    }
    #[cfg(feature = "markdown-syntax")]
    {
        let h: Arc<dyn CodeHighlighter> = match theme {
            Some(name) => Arc::new(SyntectHighlighter::with_theme(name)),
            None => Arc::new(SyntectHighlighter::new()),
        };
        return Some(h);
    }
    #[cfg(not(feature = "markdown-syntax"))]
    {
        let _ = theme;
        log::debug!(
            "MarkdownView::with_syntax_highlight(true) requires feature `markdown-syntax`; falling back to flat code blocks"
        );
        None
    }
}

pub struct MarkdownViewElement {
    id: ElementId,
    blocks: Vec<MdBlock>,
    style: MdStyle,
    max_width: Option<Dimension>,
    bounds: Rect,
    content_height: f32,
    classes: Vec<String>,
    dirty_flags: DirtyFlags,
    text_measure: Option<Arc<dyn TextMeasure>>,
    image_store: Option<Arc<Mutex<ImageStore>>>,
    images: HashMap<String, MdImageEntry>,

    copy_code: bool,
    #[allow(dead_code)]
    syntax_highlight: bool,
    syntax_theme: Option<String>,
    highlighter: Option<Arc<dyn CodeHighlighter>>,

    copy_hotspots: Mutex<Vec<(Rect, String)>>,
    hover_hotspot: Option<usize>,
    flash_until: Option<Instant>,
    flashed_hotspot: Option<usize>,

    selectable: bool,
    selection_anchor: Option<SelPos>,
    selection_focus: SelPos,
    mouse_selecting: bool,
    selectable_runs: Mutex<Vec<SelectableRun>>,
    plain_text: String,
    selection_color: Color,
    last_click_at: Option<Instant>,
    click_count: u8,
    last_click_run: Option<usize>,

    menu_open: RwSignal<bool>,
    menu_pos: RwSignal<Point>,
    menu_action: RwSignal<Option<MdMenuAction>>,
    menu_mounted: bool,

    on_link_click: Option<Arc<dyn Fn(&str) + Send + Sync>>,
    pending_link: Option<(String, Point)>,
    base_url: Option<String>,
}

const LINK_DRAG_THRESHOLD_PX: f32 = 3.0;

impl MdImageProbe for MarkdownViewElement {
    fn entry(&self, url: &str) -> Option<MdImageEntry> {
        self.images.get(url).copied()
    }
}

fn copy_button_rect(block_rect: &Rect, style: &MdStyle) -> Rect {
    let size = style.copy_btn_size;
    let margin = style.copy_btn_margin;
    Rect::new(
        Point::new(
            block_rect.origin.x + block_rect.size.width - size - margin,
            block_rect.origin.y + margin,
        ),
        Size::new(size, size),
    )
}

impl MarkdownViewElement {
    fn selection_range(&self) -> Option<(SelPos, SelPos)> {
        let a = self.selection_anchor?;
        let f = self.selection_focus;
        if a == f {
            return None;
        }
        Some(if a.key() <= f.key() { (a, f) } else { (f, a) })
    }

    fn selection_text(&self) -> Option<String> {
        let (a, b) = self.selection_range()?;
        let runs = self.selectable_runs.lock().ok()?;
        let txt = extract_selection_text(&runs, a, b);
        if txt.is_empty() { None } else { Some(txt) }
    }

    fn draw_selection(
        &self,
        list: &mut DisplayList,
        runs: &[SelectableRun],
        a: SelPos,
        b: SelPos,
    ) {
        if runs.is_empty() {
            return;
        }
        let last = runs.len().saturating_sub(1);
        let s_run = a.run_idx.min(last);
        let e_run = b.run_idx.min(last);
        for ri in s_run..=e_run {
            let run = &runs[ri];
            let lo = if ri == s_run { a.byte_in_run } else { 0 };
            let hi = if ri == e_run { b.byte_in_run } else { run.visible_text.len() };
            let lo = lo.min(run.visible_text.len());
            let hi = hi.min(run.visible_text.len());
            if hi <= lo {
                continue;
            }
            list.push_text_selection_styled(
                &run.visible_text,
                lo,
                hi,
                run.rect.origin.x,
                run.rect.origin.y,
                run.rect.size.height,
                run.font_size,
                self.selection_color,
                run.font_family.clone(),
            );
        }
    }

    fn hit_test_pos(&self, pos: Point) -> SelPos {
        let Some(tm) = self.text_measure.as_ref() else {
            return SelPos::ZERO;
        };
        let runs = match self.selectable_runs.lock() {
            Ok(g) => g,
            Err(_) => return SelPos::ZERO,
        };
        hit_test(&runs, pos, tm.as_ref())
    }

    fn link_url_at(&self, pos: Point) -> Option<String> {
        let runs = self.selectable_runs.lock().ok()?;
        for run in runs.iter() {
            if run.link.is_some() && run.rect.contains(pos) {
                return run.link.clone();
            }
        }
        None
    }

    fn dispatch_link_click(&self, url: &str) {
        let resolved = resolve_link(self.base_url.as_deref(), url);
        if let Some(cb) = self.on_link_click.as_ref() {
            cb(&resolved);
            return;
        }
        if let Err(e) = crate::open_url(&resolved) {
            log::warn!("[MarkdownView] open_url('{resolved}') failed: {e}");
        }
    }

    fn select_word_at(&mut self, pos: SelPos) {
        let Ok(runs) = self.selectable_runs.lock() else {
            return;
        };
        let Some(run) = runs.get(pos.run_idx) else { return };
        let (s, e) = word_boundaries_in_run(&run.visible_text, pos.byte_in_run);
        self.selection_anchor = Some(SelPos { run_idx: pos.run_idx, byte_in_run: s });
        self.selection_focus = SelPos { run_idx: pos.run_idx, byte_in_run: e };
    }

    fn select_whole_run(&mut self, pos: SelPos) {
        let Ok(runs) = self.selectable_runs.lock() else {
            return;
        };
        let Some(run) = runs.get(pos.run_idx) else { return };
        let line_id = run.line_id;
        let mut start_idx = pos.run_idx;
        while start_idx > 0 && runs[start_idx - 1].line_id == line_id {
            start_idx -= 1;
        }
        let mut end_idx = pos.run_idx;
        while end_idx + 1 < runs.len() && runs[end_idx + 1].line_id == line_id {
            end_idx += 1;
        }
        let end_byte = runs[end_idx].visible_text.len();
        self.selection_anchor = Some(SelPos { run_idx: start_idx, byte_in_run: 0 });
        self.selection_focus = SelPos { run_idx: end_idx, byte_in_run: end_byte };
    }

    fn write_clipboard(&self, text: &str) {
        crate::clipboard::copy(text);
    }

    fn select_all(&mut self) {
        let Ok(runs) = self.selectable_runs.lock() else {
            return;
        };
        let (a, b) = select_all_pos(&runs);
        self.selection_anchor = Some(a);
        self.selection_focus = b;
    }

    fn draw_copy_buttons(&self, list: &mut DisplayList, hotspots: &[(Rect, String)]) {
        let now = Instant::now();
        let flashing = self.flash_until.map(|d| now < d).unwrap_or(false);

        for (idx, (block_rect, _code)) in hotspots.iter().enumerate() {
            let btn = copy_button_rect(block_rect, &self.style);
            let hovering = self.hover_hotspot == Some(idx);
            let flashed = flashing && self.flashed_hotspot == Some(idx);

            let bg = if flashed {
                self.style.copy_btn_flash_bg
            } else if hovering {
                self.style.copy_btn_bg_hover
            } else {
                self.style.copy_btn_bg
            };
            let r = self.style.copy_btn_radius;
            list.push_rect(btn, bg, [r, r, r, r]);

            let icon_size = (self.style.copy_btn_size * 0.62).max(12.0);
            let icon_rect = Rect::new(
                Point::new(btn.origin.x, btn.origin.y),
                Size::new(btn.size.width, btn.size.height),
            );
            let glyph = if flashed { ICON_CHECK } else { ICON_CONTENT_COPY };
            list.push_text_styled(
                glyph,
                icon_rect,
                self.style.copy_btn_color,
                icon_size,
                TextAlign::CENTER,
                TextDecoration::None,
                400,
                Some("Material Icons".to_string()),
            );
        }
    }
}

impl MarkdownViewElement {
    fn refresh_image_cache(&mut self) {
        let Some(store) = self.image_store.clone() else {
            return;
        };
        let mut urls: Vec<String> = Vec::new();
        collect_image_urls_in_blocks(&self.blocks, &mut urls);
        if urls.is_empty() {
            return;
        }
        let mut store_g = store.lock().unwrap();
        for url in urls {
            if self.images.contains_key(&url) {
                continue;
            }
            let source = match resolve_ref(self.base_url.as_deref(), &url) {
                ResolvedRef::Path(path) => ImageSource::Path(path),
                ResolvedRef::Url(resolved) => ImageSource::Url(resolved),
            };
            let (handle, state) = store_g.request(&source);
            let dims = if state == ImageLoadState::Ready {
                store_g.dimensions(handle).unwrap_or((0, 0))
            } else {
                (0, 0)
            };
            self.images.insert(
                url,
                MdImageEntry {
                    texture_id: handle.0,
                    state,
                    natural_w: dims.0,
                    natural_h: dims.1,
                },
            );
        }
    }
}

fn collect_image_urls_in_blocks(blocks: &[MdBlock], out: &mut Vec<String>) {
    for b in blocks {
        match b {
            MdBlock::Heading { inlines, .. } | MdBlock::Paragraph { inlines } => {
                collect_image_urls_in_inlines(inlines, out);
            }
            MdBlock::BlockQuote { blocks } => collect_image_urls_in_blocks(blocks, out),
            MdBlock::UnorderedList { items } | MdBlock::OrderedList { items, .. } => {
                for item in items {
                    collect_image_urls_in_blocks(&item.blocks, out);
                }
            }
            MdBlock::TaskList { items } => {
                for item in items {
                    collect_image_urls_in_inlines(&item.inlines, out);
                }
            }
            MdBlock::Table { headers, rows, .. } => {
                for c in headers {
                    collect_image_urls_in_inlines(&c.inlines, out);
                }
                for row in rows {
                    for c in row {
                        collect_image_urls_in_inlines(&c.inlines, out);
                    }
                }
            }
            MdBlock::FootnoteDefinition { blocks, .. } => {
                collect_image_urls_in_blocks(blocks, out);
            }
            MdBlock::CodeBlock { .. } | MdBlock::HorizontalRule => {}
        }
    }
}

fn collect_image_urls_in_inlines(inlines: &[MdInline], out: &mut Vec<String>) {
    for i in inlines {
        match i {
            MdInline::Image { url, .. } => out.push(url.clone()),
            MdInline::Bold(c)
            | MdInline::Italic(c)
            | MdInline::Strikethrough(c)
            | MdInline::Link { children: c, .. } => collect_image_urls_in_inlines(c, out),
            MdInline::Text(_)
            | MdInline::Code(_)
            | MdInline::SoftBreak
            | MdInline::HardBreak
            | MdInline::FootnoteRef(_) => {}
        }
    }
}

impl Element for MarkdownViewElement {
    fn update(&mut self, widget: &dyn Widget, _ctx: &mut UpdateContext) {
        if let Some(w) = widget.as_any().downcast_ref::<MarkdownView>() {
            self.blocks = w.blocks.clone();
            self.max_width = w.max_width;
            self.copy_code = w.copy_code;
            let theme_changed = self.syntax_theme.as_deref() != w.syntax_theme.as_deref();
            let highlighter_changed = w.highlighter.is_some()
                || self.syntax_highlight != w.syntax_highlight
                || theme_changed;
            if highlighter_changed {
                self.syntax_highlight = w.syntax_highlight;
                self.syntax_theme = w.syntax_theme.clone();
                self.highlighter = build_highlighter(
                    w.syntax_highlight,
                    w.syntax_theme.as_deref(),
                    w.highlighter.clone(),
                );
            }
            self.images.clear();
            if let Ok(mut hs) = self.copy_hotspots.lock() {
                hs.clear();
            }
            self.hover_hotspot = None;
            self.selectable = w.selectable;
            self.plain_text = plain_text::linearize(&self.blocks);
            self.selection_anchor = None;
            self.selection_focus = SelPos::ZERO;
            self.mouse_selecting = false;
            if let Ok(mut runs) = self.selectable_runs.lock() {
                runs.clear();
            }
            self.last_click_at = None;
            self.click_count = 0;
            self.last_click_run = None;
            self.on_link_click = w.on_link_click.clone();
            self.pending_link = None;
            self.base_url = w.base_url.clone();
            self.menu_mounted = false;
            self.mark_dirty(DirtyFlags::LAYOUT | DirtyFlags::RENDER);
        }
    }

    fn manages_own_children(&self) -> bool {
        true
    }

    fn needs_rebuild(&self) -> bool {
        !self.menu_mounted
    }

    fn build_children(&self) -> Vec<Box<dyn Widget>> {
        if self.selectable {
            vec![Box::new(context_menu(self.menu_open, self.menu_pos, self.menu_action))]
        } else {
            vec![]
        }
    }

    fn clear_rebuild(&mut self) {
        self.menu_mounted = true;
    }

    fn layout(&mut self, constraints: Constraints) -> Size {
        let upper = self.max_width
            .map(|d| d.resolve(constraints.max_width))
            .unwrap_or(constraints.max_width)
            .min(constraints.max_width);

        self.refresh_image_cache();

        let tm = self.text_measure.as_deref();
        let natural = measure_natural_width(&self.blocks, &self.style, tm);

        let w = natural
            .min(upper)
            .max(constraints.min_width)
            .min(constraints.max_width);

        let probe: Option<&dyn MdImageProbe> = if self.image_store.is_some() {
            Some(self)
        } else {
            None
        };
        self.content_height = measure_blocks(&self.blocks, &self.style, w, tm, probe);
        let h = self.content_height.max(self.style.text_size * self.style.line_height);

        self.bounds = Rect::new(Point::zero(), Size::new(w, h));
        Size::new(w, h)
    }

    fn build_display_list(&self, list: &mut DisplayList, _clip: Rect) {
        if self.blocks.is_empty() {
            return;
        }

        let origin = self.bounds.origin;
        let max_w = self.bounds.size.width;

        let probe: Option<&dyn MdImageProbe> = if self.image_store.is_some() {
            Some(self)
        } else {
            None
        };

        let mut sink: Vec<(Rect, String)> = Vec::new();
        let mut sel_sink: Vec<SelectableRun> = Vec::new();
        {
            let mut renderer = MdRenderer::new(list, &self.style, origin, max_w)
                .with_text_measure(self.text_measure.clone())
                .with_images(probe)
                .with_highlighter(self.highlighter.clone())
                .with_copy_hotspots(&mut sink);
            if self.selectable {
                renderer = renderer.with_selection_sink(&mut sel_sink);
            }
            renderer.render_blocks(&self.blocks);
        }

        if self.selectable {
            if let Some((a, b)) = self.selection_range() {
                self.draw_selection(list, &sel_sink, a, b);
            }
        }

        if self.copy_code {
            self.draw_copy_buttons(list, &sink);
        }

        if let Ok(mut hs) = self.copy_hotspots.lock() {
            *hs = sink;
        }
        if let Ok(mut rs) = self.selectable_runs.lock() {
            *rs = sel_sink;
        }
    }

    /// Тик нужен, пока открыто контекстное меню (его выбор приходит
    /// отложенно через `menu_action`), пока не отыграла подсветка кнопки
    /// «копировать код» и пока грузятся картинки. Иначе точечный реестр
    /// анимаций элемент не обходит и «Копировать»/«Выделить всё» из меню
    /// не срабатывают.
    fn wants_animate_tick(&self) -> bool {
        self.menu_open.get_untracked()
            || self.menu_action.get_untracked().is_some()
            || self.flash_until.is_some()
            || self
                .images
                .values()
                .any(|e| e.state == ImageLoadState::Loading)
    }

    fn animate(&mut self, _dt: std::time::Duration) -> bool {
        if let Some(deadline) = self.flash_until {
            if Instant::now() >= deadline {
                self.flash_until = None;
                self.flashed_hotspot = None;
                self.mark_dirty(DirtyFlags::RENDER);
            }
        }

        if let Some(action) = self.menu_action.get_untracked() {
            match action {
                MdMenuAction::CopySelection => {
                    if let Some(text) = self.selection_text() {
                        self.write_clipboard(&text);
                    }
                }
                MdMenuAction::SelectAll => {
                    self.select_all();
                    self.mark_dirty(DirtyFlags::RENDER);
                }
                MdMenuAction::CopyAll => {
                    let text = self.plain_text.clone();
                    self.write_clipboard(&text);
                }
            }
            self.menu_action.set(None);
        }

        let Some(store) = self.image_store.clone() else {
            return false;
        };
        let mut any_loading = false;
        let mut updated_any = false;
        let store_g = store.lock().unwrap();
        for (_url, entry) in self.images.iter_mut() {
            if entry.state != ImageLoadState::Loading {
                continue;
            }
            let handle = crate::gpu::image_store::ImageHandle(entry.texture_id);
            if let Some(state) = store_g.state_of(handle) {
                if state != entry.state {
                    entry.state = state;
                    if state == ImageLoadState::Ready {
                        if let Some((w, h)) = store_g.dimensions(handle) {
                            entry.natural_w = w;
                            entry.natural_h = h;
                        }
                    }
                    updated_any = true;
                }
            }
            if entry.state == ImageLoadState::Loading {
                any_loading = true;
            }
        }
        drop(store_g);
        if updated_any {
            self.mark_dirty(DirtyFlags::LAYOUT | DirtyFlags::RENDER);
        }
        any_loading || self.flash_until.is_some()
    }

    fn handle_event(
        &mut self,
        event: &Event,
        ctx: &mut EventContext,
    ) -> EventResult {
        let copy_buttons: Vec<(Rect, String)> = if self.copy_code {
            match self.copy_hotspots.lock() {
                Ok(g) => g.iter()
                    .map(|(rect, code)| (copy_button_rect(rect, &self.style), code.clone()))
                    .collect(),
                Err(_) => Vec::new(),
            }
        } else {
            Vec::new()
        };

        match event {
            Event::MouseMove(pos) => {
                if !copy_buttons.is_empty() {
                    let new_hover = copy_buttons.iter().position(|(r, _)| r.contains(*pos));
                    if new_hover.is_some() {
                        ctx.set_cursor(CursorIcon::Pointer);
                    }
                    if new_hover != self.hover_hotspot {
                        self.hover_hotspot = new_hover;
                        self.mark_dirty(DirtyFlags::RENDER);
                    }
                    if new_hover.is_some() {
                        return EventResult::Handled;
                    }
                }
                if let Some((_, down_pos)) = &self.pending_link.clone() {
                    let dx = pos.x - down_pos.x;
                    let dy = pos.y - down_pos.y;
                    if (dx * dx + dy * dy).sqrt() > LINK_DRAG_THRESHOLD_PX {
                        let down = *down_pos;
                        self.pending_link = None;
                        if self.selectable {
                            let anchor = self.hit_test_pos(down);
                            self.selection_anchor = Some(anchor);
                            self.selection_focus = self.hit_test_pos(*pos);
                            self.mouse_selecting = true;
                            self.mark_dirty(DirtyFlags::RENDER);
                        }
                    }
                }
                if self.selectable && self.mouse_selecting {
                    let new_focus = self.hit_test_pos(*pos);
                    if new_focus != self.selection_focus {
                        self.selection_focus = new_focus;
                        self.mark_dirty(DirtyFlags::RENDER);
                    }
                    ctx.set_cursor(CursorIcon::Text);
                    return EventResult::Handled;
                }
                if self.link_url_at(*pos).is_some() {
                    ctx.set_cursor(CursorIcon::Pointer);
                    return EventResult::Ignored;
                }
                if self.selectable && self.bounds.contains(*pos) {
                    ctx.set_cursor(CursorIcon::Text);
                }
                EventResult::Ignored
            }
            Event::MouseDown { button: MouseButton::Left, position } => {
                if let Some(idx) = copy_buttons.iter().position(|(r, _)| r.contains(*position)) {
                    let code = copy_buttons[idx].1.clone();
                    ctx.copy_to_clipboard(&code);
                    self.flash_until = Some(Instant::now() + COPY_FLASH_DURATION);
                    self.flashed_hotspot = Some(idx);
                    self.mark_dirty(DirtyFlags::RENDER);
                    return EventResult::Handled;
                }
                if let Some(url) = self.link_url_at(*position) {
                    self.pending_link = Some((url, *position));
                    return EventResult::Handled;
                }
                if !self.selectable || !self.bounds.contains(*position) {
                    return EventResult::Ignored;
                }
                let pos_sel = self.hit_test_pos(*position);
                let now = Instant::now();
                let same_run = self.last_click_run == Some(pos_sel.run_idx);
                let within_window = self
                    .last_click_at
                    .map(|t| now.duration_since(t).as_millis() < MULTI_CLICK_WINDOW_MS)
                    .unwrap_or(false);
                if same_run && within_window {
                    self.click_count = (self.click_count + 1).min(3);
                } else {
                    self.click_count = 1;
                }
                self.last_click_at = Some(now);
                self.last_click_run = Some(pos_sel.run_idx);

                match self.click_count {
                    2 => {
                        self.select_word_at(pos_sel);
                        self.mouse_selecting = false;
                    }
                    3 => {
                        self.select_whole_run(pos_sel);
                        self.mouse_selecting = false;
                    }
                    _ => {
                        self.selection_anchor = Some(pos_sel);
                        self.selection_focus = pos_sel;
                        self.mouse_selecting = true;
                    }
                }
                self.mark_dirty(DirtyFlags::RENDER);
                EventResult::Handled
            }
            Event::MouseUp { button: MouseButton::Left, position } => {
                if let Some((url, down_pos)) = self.pending_link.take() {
                    let dx = position.x - down_pos.x;
                    let dy = position.y - down_pos.y;
                    if (dx * dx + dy * dy).sqrt() <= LINK_DRAG_THRESHOLD_PX {
                        self.dispatch_link_click(&url);
                        return EventResult::Handled;
                    }
                }
                if self.mouse_selecting {
                    self.mouse_selecting = false;
                    return EventResult::Handled;
                }
                EventResult::Ignored
            }
            Event::MouseDown { button: MouseButton::Right, position } => {
                if !self.selectable || !self.bounds.contains(*position) {
                    return EventResult::Ignored;
                }
                self.menu_pos.set(*position);
                self.menu_open.set(true);
                ctx.request_paint();
                EventResult::Handled
            }
            Event::KeyDown(Key::C) if self.selectable && ctx.modifiers.ctrl => {
                if let Some(text) = self.selection_text() {
                    ctx.copy_to_clipboard(&text);
                    return EventResult::Handled;
                }
                EventResult::Ignored
            }
            Event::KeyDown(Key::A) if self.selectable && ctx.modifiers.ctrl => {
                self.select_all();
                self.mark_dirty(DirtyFlags::RENDER);
                EventResult::Handled
            }
            Event::FocusLost => {
                self.mouse_selecting = false;
                self.last_click_at = None;
                self.click_count = 0;
                self.pending_link = None;
                EventResult::Ignored
            }
            _ => EventResult::Ignored,
        }
    }

    fn children(&self) -> &[ElementId] { &[] }
    fn bounds(&self) -> Rect { self.bounds }
    fn set_position(&mut self, pos: Point) { self.bounds.origin = pos; }
    fn mark_dirty(&mut self, flags: DirtyFlags) { self.dirty_flags |= flags; }
    fn clear_dirty(&mut self, flags: DirtyFlags) { self.dirty_flags.remove(flags); }
    fn is_dirty(&self, flags: DirtyFlags) -> bool { self.dirty_flags.contains(flags) }
    fn id(&self) -> ElementId { self.id }
    fn set_id(&mut self, id: ElementId) { self.id = id; }
    fn mount(&mut self, tree: &mut ElementTree) {
        self.text_measure = tree.text_measure.clone();
        self.image_store = tree.image_store.clone();
    }

    fn element_type_name(&self) -> &str { "MarkdownView" }

    fn set_classes(&mut self, classes: Vec<String>) {
        self.classes = classes;
        self.mark_dirty(DirtyFlags::RENDER);
    }

    fn get_classes(&self) -> &[String] {
        &self.classes
    }

    fn reset_mss_styles(&mut self) {
        self.style = MdStyle::default();
        self.max_width = None;
    }

    fn apply_computed_style(&mut self, style: &ComputedStyle) {
        self.apply_style(style);
    }
}

impl StyledElement for MarkdownViewElement {
    fn apply_style(&mut self, style: &ComputedStyle) {
        if let Some(c) = style.color() {
            self.style.text_color = mss_color_to_core(c);
        }
        let fs = style.font_size();
        if fs != 16.0 {
            self.style.text_size = fs;
            self.style.code_font_size = (fs - 1.0).max(10.0);
        }
        if let Some(lh) = style.get("line-height").and_then(|v| v.as_px()) {
            self.style.line_height = lh;
        }
        if let Some(d) = style.width() {
            self.max_width = Some(d);
        }

        if let Some(c) = style.get("--md-heading-color").and_then(|v| v.as_color()) {
            self.style.heading_color = mss_color_to_core(c);
        }
        for (i, name) in [
            "--md-h1-size", "--md-h2-size", "--md-h3-size",
            "--md-h4-size", "--md-h5-size", "--md-h6-size",
        ].iter().enumerate() {
            if let Some(px) = style.get(name).and_then(|v| v.as_px()) {
                self.style.heading_sizes[i] = px;
            }
        }
        if let Some(px) = style.get("--md-heading-spacing").and_then(|v| v.as_px()) {
            self.style.heading_spacing_above = px;
        }

        if let Some(c) = style.get("--md-link-color").and_then(|v| v.as_color()) {
            self.style.link_color = mss_color_to_core(c);
        }

        if let Some(c) = style.get("--md-code-bg").and_then(|v| v.as_color()) {
            self.style.code_bg = mss_color_to_core(c);
        }
        if let Some(c) = style.get("--md-code-color").and_then(|v| v.as_color()) {
            self.style.code_color = mss_color_to_core(c);
        }
        if let Some(px) = style.get("--md-code-font-size").and_then(|v| v.as_px()) {
            self.style.code_font_size = px;
        }
        if let Some(px) = style.get("--md-code-padding-h").and_then(|v| v.as_px()) {
            self.style.code_padding_h = px;
        }
        if let Some(px) = style.get("--md-code-radius").and_then(|v| v.as_px()) {
            self.style.code_radius = px;
        }

        if let Some(c) = style.get("--md-code-block-bg").and_then(|v| v.as_color()) {
            self.style.code_block_bg = mss_color_to_core(c);
        }
        if let Some(c) = style.get("--md-code-block-color").and_then(|v| v.as_color()) {
            self.style.code_block_color = mss_color_to_core(c);
        }
        if let Some(px) = style.get("--md-code-block-radius").and_then(|v| v.as_px()) {
            self.style.code_block_radius = px;
        }
        if let Some(px) = style.get("--md-code-block-padding").and_then(|v| v.as_px()) {
            self.style.code_block_padding = px;
        }

        if let Some(c) = style.get("--md-quote-bg").and_then(|v| v.as_color()) {
            self.style.quote_bg = mss_color_to_core(c);
        }
        if let Some(c) = style.get("--md-quote-text-color").and_then(|v| v.as_color()) {
            self.style.quote_text_color = mss_color_to_core(c);
        }
        if let Some(c) = style.get("--md-quote-border-color").and_then(|v| v.as_color()) {
            self.style.quote_border_color = mss_color_to_core(c);
        }
        if let Some(px) = style.get("--md-quote-border-width").and_then(|v| v.as_px()) {
            self.style.quote_border_width = px;
        }
        if let Some(px) = style.get("--md-quote-padding-left").and_then(|v| v.as_px()) {
            self.style.quote_padding_left = px;
        }
        if let Some(px) = style.get("--md-quote-padding-v").and_then(|v| v.as_px()) {
            self.style.quote_padding_v = px;
        }
        if let Some(px) = style.get("--md-quote-radius").and_then(|v| v.as_px()) {
            self.style.quote_radius = px;
        }

        if let Some(px) = style.get("--md-list-indent").and_then(|v| v.as_px()) {
            self.style.list_indent = px;
        }
        if let Some(c) = style.get("--md-bullet-color").and_then(|v| v.as_color()) {
            self.style.bullet_color = mss_color_to_core(c);
        }
        if let Some(c) = style.get("--md-checkbox-color").and_then(|v| v.as_color()) {
            self.style.checkbox_color = mss_color_to_core(c);
        }
        if let Some(c) = style.get("--md-checkbox-check-color").and_then(|v| v.as_color()) {
            self.style.checkbox_check_color = mss_color_to_core(c);
        }

        if let Some(c) = style.get("--md-table-border-color").and_then(|v| v.as_color()) {
            self.style.table_border_color = mss_color_to_core(c);
        }
        if let Some(c) = style.get("--md-table-header-bg").and_then(|v| v.as_color()) {
            self.style.table_header_bg = mss_color_to_core(c);
        }
        if let Some(c) = style.get("--md-table-header-color").and_then(|v| v.as_color()) {
            self.style.table_header_color = mss_color_to_core(c);
        }
        if let Some(c) = style.get("--md-table-stripe-bg").and_then(|v| v.as_color()) {
            self.style.table_stripe_bg = mss_color_to_core(c);
        }

        if let Some(c) = style.get("--md-hr-color").and_then(|v| v.as_color()) {
            self.style.hr_color = mss_color_to_core(c);
        }
        if let Some(px) = style.get("--md-hr-thickness").and_then(|v| v.as_px()) {
            self.style.hr_thickness = px;
        }

        if let Some(px) = style.get("--md-block-spacing").and_then(|v| v.as_px()) {
            self.style.block_spacing = px;
        }
        if let Some(c) = style.get("--md-strikethrough-color").and_then(|v| v.as_color()) {
            self.style.strikethrough_color = Some(mss_color_to_core(c));
        }
        if let Some(c) = style.get("--md-image-placeholder-bg").and_then(|v| v.as_color()) {
            self.style.image_placeholder_bg = mss_color_to_core(c);
        }
        if let Some(c) = style.get("--md-image-placeholder-color").and_then(|v| v.as_color()) {
            self.style.image_placeholder_color = mss_color_to_core(c);
        }
        if let Some(px) = style.get("--md-image-height").and_then(|v| v.as_px()) {
            self.style.image_placeholder_height = px;
        }

        if let Some(c) = style.get("--md-footnote-color").and_then(|v| v.as_color()) {
            self.style.footnote_color = mss_color_to_core(c);
        }
        if let Some(c) = style.get("--md-footnote-divider-color").and_then(|v| v.as_color()) {
            self.style.footnote_divider_color = mss_color_to_core(c);
        }

        if let Some(c) = style.get("--md-copy-bg").and_then(|v| v.as_color()) {
            self.style.copy_btn_bg = mss_color_to_core(c);
        }
        if let Some(c) = style.get("--md-copy-bg-hover").and_then(|v| v.as_color()) {
            self.style.copy_btn_bg_hover = mss_color_to_core(c);
        }
        if let Some(c) = style.get("--md-copy-color").and_then(|v| v.as_color()) {
            self.style.copy_btn_color = mss_color_to_core(c);
        }
        if let Some(c) = style.get("--md-copy-flash-bg").and_then(|v| v.as_color()) {
            self.style.copy_btn_flash_bg = mss_color_to_core(c);
        }
        if let Some(px) = style.get("--md-copy-radius").and_then(|v| v.as_px()) {
            self.style.copy_btn_radius = px;
        }
        if let Some(px) = style.get("--md-copy-size").and_then(|v| v.as_px()) {
            self.style.copy_btn_size = px;
        }
        if let Some(px) = style.get("--md-copy-margin").and_then(|v| v.as_px()) {
            self.style.copy_btn_margin = px;
        }

        if let Some(c) = style.get("selection-color").and_then(|v| v.as_color()) {
            self.selection_color =
                Color::from_srgb(c.r, c.g, c.b, c.a as f32 / 255.0);
        }

        self.mark_dirty(DirtyFlags::LAYOUT | DirtyFlags::RENDER);
    }

    fn classes(&self) -> &[String] {
        &self.classes
    }

    fn set_classes(&mut self, classes: Vec<String>) {
        self.classes = classes;
        self.mark_dirty(DirtyFlags::RENDER);
    }
}

#[cfg(test)]
mod rebuild_tests {
    use super::*;
    use crate::signal::use_signal;
    use crate::testing::TestHarness;
    use crate::widgets::Reactive;

    #[test]
    fn context_menu_survives_parent_rebuild() {
        let tick = use_signal(0u32);
        let widget = Reactive::new(move || {
            let _ = tick.get();
            vec![Box::new(MarkdownView::new("hello")) as Box<dyn Widget>]
        });
        let mut harness = TestHarness::new(Box::new(widget));
        harness.rebuild();
        assert_eq!(harness.find_by_type_name("PopupMenu").len(), 1);
        tick.set(1);
        harness.rebuild();
        assert_eq!(harness.find_by_type_name("PopupMenu").len(), 1);
    }

    #[test]
    fn non_selectable_has_no_menu() {
        let mut harness = TestHarness::new(Box::new(MarkdownView::new("hello").selectable(false)));
        harness.rebuild();
        assert_eq!(harness.find_by_type_name("PopupMenu").len(), 0);
    }
}
