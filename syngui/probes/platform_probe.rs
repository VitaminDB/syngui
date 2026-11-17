//! Проверка `syngui::platform` без окна:
//! `cargo run --example platform_probe --features portal -- pick image|video|any`
//! `… -- share-text "текст"`, `… -- share-file ПУТЬ`, `… -- open ПУТЬ`.
//! Результат выбора печатается в stdout (отмена — `Ok(None)`).
use std::path::Path;
use std::sync::mpsc;
use std::time::Duration;

use syngui::platform::{self, MediaKind};

fn main() {
    let args: Vec<String> = std::env::args().skip(1).collect();
    let arg = |i: usize| args.get(i).map(String::as_str).unwrap_or("");
    match arg(0) {
        "pick" => {
            let kind = match arg(1) {
                "video" => MediaKind::Video,
                "any" => MediaKind::ImageOrVideo,
                _ => MediaKind::Image,
            };
            let (tx, rx) = mpsc::channel();
            platform::pick_media(kind, move |r| {
                let _ = tx.send(r);
            });
            // Колбэк приходит через run_on_main_thread — без event loop дренируем сами.
            loop {
                syngui::async_runtime::drain_main_thread_callbacks();
                if let Ok(r) = rx.try_recv() {
                    println!("{r:?}");
                    break;
                }
                std::thread::sleep(Duration::from_millis(50));
            }
        }
        "share-text" => println!("{:?}", platform::share_text("syngui", arg(1))),
        "share-file" => println!("{:?}", platform::share_file(Path::new(arg(1)), "", "syngui")),
        "open" => println!("{:?}", platform::open_path(Path::new(arg(1)))),
        _ => eprintln!("usage: platform_probe pick [image|video|any] | share-text T | share-file P | open P"),
    }
}
