//! Контейнер-обвязка структурных блоков (цитата, callout, toggle, дети
//! пунктов списка): Column-раскладка детей + собственная отрисовка фона,
//! скругления и левой цветной полосы. Фон рисуется до детей, поэтому
//! оказывается под ними.
//!
//! В свободной раскладке ([`super::free`]) он же — обёртка блока:
//! `absolute` переводит контейнер в `LayoutHint::Positioned`, `fixed_width`
//! задаёт ширину колонки блока, а бездетная `extent`-распорка растягивает
//! холст редактора до нужного размера (Stack меряется по максимуму детей).

use std::any::Any;
use std::time::Duration;

use crate::core::{Color, Point, Rect, Size};
use crate::input::{Event, EventResult};
use crate::layout::{Constraints, CrossAxisAlignment, MainAxisAlignment};
use crate::render::DisplayList;
use crate::widget::context::{EventContext, UpdateContext};
use crate::widget::{DirtyFlags, Element, ElementId, ElementTree, LayoutHint, Widget};

use super::model::BlockId;
use super::state::BlockRectMap;

pub struct Chrome {
    children: Vec<Box<dyn Widget>>,
    gap: f32,
    padding: [f32; 4], // l, t, r, b
    bg: Option<Color>,
    radius: f32,
    border_left: Option<(f32, Color)>,
    /// Свободная раскладка: смещение блока от начала холста.
    absolute: Option<(f32, f32)>,
    /// Свободная раскладка: ширина колонки блока.
    fixed_width: Option<f32>,
    /// Занимать ширину видимой области (колонка потока на холсте с
    /// бесконечной шириной).
    fill_width: bool,
    /// Бездетная распорка холста: минимальный размер (не меньше вьюпорта).
    extent: Option<(f32, f32)>,
    /// Центрировать детей по поперечной оси (колонка потока).
    center: bool,
    /// Публиковать свой прямоугольник как геометрию блока.
    track: Option<(BlockId, BlockRectMap)>,
}

impl Chrome {
    pub fn new() -> Self {
        Self {
            children: Vec::new(),
            gap: 0.0,
            padding: [0.0; 4],
            bg: None,
            radius: 0.0,
            border_left: None,
            absolute: None,
            fixed_width: None,
            fill_width: false,
            extent: None,
            center: false,
            track: None,
        }
    }

    /// Публиковать свои границы как прямоугольник блока (ручка ⋮⋮, цель
    /// дропа, координаты при переносе).
    pub fn track(mut self, id: BlockId, map: BlockRectMap) -> Self {
        self.track = Some((id, map));
        self
    }

    /// Колонка потока внутри свободной раскладки: центрируется так же,
    /// как корневая колонка редактора (листья сами жмут ширину).
    pub fn center(mut self, center: bool) -> Self {
        self.center = center;
        self
    }

    /// Разместить блок в точке холста (свободная раскладка).
    pub fn absolute(mut self, x: f32, y: f32) -> Self {
        self.absolute = Some((x, y));
        self
    }

    /// Ширина колонки блока в свободной раскладке.
    pub fn fixed_width(mut self, width: f32) -> Self {
        self.fixed_width = Some(width.max(40.0));
        self
    }

    /// Ширина — по видимой области (containing block), а не по детям:
    /// нужна колонке потока на холсте, который прокручивается по
    /// горизонтали и потому меряется с бесконечной шириной.
    pub fn fill_width(mut self, fill: bool) -> Self {
        self.fill_width = fill;
        self
    }

    /// Распорка холста: держит размер не меньше (w, h) и вьюпорта.
    pub fn extent(w: f32, h: f32) -> Self {
        let mut chrome = Self::new();
        chrome.extent = Some((w, h));
        chrome
    }

    pub fn gap(mut self, gap: f32) -> Self {
        self.gap = gap;
        self
    }

    pub fn padding(mut self, left: f32, top: f32, right: f32, bottom: f32) -> Self {
        self.padding = [left, top, right, bottom];
        self
    }

    pub fn bg(mut self, color: Color) -> Self {
        self.bg = Some(color);
        self
    }

    pub fn radius(mut self, r: f32) -> Self {
        self.radius = r;
        self
    }

    pub fn border_left(mut self, width: f32, color: Color) -> Self {
        self.border_left = Some((width, color));
        self
    }

    pub fn child(mut self, child: Box<dyn Widget>) -> Self {
        self.children.push(child);
        self
    }

    pub fn children(mut self, children: impl IntoIterator<Item = Box<dyn Widget>>) -> Self {
        self.children.extend(children);
        self
    }
}

impl Widget for Chrome {
    fn create_element(&self) -> Box<dyn Element> {
        Box::new(ChromeElement {
            id: ElementId::new(),
            bounds: Rect::zero(),
            dirty: DirtyFlags::LAYOUT | DirtyFlags::RENDER,
            gap: self.gap,
            padding: self.padding,
            bg: self.bg,
            radius: self.radius,
            border_left: self.border_left,
            absolute: self.absolute,
            fixed_width: self.fixed_width,
            fill_width: self.fill_width,
            extent: self.extent,
            center: self.center,
            track: self.track.clone(),
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

    fn mount(&self, tree: &mut ElementTree, parent_id: ElementId) {
        for child in &self.children {
            let element = child.create_element();
            let child_id =
                tree.insert_with_type_id(element, Some(parent_id), child.as_any().type_id());
            child.mount(tree, child_id);
        }
    }

    fn child_widgets(&self) -> Vec<&dyn Widget> {
        self.children.iter().map(|c| c.as_ref() as &dyn Widget).collect()
    }
}

pub struct ChromeElement {
    id: ElementId,
    bounds: Rect,
    dirty: DirtyFlags,
    gap: f32,
    padding: [f32; 4],
    bg: Option<Color>,
    radius: f32,
    border_left: Option<(f32, Color)>,
    absolute: Option<(f32, f32)>,
    fixed_width: Option<f32>,
    fill_width: bool,
    extent: Option<(f32, f32)>,
    center: bool,
    track: Option<(BlockId, BlockRectMap)>,
}

impl Element for ChromeElement {
    fn update(&mut self, widget: &dyn Widget, ctx: &mut UpdateContext) {
        let Some(w) = widget.as_any().downcast_ref::<Chrome>() else { return };
        let layout_changed = self.gap != w.gap
            || self.padding != w.padding
            || self.absolute != w.absolute
            || self.fixed_width != w.fixed_width
            || self.fill_width != w.fill_width
            || self.extent != w.extent
            || self.center != w.center;
        self.gap = w.gap;
        self.padding = w.padding;
        self.bg = w.bg;
        self.radius = w.radius;
        self.border_left = w.border_left;
        self.absolute = w.absolute;
        self.fixed_width = w.fixed_width;
        self.fill_width = w.fill_width;
        self.extent = w.extent;
        self.center = w.center;
        self.track = w.track.clone();
        self.mark_dirty(DirtyFlags::RENDER);
        if layout_changed {
            self.mark_dirty(DirtyFlags::LAYOUT);
            ctx.mark_layout_dirty();
        }
    }

    fn mount(&mut self, _tree: &mut ElementTree) {}

    fn layout(&mut self, constraints: Constraints) -> Size {
        if let Some((ew, eh)) = self.extent {
            let cb = constraints.containing_block;
            let w = ew.max(cb.width).min(constraints.max_width.max(0.0));
            let h = eh.max(cb.height);
            self.bounds.size = Size::new(w, h);
            return self.bounds.size;
        }
        let width = if let Some(w) = self.fixed_width {
            w.min(constraints.max_width)
        } else if constraints.max_width.is_finite() {
            constraints.max_width
        } else if self.fill_width {
            constraints.containing_block.width
        } else {
            0.0
        };
        // С детьми дерево зовёт layout с tight-размером (min == max) — его
        // принимаем (фон/полоса рисуются по bounds); без детей — паддинги.
        let tight = constraints.min_height.is_finite()
            && (constraints.min_height - constraints.max_height).abs() < 0.5
            && constraints.min_height > 0.0;
        let height = if tight {
            constraints.min_height
        } else {
            self.padding[1] + self.padding[3]
        };
        self.bounds.size = Size::new(width, height);
        self.bounds.size
    }

    fn explicit_dimensions(&self, parent_width: f32, _parent_height: f32) -> (Option<f32>, Option<f32>) {
        if self.fixed_width.is_none() && self.fill_width && parent_width.is_finite() && parent_width > 0.0 {
            return (Some(parent_width), None);
        }
        (self.fixed_width, None)
    }

    fn layout_hint(&self) -> LayoutHint {
        if let Some((x, y)) = self.absolute {
            return LayoutHint::Positioned { x, y };
        }
        LayoutHint::Column {
            gap: self.gap,
            cross_align: if self.center {
                CrossAxisAlignment::Center
            } else {
                CrossAxisAlignment::Stretch
            },
            main_align: MainAxisAlignment::Start,
            padding_left: self.padding[0],
            padding_top: self.padding[1],
            padding_right: self.padding[2],
            padding_bottom: self.padding[3],
            expand: false,
        }
    }

    fn build_display_list(&self, list: &mut DisplayList, _clip: Rect) {
        if let Some(bg) = self.bg {
            let r = self.radius;
            list.push_rect(self.bounds, bg, [r, r, r, r]);
        }
        if let Some((width, color)) = self.border_left {
            let bar = Rect::new(
                self.bounds.origin,
                Size::new(width, self.bounds.size.height),
            );
            let r = (self.radius).min(width / 2.0);
            list.push_rect(bar, color, [r, r, r, r]);
        }
    }

    fn element_type_name(&self) -> &str {
        "doc-chrome"
    }

    /// Распорка лежит поверх всего холста последним ребёнком Stack'а: без
    /// прозрачности для хит-теста она перехватывала бы путь события, и
    /// клики не доходили бы до виджетов закреплённых блоков (кнопки доски,
    /// поля диаграммы).
    fn passthrough_hit_test(&self) -> bool {
        self.extent.is_some()
    }

    fn handle_event(&mut self, _event: &Event, _ctx: &mut EventContext) -> EventResult {
        EventResult::Ignored
    }

    fn animate(&mut self, _dt: Duration) -> bool {
        false
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
        // Дерево ставит Positioned-элемент в позицию родителя и смещает
        // только его ребёнка (`position_recursive`). Собственные границы
        // при этом остались бы в начале холста — а по ним считаются
        // хит-тест детей, ручка ⋮⋮ и цель дропа. Сдвигаем сами.
        self.bounds.origin = match self.absolute {
            Some((x, y)) => Point::new(pos.x + x, pos.y + y),
            None => pos,
        };
        // Размер уже посчитан — публикуем прямоугольник блока целиком.
        if let Some((id, map)) = &self.track {
            if let Ok(mut m) = map.lock() {
                m.insert(*id, self.bounds);
            }
        }
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
