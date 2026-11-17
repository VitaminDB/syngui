//! Окна ОС во время работы (`open_window` / `close_window` / `WindowHandle`)
//! без дисплея: заявки выполняет `testing::WindowHost` тем же кодом сборки и
//! освобождения окна, что и цикл событий.

use std::cell::Cell;
use std::rc::Rc;
use std::sync::atomic::{AtomicUsize, Ordering::SeqCst};
use std::sync::Arc;

use syngui::app::{
    close_window, current_window, open_window, open_windows, WindowConfig, WindowHandle,
};
use syngui::core::Point;
use syngui::input::{Event, MouseButton};
use syngui::prelude::*;
use syngui::signal::drain_and_run_effects;
use syngui::testing::*;
use syngui::widgets::overlay::WindowControl;
use syngui::window::WindowState;

/// Ручка годна сразу, окно появляется проходом цикла, закрытие отмечает её
/// закрытой и зовёт `on_close` один раз.
#[test]
fn handle_lifecycle() {
    let mut host = WindowHost::new();
    let closed_calls = Rc::new(Cell::new(0));
    let calls = closed_calls.clone();
    let handle = open_window(
        WindowConfig::new("panel")
            .title("Панель")
            .on_close(move || calls.set(calls.get() + 1)),
        |_| Box::new(Text::new("второе окно")),
    );
    assert!(!handle.is_main());
    assert!(handle.is_open(), "ручка открывающегося окна — открыта");
    assert!(!handle.closed().get_untracked());
    assert_eq!(handle.title().as_deref(), Some("Панель"));
    assert!(
        host.window(handle).is_none(),
        "до прохода цикла окна ещё нет"
    );

    host.pump();
    assert_eq!(host.open(), vec![handle]);
    assert!(open_windows().contains(&handle));
    let tree = host.window(handle).expect("окно открылось");
    assert_eq!(tree.find_by_type_name("Text").len(), 1);

    handle.set_title("Панель — проект");
    assert_eq!(handle.title().as_deref(), Some("Панель — проект"));
    handle.focus();
    host.pump();
    assert_eq!(host.focus_requests(), &[handle]);

    close_window(handle);
    host.pump();
    assert!(host.open().is_empty());
    assert!(!handle.is_open());
    assert!(handle.closed().get_untracked(), "сигнал closed не взведён");
    assert_eq!(closed_calls.get(), 1);
    assert!(!open_windows().contains(&handle));

    // Повторное закрытие — ничего.
    close_window(handle);
    handle.close();
    host.pump();
    assert_eq!(closed_calls.get(), 1);
}

/// Закрытие главного окна — заявка циклу (там: on_close_request, трей,
/// выход), а не освобождение.
#[test]
fn closing_main_is_forwarded() {
    let mut host = WindowHost::new();
    close_window(WindowHandle::MAIN);
    host.pump();
    assert_eq!(host.main_close_requests(), 1);
    assert!(!WindowHandle::MAIN.closed().get_untracked());
}

/// Эффекты окна — и созданные прямо в сборщике, и внутри его `Reactive` —
/// перестают работать после закрытия окна; подписки элементов сняты.
#[test]
fn closed_window_releases_effects() {
    let mut host = WindowHost::new();
    let counter = use_signal(0u32);
    // Reactive требует Send + Sync — счётчики атомарные.
    let top_runs = Arc::new(AtomicUsize::new(0));
    let inner_runs = Arc::new(AtomicUsize::new(0));
    let reactive_builds = Arc::new(AtomicUsize::new(0));

    let (t, i, b) = (
        top_runs.clone(),
        inner_runs.clone(),
        reactive_builds.clone(),
    );
    let handle = open_window(WindowConfig::new("effects"), move |_| {
        // Эффект прямо в сборщике окна.
        create_effect(move || {
            counter.get();
            t.fetch_add(1, SeqCst);
        });
        let i = i.clone();
        let b = b.clone();
        Box::new(Reactive::new(move || {
            b.fetch_add(1, SeqCst);
            let i = i.clone();
            // Эффект внутри Reactive — его владелец элемент.
            create_effect(move || {
                counter.get();
                i.fetch_add(1, SeqCst);
            });
            vec![Box::new(Text::new(format!("{}", counter.get()))) as Box<dyn Widget>]
        }))
    });
    host.pump();
    host.window(handle).unwrap().rebuild();
    let (top0, inner0) = (top_runs.load(SeqCst), inner_runs.load(SeqCst));
    assert!(top0 >= 1 && inner0 >= 1, "эффекты окна не запустились");

    counter.set(1);
    drain_and_run_effects();
    assert!(
        top_runs.load(SeqCst) > top0,
        "эффект сборщика не реагирует, пока окно открыто"
    );
    assert!(
        inner_runs.load(SeqCst) > inner0,
        "эффект Reactive не реагирует, пока окно открыто"
    );

    close_window(handle);
    host.pump();
    let (top1, inner1, builds1) = (
        top_runs.load(SeqCst),
        inner_runs.load(SeqCst),
        reactive_builds.load(SeqCst),
    );
    counter.set(2);
    drain_and_run_effects();
    counter.set(3);
    drain_and_run_effects();
    assert_eq!(
        top_runs.load(SeqCst),
        top1,
        "эффект сборщика пережил закрытие окна"
    );
    assert_eq!(
        inner_runs.load(SeqCst),
        inner1,
        "эффект Reactive пережил закрытие окна"
    );
    assert_eq!(reactive_builds.load(SeqCst), builds1);
    assert!(
        !syngui::signal::has_dirty_elements(),
        "элементы закрытого окна остались подписанными на сигнал"
    );
}

/// Во время сборки окна «текущее окно» — оно само; вне событий — главное.
#[test]
fn current_window_during_build() {
    let mut host = WindowHost::new();
    let seen = Rc::new(Cell::new(None));
    let s = seen.clone();
    let handle = open_window(WindowConfig::new("cur"), move |_| {
        s.set(Some(current_window()));
        Box::new(Text::new("x"))
    });
    host.pump();
    assert_eq!(seen.get(), Some(handle));
    assert_eq!(current_window(), WindowHandle::MAIN);
}

/// Сигнал состояния окна: свой из `WindowConfig::with_window_state` или
/// заведённый `WindowHandle::state`.
#[test]
fn window_state_signal() {
    let sig = use_signal(WindowState::default());
    let a = open_window(WindowConfig::new("a").with_window_state(sig), |_| {
        Box::new(Text::new("a"))
    });
    assert!(a.state() == sig);
    let b = open_window(WindowConfig::new("b"), |_| Box::new(Text::new("b")));
    let sb = b.state();
    assert!(b.state() == sb, "state() отдаёт один и тот же сигнал");
    assert!(sb != sig);
    let _ = WindowHost::new().pump();
}

/// Кнопка закрытия (`WindowControl::close`) во втором окне закрывает его, а
/// не главное; проверка `on_close_request` может закрытие отменить.
#[test]
fn close_request_routes_to_own_window() {
    let mut host = WindowHost::new();
    let allow = Rc::new(Cell::new(false));
    let a = allow.clone();
    let closed = Rc::new(Cell::new(false));
    let c = closed.clone();
    let make = || {
        Box::new(
            Row::new()
                .main_axis_alignment(MainAxisAlignment::End)
                .child(WindowControl::close().child(Button::new("✕"))),
        ) as Box<dyn Widget>
    };
    let main_like = open_window(WindowConfig::new("first"), move |_| make());
    let second = open_window(
        WindowConfig::new("second")
            .on_close_request(move || a.get())
            .on_close(move || c.set(true)),
        move |_| make(),
    );
    host.pump();

    let click = |h: &mut TestHarness| {
        h.layout(400.0, 100.0);
        let btn = h.find_by_type_name("Button")[0];
        let r = h.element_bounds(btn);
        let at = Point::new(
            r.origin.x + r.size.width / 2.0,
            r.origin.y + r.size.height / 2.0,
        );
        h.send_event(&Event::MouseDown {
            button: MouseButton::Left,
            position: at,
        });
        h.send_event(&Event::MouseUp {
            button: MouseButton::Left,
            position: at,
        });
    };

    click(host.window(second).unwrap());
    assert!(host.window(second).unwrap().tree.window_close_request);
    assert!(
        !host.window(main_like).unwrap().tree.window_close_request,
        "запрос закрытия попал в чужое окно"
    );

    // Проверка отменила закрытие.
    host.process_window_requests();
    assert!(second.is_open());
    assert!(!closed.get());

    allow.set(true);
    click(host.window(second).unwrap());
    host.process_window_requests();
    assert!(!second.is_open());
    assert!(closed.get());
    assert!(main_like.is_open(), "закрылось не то окно");
    assert_eq!(host.open(), vec![main_like]);
    close_window(main_like);
    host.pump();
}
