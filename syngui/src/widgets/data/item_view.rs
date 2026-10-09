//! `ItemView` — виртуализированный вид элементов одинакового размера
//! (сетка значков или строки таблицы) с выделением как в файловых
//! менеджерах.
//!
//! - Раскладка: [`ItemLayout::Grid`] (ячейки фиксированного размера, столбцов
//!   сколько влезет) или [`ItemLayout::Rows`] (строки во всю ширину).
//! - Строятся только видимые элементы; геометрия известна заранее, поэтому
//!   попадание, рамка и прокрутка к элементу считаются без обхода дерева.
//! - Выделение **управляемое**: вид получает [`ItemSelection`] и сообщает
//!   новое через `on_selection_change` — хозяин хранит его в сигнале и
//!   перестраивает вид (как `TextField` со значением).
//! - Мышь: щелчок, Ctrl — переключить, Shift — диапазон от якоря, рамка по
//!   пустому месту (с Ctrl — добавляет), двойной щелчок/Enter — открыть,
//!   правая кнопка — меню (выделяет элемент под курсором), средняя — отдельный
//!   колбэк, перетаскивание выделенного (`on_drag_start`), приём переноса на
//!   элемент или фон (`drop_filter`, `on_drop`), автопрокрутка у краёв.
//! - Палец: прокрутка с инерцией (вертикальный жест; горизонтальный отдаётся
//!   наружу). Касание, остановившее инерцию, — не тап. В сенсорном режиме
//!   (`touch_mode`, телефон) тап открывает элемент, удержание выделяет его
//!   (режим выбора), тап в режиме выбора — переключает; удержание на фоне —
//!   меню. Без него касания ведут себя как мышь (тап — щелчок, удержание —
//!   правая кнопка).
//! - Клавиатура (когда `keyboard_active` и фокус не в поле ввода): стрелки
//!   с учётом сетки, Home/End, PgUp/PgDn, Shift — расширить, Ctrl — двигать
//!   курсор без выделения, Ctrl+Space — переключить, Ctrl+A, Escape, поиск
//!   по первым буквам (`item_label`).
//!
//! Рамка выделения рисуется цветом `selection-color` (заливка с
//! прозрачностью, обводка — сам цвет), полоса прокрутки — `color`.

use std::any::Any;
use std::collections::BTreeSet;
use std::sync::{Arc, Mutex};
use std::time::{Duration, Instant};

use crate::core::{Color, Point, Rect, Size, Transform};
use crate::input::{DragData, Event, EventResult, Key, Modifiers, MouseButton, VelocityTracker};
use crate::layout::Constraints;
use crate::mss::{ComputedStyle, MssFields};
use crate::render::{display_list::Border, DisplayList};
use crate::widget::context::{EventContext, EventContextExt};
use crate::widget::{
    DirtyFlags, Element, ElementId, ElementTree, LayoutHint, StyledElement, UpdateContext, Widget,
};
use crate::widgets::containers::{Positioned, Stack};

/// Как раскладывать элементы.
#[derive(Clone, Copy, Debug, PartialEq)]
pub enum ItemLayout {
    /// Ячейки `item_width × item_height`, столбцов — сколько влезет.
    Grid { item_width: f32, item_height: f32, gap: f32 },
    /// Строки во всю ширину (`min_width` — не уже, иначе горизонтальной
    /// прокрутки нет: строки просто сжимаются).
    Rows { row_height: f32 },
}

/// Выделение: множество индексов, курсор (фокус клавиатуры) и якорь
/// диапазона для Shift.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct ItemSelection {
    pub selected: BTreeSet<usize>,
    pub cursor: Option<usize>,
    pub anchor: Option<usize>,
}

impl ItemSelection {
    pub fn single(i: usize) -> Self {
        Self { selected: [i].into_iter().collect(), cursor: Some(i), anchor: Some(i) }
    }

    pub fn is_selected(&self, i: usize) -> bool {
        self.selected.contains(&i)
    }

    pub fn all(count: usize) -> Self {
        Self { selected: (0..count).collect(), cursor: None, anchor: None }
    }
}

/// Состояние элемента для построителя.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct ItemState {
    pub selected: bool,
    /// Курсор клавиатуры (рисовать обводку фокуса).
    pub cursor: bool,
    /// Над элементом держат перенос, и он его примет.
    pub drop_target: bool,
}

/// Что сделать при переносе на вид.
#[derive(Clone, Debug)]
pub struct ItemDrop {
    /// Элемент под курсором (`None` — фон).
    pub target: Option<usize>,
    pub data: DragData,
    /// Модификаторы в момент отпускания (Shift — переместить, Ctrl — копировать…).
    pub modifiers: Modifiers,
    /// Точка в координатах окна.
    pub position: Point,
}

type Builder = Arc<dyn Fn(usize, ItemState) -> Box<dyn Widget> + Send + Sync>;
type Cb<A> = Arc<Mutex<dyn FnMut(A) + Send>>;
type LabelFn = Arc<dyn Fn(usize) -> String + Send + Sync>;
type DragStartFn = Arc<dyn Fn(&ItemSelection) -> Option<DragData> + Send + Sync>;
type DropFilter = Arc<dyn Fn(Option<usize>, &DragData) -> bool + Send + Sync>;

fn call<A>(cb: &Option<Cb<A>>, a: A) {
    if let Some(cb) = cb {
        if let Ok(mut f) = cb.lock() {
            f(a);
        }
    }
}

const DRAG_THRESHOLD: f32 = 6.0;
const TYPEAHEAD_RESET: Duration = Duration::from_millis(900);
const BUFFER_ROWS: usize = 2;
/// Скорость автопрокрутки у края при рамке/переносе (px/с на px заступа).
const AUTOSCROLL_GAIN: f32 = 14.0;
const AUTOSCROLL_EDGE: f32 = 28.0;
/// Сдвиг пальца, после которого жест — прокрутка (или чужой, если вбок).
const TOUCH_SLOP: f32 = 6.0;
/// Слабее этого (px/с) бросок не начинается и инерция гаснет.
const FLING_MIN: f32 = 60.0;
/// Затухание инерции: скорость × e^(−k·t).
const FLING_DECAY: f32 = 3.2;

pub struct ItemView {
    count: usize,
    builder: Builder,
    layout: ItemLayout,
    selection: ItemSelection,
    keyboard_active: bool,
    /// Прокрутить к элементу: `(индекс, метка)` — метка меняется на каждый
    /// новый запрос, иначе повтор того же индекса не сработает.
    scroll_to: Option<(usize, u64)>,
    /// Сбросить прокрутку в начало, когда метка меняется (новая папка).
    reset_key: u64,
    on_selection_change: Option<Cb<ItemSelection>>,
    on_activate: Option<Cb<usize>>,
    on_context_menu: Option<Cb<(Option<usize>, Point)>>,
    on_middle_click: Option<Cb<Option<usize>>>,
    on_nav_button: Option<Cb<MouseButton>>,
    on_background_double_click: Option<Cb<()>>,
    on_drop: Option<Cb<ItemDrop>>,
    on_scroll: Option<Cb<f32>>,
    on_zoom: Option<Cb<i32>>,
    item_label: Option<LabelFn>,
    drag_start: Option<DragStartFn>,
    drop_filter: Option<DropFilter>,
    touch_mode: bool,
    single_click: bool,
    stretch: bool,
    classes: Vec<String>,
}

impl ItemView {
    pub fn new(count: usize, builder: impl Fn(usize, ItemState) -> Box<dyn Widget> + Send + Sync + 'static) -> Self {
        Self {
            count,
            builder: Arc::new(builder),
            layout: ItemLayout::Rows { row_height: 28.0 },
            selection: ItemSelection::default(),
            keyboard_active: true,
            scroll_to: None,
            reset_key: 0,
            on_selection_change: None,
            on_activate: None,
            on_context_menu: None,
            on_middle_click: None,
            on_nav_button: None,
            on_background_double_click: None,
            on_drop: None,
            on_scroll: None,
            on_zoom: None,
            item_label: None,
            drag_start: None,
            drop_filter: None,
            touch_mode: false,
            single_click: false,
            stretch: false,
            classes: Vec::new(),
        }
    }

    pub fn layout(mut self, l: ItemLayout) -> Self {
        self.layout = l;
        self
    }

    pub fn selection(mut self, s: ItemSelection) -> Self {
        self.selection = s;
        self
    }

    /// Обрабатывать клавиатуру (в разделённом виде — только активная панель).
    pub fn keyboard_active(mut self, on: bool) -> Self {
        self.keyboard_active = on;
        self
    }

    pub fn scroll_to(mut self, index: usize, nonce: u64) -> Self {
        self.scroll_to = Some((index, nonce));
        self
    }

    /// Новое значение — прокрутка в начало (сменили папку).
    pub fn reset_key(mut self, key: u64) -> Self {
        self.reset_key = key;
        self
    }

    pub fn on_selection_change(mut self, f: impl FnMut(ItemSelection) + Send + 'static) -> Self {
        self.on_selection_change = Some(Arc::new(Mutex::new(f)));
        self
    }

    /// Двойной щелчок или Enter по курсору.
    pub fn on_activate(mut self, f: impl FnMut(usize) + Send + 'static) -> Self {
        self.on_activate = Some(Arc::new(Mutex::new(f)));
        self
    }

    /// Правая кнопка или клавиша меню: элемент (или фон) и точка в окне.
    pub fn on_context_menu(mut self, f: impl FnMut((Option<usize>, Point)) + Send + 'static) -> Self {
        self.on_context_menu = Some(Arc::new(Mutex::new(f)));
        self
    }

    pub fn on_middle_click(mut self, f: impl FnMut(Option<usize>) + Send + 'static) -> Self {
        self.on_middle_click = Some(Arc::new(Mutex::new(f)));
        self
    }

    /// Боковые кнопки мыши «назад»/«вперёд».
    pub fn on_nav_button(mut self, f: impl FnMut(MouseButton) + Send + 'static) -> Self {
        self.on_nav_button = Some(Arc::new(Mutex::new(f)));
        self
    }

    pub fn on_background_double_click(mut self, f: impl FnMut(()) + Send + 'static) -> Self {
        self.on_background_double_click = Some(Arc::new(Mutex::new(f)));
        self
    }

    /// Прокрутка (смещение в px) — чтобы хозяин мог её запомнить.
    pub fn on_scroll(mut self, f: impl FnMut(f32) + Send + 'static) -> Self {
        self.on_scroll = Some(Arc::new(Mutex::new(f)));
        self
    }

    /// Ctrl+колесо над видом: `+1` — крупнее, `-1` — мельче (как в
    /// проводниках). Пока не задан, Ctrl+колесо прокручивает, как обычно.
    pub fn on_zoom(mut self, f: impl FnMut(i32) + Send + 'static) -> Self {
        self.on_zoom = Some(Arc::new(Mutex::new(f)));
        self
    }

    /// Подпись элемента для поиска по первым буквам.
    pub fn item_label(mut self, f: impl Fn(usize) -> String + Send + Sync + 'static) -> Self {
        self.item_label = Some(Arc::new(f));
        self
    }

    /// Данные переноса выделенных элементов (`None` — не переносить).
    pub fn on_drag_start(mut self, f: impl Fn(&ItemSelection) -> Option<DragData> + Send + Sync + 'static) -> Self {
        self.drag_start = Some(Arc::new(f));
        self
    }

    /// Можно ли сбросить перенос на элемент (`Some`) или на фон (`None`).
    pub fn drop_filter(mut self, f: impl Fn(Option<usize>, &DragData) -> bool + Send + Sync + 'static) -> Self {
        self.drop_filter = Some(Arc::new(f));
        self
    }

    pub fn on_drop(mut self, f: impl FnMut(ItemDrop) + Send + 'static) -> Self {
        self.on_drop = Some(Arc::new(Mutex::new(f)));
        self
    }

    /// Сенсорный режим (телефон): тап открывает элемент, удержание выделяет
    /// (режим выбора), тап при непустом выделении переключает элемент. Мышь
    /// ведёт себя как обычно.
    pub fn touch_mode(mut self, on: bool) -> Self {
        self.touch_mode = on;
        self
    }

    /// Открывать элемент одним щелчком мыши (как в KDE), а не двойным.
    /// Щелчок с Ctrl или Shift по-прежнему только выделяет.
    pub fn single_click(mut self, on: bool) -> Self {
        self.single_click = on;
        self
    }

    /// Сетка: растянуть ячейки, чтобы столбцы заняли всю ширину без пустого
    /// хвоста справа (`item_width` — наименьшая ширина ячейки).
    pub fn stretch(mut self, on: bool) -> Self {
        self.stretch = on;
        self
    }

    pub fn class(mut self, c: impl Into<String>) -> Self {
        crate::widget::push_classes(&mut self.classes, c.into());
        self
    }
}

impl Widget for ItemView {
    fn create_element(&self) -> Box<dyn Element> {
        let mut el = ItemViewElement {
            id: ElementId::new(),
            bounds: Rect::zero(),
            count: 0,
            builder: self.builder.clone(),
            layout: self.layout,
            selection: ItemSelection::default(),
            keyboard_active: self.keyboard_active,
            scroll_nonce: None,
            pending_scroll_to: None,
            reset_key: self.reset_key,
            on_selection_change: None,
            on_activate: None,
            on_context_menu: None,
            on_middle_click: None,
            on_nav_button: None,
            on_background_double_click: None,
            on_drop: None,
            on_scroll: None,
            on_zoom: None,
            item_label: None,
            drag_start: None,
            drop_filter: None,
            scroll: 0.0,
            built_range: None,
            built_width: -1.0,
            rebuild: true,
            press: None,
            marquee: None,
            drop_target: None,
            drop_hover: false,
            pointer: Point::zero(),
            typeahead: String::new(),
            typeahead_at: None,
            scrollbar_drag: None,
            scrollbar_hover: false,
            touch_mode: self.touch_mode,
            single_click: self.single_click,
            stretch: self.stretch,
            touch: None,
            velocity: VelocityTracker::new(),
            fling: 0.0,
            swallow_tap: false,
            classes: self.classes.clone(),
            dirty: DirtyFlags::LAYOUT | DirtyFlags::RENDER,
            mss: MssFields::new(),
            padding: [0.0; 4],
            scrollbar_width: 8.0,
            selection_color: Color::from_hex("#3b82f6"),
        };
        el.take_props(self);
        Box::new(el)
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

/// Нажатие на элемент: решение о выделении откладывается до отпускания
/// (щелчок по уже выделенному без сдвига — оставить только его; сдвиг —
/// перенос всего выделения).
#[derive(Clone, Copy)]
struct Press {
    at: Point,
    item: Option<usize>,
    deferred_single: bool,
    dragging: bool,
    /// Без Ctrl и Shift — щелчок может открыть элемент (`single_click`).
    plain: bool,
}

/// Палец на виде: прокрутка начинается, когда он сдвинулся вертикально.
#[derive(Clone, Copy)]
struct TouchPan {
    id: u64,
    start: Point,
    last: Point,
    panning: bool,
}

struct Marquee {
    /// В координатах содержимого.
    start: Point,
    end: Point,
    base: BTreeSet<usize>,
    additive: bool,
}

struct ItemViewElement {
    id: ElementId,
    bounds: Rect,
    count: usize,
    builder: Builder,
    layout: ItemLayout,
    selection: ItemSelection,
    keyboard_active: bool,
    scroll_nonce: Option<u64>,
    pending_scroll_to: Option<usize>,
    reset_key: u64,
    on_selection_change: Option<Cb<ItemSelection>>,
    on_activate: Option<Cb<usize>>,
    on_context_menu: Option<Cb<(Option<usize>, Point)>>,
    on_middle_click: Option<Cb<Option<usize>>>,
    on_nav_button: Option<Cb<MouseButton>>,
    on_background_double_click: Option<Cb<()>>,
    on_drop: Option<Cb<ItemDrop>>,
    on_scroll: Option<Cb<f32>>,
    on_zoom: Option<Cb<i32>>,
    item_label: Option<LabelFn>,
    drag_start: Option<DragStartFn>,
    drop_filter: Option<DropFilter>,

    scroll: f32,
    built_range: Option<(usize, usize)>,
    built_width: f32,
    rebuild: bool,
    press: Option<Press>,
    marquee: Option<Marquee>,
    drop_target: Option<usize>,
    drop_hover: bool,
    /// Последняя точка указателя (в координатах окна) — для автопрокрутки.
    pointer: Point,
    typeahead: String,
    typeahead_at: Option<Instant>,
    /// Захват ползунка: смещение указателя от верха ползунка.
    scrollbar_drag: Option<f32>,
    scrollbar_hover: bool,
    touch_mode: bool,
    single_click: bool,
    stretch: bool,
    touch: Option<TouchPan>,
    velocity: VelocityTracker,
    /// Скорость инерции после броска (px/с, «+» — вниз по содержимому).
    fling: f32,
    /// Касание остановило инерцию — его тап не открывает элемент.
    swallow_tap: bool,

    classes: Vec<String>,
    dirty: DirtyFlags,
    mss: MssFields,
    padding: [f32; 4],
    scrollbar_width: f32,
    selection_color: Color,
}

impl ItemViewElement {
    fn take_props(&mut self, w: &ItemView) {
        if w.count != self.count || w.layout != self.layout || w.selection != self.selection {
            self.rebuild = true;
        }
        if !Arc::ptr_eq(&w.builder, &self.builder) || w.stretch != self.stretch {
            self.rebuild = true;
        }
        if w.reset_key != self.reset_key {
            self.reset_key = w.reset_key;
            self.scroll = 0.0;
            self.fling = 0.0;
            self.press = None;
            self.marquee = None;
            self.rebuild = true;
        }
        self.count = w.count;
        self.builder = w.builder.clone();
        self.layout = w.layout;
        self.selection = w.selection.clone();
        self.keyboard_active = w.keyboard_active;
        self.touch_mode = w.touch_mode;
        self.single_click = w.single_click;
        self.stretch = w.stretch;
        self.on_selection_change = w.on_selection_change.clone();
        self.on_activate = w.on_activate.clone();
        self.on_context_menu = w.on_context_menu.clone();
        self.on_middle_click = w.on_middle_click.clone();
        self.on_nav_button = w.on_nav_button.clone();
        self.on_background_double_click = w.on_background_double_click.clone();
        self.on_drop = w.on_drop.clone();
        self.on_scroll = w.on_scroll.clone();
        self.on_zoom = w.on_zoom.clone();
        self.item_label = w.item_label.clone();
        self.drag_start = w.drag_start.clone();
        self.drop_filter = w.drop_filter.clone();
        if let Some((i, nonce)) = w.scroll_to {
            if self.scroll_nonce != Some(nonce) {
                self.scroll_nonce = Some(nonce);
                // Размер ещё может быть неизвестен — запомним и прокрутим в layout.
                self.pending_scroll_to = Some(i);
            }
        }
        self.clamp_scroll();
    }

    // ---- геометрия (в координатах содержимого: 0,0 — левый верх с отступом)

    fn viewport(&self) -> Size {
        Size::new(
            (self.bounds.size.width - self.padding[0] - self.padding[2] - self.scrollbar_width).max(0.0),
            (self.bounds.size.height - self.padding[1] - self.padding[3]).max(0.0),
        )
    }

    fn cols(&self) -> usize {
        match self.layout {
            ItemLayout::Rows { .. } => 1,
            ItemLayout::Grid { item_width, gap, .. } => {
                let w = self.viewport().width;
                (((w + gap) / (item_width + gap)).floor() as usize).max(1)
            }
        }
    }

    /// Ширина ячейки сетки (с `stretch` — поровну на всю ширину).
    fn cell_width(&self) -> f32 {
        match self.layout {
            ItemLayout::Rows { .. } => self.viewport().width,
            ItemLayout::Grid { item_width, gap, .. } if self.stretch => {
                let cols = self.cols() as f32;
                ((self.viewport().width - gap * (cols - 1.0)) / cols).max(item_width)
            }
            ItemLayout::Grid { item_width, .. } => item_width,
        }
    }

    fn step(&self) -> (f32, f32) {
        match self.layout {
            ItemLayout::Rows { row_height } => (self.viewport().width, row_height),
            ItemLayout::Grid { item_height, gap, .. } => (self.cell_width() + gap, item_height + gap),
        }
    }

    fn item_size(&self) -> Size {
        match self.layout {
            ItemLayout::Rows { row_height } => Size::new(self.viewport().width, row_height),
            ItemLayout::Grid { item_height, .. } => Size::new(self.cell_width(), item_height),
        }
    }

    fn rows(&self) -> usize {
        self.count.div_ceil(self.cols())
    }

    fn content_height(&self) -> f32 {
        let rows = self.rows();
        if rows == 0 {
            return 0.0;
        }
        let (_, sy) = self.step();
        let gap = match self.layout {
            ItemLayout::Grid { gap, .. } => gap,
            _ => 0.0,
        };
        rows as f32 * sy - gap
    }

    fn max_scroll(&self) -> f32 {
        (self.content_height() - self.viewport().height).max(0.0)
    }

    fn clamp_scroll(&mut self) {
        let m = self.max_scroll();
        if self.scroll > m {
            self.scroll = m;
        }
        if self.scroll < 0.0 {
            self.scroll = 0.0;
        }
    }

    fn item_rect(&self, i: usize) -> Rect {
        let cols = self.cols();
        let (sx, sy) = self.step();
        let (r, c) = (i / cols, i % cols);
        Rect::new(Point::new(c as f32 * sx, r as f32 * sy), self.item_size())
    }

    /// Точка окна → координаты содержимого.
    fn to_content(&self, p: Point) -> Point {
        Point::new(
            p.x - self.bounds.origin.x - self.padding[0],
            p.y - self.bounds.origin.y - self.padding[1] + self.scroll,
        )
    }

    fn item_at(&self, p: Point) -> Option<usize> {
        let c = self.to_content(p);
        if c.x < 0.0 || c.y < 0.0 {
            return None;
        }
        let (sx, sy) = self.step();
        let col = (c.x / sx).floor() as usize;
        let row = (c.y / sy).floor() as usize;
        let cols = self.cols();
        if col >= cols {
            return None;
        }
        let i = row * cols + col;
        if i >= self.count {
            return None;
        }
        self.item_rect(i).contains(c).then_some(i)
    }

    fn visible_range(&self) -> (usize, usize) {
        if self.count == 0 {
            return (0, 0);
        }
        let (_, sy) = self.step();
        let cols = self.cols();
        let first = ((self.scroll / sy).floor() as usize).saturating_sub(BUFFER_ROWS);
        let last = (((self.scroll + self.viewport().height) / sy).ceil() as usize + BUFFER_ROWS).min(self.rows());
        (first * cols, (last * cols).min(self.count))
    }

    fn in_scrollbar(&self, p: Point) -> bool {
        self.max_scroll() > 0.0
            && self.bounds.contains(p)
            && p.x >= self.bounds.origin.x + self.bounds.size.width - self.scrollbar_width - 4.0
    }

    fn thumb(&self) -> Rect {
        let track_h = self.bounds.size.height;
        let ch = self.content_height().max(1.0);
        let vh = self.viewport().height;
        let h = (vh / ch * track_h).clamp(24.0, track_h);
        let m = self.max_scroll();
        let y = if m > 0.0 { self.scroll / m * (track_h - h) } else { 0.0 };
        Rect::new(
            Point::new(self.bounds.origin.x + self.bounds.size.width - self.scrollbar_width, self.bounds.origin.y + y),
            Size::new(self.scrollbar_width, h),
        )
    }

    fn scroll_to_item(&mut self, i: usize) {
        if i >= self.count {
            return;
        }
        let r = self.item_rect(i);
        let vh = self.viewport().height;
        if r.origin.y < self.scroll {
            self.scroll = r.origin.y;
        } else if r.origin.y + r.size.height > self.scroll + vh {
            self.scroll = r.origin.y + r.size.height - vh;
        }
        self.clamp_scroll();
        self.rebuild = true;
    }

    fn set_scroll(&mut self, v: f32) {
        let old = self.scroll;
        self.scroll = v;
        self.clamp_scroll();
        if (self.scroll - old).abs() > 0.01 {
            self.mark_dirty(DirtyFlags::LAYOUT | DirtyFlags::RENDER);
            call(&self.on_scroll, self.scroll);
        }
    }

    // ---- выделение

    fn emit(&mut self, s: ItemSelection) {
        if s != self.selection {
            self.selection = s.clone();
            self.rebuild = true;
            self.mark_dirty(DirtyFlags::LAYOUT | DirtyFlags::RENDER);
            call(&self.on_selection_change, s);
        }
    }

    fn click_select(&mut self, i: usize, m: Modifiers) {
        let mut s = self.selection.clone();
        if m.shift {
            let a = s.anchor.unwrap_or(i);
            let (lo, hi) = (a.min(i), a.max(i));
            if !m.ctrl {
                s.selected.clear();
            }
            s.selected.extend(lo..=hi);
            s.anchor = Some(a);
        } else if m.ctrl {
            if !s.selected.remove(&i) {
                s.selected.insert(i);
            }
            s.anchor = Some(i);
        } else {
            s = ItemSelection::single(i);
        }
        s.cursor = Some(i);
        self.emit(s);
    }

    fn update_marquee(&mut self) {
        let Some(m) = &self.marquee else { return };
        let r = rect_from(m.start, m.end);
        let mut sel = m.base.clone();
        let additive = m.additive;
        let (lo, hi) = self.items_in(r);
        for i in lo..hi {
            if self.item_rect(i).intersects(&r) {
                if additive && m.base.contains(&i) {
                    sel.remove(&i);
                } else {
                    sel.insert(i);
                }
            }
        }
        let cursor = self.selection.cursor;
        let anchor = self.selection.anchor;
        self.emit(ItemSelection { selected: sel, cursor, anchor });
    }

    /// Диапазон индексов, строки которых пересекают прямоугольник.
    fn items_in(&self, r: Rect) -> (usize, usize) {
        let (_, sy) = self.step();
        let cols = self.cols();
        let r0 = (r.origin.y.max(0.0) / sy).floor() as usize;
        let r1 = ((r.origin.y + r.size.height).max(0.0) / sy).ceil() as usize + 1;
        ((r0 * cols).min(self.count), (r1 * cols).min(self.count))
    }

    fn move_cursor(&mut self, key: Key, m: Modifiers) -> bool {
        if self.count == 0 {
            return false;
        }
        let cols = self.cols();
        let page = ((self.viewport().height / self.step().1).floor() as usize).max(1) * cols;
        let cur = self.selection.cursor;
        let target = match (key, cur) {
            (Key::Home, _) => 0,
            (Key::End, _) => self.count - 1,
            (_, None) if matches!(key, Key::Up | Key::Left | Key::PageUp) => self.count - 1,
            (_, None) => 0,
            (Key::Left, Some(c)) if cols > 1 => c.saturating_sub(1),
            (Key::Right, Some(c)) if cols > 1 => (c + 1).min(self.count - 1),
            (Key::Up, Some(c)) => c.saturating_sub(cols),
            (Key::Down, Some(c)) => {
                if c + cols < self.count {
                    c + cols
                } else if c / cols < (self.count - 1) / cols {
                    // Неполная последняя строка — на её последний элемент.
                    self.count - 1
                } else {
                    c
                }
            }
            (Key::PageUp, Some(c)) => c.saturating_sub(page),
            (Key::PageDown, Some(c)) => (c + page).min(self.count - 1),
            _ => return false,
        };
        let mut s = self.selection.clone();
        if m.shift {
            let a = s.anchor.or(cur).unwrap_or(target);
            s.selected = (a.min(target)..=a.max(target)).collect();
            s.anchor = Some(a);
        } else if !m.ctrl {
            s.selected = [target].into_iter().collect();
            s.anchor = Some(target);
        }
        s.cursor = Some(target);
        self.emit(s);
        self.scroll_to_item(target);
        self.mark_dirty(DirtyFlags::LAYOUT | DirtyFlags::RENDER);
        true
    }

    fn typeahead(&mut self, c: char) -> bool {
        let Some(label) = self.item_label.clone() else { return false };
        if self.count == 0 || c.is_control() {
            return false;
        }
        let now = Instant::now();
        if self.typeahead_at.map(|t| now.duration_since(t) > TYPEAHEAD_RESET).unwrap_or(true) {
            self.typeahead.clear();
        }
        self.typeahead_at = Some(now);
        self.typeahead.extend(c.to_lowercase());
        let q = self.typeahead.clone();
        // Одна и та же буква подряд — перебирать элементы на неё.
        let repeat = q.chars().count() > 1 && q.chars().all(|x| Some(x) == q.chars().next());
        let start = self.selection.cursor.map(|c| if repeat || q.chars().count() == 1 { c + 1 } else { c }).unwrap_or(0);
        let needle: String = if repeat { q.chars().take(1).collect() } else { q };
        let found = (0..self.count)
            .map(|k| (start + k) % self.count)
            .find(|&i| label(i).to_lowercase().starts_with(&needle));
        if let Some(i) = found {
            self.emit(ItemSelection::single(i));
            self.scroll_to_item(i);
            self.mark_dirty(DirtyFlags::LAYOUT | DirtyFlags::RENDER);
        }
        true
    }

    fn drop_allowed(&self, target: Option<usize>, data: &DragData) -> bool {
        match &self.drop_filter {
            Some(f) => f(target, data),
            None => false,
        }
    }

    /// Куда упадёт перенос: элемент (если принимает) или фон.
    fn resolve_drop(&self, p: Point, data: &DragData) -> Option<Option<usize>> {
        let item = self.item_at(p);
        if let Some(i) = item {
            // Сам на себя (перенос выделенного на выделенный) — это фон.
            let own = data.source_id == self.id.0 && self.selection.is_selected(i);
            if !own && self.drop_allowed(Some(i), data) {
                return Some(Some(i));
            }
        }
        self.drop_allowed(None, data).then_some(None)
    }

    /// Тап пальцем в сенсорном режиме: в режиме выбора — переключить
    /// элемент, иначе — открыть его.
    fn touch_tap(&mut self, item: Option<usize>) {
        let Some(i) = item else { return };
        if self.selection.selected.is_empty() {
            call(&self.on_activate, i);
            return;
        }
        let mut s = self.selection.clone();
        if !s.selected.remove(&i) {
            s.selected.insert(i);
        }
        s.cursor = Some(i);
        s.anchor = Some(i);
        self.emit(s);
    }

    fn touch_scrollable(&self) -> bool {
        self.max_scroll() > 0.0
    }

    fn autoscroll_speed(&self) -> f32 {
        let top = self.bounds.origin.y + AUTOSCROLL_EDGE;
        let bottom = self.bounds.origin.y + self.bounds.size.height - AUTOSCROLL_EDGE;
        if self.pointer.y < top {
            -(top - self.pointer.y) * AUTOSCROLL_GAIN
        } else if self.pointer.y > bottom {
            (self.pointer.y - bottom) * AUTOSCROLL_GAIN
        } else {
            0.0
        }
    }

    fn wants_autoscroll(&self) -> bool {
        (self.marquee.is_some() || self.drop_hover) && self.autoscroll_speed() != 0.0
    }
}

fn rect_from(a: Point, b: Point) -> Rect {
    Rect::new(Point::new(a.x.min(b.x), a.y.min(b.y)), Size::new((a.x - b.x).abs(), (a.y - b.y).abs()))
}

/// Пустышка заданного размера — задаёт высоту содержимого.
struct Extent(Size);

impl Widget for Extent {
    fn create_element(&self) -> Box<dyn Element> {
        Box::new(ExtentElement { id: ElementId::new(), bounds: Rect::new(Point::zero(), self.0), dirty: DirtyFlags::LAYOUT })
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

struct ExtentElement {
    id: ElementId,
    bounds: Rect,
    dirty: DirtyFlags,
}

impl Element for ExtentElement {
    fn update(&mut self, widget: &dyn Widget, _ctx: &mut UpdateContext) {
        if let Some(w) = widget.as_any().downcast_ref::<Extent>() {
            if w.0 != self.bounds.size {
                self.bounds.size = w.0;
                self.mark_dirty(DirtyFlags::LAYOUT);
            }
        }
    }
    fn layout(&mut self, _c: Constraints) -> Size {
        self.bounds.size
    }
    fn build_display_list(&self, _list: &mut DisplayList, _clip: Rect) {}
    fn handle_event(&mut self, _event: &Event, _ctx: &mut EventContext) -> EventResult {
        EventResult::Ignored
    }
    fn hit_test(&self, _p: Point) -> bool {
        false
    }
    fn children(&self) -> &[ElementId] {
        &[]
    }
    fn bounds(&self) -> Rect {
        self.bounds
    }
    fn set_position(&mut self, pos: Point) {
        self.bounds.origin = pos;
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
    fn id(&self) -> ElementId {
        self.id
    }
    fn set_id(&mut self, id: ElementId) {
        self.id = id;
    }
    fn mount(&mut self, _tree: &mut ElementTree) {}
    fn element_type_name(&self) -> &str {
        "ItemViewExtent"
    }
}

impl Element for ItemViewElement {
    fn update(&mut self, widget: &dyn Widget, _ctx: &mut UpdateContext) {
        if let Some(w) = widget.as_any().downcast_ref::<ItemView>() {
            self.take_props(w);
            self.mark_dirty(DirtyFlags::LAYOUT | DirtyFlags::RENDER);
        }
    }

    fn layout(&mut self, c: Constraints) -> Size {
        let w = if c.max_width.is_finite() { c.max_width } else { c.min_width.max(0.0) };
        let h = if c.max_height.is_finite() { c.max_height } else { c.min_height.max(0.0) };
        self.bounds.size = Size::new(w, h);
        if let Some(i) = self.pending_scroll_to.take() {
            self.scroll_to_item(i);
        }
        self.clamp_scroll();
        self.bounds.size
    }

    fn layout_hint(&self) -> LayoutHint {
        LayoutHint::Scroll {
            left: self.padding[0],
            top: self.padding[1],
            right: self.padding[2] + self.scrollbar_width,
            bottom: self.padding[3],
            unbounded_width: false,
            unbounded_height: true,
        }
    }

    fn manages_own_children(&self) -> bool {
        true
    }

    fn needs_rebuild(&self) -> bool {
        self.rebuild
            || self.built_range != Some(self.visible_range())
            || (self.viewport().width - self.built_width).abs() > 0.5
    }

    fn build_children(&self) -> Vec<Box<dyn Widget>> {
        let (lo, hi) = self.visible_range();
        let vp = self.viewport();
        let mut kids: Vec<Box<dyn Widget>> = Vec::with_capacity(hi - lo + 1);
        kids.push(Box::new(Extent(Size::new(vp.width, self.content_height()))));
        let size = self.item_size();
        for i in lo..hi {
            let r = self.item_rect(i);
            let st = ItemState {
                selected: self.selection.is_selected(i),
                cursor: self.keyboard_active && self.selection.cursor == Some(i),
                drop_target: self.drop_target == Some(i),
            };
            kids.push(Box::new(
                Positioned::new((self.builder)(i, st)).at(r.origin.x, r.origin.y).dimensions(size.width, size.height),
            ));
        }
        vec![Box::new(Stack::new().children(kids))]
    }

    fn clear_rebuild(&mut self) {
        self.built_range = Some(self.visible_range());
        self.built_width = self.viewport().width;
        self.rebuild = false;
    }

    fn build_display_list(&self, list: &mut DisplayList, _clip: Rect) {
        self.mss.paint_box(list, self.bounds);
        list.push_clip(self.bounds);
        list.push_transform(Transform::translation(0.0, -self.scroll));
    }

    fn post_build_display_list(&self, list: &mut DisplayList, _clip: Rect) {
        if let Some(m) = &self.marquee {
            let r = rect_from(m.start, m.end);
            let r = Rect::new(
                Point::new(r.origin.x + self.bounds.origin.x + self.padding[0], r.origin.y + self.bounds.origin.y + self.padding[1]),
                r.size,
            );
            let c = self.selection_color;
            list.push_rect_bordered(r, c.with_alpha(0.22), [2.0; 4], Border::new(1.0, c.with_alpha(0.9)));
        }
        if self.drop_hover && self.drop_target.is_none() {
            // Перенос на фон — подсветить весь вид.
            let r = Rect::new(
                Point::new(self.bounds.origin.x + 1.0, self.bounds.origin.y + self.scroll + 1.0),
                Size::new(self.bounds.size.width - 2.0, self.bounds.size.height - 2.0),
            );
            list.push_rect_bordered(r, Color::TRANSPARENT, [6.0; 4], Border::new(2.0, self.selection_color.with_alpha(0.6)));
        }
        list.pop_transform();
        list.pop_clip();
        if self.max_scroll() > 0.0 {
            let base = self.mss.color.unwrap_or(Color::from_hex("#9CA3AF"));
            let a = if self.scrollbar_drag.is_some() {
                0.85
            } else if self.scrollbar_hover {
                0.7
            } else {
                0.4
            };
            let t = self.thumb();
            let inset = if self.scrollbar_hover || self.scrollbar_drag.is_some() { 1.0 } else { 2.0 };
            let t = Rect::new(
                Point::new(t.origin.x + inset, t.origin.y + 2.0),
                Size::new((t.size.width - inset * 2.0).max(2.0), (t.size.height - 4.0).max(8.0)),
            );
            list.push_rect(t, base.with_alpha(a), [t.size.width / 2.0; 4]);
        }
    }

    fn handle_event(&mut self, event: &Event, ctx: &mut EventContext) -> EventResult {
        let m = ctx.modifiers;
        match event {
            Event::MouseWheel { delta, position, .. } => {
                if !self.bounds.contains(*position) {
                    return EventResult::Ignored;
                }
                if m.ctrl && self.on_zoom.is_some() {
                    if *delta != 0.0 {
                        call(&self.on_zoom, if *delta > 0.0 { 1 } else { -1 });
                    }
                    return EventResult::Handled;
                }
                if self.max_scroll() <= 0.0 {
                    return EventResult::Ignored;
                }
                self.fling = 0.0;
                self.set_scroll(self.scroll - *delta);
                ctx.request_layout();
                EventResult::Handled
            }
            Event::MouseDown { button, position } => {
                if !self.bounds.contains(*position) {
                    return EventResult::Ignored;
                }
                self.pointer = *position;
                if *button == MouseButton::Left && self.in_scrollbar(*position) {
                    let t = self.thumb();
                    let grab = if t.contains(*position) { position.y - t.origin.y } else { t.size.height / 2.0 };
                    self.scrollbar_drag = Some(grab);
                    self.drag_scrollbar(position.y);
                    ctx.request_layout();
                    return EventResult::Captured;
                }
                let item = self.item_at(*position);
                if *button == MouseButton::Left && crate::input::is_synthesized_mouse() {
                    // Касание, остановившее инерцию, только останавливает её.
                    if std::mem::take(&mut self.swallow_tap) {
                        return EventResult::Handled;
                    }
                    if self.touch_mode {
                        self.touch_tap(item);
                        ctx.request_layout();
                        return EventResult::Handled;
                    }
                }
                match button {
                    MouseButton::Left => {
                        let mut deferred = false;
                        match item {
                            Some(i) if self.selection.is_selected(i) && !m.shift && !m.ctrl => {
                                // Может начаться перенос всего выделения.
                                deferred = true;
                                let mut s = self.selection.clone();
                                s.cursor = Some(i);
                                s.anchor = Some(i);
                                self.emit(s);
                            }
                            Some(i) => self.click_select(i, m),
                            None => {
                                let c = self.to_content(*position);
                                let base = if m.ctrl || m.shift { self.selection.selected.clone() } else { BTreeSet::new() };
                                if !(m.ctrl || m.shift) {
                                    self.emit(ItemSelection { selected: BTreeSet::new(), cursor: self.selection.cursor, anchor: None });
                                }
                                self.marquee = Some(Marquee { start: c, end: c, base, additive: m.ctrl });
                            }
                        }
                        self.press = Some(Press { at: *position, item, deferred_single: deferred, dragging: false, plain: !m.shift && !m.ctrl });
                        ctx.request_paint();
                        EventResult::Captured
                    }
                    MouseButton::Right => {
                        match item {
                            Some(i) if !self.selection.is_selected(i) => self.click_select(i, Modifiers::default()),
                            None if !m.ctrl => {
                                self.emit(ItemSelection { selected: BTreeSet::new(), cursor: self.selection.cursor, anchor: None })
                            }
                            _ => {}
                        }
                        call(&self.on_context_menu, (item, *position));
                        EventResult::Handled
                    }
                    MouseButton::Middle => {
                        call(&self.on_middle_click, item);
                        EventResult::Handled
                    }
                    MouseButton::Back | MouseButton::Forward => {
                        call(&self.on_nav_button, *button);
                        EventResult::Handled
                    }
                    _ => EventResult::Ignored,
                }
            }
            Event::DoubleClick { button: MouseButton::Left, position } => {
                if !self.bounds.contains(*position) || self.in_scrollbar(*position) {
                    return EventResult::Ignored;
                }
                self.press = None;
                self.marquee = None;
                if self.touch_mode && crate::input::is_synthesized_mouse() {
                    // Второй тап подряд — тоже просто тап.
                    if !std::mem::take(&mut self.swallow_tap) {
                        self.touch_tap(self.item_at(*position));
                    }
                    ctx.request_layout();
                    return EventResult::Handled;
                }
                match self.item_at(*position) {
                    // Открыли уже первым щелчком.
                    Some(_) if self.single_click => {}
                    Some(i) => {
                        if !self.selection.is_selected(i) {
                            self.click_select(i, Modifiers::default());
                        }
                        call(&self.on_activate, i);
                    }
                    None => call(&self.on_background_double_click, ()),
                }
                ctx.request_paint();
                EventResult::Handled
            }
            Event::MouseMove(pos) => {
                self.pointer = *pos;
                if let Some(grab) = self.scrollbar_drag {
                    let _ = grab;
                    self.drag_scrollbar(pos.y);
                    ctx.request_layout();
                    return EventResult::Captured;
                }
                let hover = self.in_scrollbar(*pos);
                if hover != self.scrollbar_hover {
                    self.scrollbar_hover = hover;
                    ctx.request_paint();
                }
                if self.marquee.is_some() {
                    let c = self.to_content(*pos);
                    if let Some(mq) = self.marquee.as_mut() {
                        mq.end = c;
                    }
                    self.update_marquee();
                    ctx.request_paint();
                    return EventResult::Captured;
                }
                if let Some(p) = self.press {
                    if !p.dragging
                        && ((pos.x - p.at.x).abs() > DRAG_THRESHOLD || (pos.y - p.at.y).abs() > DRAG_THRESHOLD)
                    {
                        self.press = Some(Press { dragging: true, deferred_single: false, ..p });
                        if let Some(f) = self.drag_start.clone() {
                            if let Some(mut data) = f(&self.selection) {
                                data.source_id = self.id.0;
                                ctx.start_drag(data);
                            }
                        }
                        return EventResult::Handled;
                    }
                    return EventResult::Captured;
                }
                EventResult::Ignored
            }
            Event::MouseUp { button: MouseButton::Left, position } => {
                if self.scrollbar_drag.take().is_some() {
                    ctx.request_paint();
                    return EventResult::Handled;
                }
                let had_marquee = self.marquee.take().is_some();
                if let Some(p) = self.press.take() {
                    if p.deferred_single && !p.dragging {
                        if let Some(i) = p.item {
                            if self.item_at(*position) == Some(i) {
                                self.emit(ItemSelection::single(i));
                            }
                        }
                    }
                    if self.single_click && p.plain && !p.dragging {
                        if let Some(i) = p.item.filter(|&i| self.item_at(*position) == Some(i)) {
                            call(&self.on_activate, i);
                        }
                    }
                    ctx.request_paint();
                    return EventResult::Handled;
                }
                if had_marquee {
                    ctx.request_paint();
                    return EventResult::Handled;
                }
                EventResult::Ignored
            }
            Event::DragEnter { position, data } | Event::DragMove { position, data } => {
                if !self.bounds.contains(*position) {
                    if self.drop_hover {
                        self.drop_hover = false;
                        self.drop_target = None;
                        self.rebuild = true;
                        ctx.request_layout();
                    }
                    return EventResult::Ignored;
                }
                self.pointer = *position;
                match self.resolve_drop(*position, data) {
                    Some(t) => {
                        if !self.drop_hover || self.drop_target != t {
                            self.drop_hover = true;
                            self.drop_target = t;
                            self.rebuild = true;
                            ctx.request_layout();
                        }
                        EventResult::Handled
                    }
                    None => {
                        if self.drop_hover {
                            self.drop_hover = false;
                            self.drop_target = None;
                            self.rebuild = true;
                            ctx.request_layout();
                        }
                        EventResult::Ignored
                    }
                }
            }
            Event::DragLeave | Event::DragEnd { .. } => {
                self.press = None;
                if self.drop_hover {
                    self.drop_hover = false;
                    self.drop_target = None;
                    self.rebuild = true;
                    ctx.request_layout();
                }
                EventResult::Ignored
            }
            Event::Drop { position, data } => {
                self.press = None;
                if !self.bounds.contains(*position) {
                    return EventResult::Ignored;
                }
                let target = self.resolve_drop(*position, data);
                self.drop_hover = false;
                self.drop_target = None;
                self.rebuild = true;
                ctx.request_layout();
                match target {
                    Some(t) => {
                        call(&self.on_drop, ItemDrop { target: t, data: data.clone(), modifiers: m, position: *position });
                        EventResult::Handled
                    }
                    None => EventResult::Ignored,
                }
            }
            Event::TouchStart { id, position } => {
                if self.touch.is_some() || !self.bounds.contains(*position) {
                    return EventResult::Ignored;
                }
                self.swallow_tap = self.fling.abs() > FLING_MIN * 2.0;
                self.fling = 0.0;
                if !self.touch_scrollable() {
                    return EventResult::Ignored;
                }
                self.velocity.reset();
                self.velocity.add(*position);
                self.touch = Some(TouchPan { id: *id, start: *position, last: *position, panning: false });
                EventResult::Handled
            }
            Event::TouchMove { id, position } => {
                let Some(mut t) = self.touch.filter(|t| t.id == *id) else { return EventResult::Ignored };
                self.velocity.add(*position);
                if !t.panning {
                    let (dx, dy) = (position.x - t.start.x, position.y - t.start.y);
                    if dx.abs().max(dy.abs()) < TOUCH_SLOP {
                        return EventResult::Handled;
                    }
                    if dx.abs() > dy.abs() {
                        // Жест вбок — не наш (листание страниц, «назад»).
                        self.touch = None;
                        return EventResult::Ignored;
                    }
                    t.panning = true;
                    self.swallow_tap = false;
                    self.press = None;
                    self.marquee = None;
                }
                let dy = position.y - t.last.y;
                t.last = *position;
                self.touch = Some(t);
                self.set_scroll(self.scroll - dy);
                ctx.request_layout();
                EventResult::Handled
            }
            Event::TouchEnd { id, .. } => {
                let Some(t) = self.touch.filter(|t| t.id == *id) else { return EventResult::Ignored };
                self.touch = None;
                if !t.panning {
                    return EventResult::Ignored;
                }
                let v = -self.velocity.velocity().y;
                if v.abs() > FLING_MIN {
                    self.fling = v;
                    ctx.request_paint();
                }
                EventResult::Handled
            }
            Event::LongPress { position } if self.touch_mode => {
                if self.touch.is_some_and(|t| t.panning) || !self.bounds.contains(*position) {
                    return EventResult::Ignored;
                }
                // На фоне — меню (хост превратит удержание в правую кнопку).
                let Some(i) = self.item_at(*position) else { return EventResult::Ignored };
                let mut s = self.selection.clone();
                s.selected.insert(i);
                s.cursor = Some(i);
                s.anchor = Some(i);
                self.emit(s);
                ctx.request_layout();
                EventResult::Handled
            }
            Event::KeyDown(key) if self.keyboard_active => {
                let handled = match key {
                    Key::Up | Key::Down | Key::Left | Key::Right | Key::Home | Key::End | Key::PageUp | Key::PageDown => {
                        if m.alt {
                            false
                        } else {
                            self.move_cursor(*key, m)
                        }
                    }
                    Key::A if m.ctrl && !m.alt => {
                        let mut s = ItemSelection::all(self.count);
                        s.cursor = self.selection.cursor;
                        self.emit(s);
                        true
                    }
                    Key::Space if m.ctrl => {
                        if let Some(c) = self.selection.cursor {
                            let mut s = self.selection.clone();
                            if !s.selected.remove(&c) {
                                s.selected.insert(c);
                            }
                            self.emit(s);
                        }
                        true
                    }
                    Key::Enter if !m.ctrl && !m.alt => {
                        if let Some(c) = self.selection.cursor.or_else(|| self.selection.selected.iter().next().copied()) {
                            call(&self.on_activate, c);
                            true
                        } else {
                            false
                        }
                    }
                    Key::Escape if !self.selection.selected.is_empty() => {
                        self.emit(ItemSelection { selected: BTreeSet::new(), cursor: self.selection.cursor, anchor: None });
                        true
                    }
                    Key::ContextMenu => {
                        let at = self
                            .selection
                            .cursor
                            .map(|c| {
                                let r = self.item_rect(c);
                                Point::new(
                                    self.bounds.origin.x + self.padding[0] + r.origin.x + 12.0,
                                    self.bounds.origin.y + self.padding[1] + r.origin.y + r.size.height - self.scroll,
                                )
                            })
                            .unwrap_or(self.bounds.origin);
                        let target = self.selection.cursor.filter(|c| self.selection.is_selected(*c));
                        call(&self.on_context_menu, (target, at));
                        true
                    }
                    _ => false,
                };
                if handled {
                    ctx.request_layout();
                    EventResult::Handled
                } else {
                    EventResult::Ignored
                }
            }
            Event::CharInput(c) if self.keyboard_active && !m.ctrl && !m.alt && !m.meta => {
                if *c == ' ' && self.typeahead.is_empty() {
                    return EventResult::Ignored;
                }
                if self.typeahead(*c) {
                    ctx.request_layout();
                    EventResult::Handled
                } else {
                    EventResult::Ignored
                }
            }
            _ => EventResult::Ignored,
        }
    }

    fn animate(&mut self, dt: Duration) -> bool {
        if self.fling != 0.0 {
            let t = dt.as_secs_f32();
            let old = self.scroll;
            self.set_scroll(self.scroll + self.fling * t);
            self.fling *= (-FLING_DECAY * t).exp();
            if self.fling.abs() < FLING_MIN || (t > 0.0 && (self.scroll - old).abs() < 0.01) {
                self.fling = 0.0;
            }
            self.mark_dirty(DirtyFlags::LAYOUT | DirtyFlags::RENDER);
            return true;
        }
        if !self.wants_autoscroll() {
            return false;
        }
        let v = self.autoscroll_speed();
        let old = self.scroll;
        self.set_scroll(self.scroll + v * dt.as_secs_f32());
        if self.marquee.is_some() && (self.scroll - old).abs() > 0.01 {
            let c = self.to_content(self.pointer);
            if let Some(mq) = self.marquee.as_mut() {
                mq.end = c;
            }
            self.update_marquee();
        }
        self.mark_dirty(DirtyFlags::LAYOUT | DirtyFlags::RENDER);
        true
    }

    fn wants_animate_tick(&self) -> bool {
        self.fling != 0.0 || self.wants_autoscroll()
    }

    fn needs_repaint(&self) -> bool {
        self.fling != 0.0 || self.wants_autoscroll()
    }

    fn scroll_offset(&self) -> Point {
        Point::new(0.0, self.scroll)
    }

    fn is_scroll_container(&self) -> bool {
        true
    }

    fn intercepts_child_events(&self) -> bool {
        self.scrollbar_drag.is_some() || self.scrollbar_hover || self.marquee.is_some()
    }

    fn ensure_visible(&mut self, child_rect: Rect) -> bool {
        let vh = self.viewport().height;
        let y = child_rect.origin.y;
        let target = if y + child_rect.size.height > self.scroll + vh {
            y + child_rect.size.height - vh
        } else if y < self.scroll {
            y
        } else {
            return false;
        };
        self.set_scroll(target);
        true
    }

    fn accessibility_info(&self) -> Option<crate::a11y::AccessibilityInfo> {
        Some(crate::a11y::AccessibilityInfo {
            role: crate::a11y::Role::ListBox,
            state: crate::a11y::NodeState::default(),
            properties: crate::a11y::NodeProperties::default(),
        })
    }

    fn children(&self) -> &[ElementId] {
        &[]
    }
    fn bounds(&self) -> Rect {
        self.bounds
    }
    fn set_position(&mut self, pos: Point) {
        self.bounds.origin = pos;
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
    fn id(&self) -> ElementId {
        self.id
    }
    fn set_id(&mut self, id: ElementId) {
        self.id = id;
    }
    fn mount(&mut self, _tree: &mut ElementTree) {}
    fn element_type_name(&self) -> &str {
        "ItemView"
    }
    fn set_classes(&mut self, classes: Vec<String>) {
        self.classes = classes;
        self.mark_dirty(DirtyFlags::RENDER);
    }
    fn get_classes(&self) -> &[String] {
        &self.classes
    }
    fn reset_mss_styles(&mut self) {
        self.mss.reset();
    }
    fn mss(&self) -> Option<&MssFields> {
        Some(&self.mss)
    }
    fn apply_computed_style(&mut self, style: &ComputedStyle) {
        self.mss.apply(style);
        self.padding = [
            self.mss.padding_left.unwrap_or(0.0),
            self.mss.padding_top.unwrap_or(0.0),
            self.mss.padding_right.unwrap_or(0.0),
            self.mss.padding_bottom.unwrap_or(0.0),
        ];
        if let Some(v) = style.get("scrollbar-width").and_then(|v| v.as_px()) {
            self.scrollbar_width = v;
        }
        self.selection_color = self.mss.selection_color_or_default();
        self.mark_dirty(DirtyFlags::LAYOUT | DirtyFlags::RENDER);
    }
}

impl ItemViewElement {
    fn drag_scrollbar(&mut self, y: f32) {
        let Some(grab) = self.scrollbar_drag else { return };
        let t = self.thumb();
        let track = self.bounds.size.height - t.size.height;
        if track <= 0.0 {
            return;
        }
        let rel = (y - grab - self.bounds.origin.y) / track;
        self.set_scroll(rel.clamp(0.0, 1.0) * self.max_scroll());
    }
}

impl StyledElement for ItemViewElement {
    fn apply_style(&mut self, style: &ComputedStyle) {
        self.apply_computed_style(style);
    }
    fn classes(&self) -> &[String] {
        &self.classes
    }
    fn set_classes(&mut self, classes: Vec<String>) {
        self.classes = classes;
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::input::Modifiers;
    use crate::testing::TestHarness;
    use crate::widgets::Text;

    /// Сетка 100×50 без зазора: в окне 400 px — 4 столбца (минус полоса 8 px → 3).
    fn grid(count: usize, sink: Arc<Mutex<Vec<ItemSelection>>>) -> ItemView {
        ItemView::new(count, |i, _| Box::new(Text::new(format!("{i}"))))
            .layout(ItemLayout::Grid { item_width: 100.0, item_height: 50.0, gap: 0.0 })
            .item_label(|i| format!("item{i}"))
            .on_selection_change(move |s| sink.lock().unwrap().push(s))
    }

    fn harness(count: usize) -> (TestHarness, Arc<Mutex<Vec<ItemSelection>>>) {
        let seen = Arc::new(Mutex::new(Vec::new()));
        let mut h = TestHarness::new(Box::new(grid(count, seen.clone())));
        h.layout(308.0, 200.0);
        (h, seen)
    }

    fn last(seen: &Arc<Mutex<Vec<ItemSelection>>>) -> Vec<usize> {
        seen.lock().unwrap().last().map(|s| s.selected.iter().copied().collect()).unwrap_or_default()
    }

    fn press(h: &mut TestHarness, x: f32, y: f32, m: Modifiers) {
        h.tree.modifiers = m;
        h.send_event(&Event::MouseDown { button: MouseButton::Left, position: Point::new(x, y) });
        h.send_event(&Event::MouseUp { button: MouseButton::Left, position: Point::new(x, y) });
    }

    #[test]
    fn click_ctrl_shift() {
        let (mut h, seen) = harness(20);
        press(&mut h, 150.0, 25.0, Modifiers::default()); // 3 столбца: (0,1) → 1
        assert_eq!(last(&seen), vec![1]);
        press(&mut h, 50.0, 75.0, Modifiers { shift: true, ..Default::default() }); // → 3
        assert_eq!(last(&seen), vec![1, 2, 3]);
        press(&mut h, 250.0, 125.0, Modifiers { ctrl: true, ..Default::default() }); // → 8
        assert_eq!(last(&seen), vec![1, 2, 3, 8]);
        press(&mut h, 250.0, 125.0, Modifiers { ctrl: true, ..Default::default() });
        assert_eq!(last(&seen), vec![1, 2, 3]);
    }

    #[test]
    fn click_on_selected_keeps_until_release() {
        let (mut h, seen) = harness(20);
        h.tree.modifiers = Modifiers::default();
        press(&mut h, 50.0, 25.0, Modifiers::default());
        press(&mut h, 250.0, 25.0, Modifiers { shift: true, ..Default::default() });
        assert_eq!(last(&seen), vec![0, 1, 2]);
        h.tree.modifiers = Modifiers::default();
        h.send_event(&Event::MouseDown { button: MouseButton::Left, position: Point::new(150.0, 25.0) });
        assert_eq!(last(&seen), vec![0, 1, 2], "нажатие по выделенному не сбрасывает выделение");
        h.send_event(&Event::MouseUp { button: MouseButton::Left, position: Point::new(150.0, 25.0) });
        assert_eq!(last(&seen), vec![1]);
    }

    #[test]
    fn single_click_opens_plain_click_only() {
        let opened = Arc::new(Mutex::new(Vec::new()));
        let o = opened.clone();
        let view = ItemView::new(20, |i, _| Box::new(Text::new(format!("{i}"))))
            .layout(ItemLayout::Rows { row_height: 50.0 })
            .single_click(true)
            .on_activate(move |i| o.lock().unwrap().push(i));
        let mut h = TestHarness::new(Box::new(view));
        h.layout(308.0, 200.0);
        h.tree.modifiers = Modifiers::default();
        h.send_event(&Event::MouseDown { button: MouseButton::Left, position: Point::new(100.0, 75.0) });
        h.send_event(&Event::MouseUp { button: MouseButton::Left, position: Point::new(100.0, 75.0) });
        assert_eq!(*opened.lock().unwrap(), vec![1], "щелчок открывает");
        h.tree.modifiers = Modifiers { ctrl: true, ..Default::default() };
        h.send_event(&Event::MouseDown { button: MouseButton::Left, position: Point::new(100.0, 125.0) });
        h.send_event(&Event::MouseUp { button: MouseButton::Left, position: Point::new(100.0, 125.0) });
        assert_eq!(*opened.lock().unwrap(), vec![1], "с Ctrl — только выделяет");
    }

    #[test]
    fn marquee_selects_intersecting() {
        let (mut h, seen) = harness(5); // строки: 0 1 2 / 3 4 ; фон — справа от 4
        h.tree.modifiers = Modifiers::default();
        h.send_event(&Event::MouseDown { button: MouseButton::Left, position: Point::new(280.0, 90.0) });
        h.send_event(&Event::MouseMove(Point::new(150.0, 40.0)));
        assert_eq!(last(&seen), vec![1, 2, 4]);
        h.send_event(&Event::MouseMove(Point::new(20.0, 10.0)));
        assert_eq!(last(&seen), vec![0, 1, 2, 3, 4]);
        h.send_event(&Event::MouseUp { button: MouseButton::Left, position: Point::new(20.0, 10.0) });
    }

    fn touch_view(count: usize, sink: Arc<Mutex<Vec<ItemSelection>>>, opened: Arc<Mutex<Vec<usize>>>) -> TestHarness {
        let view = ItemView::new(count, |i, _| Box::new(Text::new(format!("{i}"))))
            .layout(ItemLayout::Rows { row_height: 50.0 })
            .touch_mode(true)
            .on_activate(move |i| opened.lock().unwrap().push(i))
            .on_selection_change(move |s| sink.lock().unwrap().push(s));
        let mut h = TestHarness::new(Box::new(view));
        h.layout(308.0, 200.0);
        h
    }

    #[test]
    fn touch_tap_opens_long_press_selects() {
        let seen = Arc::new(Mutex::new(Vec::new()));
        let opened = Arc::new(Mutex::new(Vec::new()));
        let mut h = touch_view(20, seen.clone(), opened.clone());
        h.touch_down(1, Point::new(100.0, 75.0));
        h.touch_up(1);
        assert_eq!(*opened.lock().unwrap(), vec![1], "тап открывает");
        assert!(seen.lock().unwrap().is_empty(), "и не выделяет");
        h.send_event(&Event::LongPress { position: Point::new(100.0, 125.0) });
        assert_eq!(last(&seen), vec![2], "удержание выделяет");
        // Режим выбора: тап переключает, а не открывает.
        std::thread::sleep(std::time::Duration::from_millis(350));
        h.touch_down(2, Point::new(100.0, 25.0));
        h.touch_up(2);
        assert_eq!(last(&seen), vec![0, 2]);
        std::thread::sleep(std::time::Duration::from_millis(350));
        h.touch_down(3, Point::new(100.0, 125.0));
        h.touch_up(3);
        assert_eq!(last(&seen), vec![0]);
        assert_eq!(*opened.lock().unwrap(), vec![1]);
    }

    #[test]
    fn touch_pan_scrolls_without_tap() {
        let seen = Arc::new(Mutex::new(Vec::new()));
        let opened = Arc::new(Mutex::new(Vec::new()));
        let mut h = touch_view(20, seen.clone(), opened.clone());
        h.touch_down(1, Point::new(100.0, 150.0));
        h.touch_move(1, Point::new(100.0, 120.0));
        h.touch_move(1, Point::new(100.0, 50.0));
        h.touch_up(1);
        assert!(opened.lock().unwrap().is_empty(), "прокрутка — не тап");
        let el = h.tree.get(h.root_id).unwrap();
        assert!((el.scroll_offset().y - 100.0).abs() < 0.5, "содержимое идёт за пальцем: {}", el.scroll_offset().y);
    }

    #[test]
    fn stretch_fills_width() {
        let seen = Arc::new(Mutex::new(Vec::new()));
        let view = grid(20, seen.clone()).stretch(true);
        let mut h = TestHarness::new(Box::new(view));
        h.layout(338.0, 200.0); // 330 px без полосы: 3 столбца по 110
        press(&mut h, 325.0, 25.0, Modifiers::default());
        assert_eq!(last(&seen), vec![2]);
    }

    #[test]
    fn keyboard_grid_navigation_and_typeahead() {
        let (mut h, seen) = harness(8);
        h.tree.modifiers = Modifiers::default();
        h.send_event(&Event::KeyDown(Key::Down));
        assert_eq!(last(&seen), vec![0]);
        h.send_event(&Event::KeyDown(Key::Down));
        assert_eq!(last(&seen), vec![3]);
        h.send_event(&Event::KeyDown(Key::Right));
        assert_eq!(last(&seen), vec![4]);
        h.tree.modifiers = Modifiers { shift: true, ..Default::default() };
        h.send_event(&Event::KeyDown(Key::Down));
        assert_eq!(last(&seen), vec![4, 5, 6, 7], "неполная строка — к последнему");
        h.tree.modifiers = Modifiers::default();
        h.send_event(&Event::CharInput('i'));
        h.send_event(&Event::CharInput('t'));
        h.send_event(&Event::CharInput('e'));
        h.send_event(&Event::CharInput('m'));
        h.send_event(&Event::CharInput('2'));
        assert_eq!(last(&seen), vec![2]);
    }
}
