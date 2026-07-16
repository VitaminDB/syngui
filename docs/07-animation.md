# Animation System

SYNGUI provides three animation types: Tween (eased interpolation), Spring (physics-based), and Sequence (chained animations).

## Animation Enum

```rust
enum Animation {
    Tween { from, to, duration, delay, easing, elapsed, ... },
    Spring { current, target, velocity, stiffness, damping, mass, delay, ... },
    Sequence { animations, current_index, ... },
    Constant(f32),
}
```

### Common Methods

```rust
animation.current_value()   // Current interpolated value
animation.initial_value()   // Start value
animation.target_value()    // End value
animation.set_target(val)   // Change target (spring: smooth transition)
animation.tick(dt)           // Advance by Duration, returns true if still running
animation.is_complete()     // Has animation finished?
animation.reset()           // Reset to initial state
```

## Tween Animation

Eased interpolation from one value to another over a fixed duration.

```rust
Animation::tween(Easing::EaseOutCubic)
    .from(0.0)
    .to(100.0)
    .duration_ms(300)
    .delay_ms(0)
```

### Easing Functions

All 30+ easing variants:

| Category | Variants |
|----------|----------|
| Linear | `Linear` |
| Sine | `EaseInSine`, `EaseOutSine`, `EaseInOutSine` |
| Quadratic | `EaseInQuad`, `EaseOutQuad`, `EaseInOutQuad` |
| Cubic | `EaseInCubic`, `EaseOutCubic`, `EaseInOutCubic` |
| Quartic | `EaseInQuart`, `EaseOutQuart`, `EaseInOutQuart` |
| Quintic | `EaseInQuint`, `EaseOutQuint`, `EaseInOutQuint` |
| Exponential | `EaseInExpo`, `EaseOutExpo`, `EaseInOutExpo` |
| Circular | `EaseInCirc`, `EaseOutCirc`, `EaseInOutCirc` |
| Back (overshoot) | `EaseInBack`, `EaseOutBack`, `EaseInOutBack` |
| Elastic | `EaseInElastic`, `EaseOutElastic`, `EaseInOutElastic` |
| Bounce | `EaseInBounce`, `EaseOutBounce`, `EaseInOutBounce` |
| Custom | `CubicBezier(x1, y1, x2, y2)`, `Steps(n)` |

CSS presets: `CSS_EASE`, `CSS_EASE_IN`, `CSS_EASE_OUT`, `CSS_EASE_IN_OUT`

```rust
easing.apply(t)  // t ∈ [0,1] → eased value ∈ [0,1]
```

## Spring Animation

Physics-based spring animation. Naturally smooth, no fixed duration.

```rust
Animation::spring()
    .from(0.0)
    .to(100.0)
    .stiffness(200.0)    // Default: 100.0. Higher = stiffer/faster
    .damping(20.0)        // Default: 10.0. Higher = less oscillation
    .mass(1.0)            // Default: 1.0
    .delay_ms(0)
```

### Spring Parameters

| Parameter | Effect | Typical Range |
|-----------|--------|---------------|
| `stiffness` | How strongly the spring pulls toward target | 50–500 |
| `damping` | How quickly oscillation dies | 5–30 |
| `mass` | Inertia of the animated object | 0.5–3.0 |

Low damping + high stiffness = bouncy. High damping = smooth settle.

### Spring Internals

```rust
let spring = Spring::new()
    .with_stiffness(200.0)
    .with_damping(20.0)
    .with_mass(1.0);

let (new_pos, new_vel) = spring.update(current, target, velocity, dt_secs);
spring.is_at_rest(displacement, velocity) // Check convergence
```

## Animated Widget

Apply transform and opacity animations to any widget:

```rust
Animated::new(my_widget)
    .translate_x(Animation::tween(Easing::EaseOutCubic).from(-50.0).to(0.0).duration_ms(300))
    .translate_y(Animation::spring().from(-20.0).to(0.0))
    .scale(Animation::tween(Easing::EaseOutBack).from(0.8).to(1.0).duration_ms(400))
    .rotate(Animation::tween(Easing::Linear).from(0.0).to(360.0).duration_ms(2000))
    .opacity(Animation::tween(Easing::EaseInOut).from(0.0).to(1.0).duration_ms(200))
    .origin(TransformOrigin::Center)
    .repeat(true)
    .repeat_mode(RepeatMode::PingPong(3))
```

### TransformOrigin

- `TopLeft` — transform pivot at top-left corner
- `Center` — transform pivot at center (default)
- `Custom(x, y)` — custom pivot point (0.0–1.0 normalized)

### RepeatMode

- `None` — play once
- `Count(n)` — repeat n times
- `PingPong(n)` — play forward then backward, n times

## AnimatedSize Widget

Smooth size transitions when child content changes:

```rust
AnimatedSize::new(
    move || {
        if expanded.get() {
            Column::new().child(item1).child(item2).child(item3)
        } else {
            Column::new().child(item1)
        }
    }
)
.duration_ms(300)
.easing(Easing::EaseOutCubic)
.clip(true)
.axis(AnimationAxis::Both)  // Width | Height | Both
```

## CSS Transitions (MSS)

Automatic property transitions via MSS (see [06-styling.md](06-styling.md)):

```css
Button {
    background-color: #2196F3;
    transition: background-color 200ms ease-out;
}

Button:hover {
    background-color: #1E88E5;
}
```

### TransitionSpec (Internal)

```rust
struct TransitionSpec {
    property: String,       // "background-color", "opacity", etc.
    duration_secs: f32,
    easing: Easing,
    delay_secs: f32,
}
```

### AnimatableValue

Properties that can be animated:

```rust
enum AnimatableValue {
    Color(Color),
    Float(f32),
    None,
}
```

Supports interpolation via `lerp()`.

## Usage Patterns

### Fade-in on mount

```rust
Animated::new(content)
    .opacity(Animation::tween(Easing::EaseOut).from(0.0).to(1.0).duration_ms(300))
```

### Slide-in from left

```rust
Animated::new(content)
    .translate_x(Animation::tween(Easing::EaseOutCubic).from(-100.0).to(0.0).duration_ms(400))
    .opacity(Animation::tween(Easing::EaseOut).from(0.0).to(1.0).duration_ms(300))
```

### Pulse effect

```rust
Animated::new(icon)
    .scale(Animation::tween(Easing::EaseInOutSine).from(1.0).to(1.2).duration_ms(600))
    .repeat_mode(RepeatMode::PingPong(0))  // Infinite
```

### Spring button press

```rust
Animated::new(button)
    .scale(Animation::spring().from(0.95).to(1.0).stiffness(300.0).damping(15.0))
```
