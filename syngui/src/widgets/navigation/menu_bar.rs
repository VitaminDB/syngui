//! Строка меню приложения («Файл  Правка  Вид …»): клик по заголовку
//! раскрывает [`PopupMenu`] под ним. Пункты строятся замыканием при каждом
//! открытии, поэтому их доступность может зависеть от состояния.
//!
//! Классы MSS: `menubar`, `menubar-item` (+ `open`), `menubar-title`.

use std::sync::Arc;

use crate::core::Rect;
use crate::layout::CrossAxisAlignment;
use crate::signal::{use_signal, RwSignal};
use crate::widget::{Text, Widget, WidgetExt};
use crate::widgets::containers::{DecoratedBox, GestureDetector, Reactive, Row};
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

        let mut row = Row::new().gap(0.0).cross_axis_alignment(CrossAxisAlignment::Center);
        for (i, (title, _)) in menus.iter().enumerate() {
            let title = title.clone();
            row = row.child(
                GestureDetector::new()
                    .on_click_with_bounds(move |_, rect| {
                        anchor.set(rect);
                        current.set(Some(i));
                        is_open.set(true);
                    })
                    .child(move || {
                        let open = is_open.get() && current.get() == Some(i);
                        DecoratedBox::new()
                            .class(if open { "menubar-item open" } else { "menubar-item" })
                            .child(Text::new(&title).class("menubar-title"))
                    }),
            );
        }
        row.child(Reactive::new(move || -> Vec<Box<dyn Widget>> {
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
                    .on_select(move |id| on_select(id)),
            )]
        }))
        .class("menubar")
    }
}

impl Default for MenuBar {
    fn default() -> Self {
        Self::new()
    }
}
