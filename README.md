# syngui

[![Donate via PayPal](https://img.shields.io/badge/donate-PayPal-0070ba?logo=paypal&logoColor=white)](https://paypal.me/vitamindbnfkz)
[![Licence: MIT OR Apache-2.0](https://img.shields.io/badge/licence-MIT%20OR%20Apache--2.0-blue)](#license)
[![Rendering: wgpu](https://img.shields.io/badge/rendering-wgpu%2028-orange)](#rendering)

A retained-mode GUI framework for Rust — GPU-rendered via wgpu, styled with CSS-like
stylesheets, wired with reactive signals.

![The widget_gallery_mss example running natively — MarkdownView with syntax highlighting, one of 110+ widgets](docs/widget-gallery.png)

```rust
use syngui::prelude::*;
use syngui::widgets::*;

fn main() {
    App::new()
        .title("Hello syngui")
        .size(400, 300)
        .run(|_| Box::new(Text::new("Hello, World!")));
}
```

## Why another Rust GUI

Most Rust GUI libraries make you choose: immediate-mode simplicity (egui) or a retained
tree with typed styling (iced), a custom DSL (Slint) or web tech (Tauri, Dioxus).
syngui takes a different combination — and it's the combination, not any single feature,
that's the point:

- **Real stylesheets.** Not a Rust-typed style struct — an actual cascade. Variables and
  `var()`, type/class/compound selectors, pseudo-classes (`:hover`, `:active`, `:focus`,
  `:checked`, `:disabled`), CSS nesting, descendant selectors, transitions, `@keyframes`,
  inheritance. Plus window-level pseudo-classes — `:window-maximized`,
  `:window-fullscreen`, `:window-focused` — synced to the window state.
- **Fine-grained reactivity.** SolidJS-style `use_signal` / `use_effect` / `create_memo` /
  `use_context`, with automatic dependency tracking. Signals are `Copy`; subtrees rebuild
  granularly instead of re-running the whole view.
- **Batteries genuinely included.** 110+ widgets across 10 categories, including things
  you normally vendor yourself: charts (line/bar/pie/radar/gauge), a tile map widget with
  pan-zoom, an embedded terminal (PTY + VT100), a markdown view with syntax highlighting,
  a rope-backed code editor, a block document editor, audio and video playback, and a
  devtools inspector.
- **Retained-mode architecture.** Immutable `Widget` → stateful `Element`, diffed via
  `can_update()`, with dirty-flag propagation (layout / paint / state / children /
  animation). Familiar if you've used Flutter or React.

## DocumentEditor

A Notion-style block editor, in the framework rather than in your app:

- Markdown is the model *and* the file format — flat style runs, inline attributes `{k=v}`,
  `[[wiki-links]]`, `![[embeds]]`, callouts and toggles as first-class blocks.
- Editing: caret and selection with IME, undo/redo history exposed to the host
  (`history_state`, `DocOp::Undo`), markdown shortcuts, a slash menu, an inline toolbar,
  multi-block selection, multi-format clipboard, Tab to nest.
- Blocks: headings, lists, checklists, tables with editable cells, syntax-highlighted code
  edited in place, media blocks with real players, charts, and **live embeds** through an
  `EmbedFactory` (your own widget inside the document, recursion-guarded).
- **Two layout modes**: flow, or a free canvas — pin any block at coordinates, snap to a
  grid, set width and height, and draw vector primitives (rectangle, ellipse, triangle,
  diamond, lines, arrows, Bézier curves with direction handles) alongside the text.
- Host API: `replace_markdown` / `append_markdown`, `on_block_drop`, block properties
  (colour, background, size, weight, alignment), a block tree, drag handles with a live
  ghost of the dragged element.

## Performance

The framework is used in a chat app with thousand-message histories, so the hot paths are
measured rather than assumed:

- **VirtualList** — variable row heights, anchoring and stick-to-bottom; a long feed mounts
  a window of rows instead of the whole history (3.4 s → 5.8 ms per frame in the app that
  drove this work).
- **Keyed children** — inserting in the middle does not recreate the neighbours.
- **MSS cascade** — the rule index is built once per stylesheet, the cascade result is
  cached per element with a style diff, and dirtiness is marked point-wise on class changes.
- **Reactivity** — a reverse subscription index, markdown parse and syntax-highlight caches,
  and a point-wise animation registry (invisible animators no longer tick).
- **Rendering** — batches ordered by bbox without flushing on clips and shadows, culling by
  constraint hash, shared frame geometry buffers, dirty-rect font-atlas uploads, a texture
  ring for streaming video frames, and mipmaps generated on the decode thread.

## Rendering

wgpu 28 (Vulkan / Metal / DX12 / GL / WebGPU) with hand-written WGSL shaders for rects,
text, lines, blur, shadows, and post-processing. The display list is batched by
shader/texture/clip into a handful of draw calls in a single render pass.

## Platforms

Linux (X11 and Wayland — including a hand-rolled Wayland drag-and-drop implementation on
top of `wl_data_device`, since [winit#1881](https://github.com/rust-windowing/winit/issues/1881)
is still open), Windows, macOS, Android, and WebAssembly.

**Android and Android TV** are first-class targets: a D-pad/remote focus model, an on-screen
keyboard (`OnScreenKeyboard`, also on the web through a hidden `<input>` agent), hardware
video through `MediaCodec` rendering straight into an Android `Surface`, a statically linked
FFmpeg with mbedTLS for HTTPS streams, `AppSuspended` / `AppResumed` lifecycle events, safe-area
aware overlays, and back-button handling.

## Styling

```rust
const STYLES: &str = include_str!("../styles/app.mss");

App::new().with_styles_str(STYLES).run(|_| Box::new(build_ui()));
```

```css
:root {
    --accent: #4f8cff;
}

.btn-primary {
    background: var(--accent);
    padding: 8px 16px;
    transition: background 150ms ease-out;

    &:hover { background: #6ea3ff; }
    &:disabled { background: #555; }
}
```

## State

```rust
fn build_ui() -> impl Widget {
    let count = use_context::<RwSignal<i32>>();

    mgui! {
        Column::new().gap(16.0).class("root") => [
            move || {
                let c = count.get();
                Text::new(&format!("Count: {c}"))
            },
            Button::new("Increment")
                .on_click(move || count.set(count.get_untracked() + 1))
                .class("btn-primary"),
        ]
    }
}
```

## Status and honest limitations

This is a young framework built by one person. It is used in production by its author
(see [synthos](https://github.com/VitaminDB/synthos), a desktop AI studio built on it), has
1 350+ tests, and carries no `todo!()` stubs — but you should know what's missing before you
adopt it:

- **Text shaping is simple.** Glyph advances are summed per-character. Latin, Cyrillic and
  CJK render correctly (CJK through a system-font fallback chain with ideographic line
  breaks); there is no kerning, no ligatures, no GSUB/GPOS, no bidirectional text, and no
  complex-script support (Arabic joining, Devanagari reordering). If you need those, this
  framework is not ready for you yet.
- **Colour emoji are drawn without a shaper.** ZWJ sequences and VS16 can render as separate
  glyphs.
- **Accessibility is behind a non-default feature.** AccessKit integration exists
  (AT-SPI / UIA / NSAccessibility) but is not enabled by default and is not continuously
  tested.
- **No CI yet.** Windows, macOS, and Android builds are verified manually.
- **i18n is deliberately small.** `syngui::i18n` gives you `key = "value"` catalogs,
  `tr!`/`trn!` with CLDR-style plural rules, live language switching and OS locale detection
  — but no number/date formatting beyond the calendar widget.
- **API is not stable.** Expect breaking changes.

## Building

```bash
cargo build -p syngui
cargo test -p syngui
```

The `ffmpeg` feature (video playback) is **not** enabled by default and requires system
FFmpeg 7+ development libraries (9.x is what current builds track). Without it, no FFmpeg
linkage occurs.

## Examples

Three example apps live under `app/` and depend only on `syngui`:

| App | What it shows |
|-----|---------------|
| `calculator` | Minimal reference app — also carries an **Android** target under `app/calculator/android/` |
| `widget_gallery_mss` | Every widget + MSS styling; also builds to **WebAssembly** |
| `linamp` | A media player — audio, waveforms, richer layout |

**Desktop:**

```bash
cargo run -p calculator
cargo run -p widget_gallery_mss
cargo run -p linamp
```

**Web (WASM)** — builds the gallery to an ES module and serves it:

```bash
rustup target add wasm32-unknown-unknown
cargo install wasm-bindgen-cli
cd app/widget_gallery_mss/web && ./build.sh
python3 -m http.server --directory .   # then open http://localhost:8000
```

The web build omits desktop-only widgets (terminal, video, native clipboard).

**Android** — the `calculator` app ships a Gradle project. Create
`app/calculator/android/local.properties` pointing at your SDK, then build with the
included `gradlew` (requires the Android SDK/NDK and `cargo-ndk`).

## How it is built

One developer, with Claude (Anthropic) as a daily coding assistant. The architecture, the
rendering and layout work and the performance numbers above are mine; the assistant carries
a large share of the typing, the tests and the refactors.

## Support

syngui is free and open source. If it is useful to you, you can support its development with a donation via [PayPal](https://paypal.me/vitamindbnfkz).

## License

Licensed under either of [Apache-2.0](LICENSE-APACHE) or [MIT](LICENSE-MIT) at your
option.
