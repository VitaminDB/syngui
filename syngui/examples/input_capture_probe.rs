//! Проверка захвата ввода (игры, ВМ): окно запирает указатель, перехватывает
//! сочетания клавиш и печатает сырые смещения мыши; через INPUT_CAPTURE_SECS
//! (по умолчанию 8 с) — снимает захват и выходит. Esc — выйти раньше.
//! `cargo run --example input_capture_probe` (Wayland: pointer-constraints,
//! relative-pointer, keyboard-shortcuts-inhibit).
use std::time::Duration;
use syngui::prelude::*;

fn main() {
    let secs: u64 = std::env::var("INPUT_CAPTURE_SECS").ok().and_then(|s| s.parse().ok()).unwrap_or(8);
    std::thread::spawn(move || {
        // окно успевает появиться и получить фокус
        std::thread::sleep(Duration::from_millis(1500));
        syngui::async_runtime::run_on_main_thread(|| {
            let total = std::rc::Rc::new(std::cell::Cell::new((0.0f64, 0.0f64, 0u32)));
            let t = total.clone();
            syngui::signal::on_raw_motion(Some(Box::new(move |dx, dy| {
                let (x, y, n) = t.get();
                t.set((x + dx, y + dy, n + 1));
                if n % 20 == 0 {
                    eprintln!("raw motion #{n}: ({dx:.1}, {dy:.1}), всего ({:.0}, {:.0})", x + dx, y + dy);
                }
            })));
            eprintln!("pointer lock: {:?}", syngui::signal::set_pointer_lock(true));
            eprintln!("inhibit shortcuts: {:?}", syngui::signal::inhibit_shortcuts(true));
            eprintln!("locked={} inhibited={}", syngui::signal::pointer_locked(), syngui::signal::shortcuts_inhibited());
        });
        std::thread::sleep(Duration::from_secs(secs));
        syngui::async_runtime::run_on_main_thread(|| {
            eprintln!("снятие: lock {:?}, inhibit {:?}", syngui::signal::set_pointer_lock(false), syngui::signal::inhibit_shortcuts(false));
            syngui::signal::quit_app();
        });
    });
    App::new()
        .title("input capture probe")
        .app_id("syngui-input-capture-probe")
        .size(640, 400)
        .run(|_| Box::new(DecoratedBox::new().style("background-color", Color::from_hex("#203040"))));
}
