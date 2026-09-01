//! Листовые элементы блоков DocumentEditor: текстовая строка с маркером,
//! код-блок, разделитель, таблица, карточки медиа и врезок.
//!
//! Каждый видимый сегмент текста рисуется отдельной командой `push_text_*`
//! с уже замеренной шириной — перенос батчера не срабатывает (паттерн
//! MarkdownView). Раскладка строк — [`super::linebox`].

use std::any::Any;
use std::sync::Arc;
use std::time::Duration;

use crate::core::canvas::CanvasContext;
use crate::core::{Color, Point, Rect, Size};
use crate::input::{Event, EventResult};
use crate::layout::Constraints;
use crate::mss::{TextAlign, TextDecoration};
use crate::render::DisplayList;
use crate::widget::context::{EventContext, TextMeasure, UpdateContext};
use crate::widget::{DirtyFlags, Element, ElementId, ElementTree, Widget};

use super::linebox::{layout_inline_text, InlineLayout};
use super::links::DocLinkProvider;
use super::model::{BlockId, InlineText, LinkTarget};
use super::state::{GeomLine, GeomMap, GeomSeg};
use super::style::DocStyle;

/// Общая часть Element-реализации листьев.
macro_rules! leaf_common {
    () => {
        fn id(&self) -> ElementId {
            self.id
        }
        fn set_id(&mut self, id: ElementId) {
            self.id = id;
        }
        fn bounds(&self) -> Rect {
            self.bounds
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
        fn handle_event(&mut self, _event: &Event, _ctx: &mut EventContext) -> EventResult {
            EventResult::Ignored
        }
        fn animate(&mut self, _dt: Duration) -> bool {
            false
        }
    };
}

/// Общая часть Widget-реализации листьев.
macro_rules! leaf_widget_common {
    () => {
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
    };
}

/// Ширина листа: доступная ширина, ограниченная колонкой контента
/// (`max_content_width`). Корневой Column центрирует ограниченные листья.
fn clamp_width(constraints: &Constraints, style: &DocStyle) -> f32 {
    let avail = if constraints.max_width.is_finite() { constraints.max_width } else { 600.0 };
    match style.max_content_width {
        Some(cap) => avail.min(cap),
        None => avail,
    }
}

// ─── TextRow ────────────────────────────────────────────────────────────────

/// Маркер в гаттере слева от текста.
#[derive(Clone, Debug, PartialEq)]
pub enum RowDecor {
    None,
    Bullet,
    Number(u64),
    Checkbox { checked: bool },
    Toggle { collapsed: bool },
}

/// Текстовая строка блока: параграф, заголовок, пункт списка, заголовок
/// callout'а — всё, у чего есть редактируемый инлайн-текст.
pub struct TextRow {
    pub block_id: BlockId,
    pub text: InlineText,
    pub font_size: f32,
    pub bold: bool,
    pub color: Color,
    pub decor: RowDecor,
    /// Ширина гаттера маркера; 0 — без гаттера.
    pub gutter: f32,
    pub style: Arc<DocStyle>,
    /// Реестр геометрии строк редактора (каретка/выделение контейнера).
    pub geom: Option<GeomMap>,
    /// Провайдер ссылок хоста — окраска битых wiki-ссылок.
    pub links: Option<Arc<dyn DocLinkProvider>>,
}

impl Widget for TextRow {
    fn create_element(&self) -> Box<dyn Element> {
        Box::new(TextRowElement {
            id: ElementId::new(),
            bounds: Rect::zero(),
            dirty: DirtyFlags::LAYOUT | DirtyFlags::RENDER,
            block_id: self.block_id,
            text: self.text.clone(),
            font_size: self.font_size,
            bold: self.bold,
            color: self.color,
            decor: self.decor.clone(),
            gutter: self.gutter,
            style: self.style.clone(),
            geom: self.geom.clone(),
            links: self.links.clone(),
            tm: None,
            cache: None,
        })
    }
    leaf_widget_common!();
}

pub struct TextRowElement {
    id: ElementId,
    bounds: Rect,
    dirty: DirtyFlags,
    pub block_id: BlockId,
    text: InlineText,
    font_size: f32,
    bold: bool,
    color: Color,
    decor: RowDecor,
    gutter: f32,
    style: Arc<DocStyle>,
    geom: Option<GeomMap>,
    links: Option<Arc<dyn DocLinkProvider>>,
    tm: Option<Arc<dyn TextMeasure>>,
    /// (ширина текстовой области, раскладка).
    cache: Option<(f32, InlineLayout)>,
}

impl TextRowElement {
    fn layout_for(&mut self, avail: f32) -> &InlineLayout {
        let dirty = match &self.cache {
            Some((w, _)) => (*w - avail).abs() > 0.5,
            None => true,
        };
        if dirty {
            let layout = match self.tm.as_deref() {
                Some(tm) => layout_inline_text(
                    &self.text,
                    self.font_size,
                    self.bold,
                    avail,
                    &self.style,
                    tm,
                ),
                None => InlineLayout {
                    lines: Vec::new(),
                    height: self.style.line_h(self.font_size),
                },
            };
            self.cache = Some((avail, layout));
            self.publish_geom();
        }
        &self.cache.as_ref().unwrap().1
    }

    /// Публикует строки в реестр геометрии (origin обновляет set_position).
    fn publish_geom(&self) {
        let Some(geom) = &self.geom else { return };
        let Some((_, layout)) = &self.cache else { return };
        // Префиксные суммы байтовых длин ранов → абсолютные смещения.
        let mut prefix = Vec::with_capacity(self.text.0.len() + 1);
        let mut acc = 0usize;
        for run in &self.text.0 {
            prefix.push(acc);
            acc += run.text.len();
        }
        prefix.push(acc);

        let lines = layout
            .lines
            .iter()
            .map(|line| GeomLine {
                y: line.y,
                segs: line
                    .segs
                    .iter()
                    .map(|seg| GeomSeg {
                        x: self.gutter + seg.x,
                        width: seg.width,
                        text: seg.text.clone(),
                        abs_start: prefix.get(seg.run_idx).copied().unwrap_or(0) + seg.byte_start,
                        bold: seg.style.bold,
                        font_size: seg.style.font_size,
                        link: seg.style.link.clone(),
                    })
                    .collect(),
            })
            .collect();

        let mut map = geom.lock().unwrap();
        let entry = map.entry(self.block_id).or_default();
        entry.gutter = self.gutter;
        entry.line_h = self.style.line_h(self.font_size);
        entry.lines = lines;
        entry.origin = self.bounds.origin;
    }

    fn draw_decor(&self, list: &mut DisplayList) {
        if self.gutter <= 0.0 || matches!(self.decor, RowDecor::None) {
            return;
        }
        let s = &self.style;
        let o = self.bounds.origin;
        let line_h = s.line_h(self.font_size);
        let cy = line_h / 2.0;
        match &self.decor {
            RowDecor::None => {}
            RowDecor::Bullet => {
                let mut c = CanvasContext::new(o, self.bounds.size);
                c.set_color(s.bullet_color);
                c.fill_circle(self.gutter / 2.0, cy, s.bullet_radius);
                c.flush(list);
            }
            RowDecor::Number(n) => {
                let txt = format!("{n}.");
                let w = self
                    .tm
                    .as_deref()
                    .map(|tm| {
                        tm.measure_text_width_styled(
                            &txt,
                            self.font_size,
                            txt.chars().count(),
                            false,
                            None,
                        )
                    })
                    .unwrap_or(0.0);
                let rect = Rect::new(
                    Point::new(o.x + (self.gutter - w - 6.0).max(0.0), o.y),
                    Size::new(w, 0.0),
                );
                list.push_text_aligned(
                    &txt,
                    rect,
                    s.number_color,
                    self.font_size,
                    TextAlign::DEFAULT,
                    TextDecoration::None,
                    400,
                );
            }
            RowDecor::Checkbox { checked } => {
                let cb = s.checkbox_size;
                let x = (self.gutter - cb) / 2.0;
                let y = cy - cb / 2.0;
                let mut c = CanvasContext::new(o, self.bounds.size);
                if *checked {
                    c.set_color(s.checkbox_check_color);
                    c.fill_rounded_rect(x, y, cb, cb, 4.0);
                    c.set_color(Color::rgba(1.0, 1.0, 1.0, 0.92));
                    c.set_stroke_width(1.8);
                    c.draw_line(x + cb * 0.24, y + cb * 0.52, x + cb * 0.44, y + cb * 0.72);
                    c.draw_line(x + cb * 0.44, y + cb * 0.72, x + cb * 0.78, y + cb * 0.30);
                } else {
                    c.set_color(s.checkbox_color);
                    c.set_stroke_width(1.5);
                    c.draw_rect(x, y, cb, cb);
                }
                c.flush(list);
            }
            RowDecor::Toggle { collapsed } => {
                let cx = self.gutter / 2.0;
                let mut c = CanvasContext::new(o, self.bounds.size);
                c.set_color(s.toggle_chevron_color);
                if *collapsed {
                    c.fill_polygon(&[(cx - 3.0, cy - 5.0), (cx - 3.0, cy + 5.0), (cx + 5.0, cy)]);
                } else {
                    c.fill_polygon(&[(cx - 5.0, cy - 3.0), (cx + 5.0, cy - 3.0), (cx, cy + 5.0)]);
                }
                c.flush(list);
            }
        }
    }
}

impl Element for TextRowElement {
    fn update(&mut self, widget: &dyn Widget, ctx: &mut UpdateContext) {
        let Some(w) = widget.as_any().downcast_ref::<TextRow>() else { return };
        let changed = self.text != w.text
            || self.font_size != w.font_size
            || self.bold != w.bold
            || self.color != w.color
            || self.decor != w.decor
            || self.gutter != w.gutter
            || !Arc::ptr_eq(&self.style, &w.style);
        if changed {
            self.text = w.text.clone();
            self.font_size = w.font_size;
            self.bold = w.bold;
            self.color = w.color;
            self.decor = w.decor.clone();
            self.gutter = w.gutter;
            self.style = w.style.clone();
            self.cache = None;
            self.mark_dirty(DirtyFlags::LAYOUT | DirtyFlags::RENDER);
            ctx.mark_layout_dirty();
        }
        self.block_id = w.block_id;
        self.geom = w.geom.clone();
        self.links = w.links.clone();
    }

    fn mount(&mut self, tree: &mut ElementTree) {
        self.tm = tree.text_measure.clone();
    }

    fn layout(&mut self, constraints: Constraints) -> Size {
        let width = clamp_width(&constraints, &self.style);
        let avail = (width - self.gutter).max(20.0);
        let height = self.layout_for(avail).height;
        self.bounds.size = Size::new(width, height);
        self.bounds.size
    }

    fn build_display_list(&self, list: &mut DisplayList, _clip: Rect) {
        let Some((_, layout)) = &self.cache else { return };
        self.draw_decor(list);
        let s = &self.style;
        let o = self.bounds.origin;
        let x0 = o.x + self.gutter;
        for line in &layout.lines {
            for seg in &line.segs {
                let color = match (&seg.style.link, seg.style.code) {
                    (Some(LinkTarget::Wiki { target }), _) => {
                        let missing = self
                            .links
                            .as_deref()
                            .map(|l| !l.link_exists(target))
                            .unwrap_or(false);
                        if missing { s.link_missing_color } else { s.link_color }
                    }
                    (Some(_), _) => s.link_color,
                    (None, true) => s.code_color,
                    (None, false) => self.color,
                };
                let x = x0 + seg.x;
                let y = o.y + line.y;
                if seg.style.code {
                    let bg = Rect::new(
                        Point::new(x - s.code_padding_h, y - 1.0),
                        Size::new(seg.width + s.code_padding_h * 2.0, seg.style.font_size + 4.0),
                    );
                    let r = s.code_radius;
                    list.push_rect(bg, s.code_bg, [r, r, r, r]);
                }
                let rect = Rect::new(Point::new(x, y), Size::new(seg.width, 0.0));
                list.push_text_aligned(
                    &seg.text,
                    rect,
                    color,
                    seg.style.font_size,
                    TextAlign::DEFAULT,
                    TextDecoration::None,
                    if seg.style.bold { 700 } else { 400 },
                );
                if matches!(seg.style.link, Some(LinkTarget::Url(_) | LinkTarget::Wiki { .. })) {
                    let ul = Rect::new(
                        Point::new(x, y + seg.style.font_size + 1.0),
                        Size::new(seg.width, 1.0),
                    );
                    list.push_rect(ul, color.with_alpha(0.6), [0.0; 4]);
                }
                if seg.style.strike {
                    let strike = Rect::new(
                        Point::new(x, y + seg.style.font_size * 0.55),
                        Size::new(seg.width, 1.0),
                    );
                    list.push_rect(strike, color, [0.0; 4]);
                }
            }
        }
    }

    fn element_type_name(&self) -> &str {
        "doc-text-row"
    }

    fn set_position(&mut self, pos: Point) {
        self.bounds.origin = pos;
        if let Some(geom) = &self.geom {
            if let Ok(mut map) = geom.lock() {
                map.entry(self.block_id).or_default().origin = pos;
            }
        }
    }

    leaf_common!();
}

// ─── CodeBlockView ──────────────────────────────────────────────────────────

pub struct CodeBlockView {
    pub block_id: BlockId,
    pub language: Option<String>,
    pub code: String,
    pub style: Arc<DocStyle>,
}

impl Widget for CodeBlockView {
    fn create_element(&self) -> Box<dyn Element> {
        Box::new(CodeBlockElement {
            id: ElementId::new(),
            bounds: Rect::zero(),
            dirty: DirtyFlags::LAYOUT | DirtyFlags::RENDER,
            block_id: self.block_id,
            language: self.language.clone(),
            code: self.code.clone(),
            style: self.style.clone(),
            tm: None,
            display_lines: Vec::new(),
            cached_width: -1.0,
        })
    }
    leaf_widget_common!();
}

pub struct CodeBlockElement {
    id: ElementId,
    bounds: Rect,
    dirty: DirtyFlags,
    pub block_id: BlockId,
    language: Option<String>,
    code: String,
    style: Arc<DocStyle>,
    tm: Option<Arc<dyn TextMeasure>>,
    /// Байтовые диапазоны экранных строк в `code`.
    display_lines: Vec<(usize, usize)>,
    cached_width: f32,
}

impl CodeBlockElement {
    fn rewrap(&mut self, avail: f32) {
        if (self.cached_width - avail).abs() < 0.5 && !self.display_lines.is_empty() {
            return;
        }
        self.cached_width = avail;
        self.display_lines.clear();
        let fs = self.style.code_font_size;
        let measure = |t: &str| -> f32 {
            match self.tm.as_deref() {
                Some(tm) => tm.measure_text_width(t, fs, t.chars().count()),
                None => t.chars().count() as f32 * fs * 0.6,
            }
        };
        let mut offset = 0usize;
        for line in self.code.split('\n') {
            if line.is_empty() {
                self.display_lines.push((offset, offset));
            } else if measure(line) <= avail {
                self.display_lines.push((offset, offset + line.len()));
            } else {
                // Посимвольный жадный перенос длинной строки.
                let mut start = 0usize;
                let mut w = 0.0f32;
                for (i, ch) in line.char_indices() {
                    let cw = measure(&line[i..i + ch.len_utf8()]);
                    if w + cw > avail && i > start {
                        self.display_lines.push((offset + start, offset + i));
                        start = i;
                        w = 0.0;
                    }
                    w += cw;
                }
                self.display_lines.push((offset + start, offset + line.len()));
            }
            offset += line.len() + 1;
        }
        if self.display_lines.is_empty() {
            self.display_lines.push((0, 0));
        }
    }
}

impl Element for CodeBlockElement {
    fn update(&mut self, widget: &dyn Widget, ctx: &mut UpdateContext) {
        let Some(w) = widget.as_any().downcast_ref::<CodeBlockView>() else { return };
        if self.code != w.code
            || self.language != w.language
            || !Arc::ptr_eq(&self.style, &w.style)
        {
            self.code = w.code.clone();
            self.language = w.language.clone();
            self.style = w.style.clone();
            self.cached_width = -1.0;
            self.mark_dirty(DirtyFlags::LAYOUT | DirtyFlags::RENDER);
            ctx.mark_layout_dirty();
        }
        self.block_id = w.block_id;
    }

    fn mount(&mut self, tree: &mut ElementTree) {
        self.tm = tree.text_measure.clone();
    }

    fn layout(&mut self, constraints: Constraints) -> Size {
        let width = clamp_width(&constraints, &self.style);
        let pad = self.style.code_block_padding;
        self.rewrap((width - pad * 2.0).max(40.0));
        let line_h = self.style.line_h(self.style.code_font_size);
        let height = pad * 2.0 + self.display_lines.len() as f32 * line_h;
        self.bounds.size = Size::new(width, height);
        self.bounds.size
    }

    fn build_display_list(&self, list: &mut DisplayList, _clip: Rect) {
        let s = &self.style;
        let r = s.code_block_radius;
        list.push_rect(self.bounds, s.code_block_bg, [r, r, r, r]);
        let pad = s.code_block_padding;
        let line_h = s.line_h(s.code_font_size);
        let o = self.bounds.origin;
        for (i, (start, end)) in self.display_lines.iter().enumerate() {
            if end <= start {
                continue;
            }
            let text = &self.code[*start..*end];
            let rect = Rect::new(
                Point::new(o.x + pad, o.y + pad + i as f32 * line_h),
                Size::new(self.bounds.size.width - pad * 2.0, 0.0),
            );
            list.push_text_aligned(
                text,
                rect,
                s.code_block_color,
                s.code_font_size,
                TextAlign::DEFAULT,
                TextDecoration::None,
                400,
            );
        }
        // Метка языка в правом верхнем углу.
        if let Some(lang) = &self.language {
            let fs = (s.code_font_size - 2.0).max(9.0);
            let w = self
                .tm
                .as_deref()
                .map(|tm| tm.measure_text_width(lang, fs, lang.chars().count()))
                .unwrap_or(0.0);
            let rect = Rect::new(
                Point::new(o.x + self.bounds.size.width - w - pad, o.y + 6.0),
                Size::new(w, 0.0),
            );
            list.push_text_aligned(
                lang,
                rect,
                s.muted_color,
                fs,
                TextAlign::DEFAULT,
                TextDecoration::None,
                400,
            );
        }
    }

    fn element_type_name(&self) -> &str {
        "doc-code-block"
    }

    fn set_position(&mut self, pos: Point) {
        self.bounds.origin = pos;
    }

    leaf_common!();
}

// ─── DividerView ────────────────────────────────────────────────────────────

pub struct DividerView {
    pub style: Arc<DocStyle>,
}

impl Widget for DividerView {
    fn create_element(&self) -> Box<dyn Element> {
        Box::new(DividerElement {
            id: ElementId::new(),
            bounds: Rect::zero(),
            dirty: DirtyFlags::LAYOUT | DirtyFlags::RENDER,
            style: self.style.clone(),
        })
    }
    leaf_widget_common!();
}

pub struct DividerElement {
    id: ElementId,
    bounds: Rect,
    dirty: DirtyFlags,
    style: Arc<DocStyle>,
}

impl Element for DividerElement {
    fn update(&mut self, widget: &dyn Widget, _ctx: &mut UpdateContext) {
        if let Some(w) = widget.as_any().downcast_ref::<DividerView>() {
            self.style = w.style.clone();
            self.mark_dirty(DirtyFlags::RENDER);
        }
    }
    fn mount(&mut self, _tree: &mut ElementTree) {}

    fn layout(&mut self, constraints: Constraints) -> Size {
        let width = clamp_width(&constraints, &self.style);
        self.bounds.size = Size::new(width, 14.0);
        self.bounds.size
    }

    fn build_display_list(&self, list: &mut DisplayList, _clip: Rect) {
        let s = &self.style;
        let y = self.bounds.origin.y + (self.bounds.size.height - s.divider_thickness) / 2.0;
        let rect = Rect::new(
            Point::new(self.bounds.origin.x, y),
            Size::new(self.bounds.size.width, s.divider_thickness),
        );
        list.push_rect(rect, s.divider_color, [0.0; 4]);
    }

    fn element_type_name(&self) -> &str {
        "doc-divider"
    }

    fn set_position(&mut self, pos: Point) {
        self.bounds.origin = pos;
    }

    leaf_common!();
}

// ─── MediaCard (плейсхолдер видео/аудио/файла) ─────────────────────────────

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum MediaGlyph {
    Video,
    Audio,
    File,
    Image,
}

/// Карточка медиа, пока нет настоящего плеера/резолвера (этапы S7–S8):
/// иконка + подпись + хвост url.
pub struct MediaCard {
    pub block_id: BlockId,
    pub glyph: MediaGlyph,
    pub title: String,
    pub subtitle: String,
    pub style: Arc<DocStyle>,
}

impl Widget for MediaCard {
    fn create_element(&self) -> Box<dyn Element> {
        Box::new(MediaCardElement {
            id: ElementId::new(),
            bounds: Rect::zero(),
            dirty: DirtyFlags::LAYOUT | DirtyFlags::RENDER,
            block_id: self.block_id,
            glyph: self.glyph,
            title: self.title.clone(),
            subtitle: self.subtitle.clone(),
            style: self.style.clone(),
        })
    }
    leaf_widget_common!();
}

pub struct MediaCardElement {
    id: ElementId,
    bounds: Rect,
    dirty: DirtyFlags,
    pub block_id: BlockId,
    glyph: MediaGlyph,
    title: String,
    subtitle: String,
    style: Arc<DocStyle>,
}

impl Element for MediaCardElement {
    fn update(&mut self, widget: &dyn Widget, ctx: &mut UpdateContext) {
        let Some(w) = widget.as_any().downcast_ref::<MediaCard>() else { return };
        if self.title != w.title || self.subtitle != w.subtitle || self.glyph != w.glyph {
            self.title = w.title.clone();
            self.subtitle = w.subtitle.clone();
            self.glyph = w.glyph;
            self.mark_dirty(DirtyFlags::RENDER);
            ctx.mark_render_dirty();
        }
        self.style = w.style.clone();
        self.block_id = w.block_id;
    }
    fn mount(&mut self, _tree: &mut ElementTree) {}

    fn layout(&mut self, constraints: Constraints) -> Size {
        let width = clamp_width(&constraints, &self.style);
        self.bounds.size = Size::new(width, self.style.media_placeholder_height);
        self.bounds.size
    }

    fn build_display_list(&self, list: &mut DisplayList, _clip: Rect) {
        let s = &self.style;
        let r = s.media_radius;
        list.push_rect(self.bounds, s.media_bg, [r, r, r, r]);
        let o = self.bounds.origin;
        let h = self.bounds.size.height;
        let icon_c = Point::new(o.x + h / 2.0, o.y + h / 2.0);

        let mut c = CanvasContext::new(o, self.bounds.size);
        c.set_color(s.muted_color);
        let (lx, ly) = (icon_c.x - o.x, icon_c.y - o.y);
        match self.glyph {
            MediaGlyph::Video => {
                c.fill_polygon(&[(lx - 6.0, ly - 9.0), (lx - 6.0, ly + 9.0), (lx + 10.0, ly)]);
            }
            MediaGlyph::Audio => {
                for (i, bh) in [8.0f32, 14.0, 20.0, 12.0, 16.0].iter().enumerate() {
                    let x = lx - 10.0 + i as f32 * 5.0;
                    c.fill_rounded_rect(x, ly - bh / 2.0, 3.0, *bh, 1.5);
                }
            }
            MediaGlyph::File | MediaGlyph::Image => {
                c.set_stroke_width(1.5);
                c.draw_rect(lx - 8.0, ly - 10.0, 16.0, 20.0);
                c.draw_line(lx - 4.0, ly - 3.0, lx + 4.0, ly - 3.0);
                c.draw_line(lx - 4.0, ly + 2.0, lx + 4.0, ly + 2.0);
            }
        }
        c.flush(list);

        let text_x = o.x + h + 4.0;
        let text_w = (self.bounds.size.width - h - 16.0).max(20.0);
        list.push_text_styled_singleline(
            &self.title,
            Rect::new(Point::new(text_x, o.y + h / 2.0 - s.text_size - 2.0), Size::new(text_w, s.text_size * 1.4)),
            s.text_color,
            s.text_size,
            TextAlign::DEFAULT,
            TextDecoration::None,
            600,
            None,
        );
        list.push_text_styled_singleline(
            &self.subtitle,
            Rect::new(Point::new(text_x, o.y + h / 2.0 + 3.0), Size::new(text_w, s.text_size * 1.3)),
            s.muted_color,
            (s.text_size - 2.0).max(10.0),
            TextAlign::DEFAULT,
            TextDecoration::None,
            400,
            None,
        );
    }

    fn element_type_name(&self) -> &str {
        "doc-media-card"
    }

    fn set_position(&mut self, pos: Point) {
        self.bounds.origin = pos;
    }

    leaf_common!();
}

// ─── EmbedCard (плейсхолдер врезки ![[…]]) ─────────────────────────────────

pub struct EmbedCard {
    pub block_id: BlockId,
    pub target: String,
    pub style: Arc<DocStyle>,
}

impl Widget for EmbedCard {
    fn create_element(&self) -> Box<dyn Element> {
        Box::new(EmbedCardElement {
            id: ElementId::new(),
            bounds: Rect::zero(),
            dirty: DirtyFlags::LAYOUT | DirtyFlags::RENDER,
            block_id: self.block_id,
            target: self.target.clone(),
            style: self.style.clone(),
        })
    }
    leaf_widget_common!();
}

pub struct EmbedCardElement {
    id: ElementId,
    bounds: Rect,
    dirty: DirtyFlags,
    pub block_id: BlockId,
    target: String,
    style: Arc<DocStyle>,
}

impl Element for EmbedCardElement {
    fn update(&mut self, widget: &dyn Widget, ctx: &mut UpdateContext) {
        let Some(w) = widget.as_any().downcast_ref::<EmbedCard>() else { return };
        if self.target != w.target {
            self.target = w.target.clone();
            self.mark_dirty(DirtyFlags::RENDER);
            ctx.mark_render_dirty();
        }
        self.style = w.style.clone();
        self.block_id = w.block_id;
    }
    fn mount(&mut self, _tree: &mut ElementTree) {}

    fn layout(&mut self, constraints: Constraints) -> Size {
        let width = clamp_width(&constraints, &self.style);
        self.bounds.size = Size::new(width, 44.0);
        self.bounds.size
    }

    fn build_display_list(&self, list: &mut DisplayList, _clip: Rect) {
        let s = &self.style;
        list.push_rect(self.bounds, s.embed_bg, [8.0; 4]);
        // Пунктирная рамка обозначает «живую врезку появится позже».
        let b = self.bounds;
        let border = Rect::new(b.origin, Size::new(b.size.width, 1.0));
        list.push_rect(border, s.embed_border_color, [0.0; 4]);
        let bottom = Rect::new(
            Point::new(b.origin.x, b.origin.y + b.size.height - 1.0),
            Size::new(b.size.width, 1.0),
        );
        list.push_rect(bottom, s.embed_border_color, [0.0; 4]);

        let label = format!("⧉ {}", self.target);
        list.push_text_styled_singleline(
            &label,
            Rect::new(
                Point::new(b.origin.x + 12.0, b.origin.y + (b.size.height - s.text_size * 1.3) / 2.0),
                Size::new(b.size.width - 24.0, s.text_size * 1.4),
            ),
            s.link_color,
            s.text_size,
            TextAlign::DEFAULT,
            TextDecoration::None,
            500,
            None,
        );
    }

    fn element_type_name(&self) -> &str {
        "doc-embed-card"
    }

    fn set_position(&mut self, pos: Point) {
        self.bounds.origin = pos;
    }

    leaf_common!();
}

// ─── TableBlockView ─────────────────────────────────────────────────────────

pub struct TableBlockView {
    pub block_id: BlockId,
    pub headers: Vec<InlineText>,
    pub rows: Vec<Vec<InlineText>>,
    pub style: Arc<DocStyle>,
}

impl Widget for TableBlockView {
    fn create_element(&self) -> Box<dyn Element> {
        Box::new(TableBlockElement {
            id: ElementId::new(),
            bounds: Rect::zero(),
            dirty: DirtyFlags::LAYOUT | DirtyFlags::RENDER,
            block_id: self.block_id,
            headers: self.headers.clone(),
            rows: self.rows.clone(),
            style: self.style.clone(),
            tm: None,
            col_widths: Vec::new(),
        })
    }
    leaf_widget_common!();
}

pub struct TableBlockElement {
    id: ElementId,
    bounds: Rect,
    dirty: DirtyFlags,
    pub block_id: BlockId,
    headers: Vec<InlineText>,
    rows: Vec<Vec<InlineText>>,
    style: Arc<DocStyle>,
    tm: Option<Arc<dyn TextMeasure>>,
    col_widths: Vec<f32>,
}

impl TableBlockElement {
    fn cols(&self) -> usize {
        self.headers
            .len()
            .max(self.rows.iter().map(|r| r.len()).max().unwrap_or(0))
            .max(1)
    }

    fn row_h(&self) -> f32 {
        self.style.line_h(self.style.text_size) + self.style.table_cell_padding_v * 2.0
    }

    fn measure_cell(&self, text: &InlineText, bold: bool) -> f32 {
        let plain = text.text();
        match self.tm.as_deref() {
            Some(tm) => tm.measure_text_width_styled(
                &plain,
                self.style.text_size,
                plain.chars().count(),
                bold,
                None,
            ),
            None => plain.chars().count() as f32 * self.style.text_size * 0.6,
        }
    }
}

impl Element for TableBlockElement {
    fn update(&mut self, widget: &dyn Widget, ctx: &mut UpdateContext) {
        let Some(w) = widget.as_any().downcast_ref::<TableBlockView>() else { return };
        if self.headers != w.headers || self.rows != w.rows {
            self.headers = w.headers.clone();
            self.rows = w.rows.clone();
            self.col_widths.clear();
            self.mark_dirty(DirtyFlags::LAYOUT | DirtyFlags::RENDER);
            ctx.mark_layout_dirty();
        }
        self.style = w.style.clone();
        self.block_id = w.block_id;
    }

    fn mount(&mut self, tree: &mut ElementTree) {
        self.tm = tree.text_measure.clone();
    }

    fn layout(&mut self, constraints: Constraints) -> Size {
        let width = clamp_width(&constraints, &self.style);
        let cols = self.cols();
        let pad = self.style.table_cell_padding_h * 2.0;
        let mut widths = vec![self.style.text_size * 3.0; cols];
        for (i, cell) in self.headers.iter().enumerate() {
            widths[i] = widths[i].max(self.measure_cell(cell, true) + pad);
        }
        for row in &self.rows {
            for (i, cell) in row.iter().enumerate() {
                if i < cols {
                    widths[i] = widths[i].max(self.measure_cell(cell, false) + pad);
                }
            }
        }
        let total: f32 = widths.iter().sum();
        if total > width && total > 0.0 {
            let k = width / total;
            for w in widths.iter_mut() {
                *w *= k;
            }
        }
        self.col_widths = widths;
        let height = (self.rows.len() as f32 + 1.0) * self.row_h() + 2.0;
        self.bounds.size = Size::new(width, height);
        self.bounds.size
    }

    fn build_display_list(&self, list: &mut DisplayList, _clip: Rect) {
        let s = &self.style;
        let o = self.bounds.origin;
        let row_h = self.row_h();
        let total_w: f32 = self.col_widths.iter().sum();
        let rows_n = self.rows.len() + 1;
        let table_h = rows_n as f32 * row_h;

        // Фон шапки.
        list.push_rect(
            Rect::new(o, Size::new(total_w, row_h)),
            s.table_header_bg,
            [4.0, 4.0, 0.0, 0.0],
        );

        // Сетка.
        for r in 0..=rows_n {
            let y = o.y + r as f32 * row_h;
            list.push_rect(
                Rect::new(Point::new(o.x, y), Size::new(total_w, 1.0)),
                s.table_border_color,
                [0.0; 4],
            );
        }
        let mut x = o.x;
        for (i, w) in self.col_widths.iter().enumerate() {
            list.push_rect(
                Rect::new(Point::new(x, o.y), Size::new(1.0, table_h)),
                s.table_border_color,
                [0.0; 4],
            );
            x += w;
            if i == self.col_widths.len() - 1 {
                list.push_rect(
                    Rect::new(Point::new(x, o.y), Size::new(1.0, table_h)),
                    s.table_border_color,
                    [0.0; 4],
                );
            }
        }

        // Текст ячеек: одна строка, клип по ячейке.
        let draw_row = |list: &mut DisplayList, row: &[InlineText], y: f32, bold: bool| {
            let mut x = o.x;
            for (i, w) in self.col_widths.iter().enumerate() {
                if let Some(cell) = row.get(i) {
                    let cell_rect = Rect::new(
                        Point::new(x + s.table_cell_padding_h, y + s.table_cell_padding_v),
                        Size::new(
                            (w - s.table_cell_padding_h * 2.0).max(4.0),
                            row_h - s.table_cell_padding_v * 2.0,
                        ),
                    );
                    list.push_clip(Rect::new(Point::new(x, y), Size::new(*w, row_h)));
                    list.push_text_styled_singleline(
                        &cell.text(),
                        cell_rect,
                        s.text_color,
                        s.text_size,
                        TextAlign::DEFAULT,
                        TextDecoration::None,
                        if bold { 700 } else { 400 },
                        None,
                    );
                    list.pop_clip();
                }
                x += w;
            }
        };
        draw_row(list, &self.headers, o.y, true);
        for (r, row) in self.rows.iter().enumerate() {
            draw_row(list, row, o.y + (r as f32 + 1.0) * row_h, false);
        }
    }

    fn element_type_name(&self) -> &str {
        "doc-table"
    }

    fn set_position(&mut self, pos: Point) {
        self.bounds.origin = pos;
    }

    leaf_common!();
}
