//! Integration tests for signals and reactive rebuilds.

use syngui::prelude::*;

#[test]
fn signal_creation_and_read() {
    let count = use_signal(42i32);
    assert_eq!(count.get(), 42);

    count.set(100);
    assert_eq!(count.get(), 100);
}

#[test]
fn memo_derives_from_signal() {
    let a = use_signal(3i32);
    let b = use_signal(4i32);
    let sum = create_memo(move || a.get() + b.get());

    assert_eq!(sum.get(), 7);

    a.set(10);
    assert_eq!(sum.get(), 14);

    b.set(20);
    assert_eq!(sum.get(), 30);
}

#[test]
fn effect_runs_on_signal_change() {
    use std::sync::{Arc, atomic::{AtomicU32, Ordering}};

    let count = use_signal(0u32);
    let run_count = Arc::new(AtomicU32::new(0));
    let rc = run_count.clone();

    use_effect(move || {
        let _ = count.get();
        rc.fetch_add(1, Ordering::Relaxed);
    });

    assert_eq!(run_count.load(Ordering::Relaxed), 1);

    count.set(1);
    syngui::signal::drain_and_run_effects();
    assert_eq!(run_count.load(Ordering::Relaxed), 2);

    count.set(2);
    syngui::signal::drain_and_run_effects();
    assert_eq!(run_count.load(Ordering::Relaxed), 3);
}

#[test]
fn effect_cleanup_runs_on_dispose() {
    use std::sync::{Arc, atomic::{AtomicBool, Ordering}};

    let cleanup_ran = Arc::new(AtomicBool::new(false));
    let cr = cleanup_ran.clone();

    let effect_id = use_effect_with_cleanup(move || {
        let cr = cr.clone();
        Some(Box::new(move || {
            cr.store(true, Ordering::Relaxed);
        }) as Box<dyn Fn()>)
    });

    assert!(!cleanup_ran.load(Ordering::Relaxed));

    dispose_effect(effect_id);
    assert!(cleanup_ran.load(Ordering::Relaxed));
}

/// Регрессия: замыкание `update()` читает и пишет другие сигналы.
/// Раньше это роняло приложение — `update` держал `RefCell` runtime'а
/// заимствованным, и вложенный `get()` паниковал «RefCell already borrowed».
#[test]
fn update_closure_may_touch_other_signals() {
    let source = use_signal(7i32);
    let mirror = use_signal(0i32);
    let items = use_signal(Vec::<i32>::new());

    items.update(|v| {
        v.push(source.get());
        mirror.set(source.get() * 2);
        v.push(mirror.get_untracked());
    });

    assert_eq!(items.get(), vec![7, 14]);
    assert_eq!(mirror.get(), 14);
}

/// Регрессия: вложенный `update()` другого сигнала внутри `update()`.
#[test]
fn update_closure_may_update_another_signal() {
    let outer = use_signal(String::new());
    let inner = use_signal(String::new());

    outer.update(|s| {
        inner.update(|i| i.push_str("inner"));
        s.push_str(&inner.get_untracked());
    });

    assert_eq!(outer.get(), "inner");
}

/// Регрессия: `tr!` внутри `update()`. `i18n::tr` подписывается на сигнал
/// ревизии языка, то есть делает ровно то чтение, что валило Synthos при
/// SHA-256-проверке скачанного файла в HuggingFace.
#[test]
fn update_closure_may_translate() {
    let log = use_signal(Vec::<String>::new());

    log.update(|v| {
        v.push(syngui::tr!("some.missing.key"));
    });

    assert_eq!(log.get(), vec!["some.missing.key".to_string()]);
}

/// Значение возвращается в слот, даже если замыкание запаниковало, —
/// иначе сигнал остался бы с внутренним плейсхолдером навсегда.
#[test]
fn update_restores_value_after_panic_in_closure() {
    let value = use_signal(vec![1i32, 2, 3]);

    let result = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
        value.update(|v| {
            v.push(4);
            panic!("boom");
        });
    }));

    assert!(result.is_err());
    assert_eq!(value.get(), vec![1, 2, 3, 4]);
}

/// Регрессия: нотификатор, который сам читает сигналы. `request_redraw`
/// теперь вызывается после снятия заимствования runtime'а.
#[test]
fn notifier_may_read_signals_during_redraw() {
    use std::sync::{
        atomic::{AtomicU32, Ordering},
        Arc,
    };

    struct ReadingNotifier {
        watched: RwSignal<i32>,
        seen: AtomicU32,
    }

    impl syngui::signal::RedrawNotifier for ReadingNotifier {
        fn request_redraw(&self) {
            let _ = self.watched.get_untracked();
            self.seen.fetch_add(1, Ordering::Relaxed);
        }
    }

    let watched = use_signal(1i32);
    let notifier = Arc::new(ReadingNotifier { watched, seen: AtomicU32::new(0) });
    syngui::signal::set_notifier(notifier.clone());

    let target = use_signal(Vec::<i32>::new());
    target.update(|v| v.push(1));
    watched.set(2);

    assert_eq!(notifier.seen.load(Ordering::Relaxed), 2);
    syngui::signal::clear_window();
}
