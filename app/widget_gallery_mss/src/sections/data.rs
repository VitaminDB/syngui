use syngui::prelude::*;

use super::{label, section_card, section_title};

/// Конструктор демо-дерева для секции «icon-color states».
/// `class_name = None` — дефолтный TreeView без MSS overrides.
fn make_demo_tree(class_name: Option<&'static str>) -> TreeView {
    let tv = TreeView::new(vec![
        TreeNode::branch("src", "src", vec![
            TreeNode::branch("widgets", "widgets", vec![
                TreeNode::leaf("button", "button.rs").icon("\u{E873}"), // description
                TreeNode::leaf("text", "text.rs").icon("\u{E873}"),
                TreeNode::leaf("input", "input.rs").icon("\u{E873}"),
            ]).icon("\u{E2C7}").expanded(true), // folder
            TreeNode::branch("layout", "layout", vec![
                TreeNode::leaf("column", "column.rs").icon("\u{E873}"),
                TreeNode::leaf("row", "row.rs").icon("\u{E873}"),
            ]).icon("\u{E2C7}"),
            TreeNode::leaf("lib", "lib.rs").icon("\u{E873}"),
            TreeNode::leaf("main", "main.rs").icon("\u{E873}"),
        ]).icon("\u{E2C7}").expanded(true),
        TreeNode::branch("tests", "tests", vec![
            TreeNode::leaf("unit", "unit_tests.rs").icon("\u{E873}"),
            TreeNode::leaf("integration", "integration.rs").icon("\u{E873}"),
        ]).icon("\u{E2C7}"),
        TreeNode::leaf("cargo", "Cargo.toml").icon("\u{E865}"),       // book
        TreeNode::leaf("readme", "README.md").icon("\u{E0E0}"),       // article
    ])
    .show_lines(true);
    match class_name {
        Some(c) => tv.class(c),
        None => tv,
    }
}

pub fn build_data_section() -> impl Widget {
    section_card(
        Column::new()
            .gap(16.0)
            .child(section_title("Data"))
            // ListView
            .child(
                Column::new()
                    .gap(8.0)
                    .child(label("ListView (single selection)"))
                    .child(
                        ListView::new(vec![
                            ListItem::new("Inbox").icon("📥").secondary("12 unread").trailing("12"),
                            ListItem::new("Starred").icon("⭐").secondary("3 items"),
                            ListItem::new("Sent").icon("📤").secondary("Last: 2h ago"),
                            ListItem::new("Drafts").icon("📝").secondary("5 drafts").trailing("5"),
                            ListItem::new("Trash").icon("🗑").secondary("Empty"),
                            ListItem::new("Spam").icon("⚠").secondary("2 items").trailing("2"),
                            ListItem::new("Archive").icon("📦").secondary("148 items"),
                            ListItem::new("Important").icon("🔴").secondary("7 items").trailing("7"),
                            ListItem::new("Labels").icon("🏷").secondary("Custom labels"),
                            ListItem::new("Settings").icon("⚙").secondary("Account settings").disabled(true),
                        ])
                        .selection_mode(SelectionMode::Single)
                        .selected(vec![0])
                        .item_height(48.0)
                        .width(400.0)
                        .height(300.0),
                    ),
            )
            // TableView
            .child(
                Column::new()
                    .gap(8.0)
                    .child(label("TableView (sortable, striped)"))
                    .child(
                        TableView::new(
                            vec![
                                TableColumn::fixed("ID", 60.0),
                                TableColumn::flex("Name", 2.0),
                                TableColumn::flex("Language", 1.5),
                                TableColumn::flex("Stars", 1.0),
                                TableColumn::flex("Status", 1.0),
                            ],
                            vec![
                                vec!["1".into(), "syngui".into(), "Rust".into(), "1.2k".into(), "Active".into()],
                                vec!["2".into(), "react".into(), "JavaScript".into(), "220k".into(), "Active".into()],
                                vec!["3".into(), "flutter".into(), "Dart".into(), "162k".into(), "Active".into()],
                                vec!["4".into(), "svelte".into(), "JavaScript".into(), "78k".into(), "Active".into()],
                                vec!["5".into(), "gtk-rs".into(), "Rust".into(), "1.8k".into(), "Active".into()],
                                vec!["6".into(), "iced".into(), "Rust".into(), "23k".into(), "Active".into()],
                            ],
                        )
                        .sortable(true)
                        .striped(true)
                        .row_height(40.0)
                        .width(600.0)
                        .height(300.0),
                    ),
            )
            // Virtual ListView (10,000 items)
            .child(
                Column::new()
                    .gap(8.0)
                    .child(label("Virtual ListView (10,000 items)"))
                    .child(
                        ListView::virtual_new(10_000, |i| {
                            let icons = ["📄", "📁", "🔗", "⚙"];
                            let categories = ["Documents", "Media", "Projects", "System"];
                            ListItem::new(format!("Item #{}", i + 1))
                                .icon(icons[i % 4])
                                .secondary(format!("{} — updated {} min ago", categories[i % 4], i % 60))
                                .trailing(format!("{}", (i * 7 + 3) % 100))
                        })
                        .selection_mode(SelectionMode::Single)
                        .buffer_size(10)
                        .item_height(48.0)
                        .width(400.0)
                        .height(300.0),
                    ),
            )
            // Virtual TableView (1,000 rows)
            .child(
                Column::new()
                    .gap(8.0)
                    .child(label("Virtual TableView (1,000 rows)"))
                    .child(
                        TableView::virtual_new(
                            vec![
                                TableColumn::fixed("ID", 80.0),
                                TableColumn::flex("Name", 2.0),
                                TableColumn::flex("Email", 2.0),
                                TableColumn::flex("Score", 1.0),
                            ],
                            1_000,
                            |i| {
                                let names = ["Alice", "Bob", "Carol", "Dave", "Eve", "Frank", "Grace", "Heidi"];
                                let domains = ["example.com", "test.org", "mail.io", "dev.net"];
                                let name = names[i % names.len()];
                                vec![
                                    format!("{}", i + 1),
                                    format!("{} {}", name, i / names.len() + 1),
                                    format!("{}{}{}", name.to_lowercase(), i + 1, domains[i % domains.len()]),
                                    format!("{:.1}", (i * 17 % 100) as f32 / 10.0),
                                ]
                            },
                        )
                        .sortable(true)
                        .striped(true)
                        .buffer_size(10)
                        .width(600.0)
                        .height(300.0),
                    ),
            )
            // TreeView
            .child(
                Column::new()
                    .gap(8.0)
                    .child(label("TreeView (icon-color states: default · accent-on-select · vibrant)"))
                    .child(
                        Row::new()
                            .gap(16.0)
                            // Default — без MSS, fallback на color/accent.
                            .child(make_demo_tree(None).width(280.0).height(300.0))
                            // accent-on-select — selected-иконка фоллбечится на accent-color.
                            .child(make_demo_tree(Some("demo-icon-tree")).width(280.0).height(300.0))
                            // Vibrant — все состояния (normal/hover/selected) разведены явно.
                            .child(make_demo_tree(Some("demo-icon-tree-vibrant")).width(280.0).height(300.0)),
                    ),
            )
            // PropertyGrid
            .child(
                Column::new()
                    .gap(8.0)
                    .child(label("PropertyGrid (inline editing)"))
                    .child(
                        PropertyGrid::new()
                            .property(Property::text("Name", "MyWidget"))
                            .property(Property::number("Width", 320.0))
                            .property(Property::number("Height", 240.0))
                            .property(Property::boolean("Visible", true))
                            .property(Property::boolean("Enabled", true))
                            .property(Property::color("Background", Color::from_hex("#3B82F6")))
                            .property(Property::color("Border Color", Color::from_hex("#E5E7EB")))
                            .property(Property::text("Font Family", "DejaVu Sans"))
                            .property(Property::number("Font Size", 14.0))
                            .property(Property::number("Border Radius", 8.0))
                            .property(Property::number("Opacity", 1.0))
                            .property(Property::choice(
                                "Overflow",
                                vec!["visible".into(), "hidden".into(), "scroll".into(), "auto".into()],
                                1,
                            ))
                            .property(Property::choice(
                                "Cursor",
                                vec!["default".into(), "pointer".into(), "text".into(), "crosshair".into()],
                                0,
                            ))
                            .property(Property::text("Tooltip", "Click to edit"))
                            .property(Property::boolean("Focusable", true))
                            .row_height(32.0)
                            .width(450.0)
                            .height(350.0),
                    ),
            ),
    )
}
