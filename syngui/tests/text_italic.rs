#![cfg(feature = "markdown")]
//! Курсив доходит до текстовых команд display list: MSS `font-style` у
//! `Text` и `*выделение*` в Markdown. Наклон глифов делает batcher.

use syngui::prelude::*;
use syngui::render::display_list::DrawCommand;
use syngui::testing::*;
use syngui::widgets::MarkdownView;

fn italic_texts(list: &syngui::render::DisplayList) -> Vec<(String, bool)> {
    list.iter_all_commands()
        .filter_map(|c| match c {
            DrawCommand::Text { text, italic, .. } => Some((text.to_string(), *italic)),
            _ => None,
        })
        .collect()
}

#[test]
fn text_font_style_italic() {
    let widget = Column::new()
        .child(Text::new("slanted").class("it"))
        .child(Text::new("upright"));
    let mut h = TestHarness::new(Box::new(widget));
    h.apply_mss(".it { font-style: italic; }");
    h.layout(400.0, 200.0);
    let texts = italic_texts(&h.paint());
    assert!(texts.iter().any(|(t, i)| t == "slanted" && *i), "{texts:?}");
    assert!(texts.iter().any(|(t, i)| t == "upright" && !*i), "{texts:?}");
}

#[test]
fn markdown_emphasis_is_italic() {
    let mut h = TestHarness::new(Box::new(MarkdownView::new("plain *slanted* tail")));
    h.apply_mss("");
    h.layout(600.0, 200.0);
    let texts = italic_texts(&h.paint());
    assert!(texts.iter().any(|(t, i)| t.contains("slanted") && *i), "{texts:?}");
    assert!(texts.iter().any(|(t, i)| t.contains("plain") && !*i), "{texts:?}");
}
