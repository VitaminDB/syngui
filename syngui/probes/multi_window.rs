//! Окна ОС во время работы (`syngui::app::open_window`).
//!
//! Главное окно: общий счётчик, «+1», «Открыть панель» (второй раз — поднять
//! уже открытую). Панель — дополнительное окно без системной рамки
//! (`frameless` + `transparent`): свой заголовок с `WindowDragRegion` и
//! `SystemWindowControls`, `WindowResizeRegion` по краю, скругление и рамка
//! снимаются в развёрнутом окне (`:window-maximized`). В панели — тот же
//! счётчик (один сигнал на оба окна), поле ввода (IME, буфер обмена),
//! контекстное меню по правому щелчку, «Закрыть панель»
//! (`current_window().close()`); Ctrl+W — закрыть панель с клавиатуры
//! (`on_key` корня). Закрытие панели (✕, Alt+F4, кнопкой) отмечается в
//! главном окне через `WindowConfig::on_close`.
//!
//! `cargo run --profile fast-release -p syngui --example multi_window`.
//! `MULTI_WINDOW_AUTO=1` — без рук: панель открывается сама, через 3 с
//! закрывается, ещё через 2 с — выход.

use std::time::Duration;

use syngui::app::{current_window, open_window, WindowConfig, WindowHandle};
use syngui::prelude::*;
use syngui::widgets::overlay::{
    ContextMenu, ControlsSide, MenuItem, SystemWindowControls, WindowDragRegion, WindowResizeRegion,
};
use syngui::window::WindowState;
use syngui::*;

const MSS: &str = r#"
:root { --bg: #1e2129; --panel: #262a34; --fg: #e6e8ee; --muted: #9aa0ad; --accent: #6c8cff; --border: #3a3f4c; }
.main { background: var(--bg); padding: 24px; gap: 14px; }
.title { font-size: 20px; font-weight: 600; color: var(--fg); }
.note { font-size: 13px; color: var(--muted); }
.count { font-size: 32px; font-weight: 700; color: var(--accent); }
.frame { background: var(--panel); border-radius: 12px; border: 1px solid var(--border); }
.frame:window-maximized, .frame:window-fullscreen { border-radius: 0px; border-width: 0px; }
.titlebar { height: 40px; padding: 0px 4px 0px 12px; align-items: center; }
.drag { height: 40px; flex-grow: 1; align-items: center; }
.tb-title { font-size: 13px; font-weight: 600; color: var(--fg); }
.tb-title:window-focused { color: var(--accent); }
SystemWindowControls { color: var(--fg); background-color: #ffffff18; }
.body { padding: 16px; gap: 12px; }
.menu-area { padding: 18px; border-radius: 8px; background: #ffffff0c; border: 1px dashed var(--border); }
"#;

fn main() {
    let auto = std::env::var("MULTI_WINDOW_AUTO").is_ok();
    App::new()
        .title("multi_window: главное")
        .app_id("syngui-multi-window-probe")
        .size(520, 380)
        .with_styles_str(MSS)
        .run(move |_| {
            let counter = use_signal(0i32);
            let panel = use_signal(None::<WindowHandle>);
            let status = use_signal(String::from("панель не открыта"));

            if auto {
                std::thread::spawn(move || {
                    std::thread::sleep(Duration::from_millis(1200));
                    syngui::async_runtime::run_on_main_thread(move || {
                        open_panel(counter, panel, status);
                        counter.update(|c| *c += 1);
                    });
                    std::thread::sleep(Duration::from_secs(3));
                    syngui::async_runtime::run_on_main_thread(move || {
                        if let Some(h) = panel.get_untracked() {
                            eprintln!("[multi_window] закрываю панель {}", h.id());
                            h.close();
                        }
                    });
                    std::thread::sleep(Duration::from_secs(2));
                    syngui::async_runtime::run_on_main_thread(move || {
                        eprintln!("[multi_window] статус: {}", status.get_untracked());
                        syngui::signal::quit_app();
                    });
                });
            }

            Box::new(
                Column::new()
                    .class("main")
                    .child(Text::new("Главное окно").class("title"))
                    .child(counter_view(counter))
                    .child(
                        Row::new()
                            .gap(10.0)
                            .child(Button::new("+1").on_click(move || counter.update(|c| *c += 1)))
                            .child(
                                Button::new("Открыть панель")
                                    .on_click(move || open_panel(counter, panel, status)),
                            ),
                    )
                    .child(Reactive::new(move || {
                        vec![Box::new(Text::new(status.get()).class("note")) as Box<dyn Widget>]
                    })),
            )
        });
}

fn counter_view(counter: RwSignal<i32>) -> Reactive {
    Reactive::new(move || {
        vec![Box::new(Text::new(format!("{}", counter.get())).class("count")) as Box<dyn Widget>]
    })
}

/// Открыть панель или поднять уже открытую.
fn open_panel(
    counter: RwSignal<i32>,
    panel: RwSignal<Option<WindowHandle>>,
    status: RwSignal<String>,
) {
    if let Some(h) = panel.get_untracked().filter(|h| h.is_open()) {
        h.focus();
        return;
    }
    let state = use_signal(WindowState::default());
    let handle = open_window(
        WindowConfig::new("panel")
            .title("multi_window: панель")
            .size(440, 360)
            .min_size(320, 240)
            .frameless()
            .transparent(true)
            .with_window_state(state)
            .on_close(move || {
                eprintln!("[multi_window] панель закрыта");
                panel.set(None);
                status.set("панель закрыта".into());
            }),
        move |_| panel_root(counter, state),
    );
    eprintln!("[multi_window] открыта панель {}", handle.id());
    panel.set(Some(handle));
    status.set(format!("панель открыта (окно {})", handle.id()));
}

fn panel_root(counter: RwSignal<i32>, state: RwSignal<WindowState>) -> Box<dyn Widget> {
    let picked = use_signal(String::from("правый щелчок здесь — меню"));
    let typed = use_signal(String::new());

    let titlebar = Row::new()
        .class("titlebar")
        .cross_axis_alignment(CrossAxisAlignment::Center)
        .child(DecoratedBox::new().class("drag").child(
            WindowDragRegion::new().child(Text::new("Панель (второе окно)").class("tb-title")),
        ))
        .child(Reactive::new(move || {
            let w = state.get();
            vec![Box::new(
                SystemWindowControls::new(ControlsSide::Right)
                    .maximized(w.maximized)
                    .active(w.focused),
            ) as Box<dyn Widget>]
        }));

    let menu = ContextMenu::new()
        .items(vec![
            MenuItem::new("copy", "Копировать").shortcut("Ctrl+C"),
            MenuItem::new("paste", "Вставить").shortcut("Ctrl+V"),
            MenuItem::separator(),
            MenuItem::new("reset", "Сбросить счётчик"),
        ])
        .on_select(move |id| {
            if id == "reset" {
                counter.set(0);
            }
            picked.set(format!("выбрано: {id}"));
        })
        .child(
            DecoratedBox::new()
                .class("menu-area")
                .child(Reactive::new(move || {
                    vec![Box::new(Text::new(picked.get()).class("note")) as Box<dyn Widget>]
                })),
        );

    let body = Column::new()
        .class("body")
        .child(counter_view(counter))
        .child(
            Row::new()
                .gap(10.0)
                .child(Button::new("+1").on_click(move || counter.update(|c| *c += 1)))
                .child(Button::new("Закрыть панель").on_click(|| current_window().close())),
        )
        .child(
            TextField::new()
                .placeholder("Поле ввода второго окна")
                .on_change(move |s| typed.set(s.to_string())),
        )
        .child(Reactive::new(move || {
            vec![
                Box::new(Text::new(format!("введено: {}", typed.get())).class("note"))
                    as Box<dyn Widget>,
            ]
        }))
        .child(menu);

    let frame = DecoratedBox::new()
        .class("frame")
        .clip(true)
        .child(Column::new().child(titlebar).child(body));

    // Горячая клавиша окна: Ctrl+W закрывает панель.
    let keys = EventHook::new().child(frame).on_key_down(|key, mods| {
        if mods.ctrl && key == Key::W {
            current_window().close();
            KeyReply::Handled
        } else {
            KeyReply::Ignore
        }
    });

    Box::new(
        WindowResizeRegion::new()
            .over_content(true)
            .inset(5.0)
            .child(keys),
    )
}
