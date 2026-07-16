# Rendering Pipeline

```
Element Tree
    ↓ build_display_list()
DisplayList (draw commands)
    ↓ Batcher (groups by shader/texture/clip)
Vec<RenderOp> (batches)
    ↓ Renderer (single render pass, wgpu)
GPU Frame
```

## DisplayList

GPU-independent list of draw commands. Elements push commands during `build_display_list()`.

### Drawing Commands

```rust
// Rectangles
list.push_rect(rect, color, corner_radius);    // [f32; 4] per-corner
list.push_rect_bordered(rect, color, radius, border);
list.push_rect_per_side_border(rect, color, radius, border, per_side);

// Text
list.push_text(text, rect, color, font_size);
list.push_text_centered(text, rect, color, font_size);
list.push_text_aligned(text, rect, color, font_size, align, decoration, font_weight);
list.push_text_styled(text, rect, color, font_size, align, decoration, font_weight, font_family);

// Text editing
list.push_text_cursor(text, cursor_pos, base_x, y, height, font_size, font_weight, color);
list.push_text_selection(text, sel_start, sel_end, base_x, y, height, font_size, color);

// Images
list.push_image(rect, texture_id, uv_rect, tint);

// Shadows
list.push_shadow(rect, color, blur_radius, offset, corner_radius);

// Canvas 2D (tessellated)
list.push_canvas(vertices, indices);
```

**Important**: Use `push_*` methods (auto-apply clip from stack), NOT `add_*` methods (raw, no clip).

### Clip/Transform/Opacity Stacks

```rust
// Clipping
list.push_clip(rect);     // Push clip rect (intersects with parent)
// ... draw children ...
list.pop_clip();

// Transforms
list.push_transform(transform);
// ... draw children ...
list.pop_transform();

// Opacity
list.push_opacity(0.5);
// ... draw children ...
list.pop_opacity();

// Overlays (drawn on top of everything)
list.begin_overlay();
// ... overlay draw commands ...
list.end_overlay();

// Absolute overlay (no transform)
list.begin_overlay_absolute();
// ... draw commands with absolute screen coords ...
list.end_overlay();
```

### DrawCommand Enum

```rust
enum DrawCommand {
    Rect { rect, color, corner_radius, border, per_side_border, clip },
    Text { text, rect, color, font_size, align, decoration, font_weight, font_family, clip },
    Image { rect, texture_id, uv_rect, tint, clip },
    Shadow { rect, color, blur_radius, offset, corner_radius, clip },
    Canvas { vertices, indices },
    TextCursor { ... },
    TextSelection { ... },
    PushClip(Rect),
    PopClip,
    PushTransform(Transform),
    PopTransform,
    PushOpacity(f32),
    PopOpacity,
    Cached(RenderHandle, Vec<DrawCommand>),
    Custom { ... },
    BeginEffectLayer { effect, bounds },
    EndEffectLayer,
}
```

## Batcher

Converts DisplayList into GPU-ready batches, minimizing draw calls:

```rust
let batcher = Batcher::new();
batcher.set_scale_factor(window_scale);
let render_ops = batcher.process(&display_list, &mut font_atlas);
```

### Batching Strategy

- Groups consecutive commands by shader type, texture, and clip rect
- Merges compatible commands into single draw calls
- Target: 5-10 draw calls per frame
- Transform composition: `new.then(current)` (innermost first)

### ShaderType

```rust
enum ShaderType {
    Rect,       // Rounded rectangles, borders
    Text,       // MSDF text rendering
    Shadow,     // Gaussian shadow
    Image,      // Texture rendering
    Effect,     // Post-processing effects
}
```

### RenderOp

```rust
enum RenderOp {
    Draw(Batch),                       // Standard draw call
    BeginEffect { effect, bounds },    // Begin effect layer
    EndEffect,                         // End effect layer
}
```

## Vertex Format

GPU vertex layout:

```rust
struct Vertex {
    position: [f32; 2],   // Screen-space position
    uv: [f32; 2],         // Texture coordinates
    color: [f32; 4],      // RGBA color (premultiplied alpha)
    data: [f32; 4],       // Shader-specific (corner radius, rect size, etc.)
    data2: [f32; 4],      // Additional shader data
}
```

## ClipRect

```rust
ClipRect::full_screen()
ClipRect::new(x, y, width, height)    // u32 pixel coords
ClipRect::from_rect(rect)             // From Rect (f32)
clip.intersect(other_rect)            // Intersect with another rect
```

**Critical**: Renderer scissor rect persists between batches — must reset to full surface when `clip_rect.enabled == false`.

## GPU Renderer

Single render pass with scissor-rect clipping and ring buffers.

```rust
let renderer = Renderer::new(&gpu, width, height, logical_w, logical_h, font_family);
```

### RenderStats

```rust
struct RenderStats {
    draw_calls: usize,
    vertex_count: usize,
}
```

## Image Pipeline

### ImageStore

Manages image loading from files, bytes, and raw RGBA:

```rust
let store = ImageStore::new();

// Request image (returns handle + load state)
let (handle, state) = store.request(&ImageSource::Path("image.png".into()));
let (handle, state) = store.request(&ImageSource::Bytes { key: "logo", data: bytes });
let (handle, state) = store.request_rgba("key", width, height, rgba_bytes);

// Check state
store.state_of(handle)      // Loading | Ready | Failed
store.dimensions(handle)    // Option<(u32, u32)>
store.has_loading()          // Any images still loading?
store.poll_bg()              // Poll background loader
```

### ImageGpuCache

Uploads images to GPU textures:

```rust
cache.upload(&device, &queue, handle, data);
cache.process_uploads(&device, &queue, &store);
cache.get_bind_group(handle_id) // GPU-ready bind group
```

### TileAtlas (feature: map)

Tile-based atlas for map rendering:

```rust
let atlas = TileAtlas::new(&device, &queue);
atlas.get_tile(&TileKey { x, y, z, provider_id }) // Option<TileSlot>
atlas.insert_tile(key, rgba_data)                  // TileSlot { uv_x, uv_y, uv_w, uv_h }
atlas.upload(&queue);
atlas.clear_provider(provider_id);
```

## Effects

Visual effects applied as layers:

```rust
use syngui::effects::*;

blur(radius)                              // Gaussian blur
shadow(color, blur_radius, offset_x, offset_y) // Drop shadow
opacity(value)                            // Opacity (0.0–1.0)
```

In DisplayList:
```rust
list.begin_effect_layer(effect, bounds);
// ... draw commands affected by effect ...
list.end_effect_layer();
```

## Focus Ring

Accessibility focus indicator:

```rust
draw_focus_ring(&mut display_list, bounds, corner_radius);
// FOCUS_COLOR = Color::new(0.0955, 0.3005, 0.9130, 1.0)
```
