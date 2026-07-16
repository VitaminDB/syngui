//! Integration tests for effect lifecycle (W1 fix verification).

use syngui::testing::*;
use syngui::prelude::*;
use std::sync::{Arc, atomic::{AtomicU32, Ordering}};

#[test]
fn effect_created_in_reactive_is_tracked() {
    let trigger = use_signal(0u32);
    let effect_ran = Arc::new(AtomicU32::new(0));
    let er = effect_ran.clone();

    let widget = Column::new()
        .child(move || {
            let _t = trigger.get();
            let er = er.clone();
            use_effect(move || {
                er.fetch_add(1, Ordering::Relaxed);
            });
            Text::new("test")
        });

    let mut harness = TestHarness::new(Box::new(widget));
    harness.layout(800.0, 600.0);
    harness.rebuild();

    assert!(effect_ran.load(Ordering::Relaxed) >= 1,
        "Effect should have run at least once");
}

#[test]
fn context_provide_and_use() {
    #[derive(Clone)]
    struct TestCtx { value: i32 }

    provide_context(TestCtx { value: 42 });
    let ctx = use_context::<TestCtx>();
    assert_eq!(ctx.value, 42);

    syngui::context_provider::remove_context::<TestCtx>();
}

#[test]
fn try_use_context_returns_none_when_missing() {
    #[derive(Clone)]
    struct MissingCtx;

    let ctx = try_use_context::<MissingCtx>();
    assert!(ctx.is_none());
}
