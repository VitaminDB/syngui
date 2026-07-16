use syngui::prelude::*;
use syngui::signal::use_signal;

use super::{label, section_card, section_title};

pub fn build_menus_section() -> impl Widget {
    let menu_open = use_signal(false);
    let menu_pos = use_signal(Point::new(0.0, 0.0));
    let menu_result = use_signal(String::from("(none)"));

    section_card(
        Column::new()
            .gap(16.0)
            .child(section_title("Menus"))
            // Popup Menu
            .child(
                Column::new()
                    .gap(8.0)
                    .child(label("Popup Menu"))
                    .child(
                        Row::new()
                            .gap(8.0)
                            .child({
                                Button::new("Open Menu")
                                    
                                    .on_click_at(move |click_pos| {
                                        menu_pos.set(click_pos);
                                        menu_open.set(true);
                                    })
                            })
                            .child(move || {
                                let result = menu_result.get();
                                Text::new(format!("Selected: {}", result))
                                    .class("label")
                            }),
                    )
                    .child(
                        PopupMenu::new()
                            .items(vec![
                                MenuItem::new("new", "New File")
                                    .icon("📄")
                                    .shortcut("Ctrl+N"),
                                MenuItem::new("open", "Open...")
                                    .icon("📂")
                                    .shortcut("Ctrl+O"),
                                MenuItem::new("save", "Save")
                                    .icon("💾")
                                    .shortcut("Ctrl+S"),
                                MenuItem::separator(),
                                MenuItem::new("export", "Export as PDF").icon("📑"),
                                MenuItem::new("print", "Print...")
                                    .icon("🖨")
                                    .shortcut("Ctrl+P"),
                                MenuItem::separator(),
                                MenuItem::new("close", "Close").shortcut("Ctrl+W"),
                            ])
                            .is_open(menu_open)
                            .position(menu_pos)
                            .on_select(move |id| {
                                menu_result.set(id.to_string());
                            }),
                    ),
            )
            // Menu Features
            .child(
                Column::new()
                    .gap(8.0)
                    .child(label("Menu Features"))
                    .child(Text::new("• Icons and keyboard shortcut hints").class("label"))
                    .child(Text::new("• Separator items for grouping").class("label"))
                    .child(Text::new("• Disabled items (greyed out)").class("label"))
                    .child(
                        Text::new("• Keyboard navigation (Up/Down/Enter/Escape)").class("label"),
                    )
                    .child(Text::new("• Click outside to dismiss").class("label")),
            )
            // Context Menu
            .child(
                Column::new()
                    .gap(8.0)
                    .child(label("Context Menu (right-click the area below)"))
                    .child(
                        ContextMenu::new()
                            .child(
                                DecoratedBox::new().style("background-color", Color::from_hex("#F3F4F6"))
                                    .child(
                                        Padding::all(16.0).child(
                                            Text::new("Right-click me for a context menu")
                                                .class("label"),
                                        ),
                                    )
                                    .class("section-card"),
                            )
                            .items(vec![
                                MenuItem::new("cut", "Cut").shortcut("Ctrl+X"),
                                MenuItem::new("copy", "Copy").shortcut("Ctrl+C"),
                                MenuItem::new("paste", "Paste").shortcut("Ctrl+V"),
                                MenuItem::separator(),
                                MenuItem::new("select_all", "Select All").shortcut("Ctrl+A"),
                            ]),
                    ),
            ),
    )
}
