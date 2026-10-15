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
//! Классы MSS: `palette`, `palette-field`, `palette-list`, `palette-item`
//! (+ `selected`), `palette-item-title`, `palette-item-category`,
//! `palette-item-shortcut`, `palette-empty`.

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

pub struct CommandPalette {
    open: RwSignal<bool>,
    commands: CommandsFn,
    on_run: RunFn,
    placeholder: String,
    width: f32,
    visible_rows: usize,
}

impl CommandPalette {
    pub fn new(open: RwSignal<bool>) -> Self {
        Self {
            open,
            commands: Arc::new(Vec::new),
            on_run: Arc::new(|_| {}),
            placeholder: "Введите команду…".into(),
            width: 600.0,
            visible_rows: 12,
        }
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
            move || {
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
                    .on_escape(move || open.set(false))
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
                let mut col = Column::new().gap(0.0).cross_axis_alignment(CrossAxisAlignment::Stretch);
                for (i, hit) in items.iter().enumerate().skip(start).take(rows) {
                    let c = &hit.command;
                    let mut row = Row::new().gap(8.0).cross_axis_alignment(CrossAxisAlignment::Center);
                    if !c.category.is_empty() {
                        row = row.child(Text::new(format!("{}:", c.category)).class("palette-item-category"));
                    }
                    let mut title = Row::new().gap(0.0);
                    for (seg, matched) in fuzzy::split_highlight(&c.title, &hit.ranges) {
                        title = title.child(Text::new(seg).class(if matched { "palette-item-title match" } else { "palette-item-title" }));
                    }
                    row = row.child(DecoratedBox::new().class("palette-item-title-wrap").child(title));
                    if !c.shortcut.is_empty() {
                        row = row.child(Text::new(&c.shortcut).class("palette-item-shortcut"));
                    }
                    let id = c.id.clone();
                    let run = run.clone();
                    col = col.child(
                        GestureDetector::new()
                            .on_click(move || run(id.clone()))
                            .child(DecoratedBox::new().class(if i == sel { "palette-item selected" } else { "palette-item" }).child(row)),
                    );
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
                DecoratedBox::new().class("palette").child(
                    Column::new()
                        .gap(6.0)
                        .cross_axis_alignment(CrossAxisAlignment::Stretch)
                        .child(field)
                        .child(DecoratedBox::new().class("palette-list").child(list)),
                ),
            );

        Portal::new()
            .is_open(open)
            .modal(true)
            .backdrop(true)
            .backdrop_color(crate::core::Color::new(0.0, 0.0, 0.0, 0.25))
            .anchor(PortalAnchor::TopCenter { margin_top: 64.0 })
            .close_on_outside_click(true)
            .width(self.width)
            .on_close(move || open.set(false))
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
    }
}
