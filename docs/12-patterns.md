# Common Patterns

## App Structure

### Typical App Layout

```rust
use syngui::prelude::*;
use syngui::widgets::*;
use syngui::mgui;

fn main() {
    let (theme, set_theme) = create_signal(false);

    App::new()
        .title("My App")
        .size(1280, 720)
        .with_theme_styles(LIGHT_MSS, DARK_MSS, theme)
        .run(move |_ctx| Box::new(build_app(set_theme)));
}

fn build_app(set_theme: SetSignal<bool>) -> impl Widget {
    let router = Arc::new(Mutex::new(Router::new(
        vec!["home".into(), "settings".into()],
        "home",
    )));
    let (sidebar_idx, set_sidebar_idx) = create_signal(0usize);

    // Provide shared context
    provide_context(AppCtx { /* ... */ });

    mgui! {
        Column::new().gap(0.0) => [
            build_header(set_theme),
            DecoratedBox::new().class("grow") => [
                Row::new().gap(0.0) => [
                    build_sidebar(sidebar_idx, set_sidebar_idx, router.clone()),
                    DecoratedBox::new().class("grow") => [
                        build_content(router.clone()),
                    ],
                ],
            ],
        ]
    }
}
```

### Context Pattern

Define a context struct with all shared state:

```rust
#[derive(Clone)]
struct AppCtx {
    router: Arc<Mutex<Router>>,
    theme: Signal<bool>,
    set_theme: SetSignal<bool>,
    data_version: Signal<u64>,
    set_data_version: SetSignal<u64>,
}

// In main:
provide_context(AppCtx { ... });

// In any component:
fn build_page() -> impl Widget {
    let ctx = use_context::<AppCtx>();
    let data_version = ctx.data_version;
    // ...
}
```

## Reactive Patterns

### Signal-driven text

```rust
let (name, set_name) = create_signal("World".to_string());

Column::new()
    .child(move || Text::new(&format!("Hello, {}!", name.get())))
    .child(TextField::new().on_change(move |text: &str| {
        set_name.set(text.to_string());
    }))
```

### Conditional rendering

```rust
let (show, set_show) = create_signal(false);

Column::new()
    .child(Button::new("Toggle").on_click(move || {
        let v = show.get_untracked();
        set_show.set(!v);
    }))
    .child(move || {
        if show.get() {
            Box::new(Text::new("Visible!")) as Box<dyn Widget>
        } else {
            Box::new(Container::new().width(0.0).height(0.0)) as Box<dyn Widget>
        }
    })
```

### List rendering

```rust
let (items, set_items) = create_signal(vec!["A", "B", "C"]);

Reactive::new(move || {
    items.get().iter().map(|item| {
        Box::new(Text::new(item)) as Box<dyn Widget>
    }).collect()
})
```

### Derived state with memo

```rust
let (items, _) = create_signal(vec![1, 2, 3, 4, 5]);
let total = create_memo(move || items.get().iter().sum::<i32>());
let count = create_memo(move || items.get().len());

Column::new()
    .child(move || Text::new(&format!("Count: {}", count.get())))
    .child(move || Text::new(&format!("Total: {}", total.get())))
```

## Timer / Interval Pattern

```rust
let (seconds, set_seconds) = create_signal(0u32);

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
            set_seconds.set(val); // Thread-safe!
        }
    });

    Some(Box::new(move || {
        running.store(false, Ordering::Relaxed);
    }) as Box<dyn Fn()>)
});
```

**Key points:**
- Use `Arc<AtomicU32>` for thread-local counter (can't read signals from background thread)
- Use `set_signal.set()` (thread-safe, auto-marshals) — NOT `update()` (main-thread only)
- Cleanup stops the thread on re-run or dispose

## Async Data Fetching

```rust
let (profile, set_profile) = create_signal(Some("default".to_string()));

let (data, loading) = use_async(move || {
    let p = profile.get(); // Auto-tracks
    async move {
        match p {
            Some(name) => fetch_data(&name).await,
            None => Vec::new(),
        }
    }
});

Column::new()
    .child(move || {
        if loading.get() {
            Box::new(CircularProgress::new()) as Box<dyn Widget>
        } else if let Some(items) = data.get() {
            Box::new(ListView::new(
                items.iter().map(|i| ListItem::new(i)).collect()
            )) as Box<dyn Widget>
        } else {
            Box::new(Text::new("No data"))  as Box<dyn Widget>
        }
    })
```

## Dialog Pattern

```rust
let (dialog_open, set_dialog_open) = create_signal(false);

Stack::new()
    .child(
        Button::new("Delete")
            .danger()
            .on_click(move || set_dialog_open.set(true))
    )
    .child(
        Dialog::new("Confirm")
            .body("Delete this item?")
            .is_open(dialog_open)
            .action(DialogAction::new("Cancel", move || set_dialog_open.set(false)))
            .action(DialogAction::new("Delete", move || {
                // perform delete
                set_dialog_open.set(false);
            }).primary())
            .on_close(move || set_dialog_open.set(false))
    )
```

## Theme Switching

```rust
let (dark, set_dark) = create_signal(false);

App::new()
    .with_theme_styles(LIGHT_MSS, DARK_MSS, dark)
    .run(move |_| Box::new(
        Column::new()
            .child(Toggle::new().on(false).on_change(move |on| {
                set_dark.set(on);
            }))
            .child(Text::new("Content"))
    ));
```

## Navigation with Router

```rust
let route_keys = vec!["home", "settings", "about"];
let router = Arc::new(Mutex::new(Router::new(
    route_keys.iter().map(|s| s.to_string()).collect(),
    "home",
)));

// Sidebar navigation
Sidebar::new()
    .item("Home")
    .item("Settings")
    .item("About")
    .on_select({
        let r = router.clone();
        move |idx| {
            if let Some(key) = route_keys.get(idx) {
                r.lock().unwrap().navigate(key);
            }
        }
    });

// Content
RouterView::new(router)
    .route("home", || Box::new(home_page()))
    .route("settings", || Box::new(settings_page()))
    .route("about", || Box::new(about_page()))
```

## Form Pattern

```rust
let (name, set_name) = create_signal(String::new());
let (email, set_email) = create_signal(String::new());
let (agreed, set_agreed) = create_signal(false);

mgui! {
    Column::new().gap(16.0) => [
        TextField::new()
            .placeholder("Name")
            .on_change(move |t: &str| set_name.set(t.to_string())),
        TextField::new()
            .placeholder("Email")
            .on_change(move |t: &str| set_email.set(t.to_string())),
        Checkbox::new()
            .label("I agree to terms")
            .on_change(move |v| set_agreed.set(v)),
        Button::new("Submit")
            .primary()
            .on_click(move || {
                let n = name.get_untracked();
                let e = email.get_untracked();
                let a = agreed.get_untracked();
                if !n.is_empty() && !e.is_empty() && a {
                    submit_form(&n, &e);
                }
            }),
    ]
}
```

## Macros

### mgui! — Declarative Tree

```rust
mgui! {
    Column::new().gap(12.0) => [
        Text::new("Title").bold(),
        Row::new().gap(8.0) => [
            Button::new("A"),
            Button::new("B"),
        ],
        move || Text::new(&format!("{}", sig.get())),
    ]
}
```

### children! — Batch Boxing

```rust
Row::new().children(children![
    Text::new("One"),
    Text::new("Two"),
    Text::new("Three"),
])
// Equivalent to: vec![Box::new(...), Box::new(...), Box::new(...)]
```
