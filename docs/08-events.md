# Events & Input

## Event Enum

All input events flow through the `Event` enum:

```rust
enum Event {
    // Mouse
    MouseMove(Point),
    MouseDown { button: MouseButton, position: Point },
    MouseUp { button: MouseButton, position: Point },
    MouseWheel { delta: f32, delta_x: f32, position: Point },
    DoubleClick { button: MouseButton, position: Point },

    // Keyboard
    KeyDown(Key),
    KeyUp(Key),
    CharInput(char),

    // Touch
    TouchStart { id: u64, position: Point },
    TouchMove { id: u64, position: Point },
    TouchEnd { id: u64, position: Point },

    // Focus
    FocusGained,
    FocusLost,

    // Window
    Resized { width: u32, height: u32 },
    CloseRequested,

    // Drag & Drop
    DragStart { position: Point, data: DragData },
    DragMove { position: Point, data: DragData },
    DragEnter { position: Point, data: DragData },
    DragLeave,
    Drop { position: Point, data: DragData },
    DragEnd { cancelled: bool },

    // Other
    BackPressed,            // Android back / Escape
    Custom(String),
}
```

### Event Methods

```rust
event.position()            // Option<Point> — position if applicable
event.with_offset(dx, dy)   // New event with offset positions
```

## EventResult

Elements return `EventResult` from `handle_event()`:

```rust
enum EventResult {
    Ignored,    // Event not consumed, propagate to parent
    Handled,    // Event consumed, stop propagation
    Captured,   // Consumed + capture future mouse events
}
```

## MouseButton

```rust
enum MouseButton {
    Left,
    Right,
    Middle,
    Back,
    Forward,
    Other(u16),
}
```

## Key

```rust
enum Key {
    A..Z,                   // Letter keys
    Num0..Num9,             // Number keys
    F1..F12,                // Function keys
    Escape, Enter, Tab,
    Backspace, Delete, Insert,
    Home, End, PageUp, PageDown,
    Left, Right, Up, Down,  // Arrow keys
    Shift, Ctrl, Alt, Meta,
    Space,
    Unknown(u32),
}
```

### Function keys on web

On wasm32 winit calls `preventDefault()` for every key delivered to the canvas,
which silently disables browser shortcuts such as F5 (reload), F11 (browser
fullscreen) and F12 (devtools). `syngui::input::FunctionKeys` declares which of
F1–F12 the application captures; every other function key is stopped by a
capture-phase listener on `window` before it reaches the canvas, so the browser
performs its default action.

```rust
use syngui::input::{FunctionKeys, Key};

App::new()
    .capture_function_keys(FunctionKeys::of(&[Key::F2, Key::F11]))  // default: NONE
    .run(...);

// Change at runtime:
syngui::input::set_captured_function_keys(FunctionKeys::ALL);
```

Captured keys arrive as regular `Event::KeyDown`; F11 additionally toggles
canvas fullscreen and F12 the syngui devtools. On native and Android the
application always receives all keys and the setting has no effect.

## Event Dispatch

Events flow through the element tree via depth-first traversal:

1. **Overlay stack first** — topmost overlay receives events first
2. **Normal DFS** — root → children, deepest element hit-tested first
3. **Bubbling** — if child returns `Ignored`, parent gets the event

### Capture

When an element returns `EventResult::Captured`, subsequent mouse events are delivered directly to that element until mouse button is released.

## EventContext

Provided to `handle_event()` with utilities for side effects:

```rust
fn handle_event(&mut self, event: &Event, ctx: &mut EventContext) -> EventResult {
    match event {
        Event::MouseDown { position, .. } => {
            ctx.capture();                     // Capture mouse
            ctx.set_cursor(CursorIcon::Pointer);
            EventResult::Handled
        }
        _ => EventResult::Ignored,
    }
}
```

### EventContext Methods

```rust
ctx.capture()                          // Capture mouse events
ctx.set_cursor(CursorIcon::Pointer)    // Request cursor icon
ctx.register_overlay(bounds, modal)    // Register as overlay
ctx.unregister_overlay()               // Unregister overlay
ctx.start_drag(DragData { ... })       // Start drag operation
ctx.copy_to_clipboard("text")          // Copy to clipboard
ctx.paste_from_clipboard()             // Paste → Option<String>
ctx.set_virtual_keyboard_visible(true) // Show/hide IME (Android)
ctx.viewport_size()                    // Window size
ctx.measure_text_width(text, font_size, char_count) // Measure text
ctx.hit_test_char(text, font_size, x_offset) // Hit-test character
```

## Drag & Drop

### DragData

```rust
DragData::new("task", "{\"id\": 42}", source_element_id)
    .with_label("Move Task #42")
```

Fields:
- `drag_type: String` — type identifier for drop target filtering
- `payload: String` — serialized data
- `source_id: u64` — source element ID
- `label: Option<String>` — display label for ghost overlay

### Draggable / DropArea Widgets

```rust
// Source
Draggable::new(content_widget)
    .drag_type("task")
    .payload(serde_json::to_string(&task).unwrap())
    .label("Move task")

// Target
DropArea::new(target_widget)
    .accept_type("task")
    .on_drop(|data: DragData| {
        let task: Task = serde_json::from_str(&data.payload).unwrap();
        // Handle drop
    })
```

## Widget-Level Event Handling

Most widgets expose event callbacks via builder methods:

```rust
// Click
Button::new("OK").on_click(|| { ... })
Button::new("OK").on_click_at(|pos: Point| { ... })

// Change
TextField::new().on_change(|text: &str| { ... })
TextField::new().on_submit(|text: &str| { ... })
Checkbox::new().on_change(|checked: bool| { ... })
Slider::new().on_change(|value: f32| { ... })
Toggle::new().on_change(|on: bool| { ... })

// Hover
GestureDetector::new()
    .on_hover_change(|hovered: bool| { ... })
    .on_double_click(|| { ... })
    .on_mouse_down(|pos| { ... })
    .on_mouse_up(|pos| { ... })
```

## Text Selection

Built-in text selection support for text input widgets:

```rust
struct TextSelectionState {
    anchor: Option<usize>,    // Selection start (byte offset)
    mouse_selecting: bool,    // Dragging to select
}
```

Methods:
```rust
sel.start(pos)                      // Begin selection at byte offset
sel.extend_or_start(cursor_pos)     // Extend or start (Shift+click)
sel.range(cursor_pos)               // (start, end) or None
sel.selected_text(text, cursor_pos) // &str slice
sel.delete_selection(text, cursor)  // Delete selected, return success
sel.replace_selection(text, cursor, replacement) // Replace or insert
sel.clear()
sel.has_selection(cursor_pos)
```

Selection color: `Color::new(0.231, 0.510, 0.965, 0.3)` (blue highlight)
