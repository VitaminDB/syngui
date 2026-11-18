# Layout System

SYNGUI uses constraint-based layout similar to Flutter. Parent passes constraints (min/max width and height), child returns its size within those constraints.

## Constraints

```rust
struct Constraints {
    min_width: f32,
    max_width: f32,
    min_height: f32,
    max_height: f32,
}
```

### Constructors

```rust
Constraints::new(min_w, max_w, min_h, max_h)
Constraints::tight(size)    // min == max (fixed size)
Constraints::loose(size)    // min = 0, max = size
Constraints::expand()       // All infinity (unbounded)
```

### Methods

```rust
constraints.constrain(size)         // Clamp size to constraints
constraints.constrain_width(w)      // Clamp width only
constraints.constrain_height(h)     // Clamp height only
constraints.normalize()             // Ensure min <= max
constraints.loosen()                // Reset min to 0, keep max
constraints.is_tight()              // min == max?
constraints.has_bounded_width()     // max_width is finite?
constraints.has_bounded_height()    // max_height is finite?
constraints.hash_key()              // Cache key for layout caching
```

## Layout Hints

Each element declares a `LayoutHint` that tells the ElementTree how to arrange its children:

| Hint | Description |
|------|-------------|
| `Center` | Center single child (default) |
| `Column { gap, cross_align, padding_* }` | Vertical stack |
| `Row { gap, offset_x, cross_align, main_align, padding_* }` | Horizontal stack |
| `Stack` | Overlay children on top of each other |
| `Padding { left, top, right, bottom }` | Single child with padding |
| `Grid { columns, row_gap, col_gap }` | Grid layout |
| `Scroll { padding, unbounded_* }` | Scrollable container |
| `Split { horizontal, ratio, divider }` | Split view |
| `AnimatedSize` | Smooth size transitions |
| `Container { padding }` | Size from layout() with child constraints |
| `Loose` | Min=0 constraints (Reactive wrapper) |
| `Portal { anchor, margins }` | Overlay (zero space in parent) |
| `FloatingWindow { x, y }` | Absolute positioned overlay |
| `Flex { col_gap, row_gap, justify, align_items }` | Wrap/flex layout |
| `HorizontalPages` | Carousel page sliding |

## Alignment

### MainAxisAlignment (along primary direction)

```
Start        [A B C          ]
Center       [     A B C     ]
End          [          A B C]
SpaceBetween [A     B      C]
SpaceAround  [  A    B    C  ]
SpaceEvenly  [  A   B   C   ]
```

### CrossAxisAlignment (perpendicular to primary)

- `Start` — align at start edge
- `Center` — center
- `End` — align at end edge
- `Stretch` — fill cross axis

## Layout Process

The ElementTree performs two recursive passes:

1. **`measure_recursive(id, constraints)`** — top-down. Each element receives constraints from its parent, measures children according to its LayoutHint, and returns its own size.

2. **`position_recursive(id, offset)`** — top-down. Each element positions its children relative to itself based on the sizes computed in step 1.

### Layout Caching

Each element has a `LayoutCache { size, constraints_hash }`. If the constraints hash matches, the layout pass is skipped for that subtree. Dirty propagation invalidates ancestor cache entries.

## Container Widgets

### Row / Column

```rust
Row::new()
    .gap(8.0)                                    // Space between children
    .main_axis_alignment(MainAxisAlignment::Center)
    .cross_axis_alignment(CrossAxisAlignment::Stretch)
    .child(widget1)
    .child(widget2)

Column::new()
    .gap(12.0)
    .center()   // Shorthand: center on both axes
    .child(widget1)
    .child(widget2)
```

### Flex-grow (заполнение свободного места)

Чтобы ребёнок `Row`/`Column` занимал свободное пространство, задайте ему MSS-свойство `flex-grow`. Рекомендованный шаблон — завести несколько вспомогательных классов в stylesheet приложения:

```mss
.grow   { flex-grow: 1; }
.grow-2 { flex-grow: 2; }
.grow-3 { flex-grow: 3; }
```

Применение в коде:

```rust
Row::new()
    .child(Text::new("Label"))
    .child(TextField::new().class("grow"))    // занять всё свободное место
    .child(Button::new("OK"))

// пропорциональное деление 2:1
Row::new()
    .child(panel_a.class("grow-2"))
    .child(panel_b.class("grow"))
```

Для runtime-значений — inline-стиль:

```rust
widget.style("flex-grow", StyleValue::Number(factor))
```

Занять всю ширину/высоту родителя — через MSS-свойства `width: 100%` / `height: 100%`
(обычно в виде соответствующих классов).

### Grid

```rust
Grid::new(3)           // 3 columns
    .gap(8.0)           // Both row and column gap
    .row_gap(12.0)      // Override row gap
    .col_gap(8.0)       // Override column gap
    .child(widget1)
    .child(widget2)
    // ...
```

### Flex (Wrap Layout)

```rust
Flex::new()
    .direction(FlexDirection::Row)
    .gap(8.0)
    .wrap()
    .main_axis_alignment(MainAxisAlignment::Start)
    .cross_axis_alignment(CrossAxisAlignment::Start)
    .child(chip1)
    .child(chip2)
```

### Stack (Overlay)

```rust
Stack::new()
    .fit(StackFit::Expand)  // Loose | Expand | Passthrough
    .child(background_image)
    .child(overlay_text)
```

### Padding

```rust
Padding::all(16.0).child(content)
Padding::symmetric(16.0, 8.0).child(content)  // h, v
Padding::only(8.0, 16.0, 8.0, 0.0).child(content) // l, t, r, b
```

### SplitView

```rust
SplitView::new(left_panel, right_panel)
    .direction(SplitDirection::Horizontal) // or Vertical
    .initial_ratio(0.3)       // 30% left
    .min_size(200.0)          // Minimum panel size
    .divider_width(4.0)       // Draggable divider width
```
