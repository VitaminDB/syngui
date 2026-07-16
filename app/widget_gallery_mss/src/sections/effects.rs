use syngui::mgui;
use syngui::prelude::*;
use std::sync::atomic::{AtomicBool, AtomicU32, Ordering};
use std::sync::Arc;
use std::time::Duration;

use super::{section_card, section_title, label};

pub fn build_effects_section() -> impl Widget {
    // --- Demo 1: Auto-incrementing timer ---
    let seconds = use_signal(0u32);

    #[cfg(not(target_arch = "wasm32"))]
    use_effect_with_cleanup(move || {
        let running = Arc::new(AtomicBool::new(true));
        let flag = running.clone();
        let counter = Arc::new(AtomicU32::new(0));
        let cnt = counter.clone();
        std::thread::spawn(move || {
            while flag.load(Ordering::Relaxed) {
                std::thread::sleep(Duration::from_secs(1));
                if !flag.load(Ordering::Relaxed) { break; }
                let val = cnt.fetch_add(1, Ordering::Relaxed) + 1;
                seconds.set(val);
            }
        });
        Some(Box::new(move || {
            running.store(false, Ordering::Relaxed);
        }) as Box<dyn Fn()>)
    });

    // --- Demo 2: Stopwatch with start/stop ---
    let stopwatch = use_signal(0u32);
    let sw_running = use_signal(false);

    #[cfg(not(target_arch = "wasm32"))]
    use_effect_with_cleanup(move || {
        let is_running = sw_running.get();
        if !is_running {
            return None;
        }
        let running = Arc::new(AtomicBool::new(true));
        let flag = running.clone();
        let counter = Arc::new(AtomicU32::new(stopwatch.get_untracked()));
        let cnt = counter.clone();
        std::thread::spawn(move || {
            while flag.load(Ordering::Relaxed) {
                std::thread::sleep(Duration::from_millis(100));
                if !flag.load(Ordering::Relaxed) { break; }
                let val = cnt.fetch_add(1, Ordering::Relaxed) + 1;
                stopwatch.set(val);
            }
        });
        Some(Box::new(move || {
            running.store(false, Ordering::Relaxed);
        }) as Box<dyn Fn()>)
    });

    // --- Demo 3: Derived effect (change counter) ---
    let click_count = use_signal(0u32);
    let effect_log = use_signal(String::from("No clicks yet"));

    use_effect(move || {
        let count = click_count.get();
        if count == 0 {
            effect_log.set_always("No clicks yet".to_string());
        } else {
            effect_log.set_always(format!("Effect triggered! Click count: {count}"));
        }
    });

    mgui! {
        Column::new().gap(24.0) => [
            Text::new("Effects (use_effect)").class("page-title"),
            Text::new("Reactive side effects that auto-track signal dependencies")
                .class("page-subtitle"),

            section_card(mgui! {
                Column::new().gap(12.0) => [
                    section_title("Timer (use_effect_with_cleanup)"),
                    label("A background thread ticks every second. Cleanup stops the thread."),
                    move || {
                        let s = seconds.get();
                        let mins = s / 60;
                        let secs = s % 60;
                        Text::new(&format!("{mins:02}:{secs:02}"))
                            .bold()
                            .style("font-size", 32.0_f32)
                    },
                ]
            }),

            section_card(mgui! {
                Column::new().gap(12.0) => [
                    section_title("Stopwatch (reactive effect)"),
                    label("Effect re-runs when running signal changes — starts/stops the thread."),
                    move || {
                        let ticks = stopwatch.get();
                        let secs = ticks / 10;
                        let tenths = ticks % 10;
                        Text::new(&format!("{secs}.{tenths}s"))
                            .bold()
                            .style("font-size", 32.0_f32)
                    },
                    Row::new().gap(8.0) => [
                        Button::new("Start")
                            .on_click(move || sw_running.set(true)),
                        Button::new("Stop")
                            .on_click(move || sw_running.set(false)),
                        Button::new("Reset").on_click(move || {
                            sw_running.set(false);
                            stopwatch.set(0);
                        }),
                    ],
                ]
            }),

            section_card(mgui! {
                Column::new().gap(12.0) => [
                    section_title("Derived Effect (use_effect)"),
                    label("use_effect auto-tracks signal reads and re-runs the closure."),
                    Button::new("Click me")
                        .on_click(move || {
                            let c = click_count.get_untracked();
                            click_count.set(c + 1);
                        }),
                    move || Text::new(&effect_log.get()),
                ]
            }),
        ]
    }
}
