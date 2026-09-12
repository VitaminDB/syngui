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
    use std::sync::{
        atomic::{AtomicU32, Ordering},
        Arc,
    };

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
    use std::sync::{
        atomic::{AtomicBool, Ordering},
        Arc,
    };

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
    let notifier = Arc::new(ReadingNotifier {
        watched,
        seen: AtomicU32::new(0),
    });
    syngui::signal::set_notifier(notifier.clone());

    let target = use_signal(Vec::<i32>::new());
    target.update(|v| v.push(1));
    watched.set(2);

    assert_eq!(notifier.seen.load(Ordering::Relaxed), 2);
    syngui::signal::clear_window();
}

/// Значение со счётчиком клонов: `with` обязан читать по ссылке.
struct CloneCounted {
    clones: std::rc::Rc<std::cell::Cell<u32>>,
    payload: Vec<i32>,
}

impl Clone for CloneCounted {
    fn clone(&self) -> Self {
        self.clones.set(self.clones.get() + 1);
        Self {
            clones: self.clones.clone(),
            payload: self.payload.clone(),
        }
    }
}

#[test]
fn with_reads_by_reference_without_clone() {
    let clones = std::rc::Rc::new(std::cell::Cell::new(0u32));
    let value = use_signal(CloneCounted {
        clones: clones.clone(),
        payload: vec![1, 2, 3],
    });

    assert_eq!(value.with(|v| v.payload.len()), 3);
    assert_eq!(value.with_untracked(|v| v.payload.iter().sum::<i32>()), 6);
    assert_eq!(clones.get(), 0, "with/with_untracked не клонируют");

    let _ = value.get();
    assert_eq!(clones.get(), 1, "а get клонирует — счётчик рабочий");
}

/// `with` подписывает строящийся элемент и эффект так же, как `get`.
#[test]
fn with_tracks_element_and_effect() {
    use std::sync::{
        atomic::{AtomicU32, Ordering},
        Arc,
    };
    use syngui::signal::{begin_tracking, clear_element_dirty, end_tracking, is_element_dirty};
    use syngui::widget::ElementId;

    let items = use_signal(vec![1i32, 2, 3]);
    let elem = ElementId::new();
    begin_tracking(elem);
    assert_eq!(items.with(|v| v.len()), 3);
    end_tracking();
    assert!(!is_element_dirty(elem));
    items.update(|v| v.push(4));
    assert!(is_element_dirty(elem), "with обязан подписать элемент");
    clear_element_dirty(elem);

    let runs = Arc::new(AtomicU32::new(0));
    let r = runs.clone();
    use_effect(move || {
        items.with(|v| assert!(!v.is_empty()));
        r.fetch_add(1, Ordering::Relaxed);
    });
    assert_eq!(runs.load(Ordering::Relaxed), 1);
    items.update(|v| v.push(5));
    syngui::signal::drain_and_run_effects();
    assert_eq!(runs.load(Ordering::Relaxed), 2, "with обязан подписать эффект");
}

#[test]
fn with_untracked_does_not_track() {
    use std::sync::{
        atomic::{AtomicU32, Ordering},
        Arc,
    };
    use syngui::signal::{begin_tracking, end_tracking, is_element_dirty};
    use syngui::widget::ElementId;

    let items = use_signal(vec![1i32]);
    let elem = ElementId::new();
    begin_tracking(elem);
    assert_eq!(items.with_untracked(|v| v.len()), 1);
    end_tracking();
    items.update(|v| v.push(2));
    assert!(
        !is_element_dirty(elem),
        "with_untracked не подписывает элемент"
    );

    let runs = Arc::new(AtomicU32::new(0));
    let r = runs.clone();
    use_effect(move || {
        items.with_untracked(|v| assert!(!v.is_empty()));
        r.fetch_add(1, Ordering::Relaxed);
    });
    items.update(|v| v.push(3));
    syngui::signal::drain_and_run_effects();
    assert_eq!(
        runs.load(Ordering::Relaxed),
        1,
        "with_untracked не подписывает эффект"
    );
}

/// Внутри `with` можно читать и менять другие сигналы и переводить строки:
/// runtime на это время не заимствован (как в `update`).
#[test]
fn with_closure_may_touch_other_signals() {
    let source = use_signal(vec![String::from("a"), String::from("b")]);
    let total = use_signal(0usize);
    let log = use_signal(Vec::<String>::new());

    let n = source.with(|v| {
        total.update(|t| *t += v.len());
        log.set(v.clone());
        log.update(|l| l.push(syngui::tr!("some.missing.key")));
        let inner = total.with(|t| *t);
        inner + log.get_untracked().len()
    });

    assert_eq!(n, 5);
    assert_eq!(total.get(), 2);
    assert_eq!(source.get(), vec!["a".to_string(), "b".to_string()]);
}

/// Чтение этого же сигнала внутри `with` — понятная паника, а не
/// «RefCell already borrowed».
#[test]
#[should_panic(expected = "own update()/with()")]
fn with_panics_on_reading_same_signal_inside() {
    let value = use_signal(vec![1i32]);
    value.with(|_| value.get_untracked());
}

/// После такой паники значение на месте: guard вернул его в слот.
#[test]
fn with_restores_value_after_panic_in_closure() {
    let value = use_signal(vec![1i32, 2]);

    let result = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
        value.with(|_| value.with_untracked(|v| v.len()))
    }));

    assert!(result.is_err());
    assert_eq!(value.get(), vec![1, 2]);
}

/// Вложенный `update` этого же сигнала — та же понятная паника (раньше
/// сообщение было «signal type mismatch»).
#[test]
#[should_panic(expected = "own update()/with()")]
fn update_panics_on_updating_same_signal_inside() {
    let value = use_signal(0i32);
    value.update(|_| value.update(|v| *v += 1));
}

/// `set_always` этого же сигнала изнутри `with` не теряется: guard не
/// затирает новое значение старым.
#[test]
fn set_always_inside_with_keeps_new_value() {
    let value = use_signal(1i32);
    value.with(|v| {
        assert_eq!(*v, 1);
        value.set_always(2);
    });
    assert_eq!(value.get(), 2);
}

/// Пересборка снимает старые подписки по обратному индексу: элемент,
/// переставший читать сигнал, больше им не будится, а индекс совпадает с
/// тем, что лежит в слотах. Раньше отписка перебирала все слоты приложения.
#[test]
fn rebuild_drops_stale_subscriptions_and_index_matches_slots() {
    use syngui::signal::{
        begin_tracking, cleanup_element, clear_element_dirty, end_tracking, is_element_dirty,
        subscription_counts,
    };
    use syngui::widget::ElementId;

    let a = use_signal(1i32);
    let b = use_signal(2i32);
    let elem = ElementId::new();

    begin_tracking(elem);
    let _ = a.get();
    let _ = b.get();
    end_tracking();
    assert_eq!(subscription_counts(elem), (2, 2));

    // Следующая сборка читает только b.
    begin_tracking(elem);
    let _ = b.get();
    end_tracking();
    assert_eq!(subscription_counts(elem), (1, 1));

    clear_element_dirty(elem);
    a.set(10);
    assert!(!is_element_dirty(elem), "элемент больше не читает a");
    b.set(20);
    assert!(is_element_dirty(elem), "подписка на b осталась");

    cleanup_element(elem);
    assert_eq!(subscription_counts(elem), (0, 0));
    clear_element_dirty(elem);
    b.set(30);
    assert!(!is_element_dirty(elem), "удалённый элемент не будится");
}
