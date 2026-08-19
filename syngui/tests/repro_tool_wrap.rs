//! Репро: длинные строки в раскрытой tool-карточке внутри группы
//! вылезают за пределы карточки (synthos, Syn-чат, minimal-режим).

use syngui::prelude::*;
use syngui::testing::*;
use syngui::widgets::visual::MarkdownView;
use syngui::widgets::{AnimatedSize, AnimationAxis, Reactive};

const LONG_BODY: &str = "$ cd /home/master/Projects/2027/ellemusic && grep -n \"fn on_click|fn on_change|pub fn \" ../syngui/syngui/src/widgets/containers/gesture_detector.rs && grep -rn \"fn on_scroll|fn on_hover|pub fn \" ../syngui/syngui/src/widgets/containers/scroll_area.rs ../syngui/syngui/src/widgets/containers/hover_region.rs\nexit: 0\n--- stdout ---\n42:    pub fn on_click(mut self, f: impl Fn() + Send + Sync + 'static) -> Self { self.on_click = Some(std::sync::Arc::new(f)); self } // длинная строка кода, которая точно шире любой карточки в ленте чата и обязана переноситься\n55:    pub fn on_change(mut self, f: impl Fn(bool) + Send + Sync + 'static) -> Self { self.on_change = Some(std::sync::Arc::new(f)); self } // и ещё одна длинная строка кода, которая тоже шире карточки";

fn body_wrap() -> Box<dyn Widget> {
    // Как в synthos: плоский вывод инструмента оборачивается код-фенсом.
    Box::new(
        DecoratedBox::new().class("tool-result-body-wrap").child(
            MarkdownView::new(format!("```\n{LONG_BODY}\n```"))
                .selectable(true)
                .class("tool-result-body"),
        ),
    )
}

fn result_card() -> Box<dyn Widget> {
    let header = Row::new()
        .gap(10.0)
        .cross_axis_alignment(CrossAxisAlignment::Center)
        .children(vec![
            Box::new(Text::new("bash").class("tool-result-name")) as Box<dyn Widget>,
            Box::new(DecoratedBox::new().class("grow")),
            Box::new(Text::new("11:04").class("msg-time")),
        ]);
    let collapsible = AnimatedSize::new(Reactive::new(
        move || -> Vec<Box<dyn Widget>> { vec![body_wrap()] },
    ))
    .axis(AnimationAxis::Height)
    .duration_ms(200);
    Box::new(
        DecoratedBox::new().class("tool-result-card").child(
            Column::new()
                .gap(8.0)
                .cross_axis_alignment(CrossAxisAlignment::Stretch)
                .children(vec![Box::new(header) as Box<dyn Widget>, Box::new(collapsible)]),
        ),
    )
}

fn group() -> Box<dyn Widget> {
    let header = Row::new()
        .gap(10.0)
        .cross_axis_alignment(CrossAxisAlignment::Center)
        .children(vec![
            Box::new(Text::new("bash ×5").class("tool-group-name")) as Box<dyn Widget>,
            Box::new(DecoratedBox::new().class("grow")),
        ]);
    let body_reactive = Reactive::new(move || -> Vec<Box<dyn Widget>> {
        let col = Column::new()
            .gap(8.0)
            .cross_axis_alignment(CrossAxisAlignment::Stretch)
            .children(vec![result_card()]);
        vec![Box::new(
            DecoratedBox::new().class("tool-group-children").child(col),
        ) as Box<dyn Widget>]
    });
    let body = AnimatedSize::new(body_reactive)
        .axis(AnimationAxis::Height)
        .duration_ms(200);
    Box::new(
        DecoratedBox::new().class("tool-group-card").child(
            Column::new()
                .gap(8.0)
                .cross_axis_alignment(CrossAxisAlignment::Stretch)
                .children(vec![Box::new(header) as Box<dyn Widget>, Box::new(body)]),
        ),
    )
}

fn lane() -> Box<dyn Widget> {
    // Как в message_area: Row(avatar, meta-column).
    let avatar = DecoratedBox::new()
        .style("width", 32.0_f32)
        .style("height", 32.0_f32);
    let meta = Column::new()
        .gap(4.0)
        .cross_axis_alignment(CrossAxisAlignment::Start)
        .children(vec![
            Box::new(Text::new("Ассистент").class("msg-author")) as Box<dyn Widget>,
            group(),
        ]);
    let row = Row::new()
        .gap(10.0)
        .cross_axis_alignment(CrossAxisAlignment::Start)
        .main_axis_alignment(MainAxisAlignment::Start)
        .children(vec![Box::new(avatar) as Box<dyn Widget>, Box::new(meta)]);
    Box::new(
        Column::new()
            .gap(14.0)
            .cross_axis_alignment(CrossAxisAlignment::Stretch)
            .children(vec![Box::new(row) as Box<dyn Widget>]),
    )
}

const MSS: &str = r#"
.tool-group-card { border-width: 1px; padding: 10px 12px; max-width: 90%; }
.tool-group-children { padding-top: 4px; }
.tool-result-card { border-width: 1px; padding: 12px 14px; max-width: 90%; }
.tool-result-body-wrap { border-width: 1px; padding: 10px 12px; }
.tool-result-body { font-size: 13px; }
.tool-result-name { font-size: 13px; }
.msg-author { font-size: 13px; }
.msg-time { font-size: 12px; }
"#;

#[test]
fn expanded_tool_card_body_stays_inside_cards() {
    let mut h = TestHarness::new(lane());
    h.apply_mss(MSS);
    h.layout(1200.0, 900.0);
    // Повторный layout после rebuild (Reactive мог достроиться позже).
    h.rebuild();
    h.apply_mss(MSS);
    h.layout(1200.0, 900.0);

    let group_b = h.element_bounds(h.find_by_class("tool-group-card")[0]);
    let card_b = h.element_bounds(h.find_by_class("tool-result-card")[0]);
    let wrap_b = h.element_bounds(h.find_by_class("tool-result-body-wrap")[0]);
    let md_id = h.find_by_type_name("MarkdownView")[0];
    let md_b = h.element_bounds(md_id);

    eprintln!("group: {:?}", group_b);
    eprintln!("card:  {:?}", card_b);
    eprintln!("wrap:  {:?}", wrap_b);
    eprintln!("md:    {:?}", md_b);

    let group_r = group_b.origin.x + group_b.size.width;
    let card_r = card_b.origin.x + card_b.size.width;
    let wrap_r = wrap_b.origin.x + wrap_b.size.width;

    assert!(
        card_r <= group_r + 1.0,
        "result-card ({card_r}) вылезает за group-card ({group_r})"
    );
    assert!(
        wrap_r <= card_r + 1.0,
        "body-wrap ({wrap_r}) вылезает за result-card ({card_r})"
    );
}
