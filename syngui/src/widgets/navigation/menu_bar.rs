//! Строка меню приложения («Файл  Правка  Вид …»): клик по заголовку
//! раскрывает [`PopupMenu`] под ним; пока меню открыто, наведение на другой
//! заголовок переключает меню, щелчок по открытому заголовку — закрывает. Пункты строятся замыканием при каждом
//! открытии, поэтому их доступность может зависеть от состояния.
//!
//! Классы MSS: `menubar`, `menubar-item` (+ `open`), `menubar-title`.

use std::sync::Arc;

use crate::core::Rect;
use crate::layout::CrossAxisAlignment;
use crate::signal::{use_signal, RwSignal};
use crate::widget::{Text, Widget, WidgetExt};
use crate::core::sync::Mutex;
use crate::widgets::containers::{DecoratedBox, GestureDetector, Reactive, Row};
use crate::widgets::input::EventHook;
use crate::widgets::overlay::{MenuItem, PopupAnchor, PopupMenu};

type ItemsFn = Arc<dyn Fn() -> Vec<MenuItem> + Send + Sync>;

pub struct MenuBar {
    menus: Vec<(String, ItemsFn)>,
    on_select: Arc<dyn Fn(&str) + Send + Sync>,
}

impl MenuBar {
    pub fn new() -> Self {
        Self { menus: Vec::new(), on_select: Arc::new(|_| {}) }
    }

    /// Меню с заголовком `title`; пункты — из `items` при каждом открытии.
    pub fn menu(mut self, title: impl Into<String>, items: impl Fn() -> Vec<MenuItem> + Send + Sync + 'static) -> Self {
        self.menus.push((title.into(), Arc::new(items)));
        self
    }

    /// Выбран пункт (id пункта).
    pub fn on_select(mut self, f: impl Fn(&str) + Send + Sync + 'static) -> Self {
        self.on_select = Arc::new(f);
        self
    }

    pub fn build(self) -> impl Widget {
        let current: RwSignal<Option<usize>> = use_signal(None);
        let is_open = use_signal(false);
        let anchor = use_signal(Rect::zero());
        let menus = Arc::new(self.menus);
        let on_select = self.on_select;
        // границы всей строки — прозрачная зона открытого меню
        let bar: Arc<Mutex<Rect>> = Arc::new(Mutex::new(Rect::zero()));

        let mut row = Row::new().gap(0.0).cross_axis_alignment(CrossAxisAlignment::Center);
        for (i, (title, _)) in menus.iter().enumerate() {
            let title = title.clone();
            // свои границы заголовка — якорь при переключении наведением
            let own: Arc<Mutex<Rect>> = Arc::new(Mutex::new(Rect::zero()));
            let own_hover = own.clone();
            row = row.child(
                EventHook::new().report_bounds(own).child(
                    GestureDetector::new()
                        .on_click_with_bounds(move |_, rect| {
                            // щелчок по заголовку открытого меню закрывает его
                            if is_open.get_untracked() && current.get_untracked() == Some(i) {
                                is_open.set(false);
                                return;
                            }
                            anchor.set(rect);
                            current.set(Some(i));
                            is_open.set(true);
                        })
                        // меню уже открыто — наведение на другой заголовок переключает
                        .on_hover_change(move |inside| {
                            if inside && is_open.get_untracked() && current.get_untracked() != Some(i) {
                                if let Ok(r) = own_hover.lock() {
                                    anchor.set(*r);
                                }
                                current.set(Some(i));
                            }
                        })
                        .child(move || {
                            let open = is_open.get() && current.get() == Some(i);
                            DecoratedBox::new()
                                .class(if open { "menubar-item open" } else { "menubar-item" })
                                .child(Text::new(&title).class("menubar-title"))
                        }),
                ),
            );
        }
        let bar_menu = bar.clone();
        let menu = Reactive::new(move || -> Vec<Box<dyn Widget>> {
            let Some(i) = current.get() else { return vec![] };
            let items = (menus[i].1)();
            let on_select = on_select.clone();
            vec![Box::new(
                PopupMenu::new()
                    .items(items)
                    .is_open(is_open)
                    .anchor(PopupAnchor::BottomStart)
                    .anchor_rect(anchor)
                    .min_width(220.0)
                    .pass_through(bar_menu.clone())
                    .on_select(move |id| on_select(id)),
            )]
        });
        EventHook::new().report_bounds(bar).child(row.child(menu).class("menubar"))
    }
}

impl Default for MenuBar {
    fn default() -> Self {
        Self::new()
    }
}
