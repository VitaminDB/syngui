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
