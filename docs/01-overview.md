# SYNGUI — Overview

Retained-mode GUI framework for Rust. Combines Flutter/React architecture (immutable Widget → stateful Element) with GPU-optimized rendering via wgpu.

## Architecture

```
Widget Tree (immutable descriptions)
    ↓ create_element() / can_update()
Element Tree (stateful, reused across frames)
    ↓ layout() — constraint-based
    ↓ build_display_list()
DisplayList (GPU-independent draw commands)
    ↓ Batcher — groups by shader/texture/clip
Batches (minimized draw calls, target 5-10)
    ↓ Renderer — single render pass
GPU (wgpu: Vulkan/Metal/DX12/GL/WebGPU)
```

### Widget / Element Separation

- **Widget** — immutable UI description, rebuilt every frame. Lightweight value type.
- **Element** — stateful counterpart, reused across frames. Handles layout, rendering, events, animations.
- **ElementTree** — manages element hierarchy, coordinates layout passes, event dispatch, and display list construction.

Diffing via `can_update()` determines whether an existing element can be reused when a new widget is provided. If types match, `element.update(widget)` is called; otherwise, the old element is destroyed and a new one created.

### Dirty Flags

Elements track what needs recalculation via `DirtyFlags` bitflags:

| Flag | Value | Meaning |
|------|-------|---------|
| `LAYOUT` | `1 << 0` | Layout needs recalculation |
| `RENDER` | `1 << 1` | Display list rebuild |
| `PAINT` | `1 << 2` | Element needs repaint |
| `STATE` | `1 << 3` | Internal state changed |
| `CHILDREN` | `1 << 4` | Children changed |
| `ANIMATION` | `1 << 5` | Animation active |

## Quick Start

```rust
use syngui::prelude::*;
use syngui::widgets::*;

fn main() {
    App::new()
        .title("Hello SYNGUI")
        .size(800, 600)
        .run(|_ctx| Box::new(build_app()));
}

fn build_app() -> impl Widget {
    let (count, set_count) = create_signal(0i32);

    Column::new()
        .gap(16.0)
        .center()
        .child(move || {
            Text::new(&format!("Count: {}", count.get()))
                .font_size(24.0)
        })
        .child(
            Button::new("Increment")
                .primary()
                .on_click(move || {
                    let c = count.get_untracked();
                    set_count.set(c + 1);
                })
        )
}
```

## Declarative Syntax with `mgui!`

The `mgui!` macro provides JSX-like declarative tree syntax:

```rust
use syngui::mgui;

fn build_app() -> impl Widget {
    mgui! {
        Column::new().gap(12.0) => [
            Text::new("Hello").bold(),
            Row::new().gap(8.0) => [
                Button::new("OK").primary(),
                Button::new("Cancel").secondary(),
            ],
        ]
    }
}
```

Rules:
- `Widget => [ children... ]` — container with children
- `Widget` (no `=> [...]`) — leaf widget
- Nested `mgui!` is supported inside children blocks
- Closures `move || expr` work as children (auto-wrapped in `Reactive`)

## App Builder

```rust
App::new()
    .title("App Name")                          // Window title
    .size(1280, 720)                             // Initial size
    .min_size(800, 600)                          // Minimum size
    .maximized(true)                             // Start maximized
    .background(Color::WHITE)                    // Background color
    .vsync(true)                                 // V-Sync
    .gpu_backend(GpuBackend::Auto)               // Vulkan/Metal/DX12/GL
    .gpu_power(GpuPowerPreference::LowPower)     // GPU selection
    .with_font_family("Inter")                   // Preferred font
    .with_font_url("fonts/Inter.ttf")            // Font URL (WASM)
    .with_emoji_font_url("fonts/Emoji.ttf")      // Emoji font
    .with_icon_font(include_bytes!("icons.ttf")) // Icon font (embedded)
    .with_styles_str(MSS_CONTENT)                // MSS stylesheet string
    .with_theme_styles(LIGHT, DARK, theme_sig)   // Light/dark themes
    .with_dynamic_theme(theme_mss_signal)        // Dynamic MSS from signal
    .with_additional_styles_str(EXTRA_MSS)       // Merge additional styles
    .with_debug_overlay(false)                   // FPS/performance overlay
    .with_dev_tools(false)                       // Element inspector (F12)
    .run(|ctx| Box::new(root_widget()));         // Launch
```

### GpuBackend

`Auto` | `Vulkan` | `Gl` | `Dx12` | `Metal`

### GpuPowerPreference

`HighPerformance` (discrete GPU) | `LowPower` (integrated GPU)

## Feature Flags

```toml
[features]
default = ["msdf", "effects"]
```

| Flag | Description |
|------|-------------|
| `msdf` | MSDF text rendering |
| `color-emoji` | Color emoji support |
| `effects` | `blur` + `shadow` effects |
| `blur` | Gaussian blur effect |
| `shadow` | Drop shadow effect |
| `taffy-layout` | Taffy layout engine integration |
| `wayland` | Wayland backend (Linux) |
| `x11` | X11 backend (Linux) |
| `tokio` | Async runtime (`use_async`, `spawn`) |
| `clipboard` | Clipboard support |
| `map` | MapView widget |
| `markdown` | MarkdownView widget |
| `debug` | Debug utilities |
| `inspector` | DevTools inspector |
| `android` | Android platform support |

## Workspace Structure

```
syngui/                 — core framework crate
examples/              — example apps
  todo/                — kanban board
  widget_gallery_mss/  — widget gallery with MSS themes
  android_demo/        — Android demo
app/                   — production apps
  volna_plus/          — Volna Plus
  aiplanner/           — AI Planner
docs/                  — documentation (this folder)
```

## Platform Support

- **Desktop**: Windows, macOS, Linux (Wayland/X11)
- **Web**: WebAssembly (wasm32)
- **Mobile**: Android (via winit + android-game-activity)
