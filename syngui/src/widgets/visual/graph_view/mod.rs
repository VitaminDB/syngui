//! `GraphView` — граф из блоков текста со стрелками (граф потока управления,
//! граф вызовов): послойная раскладка ([`layout`]), панорамирование
//! перетаскиванием, колесо — прокрутка, Ctrl+колесо — масштаб под курсором,
//! клик — выбор узла, двойной клик — «открыть». Мини-карта в углу.
//!
//! ```ignore
//! GraphView::new(
//!     vec![GraphNode::new(1, vec![vec![("mov eax, 1".into(), white)]])],
//!     vec![GraphEdge::new(1, 2, green)],
//! )
//! .root(1)
//! .on_select(|id| …)
//! ```
//!
//! Цвета из MSS: `background-color` — фон, `color` — текст по умолчанию,
//! `border-color` — рамка узла, `accent-color` — выбранный узел,
//! `selection-color` — заливка узла.

pub mod layout;

use std::any::Any;
use std::sync::Arc;
use std::time::Duration;

use crate::core::sync::Mutex;
use crate::core::{Color, Point, Rect, Size};
use crate::input::{CursorIcon, Event, EventResult, Key, MouseButton};
use crate::layout::Constraints;
use crate::mss::{ComputedStyle, MssFields, TextAlign, TextDecoration};
use crate::render::{Border, DisplayList};
use crate::widget::context::{EventContext, EventContextExt, TextMeasure, UpdateContext};
use crate::widget::{DirtyFlags, Element, ElementId, ElementTree, LayoutHint, StyledElement, Widget};

const FONT: &str = "monospace";
const PAD: f32 = 8.0;

/// Узел: строки текста из цветных кусков.
#[derive(Clone, Debug, PartialEq)]
pub struct GraphNode {
    pub id: u64,
    pub lines: Vec<Vec<(String, Color)>>,
    /// Цвет полосы заголовка (например, у входного блока).
    pub accent: Option<Color>,
}

impl GraphNode {
    pub fn new(id: u64, lines: Vec<Vec<(String, Color)>>) -> Self {
        Self { id, lines, accent: None }
    }

    pub fn accent(mut self, c: Color) -> Self {
        self.accent = Some(c);
        self
    }
}

#[derive(Clone, Copy, Debug, PartialEq)]
pub struct GraphEdge {
    pub from: u64,
    pub to: u64,
    pub color: Color,
}

impl GraphEdge {
    pub fn new(from: u64, to: u64, color: Color) -> Self {
        Self { from, to, color }
    }
}

type IdFn = Arc<Mutex<dyn FnMut(u64) + Send>>;

pub struct GraphView {
    nodes: Arc<Vec<GraphNode>>,
    edges: Arc<Vec<GraphEdge>>,
    root: Option<u64>,
    selected: Option<u64>,
    on_select: Option<IdFn>,
    on_activate: Option<IdFn>,
    classes: Vec<String>,
}

impl GraphView {
    pub fn new(nodes: Vec<GraphNode>, edges: Vec<GraphEdge>) -> Self {
        Self { nodes: Arc::new(nodes), edges: Arc::new(edges), root: None, selected: None, on_select: None, on_activate: None, classes: Vec::new() }
    }

    pub fn root(mut self, id: u64) -> Self {
        self.root = Some(id);
        self
    }

    pub fn selected(mut self, id: Option<u64>) -> Self {
        self.selected = id;
        self
    }

    pub fn on_select(mut self, f: impl FnMut(u64) + Send + 'static) -> Self {
        self.on_select = Some(Arc::new(Mutex::new(f)));
        self
    }

    pub fn on_activate(mut self, f: impl FnMut(u64) + Send + 'static) -> Self {
        self.on_activate = Some(Arc::new(Mutex::new(f)));
        self
    }

    pub fn class(mut self, c: impl Into<String>) -> Self {
        crate::widget::push_classes(&mut self.classes, c.into());
        self
    }
}

impl Widget for GraphView {
    fn create_element(&self) -> Box<dyn Element> {
        Box::new(GraphElement {
            id: ElementId::new(),
            bounds: Rect::zero(),
            classes: self.classes.clone(),
            dirty: DirtyFlags::LAYOUT | DirtyFlags::RENDER,
            mss: MssFields::new(),
            text_measure: None,
            nodes: self.nodes.clone(),
            edges: self.edges.clone(),
            root: self.root,
            selected: self.selected,
            on_select: self.on_select.clone(),
            on_activate: self.on_activate.clone(),
            layout: None,
            sizes: Vec::new(),
            zoom: 1.0,
            pan: Point::new(0.0, 0.0),
            centered: false,
            drag: None,
            cw: 7.8,
            row_h: 18.0,
            font_size: 13.0,
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
}

struct GraphElement {
    id: ElementId,
    bounds: Rect,
    classes: Vec<String>,
    dirty: DirtyFlags,
    mss: MssFields,
    text_measure: Option<Arc<dyn TextMeasure>>,
    nodes: Arc<Vec<GraphNode>>,
    edges: Arc<Vec<GraphEdge>>,
    root: Option<u64>,
    selected: Option<u64>,
    on_select: Option<IdFn>,
    on_activate: Option<IdFn>,
    layout: Option<layout::Layout>,
    sizes: Vec<(f32, f32)>,
    zoom: f32,
    /// Смещение графа относительно левого верхнего угла виджета (экранные px).
    pan: Point,
    centered: bool,
    /// Перетаскивание: точка начала и смещение тогда.
    drag: Option<(Point, Point, bool)>,
    cw: f32,
    row_h: f32,
    font_size: f32,
}

impl GraphElement {
    fn relayout(&mut self) {
        self.font_size = self.mss.font_size.unwrap_or(13.0);
        self.cw = self
            .text_measure
            .as_ref()
            .map(|tm| tm.measure_text_width_styled("0", self.font_size, 1, false, Some(FONT)))
            .filter(|w| *w > 1.0)
            .unwrap_or(self.font_size * 0.6);
        self.row_h = (self.font_size * 1.45).round();
        self.sizes = self
            .nodes
            .iter()
            .map(|n| {
                let chars = n.lines.iter().map(|l| l.iter().map(|(s, _)| s.chars().count()).sum::<usize>()).max().unwrap_or(4);
                (chars.max(6) as f32 * self.cw + 2.0 * PAD, n.lines.len().max(1) as f32 * self.row_h + 2.0 * PAD)
            })
            .collect();
        let index = |id: u64| self.nodes.iter().position(|n| n.id == id);
        let edges: Vec<(usize, usize)> = self.edges.iter().filter_map(|e| Some((index(e.from)?, index(e.to)?))).collect();
        let root = self.root.and_then(index).unwrap_or(0);
        self.layout = Some(layout::layout(&self.sizes, &edges, root, layout::Spacing::default()));
    }

    /// Граф → экран.
    fn to_screen(&self, p: (f32, f32)) -> Point {
        Point::new(self.bounds.origin.x + self.pan.x + p.0 * self.zoom, self.bounds.origin.y + self.pan.y + p.1 * self.zoom)
    }

    fn node_at(&self, p: Point) -> Option<u64> {
        let l = self.layout.as_ref()?;
        (0..self.nodes.len()).find_map(|i| {
            let tl = self.to_screen(l.pos[i]);
            let r = Rect::new(tl, Size::new(self.sizes[i].0 * self.zoom, self.sizes[i].1 * self.zoom));
            r.contains(p).then_some(self.nodes[i].id)
        })
    }

    /// Начальный вид: корень сверху по центру, масштаб — чтобы граф влез по ширине (не мельче 0.5).
    fn center_on_root(&mut self) {
        let Some(l) = &self.layout else { return };
        let w = self.bounds.size.width.max(1.0);
        self.zoom = ((w - 40.0) / l.width.max(1.0)).clamp(0.5, 1.0);
        let root = self.root.and_then(|id| self.nodes.iter().position(|n| n.id == id)).unwrap_or(0);
        let (rx, ry) = l.pos.get(root).copied().unwrap_or((0.0, 0.0));
        let rw = self.sizes.get(root).map(|s| s.0).unwrap_or(0.0);
        self.pan = Point::new(w / 2.0 - (rx + rw / 2.0) * self.zoom, 24.0 - ry * self.zoom);
    }

    /// Показать выбранный узел, если он вне экрана.
    fn reveal(&mut self, id: u64) {
        let Some(l) = &self.layout else { return };
        let Some(i) = self.nodes.iter().position(|n| n.id == id) else { return };
        let tl = self.to_screen(l.pos[i]);
        let (w, h) = (self.sizes[i].0 * self.zoom, self.sizes[i].1 * self.zoom);
        let b = self.bounds;
        if tl.x < b.origin.x || tl.y < b.origin.y || tl.x + w > b.origin.x + b.size.width || tl.y + h > b.origin.y + b.size.height {
            self.pan = Point::new(b.size.width / 2.0 - (l.pos[i].0 + self.sizes[i].0 / 2.0) * self.zoom, b.size.height / 3.0 - l.pos[i].1 * self.zoom);
        }
    }

    fn fire(cb: &Option<IdFn>, id: u64) {
        if let Some(f) = cb {
            if let Ok(mut g) = f.lock() {
                (g)(id);
            }
        }
    }
}

impl Element for GraphElement {
    fn update(&mut self, widget: &dyn Widget, _ctx: &mut UpdateContext) {
        let Some(w) = widget.as_any().downcast_ref::<GraphView>() else { return };
        let changed = !Arc::ptr_eq(&w.nodes, &self.nodes) && (*w.nodes != *self.nodes || *w.edges != *self.edges);
        self.on_select = w.on_select.clone();
        self.on_activate = w.on_activate.clone();
        self.classes = w.classes.clone();
        if changed {
            let new_root = w.root != self.root;
            self.nodes = w.nodes.clone();
            self.edges = w.edges.clone();
            self.root = w.root;
            self.relayout();
            if new_root {
                self.centered = false;
            }
        }
        if w.selected != self.selected {
            self.selected = w.selected;
            if let Some(id) = self.selected {
                self.reveal(id);
            }
        }
        self.mark_dirty(DirtyFlags::LAYOUT | DirtyFlags::RENDER);
    }

    fn layout(&mut self, c: Constraints) -> Size {
        let w = if c.max_width.is_finite() { c.max_width } else { 800.0 };
        let h = if c.max_height.is_finite() { c.max_height } else { 600.0 };
        self.bounds = Rect::new(self.bounds.origin, Size::new(w, h));
        if self.layout.is_none() {
            self.relayout();
        }
        if !self.centered && w > 10.0 {
            self.center_on_root();
            self.centered = true;
        }
        self.bounds.size
    }

    fn build_display_list(&self, list: &mut DisplayList, _clip: Rect) {
        let b = self.bounds;
        let text_c = self.mss.color.unwrap_or(Color::from_hex("#D4D9E3"));
        let bg = self.mss.background_color.unwrap_or(Color::from_hex("#12151B"));
        let border = self.mss.border_color.unwrap_or(Color::from_hex("#2A303C"));
        let accent = self.mss.accent_color.unwrap_or(Color::from_hex("#4F8CFF"));
        let fill = self.mss.selection_color.unwrap_or(Color::from_hex("#181C24"));
        list.push_rect(b, bg, [0.0; 4]);
        let Some(l) = &self.layout else { return };
        list.push_clip(b);
        let z = self.zoom;

        // рёбра
        for (i, e) in self.edges.iter().enumerate() {
            let Some(route) = l.routes.get(i) else { continue };
            if route.len() < 2 {
                continue;
            }
            let pts: Vec<[f32; 2]> = route.iter().map(|&p| {
                let s = self.to_screen(p);
                [s.x, s.y]
            }).collect();
            let col = if l.back.get(i).copied().unwrap_or(false) { e.color.with_alpha(0.75) } else { e.color };
            list.push_line_strip(pts.clone(), col, (1.6 * z).max(1.0));
            // стрелка
            let [x, y] = pts[pts.len() - 1];
            let a = 6.0 * z.max(0.6);
            list.push_line_strip(vec![[x - a, y - a * 1.3], [x, y], [x + a, y - a * 1.3]], col, (1.6 * z).max(1.0));
        }

        // узлы
        let saved = list.set_text_bbox_sample(Some("Mgyqlj|".into()));
        let fs = self.font_size * z;
        let rh = self.row_h * z;
        for (i, n) in self.nodes.iter().enumerate() {
            let tl = self.to_screen(l.pos[i]);
            let size = Size::new(self.sizes[i].0 * z, self.sizes[i].1 * z);
            let r = Rect::new(tl, size);
            if r.origin.x > b.origin.x + b.size.width || r.origin.y > b.origin.y + b.size.height || r.origin.x + size.width < b.origin.x || r.origin.y + size.height < b.origin.y {
                continue;
            }
            let sel = self.selected == Some(n.id);
            list.push_shadow(r, Color::from_srgb(0, 0, 0, 0.35), 8.0 * z, (0.0, 2.0 * z), [6.0 * z; 4]);
            list.push_rect_bordered(r, fill, [6.0 * z; 4], Border::new(if sel { 2.0 } else { 1.0 }, if sel { accent } else { border }));
            if let Some(a) = n.accent {
                list.push_rect(Rect::new(tl, Size::new(size.width, 3.0 * z)), a, [6.0 * z, 6.0 * z, 0.0, 0.0]);
            }
            if fs < 4.0 {
                continue; // слишком мелко — только коробки
            }
            for (k, line) in n.lines.iter().enumerate() {
                let y = tl.y + PAD * z + k as f32 * rh;
                let mut x = tl.x + PAD * z;
                for (s, c) in line {
                    let w = s.chars().count() as f32 * self.cw * z;
                    list.push_text_styled_singleline(
                        s,
                        Rect::new(Point::new(x, y), Size::new(w + 4.0, rh)),
                        if *c == Color::TRANSPARENT { text_c } else { *c },
                        fs,
                        TextAlign::DEFAULT,
                        TextDecoration::None,
                        400,
                        Some(FONT.to_string()),
                    );
                    x += w;
                }
            }
        }
        list.set_text_bbox_sample(saved);

        // мини-карта
        let mw = 160.0f32.min(b.size.width * 0.25);
        let scale = mw / l.width.max(1.0);
        let mh = (l.height * scale).min(b.size.height * 0.3);
        let scale = scale.min(mh / l.height.max(1.0));
        let mr = Rect::new(Point::new(b.origin.x + b.size.width - mw - 12.0, b.origin.y + b.size.height - mh - 12.0), Size::new(mw, mh));
        if l.width * z > b.size.width || l.height * z > b.size.height {
            list.push_rect_bordered(mr, bg.with_alpha(0.92), [6.0; 4], Border::new(1.0, border));
            for i in 0..self.nodes.len() {
                let (x, y) = l.pos[i];
                let nr = Rect::new(Point::new(mr.origin.x + x * scale, mr.origin.y + y * scale), Size::new((self.sizes[i].0 * scale).max(1.0), (self.sizes[i].1 * scale).max(1.0)));
                list.push_rect(nr, if self.selected == Some(self.nodes[i].id) { accent } else { text_c.with_alpha(0.35) }, [1.0; 4]);
            }
            // видимая область
            let vx = -self.pan.x / z * scale;
            let vy = -self.pan.y / z * scale;
            let vr = Rect::new(Point::new(mr.origin.x + vx, mr.origin.y + vy), Size::new(b.size.width / z * scale, b.size.height / z * scale));
            list.push_rect_bordered(vr, accent.with_alpha(0.08), [2.0; 4], Border::new(1.0, accent.with_alpha(0.7)));
        }
        list.pop_clip();
    }

    fn handle_event(&mut self, event: &Event, ctx: &mut EventContext) -> EventResult {
        match event {
            Event::MouseDown { button: MouseButton::Left, position } if self.bounds.contains(*position) => {
                let hit = self.node_at(*position);
                if let Some(id) = hit {
                    self.selected = Some(id);
                    Self::fire(&self.on_select, id);
                }
                self.drag = Some((*position, self.pan, false));
                ctx.capture();
                ctx.request_paint();
                EventResult::Captured
            }
            Event::DoubleClick { position, .. } if self.bounds.contains(*position) => {
                if let Some(id) = self.node_at(*position) {
                    Self::fire(&self.on_activate, id);
                }
                EventResult::Handled
            }
            Event::MouseMove(pos) => {
                if let Some((start, pan, _)) = self.drag {
                    let moved = (pos.x - start.x).abs() + (pos.y - start.y).abs() > 3.0;
                    self.pan = Point::new(pan.x + pos.x - start.x, pan.y + pos.y - start.y);
                    self.drag = Some((start, pan, moved));
                    ctx.set_cursor(CursorIcon::Grabbing);
                    ctx.request_paint();
                    return EventResult::Handled;
                }
                if self.bounds.contains(*pos) {
                    ctx.set_cursor(if self.node_at(*pos).is_some() { CursorIcon::Pointer } else { CursorIcon::Grab });
                    return EventResult::Handled;
                }
                EventResult::Ignored
            }
            Event::MouseUp { button: MouseButton::Left, .. } if self.drag.is_some() => {
                self.drag = None;
                EventResult::Handled
            }
            Event::MouseWheel { delta, position, .. } if self.bounds.contains(*position) => {
                if ctx.modifiers.ctrl {
                    let k = if *delta > 0.0 { 1.12 } else { 1.0 / 1.12 };
                    let nz = (self.zoom * k).clamp(0.1, 3.0);
                    // точка под курсором остаётся на месте
                    let gx = (position.x - self.bounds.origin.x - self.pan.x) / self.zoom;
                    let gy = (position.y - self.bounds.origin.y - self.pan.y) / self.zoom;
                    self.zoom = nz;
                    self.pan = Point::new(position.x - self.bounds.origin.x - gx * nz, position.y - self.bounds.origin.y - gy * nz);
                } else if ctx.modifiers.shift {
                    self.pan.x += delta.signum() * 60.0;
                } else {
                    self.pan.y += delta.signum() * 60.0;
                }
                ctx.request_paint();
                EventResult::Handled
            }
            Event::KeyDown(Key::Home) => {
                self.center_on_root();
                ctx.request_paint();
                EventResult::Handled
            }
            _ => EventResult::Ignored,
        }
    }

    fn animate(&mut self, _dt: Duration) -> bool {
        false
    }
    fn children(&self) -> &[ElementId] {
        &[]
    }
    fn bounds(&self) -> Rect {
        self.bounds
    }
    fn set_position(&mut self, p: Point) {
        self.bounds.origin = p;
    }
    fn mark_dirty(&mut self, f: DirtyFlags) {
        self.dirty |= f;
    }
    fn clear_dirty(&mut self, f: DirtyFlags) {
        self.dirty.remove(f);
    }
    fn is_dirty(&self, f: DirtyFlags) -> bool {
        self.dirty.contains(f)
    }
    fn id(&self) -> ElementId {
        self.id
    }
    fn set_id(&mut self, id: ElementId) {
        self.id = id;
    }
    fn mount(&mut self, tree: &mut ElementTree) {
        self.text_measure = tree.text_measure.clone();
    }
    fn element_type_name(&self) -> &str {
        "GraphView"
    }
    fn layout_hint(&self) -> LayoutHint {
        LayoutHint::Padding { left: 0.0, top: 0.0, right: 0.0, bottom: 0.0 }
    }
    fn text_input_hit(&self, point: Point) -> bool {
        self.bounds.contains(point)
    }
    fn set_classes(&mut self, c: Vec<String>) {
        self.classes = c;
        self.mark_dirty(DirtyFlags::RENDER);
    }
    fn get_classes(&self) -> &[String] {
        &self.classes
    }
    fn mss(&self) -> Option<&MssFields> {
        Some(&self.mss)
    }
    fn reset_mss_styles(&mut self) {
        self.mss.reset();
    }
    fn apply_computed_style(&mut self, s: &ComputedStyle) {
        self.mss.apply(s);
        self.layout = None;
        self.mark_dirty(DirtyFlags::LAYOUT | DirtyFlags::RENDER);
    }
}

impl StyledElement for GraphElement {
    fn apply_style(&mut self, _s: &ComputedStyle) {
        self.mark_dirty(DirtyFlags::RENDER);
    }
    fn classes(&self) -> &[String] {
        &self.classes
    }
    fn set_classes(&mut self, c: Vec<String>) {
        self.classes = c;
    }
}
