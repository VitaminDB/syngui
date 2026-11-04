//! Снимок: шрифт приложения из файла под своим именем семейства
//! (`with_font_data` / `register_font`), насыщенность осью `wght`
//! переменного шрифта и контур текста `text-stroke`.
//!
//! `cargo run --example font_shot -- Шрифт.ttf [фон.jpg] out.png`

use syngui::prelude::*;

fn main() {
    let args: Vec<String> = std::env::args().skip(1).collect();
    let font = args.first().cloned().expect("путь к шрифту");
    let out = args.last().cloned().unwrap_or_else(|| "font_shot.png".into());
    let bg = (args.len() > 2).then(|| args[1].clone());
    // Имя нарочно не совпадает с системным: виден именно зарегистрированный файл.
    syngui::text::register_font_file("Рукопись", &font).expect("шрифт");
    let mss = r#"
        .root { background-color: #FFF8F0; padding: 16; }
        .t { color: #3A2A1A; font-family: "Рукопись"; font-size: 34px; }
        .w4 { font-weight: 400; } .w5 { font-weight: 500; } .w6 { font-weight: 600; } .w7 { font-weight: 700; }
        .base { color: #3A2A1A; font-size: 20px; }
        .btn { font-family: "Рукопись"; font-size: 22px; padding: 8px 18px; background-color: #EFE4D6; color: #3A2A1A; border-radius: 14px; }
        .photo { width: 520px; height: 200px; background-color: #6E4A2F; }
        .over { color: #FFFFFF; font-family: "Рукопись", cursive; font-size: 44px; font-weight: 700;
                text-shadow: 0 2px 8px #00000066; text-stroke: 1.5px #00000080; }
        .stroke { color: #FFFFFF; font-family: "Рукопись"; font-size: 44px; font-weight: 700; text-stroke: 1.5px #00000080; }
        .plain { color: #FFFFFF; font-family: "Рукопись"; font-size: 44px; font-weight: 700; }
    "#;
    syngui::shot::render_png_with(
        move || {
            let photo: Box<dyn Widget> = match &bg {
                Some(p) => Box::new(DecoratedBox::new().class("photo").clip(true).child(Image::new(p.clone()).fit(ImageFit::Cover))),
                None => Box::new(DecoratedBox::new().class("photo")),
            };
            Box::new(
                DecoratedBox::new().class("root").child(
                    Column::new()
                        .gap(6.0)
                        .child(Text::new("Бабушкин борщ — 400").class("t w4"))
                        .child(Text::new("Бабушкин борщ — 500").class("t w5"))
                        .child(Text::new("Бабушкин борщ — 600").class("t w6"))
                        .child(Text::new("Бабушкин борщ — 700").class("t w7"))
                        .child(Text::new("Основной шрифт для сравнения ✓ 🍲").class("base"))
                        .child(Row::new().child(Button::new("Кнопка рукописью").class("btn")))
                        .child(
                            Stack::new()
                                .child(photo)
                                .child(
                                    Column::new()
                                        .gap(0.0)
                                        .child(Text::new("Пирог с вишней").class("over"))
                                        .child(Text::new("Пирог с вишней").class("stroke"))
                                        .child(Text::new("Пирог с вишней").class("plain")),
                                ),
                        ),
                ),
            )
        },
        mss,
        (560, 700),
        2.0,
        &out,
        syngui::shot::ShotOptions { wait: std::time::Duration::from_millis(800), ..Default::default() },
    )
    .expect("shot");
    eprintln!("→ {out}");
}
