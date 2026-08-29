use crate::core::{Color, Point, Rect, RectExt, Size};
use crate::input::{CursorIcon, Event, EventResult, MouseButton};
use crate::layout::Constraints;
use crate::mss::{ComputedStyle, Dimension, TextAlign, TextDecoration};
use crate::mss::{IconState, MssFields};
use crate::render::{Border, DisplayList};
use crate::widget::context::{EventContext, EventContextExt};
use crate::widget::{DirtyFlags, Element, ElementId, ElementTree, StyledElement, UpdateContext, Widget};
use std::any::Any;
use std::sync::Arc;
use crate::core::sync::Mutex;

use super::list_view::SelectionMode;

#[derive(Clone, Debug, Default, PartialEq)]
pub struct TreeNodeDecoration {
    pub label_color: Option<Color>,
    pub icon_color: Option<Color>,
    pub badge_color: Option<Color>,
    pub strikethrough: bool,
}

#[derive(Clone, Debug)]
pub struct TreeNode {
    pub id: String,
    pub label: String,
    pub icon: Option<String>,
    pub children: Vec<TreeNode>,
    pub expanded: bool,
    pub decoration: Option<TreeNodeDecoration>,
}

impl TreeNode {
    pub fn leaf(id: impl Into<String>, label: impl Into<String>) -> Self {
        Self {
            id: id.into(),
            label: label.into(),
            icon: None,
            children: Vec::new(),
            expanded: false,
            decoration: None,
        }
    }

    pub fn branch(id: impl Into<String>, label: impl Into<String>, children: Vec<TreeNode>) -> Self {
        Self {
            id: id.into(),
            label: label.into(),
            icon: None,
            children,
            expanded: false,
            decoration: None,
        }
    }

    pub fn icon(mut self, icon: impl Into<String>) -> Self {
        self.icon = Some(icon.into());
        self
    }

    pub fn expanded(mut self, expanded: bool) -> Self {
        self.expanded = expanded;
        self
    }

    pub fn decoration(mut self, deco: TreeNodeDecoration) -> Self {
        self.decoration = Some(deco);
        self
    }

    pub fn label_color(mut self, c: Color) -> Self {
        let mut deco = self.decoration.take().unwrap_or_default();
        deco.label_color = Some(c);
        self.decoration = Some(deco);
        self
    }

    pub fn badge(mut self, c: Color) -> Self {
        let mut deco = self.decoration.take().unwrap_or_default();
        deco.badge_color = Some(c);
        self.decoration = Some(deco);
        self
    }

    pub fn strikethrough(mut self, on: bool) -> Self {
        let mut deco = self.decoration.take().unwrap_or_default();
        deco.strikethrough = on;
        self.decoration = Some(deco);
        self
    }
}

#[derive(Clone, Debug)]
struct FlatNode {
    id: String,
    label: String,
    icon: Option<String>,
    depth: usize,
    has_children: bool,
    expanded: bool,
    decoration: Option<TreeNodeDecoration>,
}

fn flatten_nodes(nodes: &[TreeNode], depth: usize, result: &mut Vec<FlatNode>) {
    for node in nodes {
        result.push(FlatNode {
            id: node.id.clone(),
            label: node.label.clone(),
            icon: node.icon.clone(),
            depth,
            has_children: !node.children.is_empty(),
            expanded: node.expanded,
            decoration: node.decoration.clone(),
        });
        if node.expanded && !node.children.is_empty() {
            flatten_nodes(&node.children, depth + 1, result);
        }
    }
}

fn toggle_node(nodes: &mut [TreeNode], target_id: &str) -> bool {
    for node in nodes.iter_mut() {
        if node.id == target_id {
            node.expanded = !node.expanded;
            return true;
        }
        if toggle_node(&mut node.children, target_id) {
            return true;
        }
    }
    false
}

pub struct TreeView {
    nodes: Vec<TreeNode>,
    indent: f32,
    item_height: f32,
    show_lines: bool,
    selection_mode: SelectionMode,
    selected: Vec<String>,
    on_select: Option<Arc<Mutex<dyn FnMut(&str) + Send>>>,
    on_toggle: Option<Arc<Mutex<dyn FnMut(&str, bool) + Send>>>,
    width: Option<Dimension>,
    height: Option<Dimension>,
    classes: Vec<String>,
}

impl TreeView {
    pub fn new(nodes: Vec<TreeNode>) -> Self {
        Self {
            nodes,
            indent: 24.0,
            item_height: 32.0,
            show_lines: false,
            selection_mode: SelectionMode::None,
            selected: Vec::new(),
            on_select: None,
            on_toggle: None,
            width: None,
            height: None,
            classes: Vec::new(),
        }
    }

    pub fn class(mut self, class: impl Into<String>) -> Self {
        self.classes.push(class.into());
        self
    }

    pub fn indent(mut self, indent: f32) -> Self { self.indent = indent; self }
    pub fn item_height(mut self, h: f32) -> Self { self.item_height = h; self }
    pub fn show_lines(mut self, show: bool) -> Self { self.show_lines = show; self }
    pub fn selection_mode(mut self, mode: SelectionMode) -> Self { self.selection_mode = mode; self }
    pub fn selected(mut self, ids: Vec<String>) -> Self { self.selected = ids; self }
    pub fn width(mut self, w: f32) -> Self { self.width = Some(Dimension::Px(w)); self }
    pub fn height(mut self, h: f32) -> Self { self.height = Some(Dimension::Px(h)); self }

    pub fn on_select(mut self, f: impl FnMut(&str) + Send + 'static) -> Self {
        self.on_select = Some(Arc::new(Mutex::new(f)));
        self
    }

    pub fn on_toggle(mut self, f: impl FnMut(&str, bool) + Send + 'static) -> Self {
        self.on_toggle = Some(Arc::new(Mutex::new(f)));
        self
    }
}

impl TreeView {
    /// Собрать элемент. Отдельно от [`Widget::create_element`], чтобы тесты
    /// могли работать с конкретным типом, а не с `Box<dyn Element>`.
    fn element(&self) -> TreeViewElement {
        let mut flat = Vec::new();
        flatten_nodes(&self.nodes, 0, &mut flat);
        TreeViewElement {
            id: ElementId::new(),
            nodes: self.nodes.clone(),
            flat_nodes: flat,
            indent: self.indent,
            item_height: self.item_height,
            show_lines: self.show_lines,
            selection_mode: self.selection_mode,
            selected: self.selected.clone(),
            on_select: self.on_select.clone(),
            on_toggle: self.on_toggle.clone(),
            fixed_width: self.width,
            fixed_height: self.height,
            scroll_offset: 0.0,
            scrollbar_fader: crate::widgets::scroll::ScrollbarFader::default(),
            scrollbar_interaction: crate::widgets::scroll::ScrollbarInteraction::default(),
            rows: Vec::new(),
            rows_width: -1.0,
            text_measure: None,
            hovered_index: None,
            bounds: Rect::zero(),
            classes: self.classes.clone(),
            dirty_flags: DirtyFlags::LAYOUT | DirtyFlags::RENDER,
            mss: MssFields::new(),
        }
    }
}

impl Widget for TreeView {
    fn create_element(&self) -> Box<dyn Element> {
        Box::new(self.element())
    }

    fn can_update(&self, other: &dyn Any) -> bool { other.is::<Self>() }
    fn as_any(&self) -> &dyn Any { self }
    fn as_any_mut(&mut self) -> &mut dyn Any { self }
    fn mount(&self, _tree: &mut ElementTree, _parent_id: ElementId) {}
    fn widget_classes(&self) -> &[String] { &self.classes }
}

const ARROW_ZONE_WIDTH: f32 = 20.0;

/// Размер подписи узла и шаг переноса. `* 1.3` — тот же множитель, что
/// `FontAtlas::shape_text` использует для межстрочного интервала.
const LABEL_FONT_SIZE: f32 = 14.0;
const LABEL_LINE_HEIGHT: f32 = LABEL_FONT_SIZE * 1.3;

/// Плашка выделения/hover. `PAD` — воздух между рамкой и содержимым: он
/// одинаков слева и справа, поэтому левый край плашки считается от первого
/// нарисованного элемента строки, а не от края виджета — иначе отступ
/// вложенности и пустая зона стрелки уезжали бы внутрь рамки.
/// `EDGE` — зазор от правого края виджета, чтобы рамка не липла к скроллбару.
const ROW_PILL_RADIUS: f32 = 9.0;
const ROW_PILL_PAD: f32 = 6.0;
const ROW_PILL_EDGE: f32 = 4.0;
const ROW_PILL_INSET_Y: f32 = 1.0;

const ICON_GLYPH_SIZE: f32 = 16.0;
const BADGE_DIAMETER_RATIO: f32 = 0.55;
const BADGE_CORNER_INSET_RATIO: f32 = 0.0;
const BADGE_HALO_RATIO: f32 = 0.08;
const BADGE_GLYPH: &str = "\u{EF4A}";

/// Вертикальная геометрия одной видимой строки. Строки больше не лежат на
/// равномерной сетке: подпись, не влезшая в ширину панели, переносится, и
/// строка становится выше на столько, сколько заняли переносы.
#[derive(Clone, Copy, Debug, PartialEq)]
struct RowGeometry {
    /// Верх строки относительно начала контента (без учёта скролла).
    top: f32,
    height: f32,
}

pub struct TreeViewElement {
    id: ElementId,
    nodes: Vec<TreeNode>,
    flat_nodes: Vec<FlatNode>,
    indent: f32,
    item_height: f32,
    show_lines: bool,
    selection_mode: SelectionMode,
    selected: Vec<String>,
    on_select: Option<Arc<Mutex<dyn FnMut(&str) + Send>>>,
    on_toggle: Option<Arc<Mutex<dyn FnMut(&str, bool) + Send>>>,
    fixed_width: Option<Dimension>,
    fixed_height: Option<Dimension>,
    scroll_offset: f32,
    hovered_index: Option<usize>,
    bounds: Rect,
    classes: Vec<String>,
    dirty_flags: DirtyFlags,
    mss: MssFields,
    scrollbar_fader: crate::widgets::scroll::ScrollbarFader,
    scrollbar_interaction: crate::widgets::scroll::ScrollbarInteraction,
    /// Кэш высот строк, пересчитывается в `layout` при смене ширины или
    /// состава дерева.
    rows: Vec<RowGeometry>,
    rows_width: f32,
    text_measure: Option<Arc<dyn crate::widget::context::TextMeasure>>,
}

impl TreeViewElement {
    /// Левый край подписи узла относительно левого края виджета: отступ,
    /// вложенность, зона стрелки и, если есть, иконка типа.
    fn label_offset(&self, node: &FlatNode) -> f32 {
        let mut x = 8.0 + node.depth as f32 * self.indent + ARROW_ZONE_WIDTH;
        if node.icon.is_some() {
            x += 24.0;
        }
        x
    }

    /// Левый край содержимого строки — то, вокруг чего рисуется плашка
    /// выделения: стрелка раскрытия, а у листьев — иконка типа.
    fn content_offset(&self, node: &FlatNode) -> f32 {
        let x = 8.0 + node.depth as f32 * self.indent;
        if node.has_children { x } else { x + ARROW_ZONE_WIDTH }
    }

    /// Правый край текста: внутренняя граница плашки.
    fn label_right(&self, total_width: f32) -> f32 {
        total_width - ROW_PILL_EDGE - ROW_PILL_PAD
    }

    /// Плашка выделения/hover для строки. Обнимает содержимое с одинаковым
    /// воздухом со всех сторон: слева начинается от первого нарисованного
    /// элемента, справа заканчивается за концом подписи. У переносящейся
    /// подписи упирается в край панели.
    fn pill_rect(&self, node: &FlatNode, row_rect: Rect) -> Rect {
        let total_width = row_rect.size.width;
        let left = row_rect.x() + self.content_offset(node) - ROW_PILL_PAD;
        let text_end = row_rect.x()
            + self.label_offset(node)
            + self.label_painted_width(node, total_width);
        let right = (text_end + ROW_PILL_PAD)
            .min(row_rect.x() + total_width - ROW_PILL_EDGE);
        Rect::new(
            Point::new(left, row_rect.y() + ROW_PILL_INSET_Y),
            Size::new(
                (right - left).max(0.0),
                (row_rect.size.height - ROW_PILL_INSET_Y * 2.0).max(0.0),
            ),
        )
    }

    /// Ширина, остающаяся подписи в строке заданной общей ширины.
    fn label_width(&self, node: &FlatNode, total_width: f32) -> f32 {
        (self.label_right(total_width) - self.label_offset(node)).max(0.0)
    }

    /// Сколько подпись занимает на самом деле: своя ширина, но не больше
    /// доступной. Длинная подпись переносится и забирает всё место, короткая
    /// оставляет хвост пустым — по ней и обрезается плашка выделения.
    fn label_painted_width(&self, node: &FlatNode, total_width: f32) -> f32 {
        let available = self.label_width(node, total_width);
        let Some(tm) = self.text_measure.as_deref() else {
            return available;
        };
        let natural = tm.measure_text_width_styled(
            &node.label,
            LABEL_FONT_SIZE,
            node.label.chars().count(),
            false,
            None,
        );
        natural.min(available)
    }

    /// Высота строки: базовая плюс место под перенос подписи.
    fn measure_row(&self, node: &FlatNode, total_width: f32) -> f32 {
        let Some(tm) = self.text_measure.as_deref() else {
            return self.item_height;
        };
        // Запас в пиксель: рендерер переносит текст в физических пикселях с
        // округлённым кеглем, и без запаса расчёт изредка выходит
        // оптимистичнее реальной разбивки — строка бы всё равно наехала.
        let width = self.label_width(node, total_width) - 1.0;
        if width <= 0.0 {
            return self.item_height;
        }
        let lines = crate::widget::count_visual_lines_via_measure(
            &node.label,
            width,
            LABEL_FONT_SIZE,
            false,
            None,
            tm,
        );
        self.item_height + lines.saturating_sub(1) as f32 * LABEL_LINE_HEIGHT
    }

    /// Пересчитать кэш высот под текущую ширину виджета.
    fn recompute_rows(&mut self) {
        let width = self.bounds.size.width;
        let heights: Vec<f32> = self
            .flat_nodes
            .iter()
            .map(|n| self.measure_row(n, width))
            .collect();
        self.rows.clear();
        self.rows.reserve(heights.len());
        let mut top = 0.0;
        for height in heights {
            self.rows.push(RowGeometry { top, height });
            top += height;
        }
        self.rows_width = width;
    }

    /// Геометрия строки. Пока кэш не построен (layout ещё не звали) —
    /// равномерная сетка, как было до переноса подписей.
    fn row_geometry(&self, i: usize) -> RowGeometry {
        self.rows.get(i).copied().unwrap_or(RowGeometry {
            top: i as f32 * self.item_height,
            height: self.item_height,
        })
    }

    /// Индекс первой строки, чей низ ниже `offset`.
    fn row_index_at(&self, offset: f32) -> usize {
        if self.rows.len() == self.flat_nodes.len() {
            self.rows.partition_point(|r| r.top + r.height <= offset)
        } else {
            (offset / self.item_height).max(0.0) as usize
        }
    }

    fn content_height(&self) -> f32 {
        match self.rows.last() {
            Some(r) if self.rows.len() == self.flat_nodes.len() => r.top + r.height,
            _ => self.flat_nodes.len() as f32 * self.item_height,
        }
    }

    fn max_scroll(&self) -> f32 {
        (self.content_height() - self.bounds.size.height).max(0.0)
    }

    fn reflatten(&mut self) {
        self.flat_nodes.clear();
        flatten_nodes(&self.nodes, 0, &mut self.flat_nodes);
        self.recompute_rows();
    }

    fn row_at_y(&self, y: f32) -> Option<usize> {
        if y < self.bounds.y() || y > self.bounds.y() + self.bounds.size.height {
            return None;
        }
        let local_y = y - self.bounds.y() + self.scroll_offset;
        if local_y < 0.0 {
            return None;
        }
        let idx = self.row_index_at(local_y);
        if idx < self.flat_nodes.len() { Some(idx) } else { None }
    }

    fn is_in_arrow_zone(&self, x: f32, flat_idx: usize) -> bool {
        let node = &self.flat_nodes[flat_idx];
        if !node.has_children { return false; }
        let arrow_x = self.bounds.x() + 8.0 + node.depth as f32 * self.indent;
        x >= arrow_x && x < arrow_x + ARROW_ZONE_WIDTH
    }
}

impl Element for TreeViewElement {
    fn update(&mut self, widget: &dyn Widget, _ctx: &mut UpdateContext) {
        if let Some(tv) = widget.as_any().downcast_ref::<TreeView>() {
            self.nodes = tv.nodes.clone();
            self.indent = tv.indent;
            self.item_height = tv.item_height;
            self.show_lines = tv.show_lines;
            self.selection_mode = tv.selection_mode;
            self.selected = tv.selected.clone();
            self.on_select = tv.on_select.clone();
            self.on_toggle = tv.on_toggle.clone();
            self.fixed_width = tv.width;
            self.fixed_height = tv.height;
            self.reflatten();
            self.mark_dirty(DirtyFlags::LAYOUT | DirtyFlags::RENDER);
        }
    }

    fn layout(&mut self, constraints: Constraints) -> Size {
        let w = self.mss.width.or(self.fixed_width).map(|d| d.resolve(constraints.max_width)).unwrap_or(constraints.max_width).min(constraints.max_width);
        let h = self.mss.height.or(self.fixed_height).map(|d| d.resolve(constraints.max_height)).unwrap_or(constraints.max_height).min(constraints.max_height);
        let h = if h.is_infinite() { 300.0 } else { h };
        self.bounds = Rect::new(Point::zero(), Size::new(w, h));
        // Ширина определяет, где переносится подпись, а значит и высоты строк.
        if self.rows_width != w || self.rows.len() != self.flat_nodes.len() {
            self.recompute_rows();
        }
        Size::new(w, h)
    }

    fn build_display_list(&self, list: &mut DisplayList, _clip: Rect) {
        let bg = self.mss.background_color.unwrap_or(Color::TRANSPARENT);
        let border_color = self.mss.border_color.unwrap_or(Color::TRANSPARENT);
        let fg = self.mss.color.unwrap_or(Color::from_hex("#1F2937"));
        let primary = self.mss.accent_color.unwrap_or(Color::from_hex("#3B82F6"));

        if bg != Color::TRANSPARENT || border_color != Color::TRANSPARENT {
            list.push_rect_bordered(self.bounds, bg, [8.0; 4], Border::new(1.0, border_color));
        }
        list.push_clip(self.bounds);

        let viewport_top = self.scroll_offset;
        let viewport_bottom = viewport_top + self.bounds.size.height;
        let first = self.row_index_at(viewport_top);
        let last = (self.row_index_at(viewport_bottom) + 1).min(self.flat_nodes.len());

        for i in first..last {
            let node = &self.flat_nodes[i];
            let geom = self.row_geometry(i);
            let y = self.bounds.y() + geom.top - self.scroll_offset;
            let row_rect = Rect::new(
                Point::new(self.bounds.x(), y),
                Size::new(self.bounds.size.width, geom.height),
            );

            let is_selected = self.selected.contains(&node.id);
            let is_hovered = self.hovered_index == Some(i);
            let pill = self.pill_rect(node, row_rect);
            if is_selected {
                list.push_rect_bordered(
                    pill,
                    primary.with_alpha(0.15),
                    [ROW_PILL_RADIUS; 4],
                    Border::new(1.0, primary.with_alpha(0.5)),
                );
            } else if is_hovered {
                // Фон дерева часто прозрачный, поэтому hover строится от
                // цвета текста: `bg.darken` на прозрачном не давал ничего.
                list.push_rect(pill, fg.with_alpha(0.07), [ROW_PILL_RADIUS; 4]);
            }

            let x_base = self.bounds.x() + 8.0 + node.depth as f32 * self.indent;

            if self.show_lines && node.depth > 0 {
                let line_x = self.bounds.x() + 8.0 + (node.depth as f32 - 1.0) * self.indent + ARROW_ZONE_WIDTH / 2.0;
                let vl = Rect::new(
                    Point::new(line_x, y),
                    Size::new(1.0, geom.height),
                );
                list.push_rect(vl, border_color, [0.0; 4]);
                let hl = Rect::new(
                    Point::new(line_x, y + geom.height / 2.0),
                    Size::new(self.indent / 2.0, 1.0),
                );
                list.push_rect(hl, border_color, [0.0; 4]);
            }

            let row_state = if is_selected {
                IconState::Selected
            } else if is_hovered {
                IconState::Hover
            } else {
                IconState::Normal
            };

            if node.has_children {
                let arrow = if node.expanded { "\u{E5CF}" } else { "\u{E5CC}" };
                let arrow_rect = Rect::new(
                    Point::new(x_base, y + (geom.height - ICON_GLYPH_SIZE) / 2.0),
                    Size::new(18.0, ICON_GLYPH_SIZE),
                );
                list.push_text(
                    arrow,
                    arrow_rect,
                    self.mss.icon_color(IconState::Normal, fg),
                    ICON_GLYPH_SIZE,
                );
            }

            let mut text_x = x_base + ARROW_ZONE_WIDTH;

            if let Some(ref icon) = node.icon {
                let icon_top = y + (geom.height - ICON_GLYPH_SIZE) / 2.0;
                let icon_rect = Rect::new(
                    Point::new(text_x, icon_top),
                    Size::new(20.0, ICON_GLYPH_SIZE),
                );
                let icon_col = node
                    .decoration
                    .as_ref()
                    .and_then(|d| d.icon_color.or(d.label_color))
                    .unwrap_or_else(|| self.mss.icon_color(row_state, fg));
                list.push_text(icon, icon_rect, icon_col, ICON_GLYPH_SIZE);

                if let Some(badge_color) = node.decoration.as_ref().and_then(|d| d.badge_color) {
                    let glyph = ICON_GLYPH_SIZE;
                    let badge_d = glyph * BADGE_DIAMETER_RATIO;
                    let inset = glyph * BADGE_CORNER_INSET_RATIO;
                    let badge_x = text_x + glyph - badge_d - inset;
                    let badge_y = icon_top + glyph - badge_d - inset;
                    let badge_rect = Rect::new(
                        Point::new(badge_x, badge_y),
                        Size::new(badge_d, badge_d),
                    );
                    if bg != Color::TRANSPARENT {
                        let halo_extra = glyph * BADGE_HALO_RATIO;
                        let halo_d = badge_d + halo_extra * 2.0;
                        let halo_rect = Rect::new(
                            Point::new(badge_x - halo_extra, badge_y - halo_extra),
                            Size::new(halo_d, halo_d),
                        );
                        list.push_text(BADGE_GLYPH, halo_rect, bg, halo_d);
                    }
                    list.push_text(BADGE_GLYPH, badge_rect, badge_color, badge_d);
                }

                text_x += 24.0;
            }

            // Rect подписи занимает строку целиком: рендерер центрирует
            // текст по переданному прямоугольнику, и только при полной
            // высоте центр текста совпадает с центром плашки — иначе воздух
            // сверху и снизу получается разным. Заодно сюда помещаются все
            // строки переноса.
            let label_rect = Rect::new(
                Point::new(text_x, y),
                Size::new(
                    (self.bounds.x() + self.label_right(self.bounds.size.width) - text_x).max(0.0),
                    geom.height,
                ),
            );
            let text_color = if is_selected {
                primary
            } else {
                node.decoration
                    .as_ref()
                    .and_then(|d| d.label_color)
                    .unwrap_or(fg)
            };
            let strikethrough = node
                .decoration
                .as_ref()
                .map(|d| d.strikethrough)
                .unwrap_or(false);
            if strikethrough {
                list.push_text_aligned(
                    &node.label,
                    label_rect,
                    text_color,
                    LABEL_FONT_SIZE,
                    TextAlign::DEFAULT,
                    TextDecoration::LineThrough,
                    400,
                );
            } else {
                list.push_text(&node.label, label_rect, text_color, LABEL_FONT_SIZE);
            }
        }

        let style = self.mss.scrollbar_style(fg);
        let opacity = crate::widgets::scroll::effective_opacity(&self.scrollbar_fader, &style);
        if opacity > 0.0 {
            crate::widgets::scroll::render_vertical(
                list,
                self.bounds,
                self.content_height(),
                self.scroll_offset,
                &style,
                &self.scrollbar_fader,
                opacity,
            );
        }

        list.pop_clip();

        if border_color != Color::TRANSPARENT {
            list.push_rect_bordered(self.bounds, Color::TRANSPARENT, [8.0; 4], Border::new(1.0, border_color));
        }
    }

    fn handle_event(&mut self, event: &Event, ctx: &mut EventContext) -> EventResult {
        let sb_style = self.mss.scrollbar_style(self.mss.color.unwrap_or(Color::from_hex("#9CA3AF")));
        let sb_geom = crate::widgets::scroll::ScrollbarGeom {
            viewport: self.bounds,
            content_w: 0.0,
            content_h: self.content_height(),
            scroll_x: 0.0,
            scroll_y: self.scroll_offset,
        };

        match event {
            Event::MouseMove(pos) => {
                if let Some((new_y, _)) = self.scrollbar_interaction.update_drag(
                    &mut self.scrollbar_fader, &sb_geom, &sb_style, *pos,
                ) {
                    let max = self.max_scroll();
                    self.scroll_offset = new_y.clamp(0.0, max);
                    ctx.request_paint();
                    return EventResult::Captured;
                }

                if !self.bounds.contains(*pos) {
                    let mut changed = false;
                    if self.hovered_index.is_some() {
                        self.hovered_index = None;
                        changed = true;
                    }
                    if self.scrollbar_interaction.clear_hover(&mut self.scrollbar_fader) {
                        changed = true;
                    }
                    if changed { ctx.request_paint(); }
                    return EventResult::Ignored;
                }

                let new_hover = self.row_at_y(pos.y);
                if new_hover != self.hovered_index {
                    self.hovered_index = new_hover;
                    ctx.request_paint();
                }

                if self.scrollbar_interaction.update_hover(
                    &mut self.scrollbar_fader, &sb_geom, &sb_style, *pos,
                    crate::widgets::scroll::SCROLLBAR_HIT_MARGIN,
                ) {
                    ctx.request_paint();
                }

                ctx.set_cursor(CursorIcon::Pointer);
                EventResult::Handled
            }
            Event::MouseDown { button, position } if *button == MouseButton::Right => {
                if !self.bounds.contains(*position) { return EventResult::Ignored; }
                if let Some(idx) = self.row_at_y(position.y) {
                    let node_id = self.flat_nodes[idx].id.clone();
                    let changed = match self.selection_mode {
                        SelectionMode::None => false,
                        SelectionMode::Single => {
                            if self.selected.first().map(String::as_str) != Some(node_id.as_str()) {
                                self.selected = vec![node_id.clone()];
                                true
                            } else { false }
                        }
                        SelectionMode::Multiple => {
                            if !self.selected.iter().any(|s| s == &node_id) {
                                self.selected.push(node_id.clone());
                                true
                            } else { false }
                        }
                    };
                    if changed {
                        if let Some(ref cb) = self.on_select {
                            if let Ok(mut f) = cb.lock() { f(&node_id); }
                        }
                        ctx.request_paint();
                    }
                }
                EventResult::Ignored
            }
            Event::MouseDown { button, position } if *button == MouseButton::Left => {
                if !self.bounds.contains(*position) { return EventResult::Ignored; }

                if self.scrollbar_interaction.try_begin_drag(
                    &mut self.scrollbar_fader, &sb_geom, &sb_style, *position,
                ) {
                    ctx.request_paint();
                    return EventResult::Captured;
                }

                if let Some(idx) = self.row_at_y(position.y) {
                    if self.is_in_arrow_zone(position.x, idx) {
                        let node_id = self.flat_nodes[idx].id.clone();
                        let was_expanded = self.flat_nodes[idx].expanded;
                        toggle_node(&mut self.nodes, &node_id);
                        self.reflatten();
                        if let Some(ref cb) = self.on_toggle {
                            if let Ok(mut f) = cb.lock() { f(&node_id, !was_expanded); }
                        }
                        ctx.request_paint();
                        ctx.request_layout();
                        return EventResult::Handled;
                    }

                    let node_id = self.flat_nodes[idx].id.clone();
                    match self.selection_mode {
                        SelectionMode::None => {}
                        SelectionMode::Single => {
                            self.selected = vec![node_id.clone()];
                        }
                        SelectionMode::Multiple => {
                            if let Some(pos) = self.selected.iter().position(|s| s == &node_id) {
                                self.selected.remove(pos);
                            } else {
                                self.selected.push(node_id.clone());
                            }
                        }
                    }
                    if let Some(ref cb) = self.on_select {
                        if let Ok(mut f) = cb.lock() { f(&node_id); }
                    }
                    ctx.request_paint();
                    return EventResult::Handled;
                }
                EventResult::Handled
            }
            Event::MouseUp { button, .. } if *button == MouseButton::Left => {
                if self.scrollbar_interaction.end_drag(&mut self.scrollbar_fader) {
                    ctx.request_paint();
                    return EventResult::Handled;
                }
                EventResult::Ignored
            }
            Event::MouseWheel { delta, position, .. } => {
                if !self.bounds.contains(*position) { return EventResult::Ignored; }
                let scroll_amount = *delta;
                let new_offset = (self.scroll_offset - scroll_amount).clamp(0.0, self.max_scroll());
                if (new_offset - self.scroll_offset).abs() > 0.01 {
                    self.scroll_offset = new_offset;
                    self.scrollbar_fader.flash();
                    ctx.request_paint();
                    return EventResult::Handled;
                }
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

    /// Измеритель текста нужен, чтобы знать, на сколько строк развернётся
    /// подпись, и заложить под неё высоту строки.
    fn mount(&mut self, tree: &mut ElementTree) {
        self.text_measure = tree.text_measure.clone();
        self.recompute_rows();
    }

    fn set_classes(&mut self, classes: Vec<String>) {
        self.classes = classes;
        self.mark_dirty(DirtyFlags::RENDER);
    }

    fn get_classes(&self) -> &[String] { &self.classes }

    fn element_type_name(&self) -> &str { "TreeView" }

    fn reset_mss_styles(&mut self) { self.mss.reset(); }
    fn mss(&self) -> Option<&crate::mss::MssFields> { Some(&self.mss) }
    fn apply_computed_style(&mut self, style: &ComputedStyle) {
        self.mss.apply(style);
        if let Some(w) = style.width() { self.fixed_width = Some(w); }
        if let Some(h) = style.height() { self.fixed_height = Some(h); }
        self.mark_dirty(DirtyFlags::LAYOUT | DirtyFlags::RENDER);
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

    fn animate(&mut self, dt: std::time::Duration) -> bool {
        let style = self.mss.scrollbar_style(self.mss.color.unwrap_or(Color::from_hex("#9CA3AF")));
        self.scrollbar_fader.tick(dt.as_secs_f32(), &style)
    }

    fn needs_repaint(&self) -> bool {
        self.scrollbar_fader.opacity > 0.0
    }

    fn accessibility_info(&self) -> Option<crate::a11y::AccessibilityInfo> {
        Some(crate::a11y::AccessibilityInfo {
            role: crate::a11y::Role::Tree,
            state: crate::a11y::NodeState::default(),
            properties: crate::a11y::NodeProperties {
                label: Some(format!("Tree with {} nodes", self.flat_nodes.len())),
                ..Default::default()
            },
        })
    }
}

impl StyledElement for TreeViewElement {
    fn apply_style(&mut self, _style: &ComputedStyle) {
        self.mark_dirty(DirtyFlags::LAYOUT | DirtyFlags::RENDER);
    }

    fn classes(&self) -> &[String] { &self.classes }

    fn set_classes(&mut self, classes: Vec<String>) {
        self.classes = classes;
        self.mark_dirty(DirtyFlags::RENDER);
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::widget::Widget;

    #[test]
    fn test_widget_classes_returns_self_classes() {
        let tv = TreeView::new(vec![]).class("foo").class("bar");
        assert_eq!(
            tv.widget_classes(),
            &["foo".to_string(), "bar".to_string()] as &[String]
        );
    }

    #[test]
    fn test_widget_classes_empty_when_unset() {
        let tv = TreeView::new(vec![]);
        assert!(tv.widget_classes().is_empty());
    }

    #[test]
    fn decoration_default_is_none() {
        let leaf = TreeNode::leaf("a", "A");
        assert!(leaf.decoration.is_none());
        let branch = TreeNode::branch("b", "B", vec![]);
        assert!(branch.decoration.is_none());
    }

    #[test]
    fn tree_node_builders_chain_decoration() {
        let yellow = Color::from_hex("#FFB454");
        let red = Color::from_hex("#EE5E48");
        let node = TreeNode::leaf("a", "A")
            .label_color(yellow)
            .badge(red)
            .strikethrough(true);
        let deco = node.decoration.expect("decoration set by builders");
        assert_eq!(deco.label_color, Some(yellow));
        assert_eq!(deco.badge_color, Some(red));
        assert!(deco.strikethrough);
        assert_eq!(deco.icon_color, None);
    }

    #[test]
    fn tree_node_decoration_replaces_via_full_setter() {
        let blue = Color::from_hex("#3B82F6");
        let node = TreeNode::leaf("a", "A")
            .label_color(Color::from_hex("#FF0000"))
            .decoration(TreeNodeDecoration {
                label_color: Some(blue),
                ..Default::default()
            });
        let deco = node.decoration.unwrap();
        assert_eq!(deco.label_color, Some(blue));
    }

    #[test]
    fn flatten_preserves_decoration() {
        let yellow = Color::from_hex("#FFB454");
        let leaf = TreeNode::leaf("file", "file.rs").label_color(yellow);
        let branch = TreeNode::branch(
            "dir",
            "src",
            vec![leaf],
        )
        .badge(yellow)
        .expanded(true);

        let mut flat = Vec::new();
        flatten_nodes(&[branch], 0, &mut flat);
        assert_eq!(flat.len(), 2, "branch + leaf после раскрытия");

        let branch_flat = &flat[0];
        assert_eq!(branch_flat.id, "dir");
        let bd = branch_flat.decoration.as_ref().expect("branch decoration");
        assert_eq!(bd.badge_color, Some(yellow));

        let leaf_flat = &flat[1];
        assert_eq!(leaf_flat.id, "file");
        let ld = leaf_flat.decoration.as_ref().expect("leaf decoration");
        assert_eq!(ld.label_color, Some(yellow));
    }

    // --- переменная высота строк ---
    //
    // Подпись, не влезающая в ширину панели, переносится рендерером. Пока
    // строки лежали на равномерной сетке `i * item_height`, вторая строка
    // подписи наезжала на следующий узел.

    /// 10px на символ при любом кегле — считать ожидания легко в уме.
    struct MonoMeasure;
    impl crate::widget::context::TextMeasure for MonoMeasure {
        fn measure_text_width(&self, text: &str, _font_size: f32, char_count: usize) -> f32 {
            text.chars().take(char_count).count() as f32 * 10.0
        }
        fn hit_test_char(&self, _text: &str, _font_size: f32, x_offset: f32) -> usize {
            (x_offset / 10.0).floor().max(0.0) as usize
        }
    }

    /// Элемент с готовым измерителем и посчитанной раскладкой.
    fn element(labels: &[&str], width: f32) -> TreeViewElement {
        let nodes: Vec<TreeNode> = labels
            .iter()
            .enumerate()
            .map(|(i, l)| TreeNode::leaf(format!("n{i}"), *l))
            .collect();
        let tv = TreeView::new(nodes).item_height(26.0).indent(18.0);
        let mut el = tv.element();
        el.text_measure = Some(Arc::new(MonoMeasure));
        el.layout(Constraints {
            min_width: width,
            max_width: width,
            min_height: 400.0,
            max_height: 400.0,
            containing_block: Size::new(width, 400.0),
        });
        el
    }

    /// Ширина под подпись у корневого узла без иконки:
    /// `width - (8 + 0*indent + ARROW_ZONE_WIDTH) - 8`.
    #[test]
    fn short_label_keeps_the_base_row_height() {
        let el = element(&["src"], 200.0);
        assert_eq!(el.row_geometry(0).height, 26.0);
    }

    #[test]
    fn wrapped_label_makes_its_row_taller() {
        // Под подпись остаётся 200 - 28 - 8 = 164px, минус пиксель запаса —
        // 163px, то есть 16 символов в строке. Подпись из 32 — две строки.
        let el = element(&["synthos-0.1.0-100-x86_64.pkg.tar"], 200.0);
        let h = el.row_geometry(0).height;
        assert!(h > 26.0, "строка должна была вырасти, а осталась {h}");
        assert_eq!(h, 26.0 + LABEL_LINE_HEIGHT);
    }

    #[test]
    fn row_grows_further_as_the_label_grows() {
        let two = element(&["synthos-0.1.0-100-x86_64.pkg.tar"], 200.0);
        let three = element(&["synthos-0.1.0-100-x86_64.pkg.tar.zst.backup"], 200.0);
        let expected = two.row_geometry(0).height + LABEL_LINE_HEIGHT;
        let actual = three.row_geometry(0).height;
        assert!(
            (actual - expected).abs() < 0.01,
            "каждая лишняя строка подписи добавляет ровно один межстрочный шаг: \
             ждали {expected}, получили {actual}"
        );
    }

    #[test]
    fn following_rows_shift_down_by_the_extra_height() {
        let el = element(&["synthos-0.1.0-100-x86_64.pkg.tar", "src"], 200.0);
        let first = el.row_geometry(0);
        assert_eq!(el.row_geometry(1).top, first.height, "второй узел стоит под первым");
        assert_eq!(el.row_geometry(1).height, 26.0);
    }

    #[test]
    fn content_height_sums_variable_rows() {
        let el = element(&["synthos-0.1.0-100-x86_64.pkg.tar", "src"], 200.0);
        assert_eq!(el.content_height(), el.row_geometry(0).height + 26.0);
    }

    /// Клик по второму узлу обязан попасть во второй узел, а не в хвост
    /// разросшегося первого.
    #[test]
    fn hit_test_follows_the_variable_grid() {
        let el = element(&["synthos-0.1.0-100-x86_64.pkg.tar", "src"], 200.0);
        let tall = el.row_geometry(0).height;
        assert_eq!(el.row_at_y(tall - 2.0), Some(0));
        assert_eq!(el.row_at_y(tall + 2.0), Some(1));
    }

    #[test]
    fn wider_panel_collapses_the_row_back() {
        let el = element(&["synthos-0.1.0-100-x86_64.pkg.tar"], 600.0);
        assert_eq!(el.row_geometry(0).height, 26.0, "в широкой панели переноса нет");
    }

    // --- геометрия плашки выделения ---

    /// Воздух слева от рамки до иконки обязан совпадать с воздухом справа
    /// до края текста, иначе рамка выглядит смещённой. Раньше плашка
    /// начиналась от края виджета и вбирала отступ вложенности со зоной
    /// стрелки — слева получалось заметно больше.
    #[test]
    fn pill_padding_is_symmetric_for_a_leaf() {
        // 7 символов по 10px — подпись короче доступной ширины, значит
        // плашка обрезается по её концу, а не по краю панели.
        let el = element(&["file.rs"], 300.0);
        let node = &el.flat_nodes[0];
        let row = Rect::new(Point::new(0.0, 0.0), Size::new(300.0, 26.0));
        let pill = el.pill_rect(node, row);

        // Слева: от рамки до иконки. Справа: от конца подписи до рамки.
        let text_end = el.label_offset(node) + el.label_painted_width(node, 300.0);
        let left_gap = el.content_offset(node) - pill.x();
        let right_gap = (pill.x() + pill.size.width) - text_end;
        assert_eq!(left_gap, right_gap, "воздух слева {left_gap} и справа {right_gap}");
        assert_eq!(left_gap, ROW_PILL_PAD);
        assert!(
            pill.x() + pill.size.width < 300.0 - ROW_PILL_EDGE,
            "короткая подпись не должна растягивать плашку до края панели"
        );
    }

    /// Переносящаяся подпись занимает всю доступную ширину — плашка
    /// упирается в край панели и дальше не растёт.
    #[test]
    fn pill_stops_at_the_panel_edge_for_a_wrapped_label() {
        let el = element(&["synthos-0.1.0-100-x86_64.pkg.tar"], 200.0);
        let row = Rect::new(Point::new(0.0, 0.0), Size::new(200.0, 26.0));
        let pill = el.pill_rect(&el.flat_nodes[0], row);
        assert_eq!(pill.x() + pill.size.width, 200.0 - ROW_PILL_EDGE);
    }

    /// Отступ вложенности остаётся снаружи рамки — именно он раньше делал
    /// левое поле шире правого.
    #[test]
    fn pill_excludes_the_indentation_gutter() {
        let tree = TreeNode::branch("d", "src", vec![TreeNode::leaf("f", "a.rs")]).expanded(true);
        let tv = TreeView::new(vec![tree]).item_height(26.0).indent(18.0);
        let el = tv.element();
        let row = Rect::new(Point::new(0.0, 0.0), Size::new(200.0, 26.0));
        let parent = el.pill_rect(&el.flat_nodes[0], row);
        let child = el.pill_rect(&el.flat_nodes[1], row);
        assert!(
            child.x() > parent.x(),
            "рамка вложенного узла сдвинута вправо: {} против {}",
            child.x(),
            parent.x()
        );
        // Отступ вложенности — 18px, и он остаётся снаружи рамки.
        assert_eq!(child.x() - parent.x(), 18.0 + ARROW_ZONE_WIDTH);
    }

    /// У узла со стрелкой рамка начинается от стрелки, у листа — от иконки:
    /// в обоих случаях от первого нарисованного элемента строки.
    #[test]
    fn pill_starts_at_the_first_painted_part_of_the_row() {
        let branch = TreeNode::branch("d", "src", vec![TreeNode::leaf("f", "a.rs")]);
        let tv = TreeView::new(vec![branch]).item_height(26.0).indent(18.0);
        let el = tv.element();
        let dir = &el.flat_nodes[0];
        assert_eq!(el.content_offset(dir), 8.0, "у папки рамка от стрелки");

        let leaf = element(&["a.rs"], 200.0);
        assert_eq!(
            leaf.content_offset(&leaf.flat_nodes[0]),
            8.0 + ARROW_ZONE_WIDTH,
            "у листа стрелки нет — рамка от иконки"
        );
    }

    /// Вложенность сдвигает рамку вместе с содержимым.
    #[test]
    fn pill_follows_indentation() {
        let tree = TreeNode::branch("d", "src", vec![TreeNode::leaf("f", "a.rs")]).expanded(true);
        let tv = TreeView::new(vec![tree]).item_height(26.0).indent(18.0);
        let el = tv.element();
        let child = &el.flat_nodes[1];
        assert_eq!(child.depth, 1);
        assert_eq!(el.content_offset(child), 8.0 + 18.0 + ARROW_ZONE_WIDTH);
    }

    /// Без измерителя (например, в headless-тесте) поведение прежнее —
    /// равномерная сетка.
    #[test]
    fn falls_back_to_the_uniform_grid_without_a_measurer() {
        let tv = TreeView::new(vec![TreeNode::leaf("a", "очень длинная подпись узла")])
            .item_height(26.0);
        let mut el = tv.element();
        el.layout(Constraints {
            min_width: 100.0,
            max_width: 100.0,
            min_height: 400.0,
            max_height: 400.0,
            containing_block: Size::new(100.0, 400.0),
        });
        assert_eq!(el.row_geometry(0).height, 26.0);
    }
}
