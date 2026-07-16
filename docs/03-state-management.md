# State Management

SYNGUI provides a reactive signal system inspired by SolidJS, plus context providers for dependency injection.

## Signals

Fine-grained reactive state. Both `Signal<T>` and `SetSignal<T>` are `Copy` — cheap to pass around.

```rust
let (count, set_count) = create_signal(0i32);
```

### Signal\<T\> (read handle)

```rust
signal.get()            // Read value, auto-subscribe if inside Reactive
signal.get_untracked()  // Read without subscribing (for event handlers)
```

### SetSignal\<T\> (write handle)

`Send + Sync` — safe to call from any thread.

```rust
set_signal.set(new_value)       // Set if different (PartialEq), thread-safe
set_signal.set_always(new_value) // Set unconditionally, thread-safe
set_signal.update(|val| *val += 1) // Modify in-place (main-thread only!)
```

**Thread safety**: `set()` and `set_always()` auto-marshal to the GUI thread via `run_on_main_thread()` when called from a background thread. `update()` is main-thread only (debug_assert).

### Auto-subscription

When `signal.get()` is called inside a `Reactive` closure or an effect, the calling element/effect automatically subscribes to future changes:

```rust
// This closure re-runs when `count` changes
move || {
    let c = count.get(); // auto-subscribes
    Text::new(&format!("Count: {c}"))
}
```

## Memo (Computed Values)

Derived reactive values that auto-track dependencies:

```rust
let doubled = create_memo(move || count.get() * 2);

// Usage:
doubled.get() // Returns computed value, auto-subscribes
```

## Effects

Side-effect computations that auto-track signal dependencies and re-run when dependencies change.

### use_effect

```rust
use_effect(move || {
    let query = search.get(); // auto-tracks
    println!("Search query: {query}");
});
```

### use_effect_with_cleanup

Returns an optional cleanup function that runs before re-execution and on disposal:

```rust
use_effect_with_cleanup(move || {
    let running = Arc::new(AtomicBool::new(true));
    let flag = running.clone();

    std::thread::spawn(move || {
        while flag.load(Ordering::Relaxed) {
            std::thread::sleep(Duration::from_secs(1));
            if !flag.load(Ordering::Relaxed) { break; }
            set_seconds.set(tick);
        }
    });

    // Cleanup: stop thread before re-run or dispose
    Some(Box::new(move || {
        running.store(false, Ordering::Relaxed);
    }) as Box<dyn Fn()>)
});
```

### dispose_effect

Manual cleanup:

```rust
let effect_id = use_effect(move || { ... });
dispose_effect(effect_id); // Deactivate, run cleanup, unsubscribe
```

### Low-level aliases

`create_effect` = `use_effect`, `create_effect_with_cleanup` = `use_effect_with_cleanup`. The `use_*` names follow the hook naming convention (`use_async`, `use_context`).

## Reactive Widget

The `Reactive` widget rebuilds its children when subscribed signals change:

```rust
// Explicit
Reactive::new(move || {
    let items = items_signal.get();
    items.iter().map(|item| {
        Box::new(Text::new(item)) as Box<dyn Widget>
    }).collect()
})

// Implicit — closures auto-wrap in Reactive
Column::new()
    .child(move || {
        Text::new(&format!("Value: {}", value.get()))
    })
```

Any closure `Fn() -> impl Widget` passed to `.child()` is automatically wrapped in a `Reactive`.

## use_async (requires `tokio` feature)

Async data fetching hook. Auto-refetches when signal dependencies change, aborts previous in-flight tasks.

```rust
let (data, loading) = use_async(move || {
    let profile = selected_profile.get(); // auto-tracks
    async move {
        fetch_data(&profile).await
    }
});

// Usage:
// data.get()    → Option<T> (None until first result, keeps previous during reload)
// loading.get() → bool
```

**Signature:**
```rust
pub fn use_async<T, F, Fut>(factory: F) -> (Signal<Option<T>>, Signal<bool>)
where
    T: Clone + Send + PartialEq + 'static,
    F: Fn() -> Fut + Send + Sync + 'static,
    Fut: Future<Output = T> + Send + 'static,
```

## Context Provider

Thread-local type-map for avoiding prop drilling. Store values at app initialization, access from anywhere on the GUI thread.

```rust
// Define context type
#[derive(Clone)]
struct AppCtx {
    theme: Signal<bool>,
    set_theme: SetSignal<bool>,
    router: Arc<Mutex<Router>>,
}

// Provide once (in main/build_app)
provide_context(AppCtx {
    theme,
    set_theme,
    router: router.clone(),
});

// Use anywhere on GUI thread
let ctx = use_context::<AppCtx>();
let theme = ctx.theme.get();

// Safe variant (returns Option)
let ctx = try_use_context::<AppCtx>();

// Remove
remove_context::<AppCtx>();
```

## Async Runtime Utilities

```rust
// Run closure on GUI thread from any thread
run_on_main_thread(move || {
    set_signal.update(|v| *v += 1);
});

// Spawn async task (requires tokio feature)
spawn(async move {
    let result = fetch_data().await;
    run_on_main_thread(move || {
        set_data.set(result);
    });
});

// Get cloneable sender for main-thread callbacks
let sender = main_thread_sender();
```

## Summary Table

| Primitive | Purpose | Thread-safe |
|-----------|---------|-------------|
| `create_signal(val)` | Reactive state | Read: main only. Write: `set`/`set_always` any thread |
| `create_memo(\|\| ...)` | Computed value | Main thread |
| `use_effect(\|\| ...)` | Side effect | Main thread |
| `use_effect_with_cleanup(\|\| ...)` | Side effect + cleanup | Main thread |
| `use_async(\|\| async { ... })` | Async data fetch | Factory: main. Future: any |
| `provide_context(val)` | Store context | Main thread |
| `use_context::<T>()` | Retrieve context | Main thread |
| `run_on_main_thread(\|\| ...)` | Marshal to GUI thread | Any thread |
