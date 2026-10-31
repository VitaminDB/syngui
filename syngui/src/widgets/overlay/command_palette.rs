//! Палитра команд (как Ctrl+Shift+P в редакторах кода): поле ввода сверху
//! окна и список команд, отфильтрованный нечётким поиском. ↑/↓ — выбор,
//! Enter — выполнить, Esc или клик мимо — закрыть.
//!
//! ```ignore
//! let open = use_signal(false);
//! CommandPalette::new(open)
//!     .commands(|| vec![PaletteCommand::new("file.open", "Открыть файл").category("Файл").shortcut("Ctrl+O")])
//!     .on_run(|id| run_command(id))
//!     .build()
//! ```
//!
//! Та же палитра — выбор из списка (`choose`): заголовок окна (`title`), значки и
//! пояснения второй строкой у пунктов (`PaletteCommand::icon`, `description`),
//! группы-секции по категории (`grouped`), закрытие без выбора (`on_dismiss`).
//!
//! Классы MSS: `palette`, `palette-title`, `palette-field`, `palette-list`,
//! `palette-item` (+ `selected`, `rich` — с пояснением), `palette-item-title`,
//! `palette-item-desc`, `palette-item-icon-wrap`, `palette-item-icon`,
//! `palette-item-category`, `palette-item-shortcut`, `palette-group`, `palette-empty`.

use std::sync::Arc;

use crate::fuzzy;
use crate::input::{Key, Modifiers};
use crate::KeyReply;
use crate::layout::CrossAxisAlignment;
use crate::signal::{use_signal, RwSignal};
use crate::widget::{Text, Widget, WidgetExt};
use crate::widgets::containers::{Column, DecoratedBox, GestureDetector, Reactive, Row};
use crate::widgets::input::{EventHook, TextField};
use crate::widgets::overlay::{Portal, PortalAnchor};

/// Команда в палитре.
#[derive(Clone, Debug, Default, PartialEq)]
pub struct PaletteCommand {
    pub id: String,
    pub title: String,
    pub category: String,
    pub shortcut: String,
    /// Доп. слова для поиска (синонимы), не показываются.
    pub keywords: String,
    /// Значок слева (символ шрифта значков); пусто — без значка.
    pub icon: String,
    /// Пояснение второй строкой (тоже ищется).
    pub description: String,
}

impl PaletteCommand {
    pub fn new(id: impl Into<String>, title: impl Into<String>) -> Self {
        Self { id: id.into(), title: title.into(), ..Self::default() }
    }

    pub fn category(mut self, c: impl Into<String>) -> Self {
        self.category = c.into();
        self
    }

    pub fn shortcut(mut self, s: impl Into<String>) -> Self {
        self.shortcut = s.into();
        self
    }

    pub fn keywords(mut self, k: impl Into<String>) -> Self {
        self.keywords = k.into();
        self
    }

    pub fn icon(mut self, i: impl Into<String>) -> Self {
        self.icon = i.into();
        self
    }

    pub fn description(mut self, d: impl Into<String>) -> Self {
        self.description = d.into();
        self
    }
}

/// Команда с результатом сопоставления (диапазоны совпадений в заголовке — в символах).
#[derive(Clone, Debug)]
pub struct PaletteHit {
    pub command: PaletteCommand,
    pub ranges: Vec<(usize, usize)>,
}

/// Отфильтровать и отсортировать команды по запросу ([`crate::fuzzy`]: токены,
/// начала слов, смена раскладки). Пустой запрос — все команды по порядку.
pub fn filter_commands(commands: &[PaletteCommand], query: &str) -> Vec<PaletteHit> {
    let Some(plan) = fuzzy::plan(query) else {
        return commands.iter().map(|c| PaletteHit { command: c.clone(), ranges: Vec::new() }).collect();
    };
    let mut scored: Vec<(i32, usize, Vec<(usize, usize)>)> = commands
        .iter()
        .enumerate()
        .filter_map(|(i, c)| {
            let fields = [
                fuzzy::SearchField::new(&c.category, fuzzy::SUBTITLE_BONUS),
                fuzzy::SearchField::new(&c.keywords, 0),
                fuzzy::SearchField::new(&c.description, 0),
                fuzzy::SearchField::new(&c.id, 0),
            ];
            let m = fuzzy::score(&c.title, &fuzzy::normalize(&c.title), &fields, &plan)?;
            Some((m.score, i, m.title_ranges))
        })
        .collect();
    scored.sort_by(|a, b| b.0.cmp(&a.0).then(a.1.cmp(&b.1)));
    scored.into_iter().map(|(_, i, ranges)| PaletteHit { command: commands[i].clone(), ranges }).collect()
}

type CommandsFn = Arc<dyn Fn() -> Vec<PaletteCommand> + Send + Sync>;
type RunFn = Arc<dyn Fn(&str) + Send + Sync>;
type DismissFn = Arc<dyn Fn() + Send + Sync>;

pub struct CommandPalette {
    open: RwSignal<bool>,
    commands: CommandsFn,
    on_run: RunFn,
    on_dismiss: Option<DismissFn>,
    placeholder: String,
    title: String,
    grouped: bool,
    width: f32,
    visible_rows: usize,
    margin_top: f32,
}

impl CommandPalette {
    pub fn new(open: RwSignal<bool>) -> Self {
        Self {
            open,
            commands: Arc::new(Vec::new),
            on_run: Arc::new(|_| {}),
            on_dismiss: None,
            placeholder: "Введите команду…".into(),
            title: String::new(),
            grouped: false,
            width: 600.0,
            visible_rows: 12,
            margin_top: 64.0,
        }
    }

    /// Закрыта без выбора (Esc, клик мимо).
    pub fn on_dismiss(mut self, f: impl Fn() + Send + Sync + 'static) -> Self {
        self.on_dismiss = Some(Arc::new(f));
        self
    }

    /// Заголовок над полем ввода (выбор из списка: «Экспорт декомпиляции»).
    pub fn title(mut self, t: impl Into<String>) -> Self {
        self.title = t.into();
        self
    }

    /// Категории — секциями с подзаголовком (пока запрос пуст), а не приставкой
    /// «Категория:» у каждого пункта.
    pub fn grouped(mut self, on: bool) -> Self {
        self.grouped = on;
        self
    }

    /// Отступ окна от верхнего края.
    pub fn margin_top(mut self, px: f32) -> Self {
        self.margin_top = px;
        self
    }

    /// Список команд; зовётся при каждом открытии (может зависеть от состояния).
    pub fn commands(mut self, f: impl Fn() -> Vec<PaletteCommand> + Send + Sync + 'static) -> Self {
        self.commands = Arc::new(f);
        self
    }

    /// Выполнить команду по id (палитра уже закрыта).
    pub fn on_run(mut self, f: impl Fn(&str) + Send + Sync + 'static) -> Self {
        self.on_run = Arc::new(f);
        self
    }

    pub fn placeholder(mut self, p: impl Into<String>) -> Self {
        self.placeholder = p.into();
        self
    }

    pub fn width(mut self, w: f32) -> Self {
        self.width = w;
        self
    }

    pub fn visible_rows(mut self, n: usize) -> Self {
        self.visible_rows = n.max(1);
        self
    }

    pub fn build(self) -> Portal {
        let open = self.open;
        let query = use_signal(String::new());
        let selected = use_signal(0usize);
        let snapshot: RwSignal<Vec<PaletteCommand>> = use_signal(Vec::new());
        let commands = self.commands.clone();
        let on_run = self.on_run.clone();
        let rows = self.visible_rows;
        let placeholder = self.placeholder.clone();
        let grouped = self.grouped;
        let on_dismiss = self.on_dismiss.clone();
        let dismiss = Arc::new(move || {
            if open.get_untracked() {
                open.set(false);
                if let Some(f) = &on_dismiss {
                    f();
                }
            }
        });

        let run = {
            let on_run = on_run.clone();
            Arc::new(move |id: String| {
                open.set(false);
                on_run(&id);
            })
        };
        let current = move || -> Vec<PaletteHit> { filter_commands(&snapshot.get_untracked(), &query.get_untracked()) };

        let field = {
            let run = run.clone();
            let dismiss = dismiss.clone();
            move || {
                let dismiss = dismiss.clone();
                // пересоздаётся при каждом открытии: пустое поле в фокусе, свежий список
                if open.get() {
                    snapshot.set_always(commands());
                    query.set(String::new());
                    selected.set(0);
                }
                let run = run.clone();
                TextField::new()
                    .placeholder(placeholder.clone())
                    .autofocus(true)
                    .on_change(move |q: &str| {
                        query.set(q.to_string());
                        selected.set(0);
                    })
                    .on_submit(move |_| {
                        if let Some(h) = current().get(selected.get_untracked()) {
                            run(h.command.id.clone());
                        }
                    })
                    .on_escape(move || dismiss())
                    .class("palette-field")
            }
        };

        let list = {
            let run = run.clone();
            Reactive::new(move || -> Vec<Box<dyn Widget>> {
                let items = filter_commands(&snapshot.get(), &query.get());
                let sel = selected.get().min(items.len().saturating_sub(1));
                if items.is_empty() {
                    return vec![Box::new(Text::new("Ничего не найдено").class("palette-empty"))];
                }
                // окно из `rows` строк, в котором виден выбранный пункт
                let start = sel.saturating_sub(rows - 1).min(items.len().saturating_sub(rows));
                // секции — только без запроса (с запросом порядок по совпадению)
                let sections = grouped && query.get().trim().is_empty();
                let mut col = Column::new().gap(0.0).cross_axis_alignment(CrossAxisAlignment::Stretch);
                let mut last_group: Option<String> = None;
                for (i, hit) in items.iter().enumerate().skip(start).take(rows) {
                    let c = &hit.command;
                    if sections && !c.category.is_empty() && last_group.as_deref() != Some(c.category.as_str()) {
                        col = col.child(Text::new(c.category.clone()).class("palette-group"));
                    }
                    last_group = Some(c.category.clone());
                    let mut row = Row::new().gap(10.0).cross_axis_alignment(CrossAxisAlignment::Center);
                    if !c.icon.is_empty() {
                        row = row.child(DecoratedBox::new().class("palette-item-icon-wrap").child(crate::widgets::Icon::new(c.icon.clone()).class("palette-item-icon")));
                    }
                    if !c.category.is_empty() && !grouped {
                        row = row.child(Text::new(format!("{}:", c.category)).class("palette-item-category"));
                    }
                    let mut title = Row::new().gap(0.0);
                    for (seg, matched) in fuzzy::split_highlight(&c.title, &hit.ranges) {
                        title = title.child(Text::new(seg).max_lines(1).class(if matched { "palette-item-title match" } else { "palette-item-title" }));
                    }
                    let body: Box<dyn Widget> = if c.description.is_empty() {
                        Box::new(title)
                    } else {
                        Box::new(
                            Column::new()
                                .gap(2.0)
                                .cross_axis_alignment(CrossAxisAlignment::Stretch)
                                .child(title)
                                .child(Text::new(c.description.clone()).max_lines(2).class("palette-item-desc")),
                        )
                    };
                    row = row.child(DecoratedBox::new().class("palette-item-title-wrap").child(body));
                    if !c.shortcut.is_empty() {
                        row = row.child(Text::new(&c.shortcut).class("palette-item-shortcut"));
                    }
                    let id = c.id.clone();
                    let run = run.clone();
                    let rich = !c.description.is_empty() || !c.icon.is_empty() && grouped;
                    let class = match (i == sel, rich) {
                        (true, true) => "palette-item rich selected",
                        (true, false) => "palette-item selected",
                        (false, true) => "palette-item rich",
                        (false, false) => "palette-item",
                    };
                    col = col.child(GestureDetector::new().on_click(move || run(id.clone())).child(DecoratedBox::new().class(class).child(row)));
                }
                vec![Box::new(col)]
            })
        };

        let keys = EventHook::new()
            .on_key_down(move |key: Key, _mods: Modifiers| {
                let n = current().len();
                match key {
                    Key::Down if n > 0 => selected.set((selected.get_untracked() + 1).min(n - 1)),
                    Key::Up => selected.set(selected.get_untracked().saturating_sub(1)),
                    Key::PageDown if n > 0 => selected.set((selected.get_untracked() + rows).min(n - 1)),
                    Key::PageUp => selected.set(selected.get_untracked().saturating_sub(rows)),
                    _ => return KeyReply::Ignore,
                }
                KeyReply::Handled
            })
            .child(
                DecoratedBox::new().class("palette").child({
                    let mut c = Column::new().gap(6.0).cross_axis_alignment(CrossAxisAlignment::Stretch);
                    if !self.title.is_empty() {
                        c = c.child(Text::new(self.title.clone()).class("palette-title"));
                    }
                    c.child(field).child(DecoratedBox::new().class("palette-list").child(list))
                }),
            );

        Portal::new()
            .is_open(open)
            .modal(true)
            .backdrop(true)
            .backdrop_color(crate::core::Color::new(0.0, 0.0, 0.0, 0.25))
            .anchor(PortalAnchor::TopCenter { margin_top: self.margin_top })
            .close_on_outside_click(true)
            .width(self.width)
            .on_close(move || dismiss())
            .child(keys)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn filter_order() {
        let cmds = vec![
            PaletteCommand::new("a", "Перейти к адресу").category("Навигация"),
            PaletteCommand::new("b", "Открыть файл").category("Файл"),
            PaletteCommand::new("c", "Закрыть").category("Файл").keywords("выйти"),
        ];
        assert_eq!(filter_commands(&cmds, "").len(), 3);
        assert_eq!(filter_commands(&cmds, "откр")[0].command.id, "b");
        assert_eq!(filter_commands(&cmds, "файл").len(), 2);
        assert_eq!(filter_commands(&cmds, "выйти")[0].command.id, "c");
        // не та раскладка
        assert_eq!(filter_commands(&cmds, "jnrh")[0].command.id, "b");
        assert_eq!(filter_commands(&cmds, "откр")[0].ranges, vec![(0, 4)]);
        // поиск по пояснению
        let cmds = vec![
            PaletteCommand::new("x", "Один файл").description("для чтения: .c и .h"),
            PaletteCommand::new("y", "Проект исходников").description("модули, заголовки, Makefile"),
        ];
        assert_eq!(filter_commands(&cmds, "makefile")[0].command.id, "y");
    }
}
