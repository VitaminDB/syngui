use syngui::prelude::*;
use syngui::signal::use_signal;
use syngui::widgets::*;
use super::{section_card, section_title, label};

const DEMO_MARKDOWN: &str = r#"# SYNGUI Markdown Viewer

This is a **MarkdownView** widget that renders CommonMark markdown as formatted text using the existing rendering pipeline.

Visit https://github.com/anthropics/claude-code for autolinks support.

---

## Text Formatting

You can use **bold text**, *italic text*, ~~strikethrough~~, and `inline code` within paragraphs. You can also combine **bold and *italic*** together.

Footnotes work too[^1] — see definitions at the bottom.

## Headers

### Third Level (H3)

#### Fourth Level (H4)

## Code Blocks (with syntax highlighting)

```rust
use syngui::prelude::*;
use syngui::widgets::*;

fn main() {
    AppBuilder::new()
        .title("Hello SYNGUI")
        .size(800, 600)
        .run(|_ctx| Box::new(Text::new("Hello, World!")));
}
```

```json
{
    "name": "syngui",
    "version": "0.1.0",
    "features": ["markdown", "markdown-syntax"]
}
```

```cpp
#include <iostream>

int main() {
    std::cout << "Hello, World!" << std::endl;
    return 0;
}
```

```python
def fibonacci(n: int) -> int:
    if n <= 1:
        return n
    return fibonacci(n - 1) + fibonacci(n - 2)

print([fibonacci(i) for i in range(10)])
```

## Lists

- First item in the list
- Second item with more details
- Third item to complete the set

1. Step one: install Rust
2. Step two: add syngui to dependencies
3. Step three: build your UI

## Task List

- [x] Implement basic text rendering
- [x] Add code block support
- [x] Build table rendering
- [x] Add syntax highlighting
- [ ] Support image loading via streaming

## Blockquotes

> "The best way to predict the future is to invent it."
> — Alan Kay

## Tables

| Widget | Category | Status |
|--------|----------|--------|
| Text | Basic | Stable |
| Button | Input | Stable |
| MarkdownView | Visual | Updated |
| MarkdownEditor | Visual | New |

[^1]: Footnotes are rendered as numbered superscript references with definitions
      collected at the document end, separated by a horizontal rule.
"#;

const EDITOR_INITIAL: &str = r#"# Try editing me!

Switch between **Edit / Preview / Split** with the toolbar.

```rust
fn main() {
    println!("Hello from MarkdownEditor!");
}
```

```json
{ "edit": true, "preview": true, "split": true }
```

> Tip: hover over a code block in preview to copy its contents.
"#;

pub fn build_markdown_section() -> impl Widget {
    let editor_text = use_signal(EDITOR_INITIAL.to_string());

    section_card(
        Column::new()
            .gap(20.0)
            .child(section_title("Markdown"))
            .child(label(
                "MarkdownView renders CommonMark with syntax highlighting, autolinks, footnotes and copy buttons.",
            ))
            .child(
                MarkdownView::new(DEMO_MARKDOWN)
                    .with_syntax_highlight(true)
                    .with_copy_code(true)
                    .class("markdown-view"),
            )
            .child(section_title("MarkdownEditor"))
            .child(label(
                "Edit / Preview / Split — toolbar driven, fully reactive via RwSignal<String>.",
            ))
            .child(
                MarkdownEditor::new(editor_text)
                    .syntax_highlight(true)
                    .copy_code(true)
                    .rows(14)
                    .class("markdown-editor"),
            )
    )
}
