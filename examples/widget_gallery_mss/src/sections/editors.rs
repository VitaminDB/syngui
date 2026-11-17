//! Раздел «Редакторы»: редактор кода с подсветкой, палитра команд и
//! блочный WYSIWYG-редактор документов.
//!
//! Цвета всех трёх виджетов — только из переменных темы (styles/pages/editors.mss):
//! палитра токенов CodeEditor (`token-*`), переменные `--doc-*` DocumentEditor и
//! классы `palette-*` палитры команд.

use syngui::prelude::*;
use syngui::signal::use_signal;
use syngui::widgets::*;

use super::{label, section_card, section_title};

pub fn build_editors_section() -> impl Widget {
    let palette_open = use_signal(false);
    // Ctrl+K открывает палитру, пока фокус внутри раздела (EventHook ловит
    // всплывающие необработанные клавиши).
    EventHook::new()
        .on_key_down(move |key, mods| {
            if key == Key::K && mods.ctrl {
                palette_open.set(true);
                KeyReply::Handled
            } else {
                KeyReply::Ignore
            }
        })
        .child(
            Column::new()
                .gap(20.0)
                .cross_axis_alignment(CrossAxisAlignment::Stretch)
                .child(code_editor_card())
                .child(command_palette_card(palette_open))
                .child(document_editor_card()),
        )
}

// ─── CodeEditor ─────────────────────────────────────────────────────────────

const RUST_SAMPLE: &str = r#"use std::collections::HashMap;

/// Частотный словарь: сколько раз встречается каждое слово.
#[derive(Debug, Default)]
pub struct WordCount {
    counts: HashMap<String, usize>,
}

impl WordCount {
    pub fn feed(&mut self, text: &str) {
        for word in text.split_whitespace() {
            let w = word.trim_matches(|c: char| !c.is_alphanumeric()).to_lowercase();
            if !w.is_empty() {
                *self.counts.entry(w).or_insert(0) += 1;
            }
        }
    }

    /// Самые частые слова, по убыванию.
    pub fn top(&self, n: usize) -> Vec<(&str, usize)> {
        let mut v: Vec<_> = self.counts.iter().map(|(k, &c)| (k.as_str(), c)).collect();
        v.sort_by(|a, b| b.1.cmp(&a.1).then(a.0.cmp(b.0)));
        v.truncate(n);
        v
    }
}

fn main() {
    let mut wc = WordCount::default();
    wc.feed("Мама мыла раму, а рама — маму. Мама!");
    for (word, n) in wc.top(3) {
        println!("{word:>8} × {n}");
    }
}
"#;

#[cfg(not(target_arch = "wasm32"))]
fn code_editor_card() -> impl Widget {
    use syngui::widgets::input::code_editor::syntax::Language;

    let cursor = use_signal((0usize, 0usize, RUST_SAMPLE.lines().count()));
    let chars = use_signal(RUST_SAMPLE.chars().count());

    section_card(
        Column::new()
            .gap(12.0)
            .cross_axis_alignment(CrossAxisAlignment::Stretch)
            .child(section_title("CodeEditor"))
            .child(label(
                "Редактор кода: подсветка синтаксиса (Rust), номера строк, отмена/повтор, \
                 поиск Ctrl+F, парные скобки. Палитра токенов — из темы (token-* в MSS).",
            ))
            .child(
                DecoratedBox::new().class("ed-code-host").child(
                    CodeEditor::new()
                        .text(RUST_SAMPLE)
                        .language(Language::Rust)
                        .show_line_numbers(true)
                        .tab_width(4)
                        .on_cursor(move |c| cursor.set((c.line, c.col, c.total_lines)))
                        .on_change(move |ch| chars.set(ch.full_text.chars().count()))
                        .class("ed-code"),
                ),
            )
            .child(
                Row::new()
                    .gap(16.0)
                    .class("ed-status")
                    .child(super::rtext("ed-status-text", move || {
                        let (l, c, _) = cursor.get();
                        format!("Стр. {}, стлб. {}", l + 1, c + 1)
                    }))
                    .child(super::rtext("ed-status-text", move || format!("{} строк", cursor.get().2)))
                    .child(super::rtext("ed-status-text", move || format!("{} символов", chars.get())))
                    .child(Text::new("Rust · UTF-8 · пробелы: 4").class("ed-status-text")),
            ),
    )
}

#[cfg(target_arch = "wasm32")]
fn code_editor_card() -> impl Widget {
    section_card(
        Column::new()
            .gap(12.0)
            .child(section_title("CodeEditor"))
            .child(label("CodeEditor в веб-сборке не подключён (feature code-editor только на desktop).")),
    )
}

// ─── CommandPalette ─────────────────────────────────────────────────────────

fn palette_commands() -> Vec<PaletteCommand> {
    vec![
        PaletteCommand::new("file.new", "Новый файл").category("Файл").shortcut("Ctrl+N").icon("\u{E24D}"),
        PaletteCommand::new("file.open", "Открыть файл…").category("Файл").shortcut("Ctrl+O").icon("\u{E2C8}"),
        PaletteCommand::new("file.save", "Сохранить").category("Файл").shortcut("Ctrl+S").icon("\u{E161}"),
        PaletteCommand::new("edit.find", "Найти в файле").category("Правка").shortcut("Ctrl+F").icon("\u{E8B6}").keywords("search поиск"),
        PaletteCommand::new("edit.replace", "Заменить").category("Правка").shortcut("Ctrl+H").icon("\u{E14D}"),
        PaletteCommand::new("edit.format", "Форматировать документ").category("Правка").shortcut("Shift+Alt+F").icon("\u{E23C}").keywords("rustfmt prettier"),
        PaletteCommand::new("view.theme", "Сменить тему").category("Вид").shortcut("Ctrl+T").icon("\u{E40A}").keywords("dark light тёмная светлая"),
        PaletteCommand::new("view.zoom_in", "Увеличить масштаб").category("Вид").shortcut("Ctrl++").icon("\u{E8FF}"),
        PaletteCommand::new("view.sidebar", "Показать/скрыть боковую панель").category("Вид").shortcut("Ctrl+B").icon("\u{E9BD}"),
        PaletteCommand::new("git.commit", "Закоммитить").category("Git").icon("\u{E86F}").keywords("vcs"),
        PaletteCommand::new("git.push", "Отправить").category("Git").icon("\u{E2C6}"),
        PaletteCommand::new("help.about", "О программе").category("Справка").icon("\u{E88E}"),
    ]
}

fn command_palette_card(open: RwSignal<bool>) -> impl Widget {
    let last = use_signal(String::from("—"));
    let count = use_signal(0usize);
    let open_choose = use_signal(false);
    let chosen = use_signal(String::from("—"));

    section_card(
        Column::new()
            .gap(12.0)
            .cross_axis_alignment(CrossAxisAlignment::Stretch)
            .child(section_title("CommandPalette"))
            .child(label(
                "Палитра команд: нечёткий поиск по названию и ключевым словам (попробуйте «тема» или «git»), \
                 ↑/↓ — выбор, Enter — выполнить, Esc — закрыть. Открывается кнопкой или Ctrl+K, \
                 когда фокус в этом разделе.",
            ))
            .child(
                Row::new()
                    .gap(12.0)
                    .cross_axis_alignment(CrossAxisAlignment::Center)
                    .child(Button::new("Открыть палитру  ·  Ctrl+K").class("primary").on_click(move || open.set(true)))
                    .child(Button::new("Выбор из списка").on_click(move || open_choose.set(true)).class("secondary"))
                    .child(super::rtext("ed-result", move || format!("Выполнено: {}  (всего {})", last.get(), count.get())))
                    .child(super::rtext("ed-result", move || format!("Выбрано: {}", chosen.get()))),
            )
            .child(
                CommandPalette::new(open)
                    .commands(palette_commands)
                    .placeholder("Введите команду…")
                    .on_run(move |id| {
                        let title = palette_commands()
                            .into_iter()
                            .find(|c| c.id == id)
                            .map(|c| c.title)
                            .unwrap_or_else(|| id.to_string());
                        last.set(title);
                        count.update(|n| *n += 1);
                    })
                    .build(),
            )
            .child(
                CommandPalette::new(open_choose)
                    .title("Экспорт документа")
                    .grouped(true)
                    .width(480.0)
                    .commands(|| {
                        vec![
                            PaletteCommand::new("md", "Markdown").category("Текст").icon("\u{E873}").description("Исходный текст с разметкой, .md"),
                            PaletteCommand::new("txt", "Простой текст").category("Текст").icon("\u{E262}").description("Без форматирования, .txt"),
                            PaletteCommand::new("html", "HTML").category("Веб").icon("\u{E86F}").description("Страница со стилями, .html"),
                            PaletteCommand::new("pdf", "PDF").category("Печать").icon("\u{E415}").description("Готово к печати, A4"),
                        ]
                    })
                    .on_run(move |id| chosen.set(id.to_uppercase()))
                    .build(),
            ),
    )
}

// ─── DocumentEditor ─────────────────────────────────────────────────────────

const DOC_SAMPLE: &str = "\
# План релиза 0.9 {icon=🚀}

Блочный редактор в духе Notion: **жирный**, *курсив*, `код`, [[wiki-ссылка]]. \
Наберите `/` в начале пустой строки — откроется меню блоков.

- [x] Собрать changelog
- [x] Обновить скриншоты галереи
- [ ] Опубликовать пакет
  - Arch (AUR)
  - Android (APK)

> [!tip] Подсказка
>
> Блоки перетаскиваются за ручку слева, Tab/Shift+Tab меняют вложенность.

```rust
fn main() {
    println!(\"Привет, syngui!\");
}
```

| Платформа | Статус |
| --- | --- |
| Linux | готово |
| Android | тестируется |

![Голосовая заметка](blob:release-notes.ogg)

![[Доска планирования]]

![[shape:rect]]{sw=2}

![[shape:arrow]]{sw=2 x1=0 y1=40 x2=240 y2=0}

---

Конец документа.
";

#[cfg(not(target_arch = "wasm32"))]
fn document_editor_card() -> impl Widget {
    use syngui::widgets::input::document_editor::{DocumentEditor, DocumentEditorHandle};

    let handle = DocumentEditorHandle::new();
    let revision = handle.revision();
    let md_len = use_signal(DOC_SAMPLE.chars().count());
    let h2 = handle.clone();

    section_card(
        Column::new()
            .gap(12.0)
            .cross_axis_alignment(CrossAxisAlignment::Stretch)
            .child(section_title("DocumentEditor"))
            .child(label(
                "WYSIWYG-редактор блочных документов поверх Markdown: заголовки, задачи, callout, \
                 код, таблицы, медиа-карточки (MediaCard), врезки (EmbedCard) и фигуры (ShapeView). \
                 Документ сохраняется обратно в Markdown без потерь.",
            ))
            .child(
                DecoratedBox::new().class("ed-doc-host").child(
                    DocumentEditor::new()
                        .markdown(DOC_SAMPLE)
                        .handle(&handle)
                        .placeholder("Пишите или нажмите «/» для команд…")
                        .on_change(move || md_len.set(h2.serialize().chars().count()))
                        .class("gallery-doc"),
                ),
            )
            .child(
                Row::new()
                    .gap(16.0)
                    .class("ed-status")
                    .child(super::rtext("ed-status-text", move || format!("Ревизия {}", revision.get())))
                    .child(super::rtext("ed-status-text", move || format!("Markdown: {} символов", md_len.get()))),
            ),
    )
}

#[cfg(target_arch = "wasm32")]
fn document_editor_card() -> impl Widget {
    let _ = DOC_SAMPLE;
    section_card(
        Column::new()
            .gap(12.0)
            .child(section_title("DocumentEditor"))
            .child(label("DocumentEditor в веб-сборке не подключён (feature document-editor только на desktop).")),
    )
}
