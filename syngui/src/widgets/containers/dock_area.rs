//! Док-панели в духе IDE: дерево из разделителей ([`SplitView`]) и групп
//! вкладок. Вкладку можно перетащить в другую группу (в центр — добавить к
//! её вкладкам, к краю — разрезать группу и встать рядом) и закрыть.
//!
//! Раскладка живёт в [`DockState`] (сигнал, `Copy`); приложение сохраняет
//! её через [`DockState::spec`] и восстанавливает [`DockState::new`].
//! Содержимое панелей строит замыкание [`DockArea::content`] по id панели.
//!
//! Перестраивание идёт по частям: смена активной вкладки перестраивает только
//! тело своей группы, перенос/закрытие — всё дерево.
//!
//! Классы для MSS (встроенные стили — `styles/dock_area.mss`):
//! `dock-group`, `dock-header`, `dock-tab` (+ `active`), `dock-tab-title`,
//! `dock-tab-close`, `dock-body`, `dock-drop-hint`, `dock-split`, `dock-empty`.

use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::Arc;

use crate::core::Size;
use crate::signal::{use_signal, RwSignal};
use crate::widget::{Text, Widget, WidgetExt};
use crate::widgets::containers::{
    Column, DecoratedBox, Positioned, Reactive, Row, SplitDirection, SplitView, Stack, StackFit,
};
use crate::widgets::overlay::{Draggable, DropArea, DropInfo};

/// Тип переноса вкладки — чужие [`DropArea`] его не принимают.
pub const DOCK_DRAG_TYPE: &str = "syngui-dock-panel";

/// Куда бросили вкладку относительно группы.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum DropZone {
    Center,
    Left,
    Right,
    Top,
    Bottom,
}

impl DropZone {
    /// Зона по точке внутри области: край — четверть размера, иначе центр.
    pub fn from_point(x: f32, y: f32, size: Size) -> Self {
        let (w, h) = (size.width.max(1.0), size.height.max(1.0));
        let (fx, fy) = (x / w, y / h);
        let edges = [(fx, DropZone::Left), (1.0 - fx, DropZone::Right), (fy, DropZone::Top), (1.0 - fy, DropZone::Bottom)];
        let (d, zone) = edges.into_iter().fold((f32::MAX, DropZone::Center), |a, b| if b.0 < a.0 { b } else { a });
        if d < 0.25 {
            zone
        } else {
            DropZone::Center
        }
    }
}

/// Сериализуемое описание раскладки (без сигналов) — для сохранения.
#[derive(Clone, Debug, PartialEq)]
pub enum DockSpec {
    Split { vertical: bool, ratio: f32, first: Box<DockSpec>, second: Box<DockSpec> },
    /// Группа вкладок. `name` непустое — именованная группа: она остаётся
    /// на месте, даже когда в ней не осталось вкладок (область документов),
    /// и в неё можно открыть панель по имени ([`DockState::open_in`]).
    Tabs { panels: Vec<String>, active: usize, name: String },
}

impl DockSpec {
    pub fn tabs<S: Into<String>>(panels: impl IntoIterator<Item = S>) -> Self {
        DockSpec::Tabs { panels: panels.into_iter().map(Into::into).collect(), active: 0, name: String::new() }
    }

    /// Сделать группу именованной (см. [`DockSpec::Tabs`]).
    pub fn named(self, group: impl Into<String>) -> Self {
        match self {
            DockSpec::Tabs { panels, active, .. } => DockSpec::Tabs { panels, active, name: group.into() },
            other => other,
        }
    }

    /// Разрез слева направо.
    pub fn row(first: DockSpec, second: DockSpec, ratio: f32) -> Self {
        DockSpec::Split { vertical: false, ratio, first: Box::new(first), second: Box::new(second) }
    }

    /// Разрез сверху вниз.
    pub fn column(first: DockSpec, second: DockSpec, ratio: f32) -> Self {
        DockSpec::Split { vertical: true, ratio, first: Box::new(first), second: Box::new(second) }
    }
}

static NEXT_GROUP: AtomicU64 = AtomicU64::new(1);

#[derive(Clone)]
enum DockNode {
    Split { dir: SplitDirection, ratio: RwSignal<f32>, first: Box<DockNode>, second: Box<DockNode> },
    Tabs(Group),
}

#[derive(Clone)]
struct Group {
    id: u64,
    name: String,
    panels: Vec<String>,
    active: RwSignal<String>,
    /// Подсказка места вставки во время переноса: зона и размер тела группы.
    hint: RwSignal<Option<(DropZone, Size)>>,
}

impl Group {
    fn new(panels: Vec<String>, active: usize, name: String) -> Self {
        let active = panels.get(active).or(panels.first()).cloned().unwrap_or_default();
        Group { id: NEXT_GROUP.fetch_add(1, Ordering::Relaxed), name, panels, active: use_signal(active), hint: use_signal(None) }
    }
}

impl DockNode {
    fn from_spec(spec: &DockSpec) -> Self {
        match spec {
            DockSpec::Split { vertical, ratio, first, second } => DockNode::Split {
                dir: if *vertical { SplitDirection::Vertical } else { SplitDirection::Horizontal },
                ratio: use_signal(ratio.clamp(0.05, 0.95)),
                first: Box::new(Self::from_spec(first)),
                second: Box::new(Self::from_spec(second)),
            },
            DockSpec::Tabs { panels, active, name } => DockNode::Tabs(Group::new(panels.clone(), *active, name.clone())),
        }
    }

    fn spec(&self) -> DockSpec {
        match self {
            DockNode::Split { dir, ratio, first, second } => DockSpec::Split {
                vertical: *dir == SplitDirection::Vertical,
                ratio: ratio.get_untracked(),
                first: Box::new(first.spec()),
                second: Box::new(second.spec()),
            },
            DockNode::Tabs(g) => {
                let active = g.active.get_untracked();
                DockSpec::Tabs {
                    panels: g.panels.clone(),
                    active: g.panels.iter().position(|p| *p == active).unwrap_or(0),
                    name: g.name.clone(),
                }
            }
        }
    }

    fn groups<'a>(&'a self, out: &mut Vec<&'a Group>) {
        match self {
            DockNode::Split { first, second, .. } => {
                first.groups(out);
                second.groups(out);
            }
            DockNode::Tabs(g) => out.push(g),
        }
    }

    fn group_mut(&mut self, id: u64) -> Option<&mut Group> {
        match self {
            DockNode::Split { first, second, .. } => first.group_mut(id).or_else(|| second.group_mut(id)),
            DockNode::Tabs(g) => (g.id == id).then_some(g),
        }
    }

    /// Убрать панель. Опустевшая группа исчезает, её разделитель
    /// схлопывается в соседа. `true` — пустым стал сам узел.
    fn remove(&mut self, panel: &str) -> bool {
        match self {
            DockNode::Tabs(g) => {
                if let Some(i) = g.panels.iter().position(|p| p == panel) {
                    g.panels.remove(i);
                    if g.active.get_untracked() == panel {
                        let next = g.panels.get(i.min(g.panels.len().saturating_sub(1))).cloned().unwrap_or_default();
                        g.active.set(next);
                    }
                }
                g.panels.is_empty() && g.name.is_empty()
            }
            DockNode::Split { first, second, .. } => {
                let first_empty = first.remove(panel);
                let second_empty = second.remove(panel);
                match (first_empty, second_empty) {
                    (true, true) => true,
                    (true, false) => {
                        *self = (**second).clone();
                        false
                    }
                    (false, true) => {
                        *self = (**first).clone();
                        false
                    }
                    (false, false) => false,
                }
            }
        }
    }

    /// Разрезать группу `id`, поставив рядом новую с панелью `panel`.
    fn split_group(&mut self, id: u64, panel: &str, zone: DropZone) -> bool {
        match self {
            DockNode::Split { first, second, .. } => {
                first.split_group(id, panel, zone) || second.split_group(id, panel, zone)
            }
            DockNode::Tabs(g) if g.id == id => {
                let old = DockNode::Tabs(g.clone());
                let new = DockNode::Tabs(Group::new(vec![panel.to_string()], 0, String::new()));
                let dir = match zone {
                    DropZone::Top | DropZone::Bottom => SplitDirection::Vertical,
                    _ => SplitDirection::Horizontal,
                };
                let (first, second) = match zone {
                    DropZone::Left | DropZone::Top => (new, old),
                    _ => (old, new),
                };
                *self = DockNode::Split { dir, ratio: use_signal(0.5), first: Box::new(first), second: Box::new(second) };
                true
            }
            DockNode::Tabs(_) => false,
        }
    }
}

/// Раскладка док-области. `Copy`: передаётся в обработчики как есть.
#[derive(Clone, Copy)]
pub struct DockState {
    root: RwSignal<Option<DockNode>>,
    focused: RwSignal<String>,
}

impl DockState {
    pub fn new(spec: DockSpec) -> Self {
        DockState { root: use_signal(Some(DockNode::from_spec(&spec))), focused: use_signal(String::new()) }
    }

    /// Последняя активированная панель (клик по вкладке, открытие, перенос).
    /// Пустая строка — ещё ни одной. Закрытая панель здесь не остаётся.
    pub fn focused(&self) -> RwSignal<String> {
        self.focused
    }

    /// Как [`spec`](Self::spec), но с подпиской на изменения структуры (для
    /// эффекта, сохраняющего раскладку). Доли разделителей не отслеживаются.
    pub fn spec_tracked(&self) -> Option<DockSpec> {
        self.root.with(|r| r.as_ref().map(DockNode::spec))
    }

    /// Панели именованной группы (пусто — группы нет или она пуста).
    pub fn group_panels(&self, group: &str) -> Vec<String> {
        self.root.with_untracked(|r| {
            let mut groups = Vec::new();
            if let Some(r) = r {
                r.groups(&mut groups);
            }
            groups.iter().find(|g| g.name == group).map(|g| g.panels.clone()).unwrap_or_default()
        })
    }

    /// Открыть панель в именованной группе (уже открыта — активировать).
    /// Группы нет — как [`open`](Self::open) без соседа.
    pub fn open_in(&self, panel: &str, group: &str) {
        if self.activate(panel) {
            return;
        }
        let mut root = self.root.get_untracked();
        let target = root.as_ref().and_then(|r| {
            let mut groups = Vec::new();
            r.groups(&mut groups);
            groups.iter().find(|g| g.name == group).map(|g| g.id)
        });
        match (target, root.as_mut()) {
            (Some(id), Some(r)) => {
                if let Some(g) = r.group_mut(id) {
                    g.panels.push(panel.to_string());
                    g.active.set(panel.to_string());
                }
                self.focused.set(panel.to_string());
                self.root.set_always(root);
            }
            _ => self.open(panel, None),
        }
    }

    /// Текущая раскладка (для сохранения). `None` — все панели закрыты.
    pub fn spec(&self) -> Option<DockSpec> {
        self.root.with_untracked(|r| r.as_ref().map(DockNode::spec))
    }

    /// Заменить раскладку целиком.
    pub fn set_spec(&self, spec: DockSpec) {
        self.root.set_always(Some(DockNode::from_spec(&spec)));
    }

    /// Все открытые панели в порядке обхода.
    pub fn panels(&self) -> Vec<String> {
        self.root.with_untracked(|r| {
            let mut groups = Vec::new();
            if let Some(r) = r {
                r.groups(&mut groups);
            }
            groups.iter().flat_map(|g| g.panels.iter().cloned()).collect()
        })
    }

    pub fn contains(&self, panel: &str) -> bool {
        self.panels().iter().any(|p| p == panel)
    }

    /// Сделать панель активной в её группе. `false` — панель не открыта.
    pub fn activate(&self, panel: &str) -> bool {
        self.root.with_untracked(|r| {
            let mut groups = Vec::new();
            if let Some(r) = r {
                r.groups(&mut groups);
            }
            match groups.iter().find(|g| g.panels.iter().any(|p| p == panel)) {
                Some(g) => {
                    g.active.set(panel.to_string());
                    self.focused.set(panel.to_string());
                    true
                }
                None => false,
            }
        })
    }

    /// Открыть панель: уже открыта — активировать; иначе добавить в группу,
    /// где есть `near` (или в первую) и активировать.
    pub fn open(&self, panel: &str, near: Option<&str>) {
        if self.activate(panel) {
            return;
        }
        let mut root = self.root.get_untracked();
        match &mut root {
            None => root = Some(DockNode::from_spec(&DockSpec::tabs([panel]))),
            Some(r) => {
                let target = {
                    let mut groups = Vec::new();
                    r.groups(&mut groups);
                    near.and_then(|n| groups.iter().find(|g| g.panels.iter().any(|p| p == n)))
                        .or(groups.first())
                        .map(|g| g.id)
                };
                if let Some(g) = target.and_then(|id| r.group_mut(id)) {
                    g.panels.push(panel.to_string());
                    g.active.set(panel.to_string());
                }
            }
        }
        self.focused.set(panel.to_string());
        self.root.set_always(root);
    }

    /// Закрыть панель.
    pub fn close(&self, panel: &str) {
        let mut root = self.root.get_untracked();
        let home = root.as_ref().and_then(|r| {
            let mut groups = Vec::new();
            r.groups(&mut groups);
            groups.iter().find(|g| g.panels.iter().any(|p| p == panel)).map(|g| g.id)
        });
        if root.as_mut().is_some_and(|r| r.remove(panel)) {
            root = None;
        }
        if self.focused.get_untracked() == panel {
            // фокус — на ту, что стала активной в той же группе, иначе на любую
            let next = root.as_ref().and_then(|r| {
                let mut groups = Vec::new();
                r.groups(&mut groups);
                groups.sort_by_key(|g| Some(g.id) != home);
                groups.iter().map(|g| g.active.get_untracked()).find(|a| !a.is_empty())
            });
            self.focused.set(next.unwrap_or_default());
        }
        self.root.set_always(root);
    }

    /// Перенести панель `panel` к группе, где стоит `near`: в центр — в ту же
    /// группу, к краю — рядом (например, два документа бок о бок).
    pub fn move_beside(&self, panel: &str, near: &str, zone: DropZone) {
        let group = self.root.with_untracked(|r| {
            let mut groups = Vec::new();
            if let Some(r) = r {
                r.groups(&mut groups);
            }
            groups.iter().find(|g| g.panels.iter().any(|p| p == near)).map(|g| g.id)
        });
        if let Some(g) = group {
            self.move_panel(panel, g, zone);
        }
    }

    /// Перенести панель в группу `group` (в зону `zone`).
    fn move_panel(&self, panel: &str, group: u64, zone: DropZone) {
        let Some(mut root) = self.root.get_untracked() else { return };
        {
            let mut groups = Vec::new();
            root.groups(&mut groups);
            let Some(target) = groups.iter().find(|g| g.id == group) else { return };
            let own = target.panels.iter().any(|p| p == panel);
            // в свою же группу: в центр — ничего, к краю единственной вкладкой — тоже
            if own && (zone == DropZone::Center || target.panels.len() == 1) {
                return;
            }
        }
        root.remove(panel);
        if zone == DropZone::Center {
            if let Some(g) = root.group_mut(group) {
                g.panels.push(panel.to_string());
                g.active.set(panel.to_string());
            }
        } else {
            root.split_group(group, panel, zone);
        }
        self.focused.set(panel.to_string());
        self.root.set_always(Some(root));
    }
}

type ContentFn = Arc<dyn Fn(&str) -> Box<dyn Widget> + Send + Sync>;
type TitleFn = Arc<dyn Fn(&str) -> String + Send + Sync>;
type FlagFn = Arc<dyn Fn(&str) -> bool + Send + Sync>;
type CloseFn = Arc<dyn Fn(&str) + Send + Sync>;

/// Построитель док-области; [`DockArea::build`] отдаёт виджет.
pub struct DockArea {
    state: DockState,
    content: ContentFn,
    title: TitleFn,
    closable: FlagFn,
    on_close: Option<CloseFn>,
    empty: Option<Arc<dyn Fn() -> Box<dyn Widget> + Send + Sync>>,
    group_empty: Option<Arc<dyn Fn(&str) -> Box<dyn Widget> + Send + Sync>>,
}

impl DockArea {
    pub fn new(state: DockState) -> Self {
        DockArea {
            state,
            content: Arc::new(|_| Box::new(DecoratedBox::new())),
            title: Arc::new(|p| p.to_string()),
            closable: Arc::new(|_| true),
            on_close: None,
            empty: None,
            group_empty: None,
        }
    }

    /// Содержимое панели по её id.
    pub fn content(mut self, f: impl Fn(&str) -> Box<dyn Widget> + Send + Sync + 'static) -> Self {
        self.content = Arc::new(f);
        self
    }

    /// Заголовок вкладки по id панели (по умолчанию — сам id).
    pub fn title(mut self, f: impl Fn(&str) -> String + Send + Sync + 'static) -> Self {
        self.title = Arc::new(f);
        self
    }

    /// Можно ли закрыть панель (по умолчанию — любую).
    pub fn closable(mut self, f: impl Fn(&str) -> bool + Send + Sync + 'static) -> Self {
        self.closable = Arc::new(f);
        self
    }

    /// Вызывается после закрытия панели крестиком.
    pub fn on_close(mut self, f: impl Fn(&str) + Send + Sync + 'static) -> Self {
        self.on_close = Some(Arc::new(f));
        self
    }

    /// Что показать, когда закрыты все панели.
    pub fn empty(mut self, f: impl Fn() -> Box<dyn Widget> + Send + Sync + 'static) -> Self {
        self.empty = Some(Arc::new(f));
        self
    }

    /// Что показать в пустой именованной группе (по имени группы) —
    /// например, стартовый экран в области документов.
    pub fn group_empty(mut self, f: impl Fn(&str) -> Box<dyn Widget> + Send + Sync + 'static) -> Self {
        self.group_empty = Some(Arc::new(f));
        self
    }

    pub fn build(self) -> Reactive {
        let this = Arc::new(self);
        Reactive::new(move || -> Vec<Box<dyn Widget>> {
            let root = this.state.root.get();
            vec![match root {
                Some(node) => this.node(&node),
                None => match &this.empty {
                    Some(f) => f(),
                    None => Box::new(DecoratedBox::new().class("dock-empty")),
                },
            }]
        })
    }

    fn node(self: &Arc<Self>, node: &DockNode) -> Box<dyn Widget> {
        match node {
            DockNode::Split { dir, ratio, first, second } => Box::new(
                SplitView::new(BoxedChild(self.node(first)), BoxedChild(self.node(second)))
                    .direction(*dir)
                    .ratio_signal(*ratio)
                    .initial_ratio(ratio.get_untracked())
                    .min_size(80.0)
                    .class("dock-split"),
            ),
            DockNode::Tabs(g) => self.group(g.clone()),
        }
    }

    fn group(self: &Arc<Self>, g: Group) -> Box<dyn Widget> {
        let header = {
            let this = self.clone();
            let g = g.clone();
            Reactive::new(move || -> Vec<Box<dyn Widget>> {
                let active = g.active.get();
                let mut row = Row::new().gap(0.0);
                for p in &g.panels {
                    row = row.child(this.tab(p, *p == active, g.active));
                }
                vec![Box::new(row)]
            })
        };
        let body = {
            let this = self.clone();
            let active = g.active;
            let name = g.name.clone();
            Reactive::new(move || -> Vec<Box<dyn Widget>> {
                let panel = active.get();
                if panel.is_empty() {
                    return match &this.group_empty {
                        Some(f) => vec![f(&name)],
                        None => vec![],
                    };
                }
                vec![(this.content)(&panel)]
            })
        };
        let hint = g.hint;
        let overlay = Reactive::new(move || -> Vec<Box<dyn Widget>> {
            let Some((zone, size)) = hint.get() else { return vec![] };
            let (w, h) = (size.width, size.height);
            let (x, y, w, h) = match zone {
                DropZone::Center => (0.0, 0.0, w, h),
                DropZone::Left => (0.0, 0.0, w / 2.0, h),
                DropZone::Right => (w / 2.0, 0.0, w / 2.0, h),
                DropZone::Top => (0.0, 0.0, w, h / 2.0),
                DropZone::Bottom => (0.0, h / 2.0, w, h / 2.0),
            };
            vec![Box::new(Positioned::new(DecoratedBox::new().class("dock-drop-hint")).at(x, y).dimensions(w, h))]
        });

        let state = self.state;
        let gid = g.id;
        let drop = DropArea::new()
            .accept_types(vec![DOCK_DRAG_TYPE.to_string()])
            .placeholder("")
            .on_drag_over(move |info: DropInfo| {
                let zone = DropZone::from_point(info.local_position.x, info.local_position.y, info.size);
                hint.set(Some((zone, info.size)));
            })
            .on_drag_leave(move || hint.set(None))
            .on_drop_positioned(move |info: DropInfo| {
                hint.set(None);
                let zone = DropZone::from_point(info.local_position.x, info.local_position.y, info.size);
                state.move_panel(&info.data.payload, gid, zone);
            })
            .child(Stack::new().fit(StackFit::Expand).child(body).child(overlay));

        // Бросок на заголовок — всегда «в эту группу».
        let header_drop = DropArea::new()
            .accept_types(vec![DOCK_DRAG_TYPE.to_string()])
            .placeholder("")
            .on_drop(move |d| state.move_panel(&d.payload, gid, DropZone::Center))
            .child(DecoratedBox::new().class("dock-header").child(header));

        Box::new(
            DecoratedBox::new().class("dock-group").child(
                Column::new()
                    .gap(0.0)
                    .cross_axis_alignment(crate::layout::CrossAxisAlignment::Stretch)
                    .child(header_drop)
                    .child(DecoratedBox::new().class("dock-body").clip(true).child(drop)),
            ),
        )
    }

    fn tab(self: &Arc<Self>, panel: &str, active: bool, active_sig: RwSignal<String>) -> Box<dyn Widget> {
        let title = (self.title)(panel);
        let mut row = Row::new()
            .gap(6.0)
            .cross_axis_alignment(crate::layout::CrossAxisAlignment::Center)
            .child(Text::new(&title).class("dock-tab-title"));
        if (self.closable)(panel) {
            let state = self.state;
            let on_close = self.on_close.clone();
            let id = panel.to_string();
            row = row.child(
                Draggable::new("syngui-dock-close", "")
                    .threshold(f32::MAX)
                    .on_click(move || {
                        state.close(&id);
                        if let Some(f) = &on_close {
                            f(&id);
                        }
                    })
                    .child(Text::new("×").class("dock-tab-close")),
            );
        }
        let id = panel.to_string();
        let focused = self.state.focused;
        Box::new(
            Draggable::new(DOCK_DRAG_TYPE, panel)
                .label(title)
                .on_click(move || {
                    active_sig.set(id.clone());
                    focused.set(id.clone());
                })
                .child(DecoratedBox::new().class(if active { "dock-tab active" } else { "dock-tab" }).child(row)),
        )
    }
}

/// `SplitView::new` принимает `impl Widget`; обёртка передаёт уже
/// упакованный виджет без лишнего уровня в дереве элементов.
struct BoxedChild(Box<dyn Widget>);

impl Widget for BoxedChild {
    fn create_element(&self) -> Box<dyn crate::widget::Element> {
        self.0.create_element()
    }
    fn can_update(&self, other: &dyn std::any::Any) -> bool {
        self.0.can_update(other)
    }
    fn as_any(&self) -> &dyn std::any::Any {
        self.0.as_any()
    }
    fn as_any_mut(&mut self) -> &mut dyn std::any::Any {
        self.0.as_any_mut()
    }
    fn mount(&self, tree: &mut crate::widget::ElementTree, parent_id: crate::widget::ElementId) {
        self.0.mount(tree, parent_id)
    }
    fn child_widgets(&self) -> Vec<&dyn Widget> {
        self.0.child_widgets()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn layout() -> DockState {
        DockState::new(DockSpec::row(DockSpec::tabs(["a", "b"]), DockSpec::tabs(["c"]), 0.3))
    }

    fn gid(s: &DockState, panel: &str) -> u64 {
        s.root.with_untracked(|r| {
            let mut gs = Vec::new();
            r.as_ref().unwrap().groups(&mut gs);
            gs.iter().find(|g| g.panels.iter().any(|p| p == panel)).unwrap().id
        })
    }

    #[test]
    fn zones() {
        let s = Size::new(400.0, 200.0);
        assert_eq!(DropZone::from_point(200.0, 100.0, s), DropZone::Center);
        assert_eq!(DropZone::from_point(10.0, 100.0, s), DropZone::Left);
        assert_eq!(DropZone::from_point(395.0, 100.0, s), DropZone::Right);
        assert_eq!(DropZone::from_point(200.0, 5.0, s), DropZone::Top);
        assert_eq!(DropZone::from_point(200.0, 195.0, s), DropZone::Bottom);
    }

    #[test]
    fn move_center_and_collapse() {
        let s = layout();
        s.move_panel("c", gid(&s, "a"), DropZone::Center);
        // группа «c» опустела — разделитель схлопнулся
        assert_eq!(s.spec(), Some(DockSpec::Tabs { panels: vec!["a".into(), "b".into(), "c".into()], active: 2, name: String::new() }));
        assert_eq!(s.focused().get_untracked(), "c");
    }

    #[test]
    fn move_to_edge_splits() {
        let s = layout();
        s.move_panel("b", gid(&s, "c"), DropZone::Bottom);
        let Some(DockSpec::Split { second, .. }) = s.spec() else { panic!() };
        let DockSpec::Split { vertical, first, second, .. } = *second else { panic!() };
        assert!(vertical);
        assert_eq!(*first, DockSpec::tabs(["c"]));
        assert_eq!(*second, DockSpec::tabs(["b"]));
        // своя единственная вкладка к своему краю — без изменений
        let before = s.spec();
        s.move_panel("b", gid(&s, "b"), DropZone::Left);
        assert_eq!(s.spec(), before);
    }

    #[test]
    fn open_close() {
        let s = layout();
        s.open("d", Some("c"));
        assert_eq!(s.panels(), ["a", "b", "c", "d"]);
        s.close("a");
        s.close("b");
        assert_eq!(s.spec(), Some(DockSpec::Tabs { panels: vec!["c".into(), "d".into()], active: 1, name: String::new() }));
        s.close("c");
        s.close("d");
        assert_eq!(s.spec(), None);
        s.open("x", None);
        assert_eq!(s.panels(), ["x"]);
    }

    #[test]
    fn move_beside_splits() {
        let s = DockState::new(DockSpec::tabs(["a", "b"]));
        s.move_beside("b", "a", DropZone::Right);
        assert_eq!(s.spec(), Some(DockSpec::row(DockSpec::tabs(["a"]), DockSpec::tabs(["b"]), 0.5)));
    }

    #[test]
    fn named_group_survives_empty() {
        let s = DockState::new(DockSpec::row(DockSpec::tabs(["tree"]), DockSpec::tabs(Vec::<String>::new()).named("docs"), 0.2));
        s.open_in("doc:1", "docs");
        s.open_in("doc:2", "docs");
        assert_eq!(s.group_panels("docs"), ["doc:1", "doc:2"]);
        assert_eq!(s.focused().get_untracked(), "doc:2");
        s.close("doc:2");
        assert_eq!(s.focused().get_untracked(), "doc:1");
        s.close("doc:1");
        assert_eq!(s.focused().get_untracked(), "tree");
        // группа документов на месте, пустая
        assert_eq!(s.group_panels("docs"), Vec::<String>::new());
        assert!(matches!(s.spec(), Some(DockSpec::Split { .. })));
        s.open_in("doc:3", "docs");
        assert_eq!(s.group_panels("docs"), ["doc:3"]);
    }
}
