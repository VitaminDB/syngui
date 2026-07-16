# Visual Effects

SYNGUI provides 30+ GPU-accelerated visual effects: blur, color filters, distortions, overlays, and more. Effects can be applied two ways: programmatically via `syngui::effects::*` with DisplayList effect layers, or declaratively via MSS `filter` / `backdrop-filter` properties.

## Rust API

```rust
use syngui::effects::*;

// Wrap draw commands in an effect layer
list.begin_effect_layer(blur(8.0), bounds);
// ... draw commands affected by the effect ...
list.end_effect_layer();
```

`begin_effect_layer` renders contents to an offscreen texture, applies the effect, then composites the result back. See [09-rendering.md](09-rendering.md) for DisplayList basics.

## MSS Usage

```css
.card {
    filter: blur(5px) grayscale(50%);
    backdrop-filter: blur(12px);
}
```

- `filter` applies to the element itself
- `backdrop-filter` applies to the content **behind** a transparent element (glassmorphism)
- Multiple filters are space-separated and applied left to right

See [06-styling.md](06-styling.md) for MSS syntax.

## Effect Reference

### Blur Effects

| Rust | MSS `filter:` | Parameters |
|------|---------------|------------|
| `blur(radius)` | `blur(8px)` | `radius: f32` — blur radius in pixels |
| `backdrop_blur(radius)` | `backdrop-filter: blur(12px)` | `radius: f32` — blur radius in pixels |
| `directional_blur(angle, radius)` | `directional-blur(45deg, 8px)` | `angle: f32` (radians), `radius: f32` (pixels) |
| `radial_blur(intensity)` | `radial-blur(0.5)` or `zoom-blur(0.5)` | `intensity: f32` (0.0–1.0) |

```css
/* Glassmorphism card */
.glass-card {
    background-color: rgba(255, 255, 255, 0.15);
    backdrop-filter: blur(12px);
    border: 1px solid rgba(255, 255, 255, 0.2);
    border-radius: 16px;
}
```

### Color Adjustments

| Rust | MSS `filter:` | Parameters |
|------|---------------|------------|
| `grayscale(amount)` | `grayscale(100%)` | `amount: f32` — 0.0 = off, 1.0 = full grayscale |
| `sepia(amount)` | `sepia(80%)` | `amount: f32` — 0.0 = off, 1.0 = full sepia |
| `invert(amount)` | `invert(100%)` | `amount: f32` — 0.0 = off, 1.0 = fully inverted |
| `brightness(amount)` | `brightness(1.5)` | `amount: f32` — 1.0 = normal, >1 brighter, <1 darker |
| `contrast(amount)` | `contrast(1.5)` | `amount: f32` — 1.0 = normal, >1 more contrast |
| `hsb_adjust(h, s, b)` | `hue-rotate(90deg)` / `saturate(200%)` | `hue: f32` (0–1 = 0–360deg), `saturation: f32` (1.0 = normal), `brightness: f32` |
| `color_grade(lift, gamma, gain)` | `color-grade(0.1, 1.0, 1.2)` | `lift: f32` (shadow offset), `gamma: f32` (midtone power), `gain: f32` (highlight multiplier) |

> **Note:** MSS `hue-rotate(Xdeg)` and `saturate(X)` both map to `HsbAdjust` internally.

### Color Mapping

| Rust | MSS `filter:` | Parameters |
|------|---------------|------------|
| `gradient_map(dark, light)` | `gradient-map(#1a1a2e, #e94560)` | Map luminance to two-color gradient |
| `duotone(shadow, highlight)` | `duotone(#0f3460, #e94560)` | Stylize to two tones |
| `silhouette(color)` | `silhouette(#000000)` | Solid color fill based on alpha |

```css
/* Duotone image treatment */
.hero-image {
    filter: duotone(#0f3460, #e94560);
}
```

### Distortion Effects

| Rust | MSS `filter:` | Parameters |
|------|---------------|------------|
| `displacement(amplitude, frequency)` | `wave(5px, 3.0)` | `amplitude: f32` (pixels), `frequency: f32` |
| `swirl(angle, radius)` | `swirl(45deg, 0.5)` | `angle: f32` (radians), `radius: f32` (0–1) |
| `bulge(strength, radius)` | `bulge(0.5)` or `pinch(0.5)` | `strength: f32` (positive=bulge, negative=pinch), `radius: f32` (0–1) |
| `heat_haze(amplitude, speed)` | `heat-haze(3px, 1.0)` | `amplitude: f32` (pixels), `speed: f32` — animated |
| `refraction(distortion, ior)` | `refraction(0.1, 1.33)` | `distortion: f32`, `ior: f32` (index of refraction) |

> **Note:** `pinch(X)` is equivalent to `bulge(-X)`. `heat_haze` is animated — the `speed` parameter controls animation rate.

### Overlay / Texture Effects

| Rust | MSS `filter:` | Parameters |
|------|---------------|------------|
| `noise(intensity)` | `noise(0.3)` | `intensity: f32` (0.0–1.0) — grain overlay |
| `vignette(radius, softness)` | `vignette(0.7)` | `radius: f32` (0–1), `softness: f32` (0–1, default 0.3 in MSS) |
| `scanlines(density, opacity)` | `crt(0.5)` | `density: f32`, `opacity: f32` — CRT scanlines + barrel distortion |

> **Note:** `noise` and `vignette` also available as standalone MSS properties (see [Standalone MSS Properties](#standalone-mss-properties)).

### Stylistic Effects

| Rust | MSS `filter:` | Parameters |
|------|---------------|------------|
| `pixelate(block_size)` | `pixelate(8px)` | `block_size: f32` — block size in pixels |
| `edge_detection(threshold)` | `edge-detect(0.3)` | `threshold: f32` (0.0–1.0) — Sobel edge detection |
| `chromatic_aberration(offset)` | `chromatic-aberration(3px)` | `offset: f32` — RGB channel offset in pixels |
| `glitch(intensity, block_size)` | `glitch(0.5)` | `intensity: f32` (0–1), `block_size: f32` (default 8.0 in MSS) |
| `dissolve(threshold)` | `dissolve(0.5)` | `threshold: f32` (0–1: 0=visible, 1=dissolved) |
| `hologram(color, intensity)` | `hologram(#00ff80, 0.8)` or `x-ray(...)` | `color: Color`, `intensity: f32` (0–1) |
| `lens_flare(threshold, intensity)` | `lens-flare(0.7, 1.0)` | `threshold: f32` (0–1), `intensity: f32` |
| `mask_reveal(progress, direction)` | `mask-reveal(0.5, 90deg)` | `progress: f32` (0–1), `direction: f32` (radians) |

```rust
// Glitch effect on hover (in build_display_list)
list.begin_effect_layer(glitch(0.6, 8.0), bounds);
// ... draw content ...
list.end_effect_layer();
```

### Compositing

| Rust | MSS | Parameters |
|------|-----|------------|
| `shadow(color, blur_radius, offset_x, offset_y)` | `box-shadow: 2px 4px 8px #00000040` | Drop shadow |
| `opacity(value)` | `opacity: 0.8` | `value: f32` (0.0–1.0) |
| `glow(radius, intensity)` | `glow: 0 0 12px #6366f180` | Blur + additive composite (bloom) |
| `Effect::BlendMode { mode }` | `mix-blend-mode: multiply` | Compositing blend mode |

**BlendModeType values:** `Normal`, `Multiply`, `Screen`, `Overlay`, `Darken`, `Lighten`, `ColorDodge`, `ColorBurn`, `SoftLight`, `HardLight`, `Difference`, `Exclusion`

## Effect Chaining

Apply multiple effects in sequence:

```rust
use syngui::effects::*;

let fx = chain(vec![blur(3.0), grayscale(0.5), vignette(0.8, 0.3)]);
list.begin_effect_layer(fx, bounds);
// ... draw content ...
list.end_effect_layer();
```

In MSS, space-separated filter functions are applied left to right:

```css
.retro {
    filter: crt(0.4) noise(0.15) vignette(0.6) chromatic-aberration(2px);
}
```

## Standalone MSS Properties

These effect-related properties exist outside the `filter:` chain:

| Property | Syntax | Notes |
|----------|--------|-------|
| `opacity` | `opacity: 0.8` | 0.0–1.0, optimized render path |
| `box-shadow` | `box-shadow: 2px 4px 8px #00000040` | Multiple comma-separated; supports `inset` |
| `glow` | `glow: 0 0 12px #2196F380` | Same syntax as box-shadow, additive blend |
| `color-tint` | `color-tint: rgba(255, 0, 0, 0.1)` | Color overlay on element |
| `noise` | `noise: 0.3` | Grain overlay intensity (0–1) |
| `vignette` | `vignette: 0.5` | Darkened edges intensity (0–1) |
| `mix-blend-mode` | `mix-blend-mode: screen` | Compositing blend mode |
| `outline-width` | `outline-width: 2px` | SDF outline ring width |
| `outline-color` | `outline-color: #6366f1` | Outline ring color |
| `outline-offset` | `outline-offset: 4px` | Offset from element edge |

```css
.card {
    box-shadow: 0 4px 12px rgba(0, 0, 0, 0.15), 0 1px 3px rgba(0, 0, 0, 0.1);
    box-shadow-inset: inset 0 1px 0 rgba(255, 255, 255, 0.1);
}

.neon-button {
    glow: 0 0 20px rgba(99, 102, 241, 0.6);
    outline-width: 2px;
    outline-color: #6366f1;
    outline-offset: 2px;
}
```

## Transitions

Filter effects support smooth CSS transitions between states:

```css
.card {
    filter: grayscale(100%);
    transition: filter 300ms ease-out;
}
.card:hover {
    filter: grayscale(0%);
}
```

Internally, `FilterEffect::lerp()` interpolates matching filter types. When a filter appears in only one state, it transitions from/to its identity value (e.g., `Blur(0.0)`, `Grayscale(0.0)`, `Brightness(1.0)`). Mismatched filter types snap at t=0.5.

`box-shadow`, `glow`, `opacity`, `noise`, and `vignette` also support transitions:

```css
.card {
    box-shadow: 0 2px 4px rgba(0, 0, 0, 0.1);
    transition: box-shadow 200ms ease-out;
}
.card:hover {
    box-shadow: 0 8px 24px rgba(0, 0, 0, 0.2);
}
```

See [07-animation.md](07-animation.md) for the animation system.

## Practical Examples

### Frosted Glass Card

```css
.glass-panel {
    background-color: rgba(255, 255, 255, 0.1);
    backdrop-filter: blur(16px);
    border: 1px solid rgba(255, 255, 255, 0.15);
    border-radius: 12px;
    box-shadow: 0 8px 32px rgba(0, 0, 0, 0.2);
}
```

### Image Hover Effect

```css
.photo {
    filter: grayscale(100%) brightness(0.8);
    transition: filter 400ms ease-out;
}
.photo:hover {
    filter: grayscale(0%) brightness(1.0);
}
```

### Retro CRT Terminal

```rust
use syngui::effects::*;

let crt_effect = chain(vec![
    scanlines(2.0, 0.4),
    noise(0.15),
    vignette(0.6, 0.4),
    chromatic_aberration(2.0),
]);

list.begin_effect_layer(crt_effect, bounds);
// ... draw terminal content ...
list.end_effect_layer();
```

### Animated Dissolve Reveal

```rust
use syngui::effects::*;
use syngui::animation::*;

// In element state:
let reveal = Animation::tween(Easing::EaseOutCubic)
    .from(1.0).to(0.0)
    .duration_ms(800);

// In build_display_list:
let t = reveal.current_value();
list.begin_effect_layer(dissolve(t), bounds);
// ... draw content ...
list.end_effect_layer();
```

## Performance Notes

- Effects use offscreen render targets — each layer adds GPU memory and draw calls
- `backdrop_blur` is the most expensive effect (reads back the framebuffer)
- Chained effects compose through intermediate textures (one per effect in the chain)
- Prefer standalone MSS properties (`opacity`, `box-shadow`) over `filter:` equivalents when possible — they use optimized render paths without offscreen targets
- Feature flags: `effects` (enabled by default) includes `blur` + `shadow`; these can also be enabled independently
