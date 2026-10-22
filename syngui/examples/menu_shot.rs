//! Снимок открытого меню с иконками (проверка выравнивания): cargo run --example menu_shot --features "material-icons msdf" -- out.png
use syngui::prelude::*;
use syngui::widgets::overlay::{MenuItem, PopupMenu};

fn main() {
    let out = std::env::args().nth(1).unwrap_or_else(|| "menu.png".into());
    let r = syngui::shot::render_png(
        || {
            let open = use_signal(true);
            let pos = use_signal(Point::new(20.0, 20.0));
            Box::new(Stack::new().child(DecoratedBox::new()).child(
                PopupMenu::new()
                    .items(vec![
                        MenuItem::new("a", "Открыть").icon("\u{E2C8}"),
                        MenuItem::new("b", "Без иконки"),
                        MenuItem::separator(),
                        MenuItem::new("c", "Копировать").icon("\u{E14D}").shortcut("Ctrl+C"),
                        MenuItem::new("d", "Создать").icon("\u{E145}").children(vec![MenuItem::new("e", "Файл")]),
                        MenuItem::separator(),
                        MenuItem::new("f", "FA Solid: дом").icon(syngui::text::icon_fonts::fa::solid(0xF015)),
                        MenuItem::new("g", "FA Regular: звезда").icon(syngui::text::icon_fonts::fa::regular(0xF005)),
                        MenuItem::new("h", "FA Brands: GitHub").icon(syngui::text::icon_fonts::fa::brands(0xF09B)),
                    ])
                    .position(pos)
                    .is_open(open),
            ))
        },
        "",
        (320, 360),
        1.0,
        &out,
    );
    println!("{r:?}");
}
