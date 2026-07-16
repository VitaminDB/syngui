//! Regression tests: cleanup closures must run *outside* the active
//! `RUNTIME.borrow_mut()` so that user code can re-enter the signal runtime
//! (e.g. `signal.set()`, `dispose_effect()`, `cleanup_element()`).
//! See plan `encapsulated-painting-abelson.md`.

use syngui::prelude::*;
use syngui::signal::{begin_element_scope, end_element_scope, cleanup_element};
use syngui::widget::ElementId;

#[test]
fn dispose_cleanup_can_mutate_signal_no_panic() {
    let flag = use_signal(true);

    let eid = use_effect_with_cleanup(move || {
        Some(Box::new(move || {
            // Re-entrant call into the runtime — panics before the fix.
            flag.set(false);
        }) as Box<dyn Fn()>)
    });

    dispose_effect(eid);
    assert_eq!(flag.get_untracked(), false);
}

#[test]
fn dispose_cleanup_can_dispose_another_effect() {
    use std::sync::{Arc, atomic::{AtomicBool, Ordering}};

    let b_cleanup_ran = Arc::new(AtomicBool::new(false));
    let bc = b_cleanup_ran.clone();
    let b_id = use_effect_with_cleanup(move || {
        let bc = bc.clone();
        Some(Box::new(move || { bc.store(true, Ordering::Relaxed); }) as Box<dyn Fn()>)
    });

    let a_id = use_effect_with_cleanup(move || {
        Some(Box::new(move || {
            // Re-entrant dispose from inside a cleanup.
            dispose_effect(b_id);
        }) as Box<dyn Fn()>)
    });

    dispose_effect(a_id);
    assert!(b_cleanup_ran.load(Ordering::Relaxed));
}

#[test]
fn re_run_cleanup_can_mutate_signal_no_panic() {
    use std::sync::{Arc, atomic::{AtomicU32, Ordering}};

    let trigger = use_signal(0u32);
    let side = use_signal(0u32);
    let cleanup_calls = Arc::new(AtomicU32::new(0));
    let cc = cleanup_calls.clone();

    let _eid = use_effect_with_cleanup(move || {
        // Subscribe to `trigger` so the effect re-runs when it changes.
        let _ = trigger.get();
        let cc = cc.clone();
        Some(Box::new(move || {
            // Re-entrant signal mutation during effect re-run cleanup.
            cc.fetch_add(1, Ordering::Relaxed);
            side.set(side.get_untracked() + 1);
        }) as Box<dyn Fn()>)
    });

    // Initial run: cleanup not yet executed.
    assert_eq!(cleanup_calls.load(Ordering::Relaxed), 0);
    assert_eq!(side.get_untracked(), 0);

    trigger.set(1);
    syngui::signal::drain_and_run_effects();

    assert_eq!(cleanup_calls.load(Ordering::Relaxed), 1);
    assert_eq!(side.get_untracked(), 1);
}

#[test]
fn cleanup_element_runs_cleanup_outside_borrow() {
    let flag = use_signal(true);
    // Synthetic element id; cleanup_element only needs ownership tracking.
    let elem_id = ElementId(0xDEAD_BEEF);

    begin_element_scope(elem_id);
    let _eid = use_effect_with_cleanup(move || {
        Some(Box::new(move || {
            // Re-entrant signal mutation from element-owned effect cleanup.
            flag.set(false);
        }) as Box<dyn Fn()>)
    });
    end_element_scope();

    cleanup_element(elem_id);
    assert_eq!(flag.get_untracked(), false);
}
