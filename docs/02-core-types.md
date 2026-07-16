# Core Types

## Color

Linear RGBA color (f32, 0.0–1.0 per channel).

```rust
// Constructors
Color::new(r, g, b, a)          // Direct f32 RGBA
Color::rgb(r, g, b)             // RGB, alpha = 1.0
Color::rgba(r, g, b, a)         // Same as new()
Color::from_srgb(r: u8, g: u8, b: u8, a: f32)  // From sRGB u8
Color::from_hex("#RRGGBB")      // Parse hex string
Color::from_hex("#RRGGBBAA")    // With alpha

// Constants
Color::WHITE
Color::BLACK
Color::RED
Color::GREEN
Color::BLUE
Color::TRANSPARENT

// Methods
color.to_array()                 // [f32; 4]
color.to_premultiplied_array()   // Pre-multiplied alpha
color.with_alpha(0.5)            // New color with changed alpha
color.multiply_alpha(0.8)        // Multiply existing alpha
color.lerp(&other, t)            // Linear interpolation (t: 0.0–1.0)
color.darken(0.8)                // Darken (0.0=black, 1.0=unchanged)
color.lighten(0.2)               // Lighten (0.0=unchanged, 1.0=white)
```

## Point, Size, Rect, Vector, Transform

Type aliases over `euclid` with compile-time `UISpace` unit:

```rust
type Point     = euclid::Point2D<f32, UISpace>;
type Size      = euclid::Size2D<f32, UISpace>;
type Rect      = euclid::Rect<f32, UISpace>;
type Vector    = euclid::Vector2D<f32, UISpace>;
type Transform = euclid::Transform2D<f32, UISpace, UISpace>;
```

### RectExt (trait extension for Rect)

```rust
rect.x()          // Left edge
rect.y()          // Top edge
rect.right()      // x + width
rect.bottom()     // y + height
rect.center()     // Center point

Rect::zero()              // Zero-sized at origin
Rect::from_size(w, h)     // At origin with given dimensions
```

## EdgeInsets

Spacing/margin/padding on four sides.

```rust
EdgeInsets::zero()
EdgeInsets::new(left, top, right, bottom)
EdgeInsets::all(8.0)                      // Uniform
EdgeInsets::symmetric(h: 16.0, v: 8.0)   // Horizontal/vertical

insets.horizontal()   // left + right
insets.vertical()     // top + bottom
```

## Shadow

Drop shadow definition.

```rust
Shadow::new(color, offset_x, offset_y, blur_radius)
Shadow::new(...).with_spread(spread)
Shadow::parse("2px 4px 8px #00000040")  // CSS shadow syntax

// Multiple shadows
let mut shadows = Shadows::new();
shadows.push(Shadow::new(...));
shadows.is_empty();
shadows.as_slice();
Shadows::parse("2px 4px 8px #000, 0 0 4px #00f")  // Comma-separated
```

## Path & Bezier

Path building for Canvas 2D:

```rust
let path = Path::new()
    .move_to(Point::new(0.0, 0.0))
    .line_to(Point::new(100.0, 0.0))
    .line_to(Point::new(100.0, 100.0))
    .close();

// Bezier evaluation
Bezier::quad(p0, p1, p2, t)    // Quadratic at t ∈ [0,1]
Bezier::cubic(p0, p1, p2, p3, t) // Cubic at t ∈ [0,1]
```

## Math Utilities

```rust
lerp(a, b, t)                              // Linear interpolation
clamp(value, min, max)                      // Clamp to range
smoothstep(edge0, edge1, x)                 // Hermite interpolation
map_range(value, from_min, from_max, to_min, to_max) // Range mapping
```

## Error Types

```rust
enum MguiError {
    RenderError(String),
    LayoutError(String),
    GpuError(String),
    IoError(std::io::Error),
}

type Result<T> = std::result::Result<T, MguiError>;
```
