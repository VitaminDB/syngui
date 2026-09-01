# MSS Styling System

MSS (MGUI Style Sheets) is a CSS-like styling language for SYNGUI. Styles are loaded at app startup and applied automatically to elements by type and class.

## Loading Styles

```rust
App::new()
    // From string
    .with_styles_str(include_str!("styles/theme.mss"))
    // From file
    .with_styles("styles/theme.mss")
    // Light/dark themes
    .with_theme_styles(LIGHT_MSS, DARK_MSS, theme_signal)
    // Merge additional styles
    .with_additional_styles_str(EXTRA_MSS)
    // Dynamic theme from signal
    .with_dynamic_theme(theme_mss_signal)
```

## Syntax

MSS follows CSS syntax with element type selectors and class selectors:

```css
/* Element type selector */
Button {
    background-color: #2196F3;
    color: #FFFFFF;
    border-radius: 8px;
    padding: 8px 16px;
    font-size: 14px;
}

/* Class selector */
.primary {
    background-color: #1976D2;
}

/* Pseudo-states */
Button:hover {
    background-color: #1E88E5;
}

Button:active {
    background-color: #1565C0;
}

Button:disabled {
    background-color: #BDBDBD;
    opacity: 0.6;
}

Button:selected {
    background-color: #0D47A1;
}

Button:focus {
    border-color: #2196F3;
    border-width: 2px;
}

/* Compound: element + class + pseudo */
Button.danger:hover {
    background-color: #D32F2F;
}
```

## Selectors

| Selector | Example | Description |
|----------|---------|-------------|
| Element | `Button { ... }` | Match by widget type name |
| Class | `.card { ... }` | Match by CSS class |
| ID | `#main { ... }` | Match by ID |
| Universal | `* { ... }` | Match all elements |
| Element.class | `Text.heading { ... }` | Element with class |
| Pseudo | `Button:hover { ... }` | State-based |
| Descendant | `.panel Text { ... }` | Text inside .panel |
| Child | `.panel > Text { ... }` | Direct child only |
| Group | `Button, .btn { ... }` | Multiple selectors |

### Pseudo-states

| Pseudo | When |
|--------|------|
| `:hover` | Mouse is over element |
| `:active` | Mouse button pressed |
| `:focus` | Element has keyboard focus |
| `:selected` | Element is selected |
| `:disabled` | Element is disabled |

### Element Type Names

Each widget has a type name used in MSS selectors:

| Widget | Type Name |
|--------|-----------|
| Button | `Button` |
| Text | `Text` |
| TextField | `TextField` |
| Checkbox | `Checkbox` |
| Toggle | `Toggle` |
| Slider | `Slider` |
| Dropdown | `Dropdown` |
| Card | `Card` |
| DecoratedBox | `DecoratedBox` |
| Container | `Container` |
| Row | `Row` |
| Column | `Column` |
| TabBar | `TabBar` |
| Tab | `Tab` |
| Sidebar | `Sidebar` |
| TopAppBar | `TopAppBar` |
| Toolbar | `Toolbar` |
| Dialog | `Dialog` |
| ListView | `ListView` |
| ListItem | `ListItem` |
| TreeView | `TreeView` |
| ProgressBar | `ProgressBar` |
| Badge | `Badge` |
| Chip | `Chip` |
| Divider | `Divider` |
| ScrollView | `ScrollView` |
| Accordion | `Accordion` |
| Avatar | `Avatar` |
| Tooltip | `Tooltip` |
| Icon | `Icon` |

## Properties

### Colors

```css
.element {
    background-color: #FF5722;       /* Hex RGB */
    background-color: #FF572280;     /* Hex RGBA */
    background-color: rgb(255, 87, 34);
    background-color: rgba(255, 87, 34, 0.5);
    color: #333333;                  /* Text/foreground color */
    border-color: #CCCCCC;
    accent-color: #2196F3;           /* Accent/highlight */
}
```

### Dimensions

```css
.element {
    width: 200px;
    height: 100px;
    min-width: 50px;
    max-width: 500px;
    min-height: 30px;
    max-height: 400px;
}
```

### Spacing

```css
.element {
    padding: 16px;                   /* All sides */
    padding: 8px 16px;              /* Vertical Horizontal */
    padding: 8px 16px 12px 16px;    /* Top Right Bottom Left */
    padding-left: 8px;
    padding-top: 8px;
    padding-right: 8px;
    padding-bottom: 8px;
    gap: 8px;                        /* Between children */
}
```

### Borders

```css
.element {
    border-width: 1px;
    border-color: #CCCCCC;
    border-radius: 8px;              /* Uniform */
    border-radius: 8px 8px 0 0;     /* TL TR BR BL */
}
```

### Typography

```css
.element {
    font-size: 16px;
    font-weight: bold;               /* normal | bold */
    font-family: "Inter";
    text-align: center;              /* left | center | right */
    text-decoration: underline;      /* none | underline | line-through */
}
```

### Visual Effects

```css
.element {
    opacity: 0.8;                    /* 0.0 to 1.0 */
    box-shadow: 2px 4px 8px #00000040;
    box-shadow: 0 2px 4px rgba(0,0,0,0.1), 0 4px 8px rgba(0,0,0,0.05);
    overflow: hidden;                /* visible | hidden | scroll */
    cursor: pointer;                 /* default | pointer | text | ... */
}
```

### Text Inputs

```css
.my-input {
    caret-color: #2196F3;
    selection-color: #2196F340;
    clipboard-hint: on;              /* on | off — chip with clipboard text on
                                        focus; tap inserts it (TextField) */
}
```

## Variables

```css
:root {
    --primary: #2196F3;
    --surface: #FFFFFF;
    --on-surface: #333333;
    --radius: 8px;
}

Button {
    background-color: var(--primary);
    border-radius: var(--radius);
}

.card {
    background-color: var(--surface);
    color: var(--on-surface);
}
```

## Transitions

CSS-like transitions for smooth property animation:

```css
Button {
    background-color: #2196F3;
    transition: background-color 200ms ease-out;
}

Button:hover {
    background-color: #1E88E5;
}

/* Multiple properties */
.card {
    transition: background-color 300ms ease-in-out,
                opacity 200ms ease-out,
                border-color 200ms linear;
}

/* Shorthand: all properties */
.animated {
    transition: all 300ms ease;
}
```

### Easing Functions

| Name | Description |
|------|-------------|
| `linear` | Constant speed |
| `ease` | CSS ease (cubic-bezier) |
| `ease-in` | Slow start |
| `ease-out` | Slow end |
| `ease-in-out` | Slow start and end |
| `ease-in-sine` | Sine acceleration |
| `ease-out-sine` | Sine deceleration |
| `ease-in-out-sine` | Sine both |
| `ease-in-quad` | Quadratic acceleration |
| `ease-out-quad` | Quadratic deceleration |
| `ease-in-cubic` | Cubic acceleration |
| `ease-out-cubic` | Cubic deceleration |
| `ease-in-out-cubic` | Cubic both |
| `ease-in-quart` | Quartic |
| `ease-out-quart` | Quartic |
| `ease-in-expo` | Exponential |
| `ease-out-expo` | Exponential |
| `ease-in-circ` | Circular |
| `ease-out-circ` | Circular |
| `ease-in-back` | Overshoot start |
| `ease-out-back` | Overshoot end |
| `ease-in-elastic` | Elastic bounce start |
| `ease-out-elastic` | Elastic bounce end |
| `ease-in-bounce` | Bounce start |
| `ease-out-bounce` | Bounce end |
| `cubic-bezier(x1, y1, x2, y2)` | Custom curve |
| `steps(n)` | Step function |

## Keyframes

```css
@keyframes fadeIn {
    0% {
        opacity: 0;
    }
    100% {
        opacity: 1;
    }
}

@keyframes pulse {
    0% {
        opacity: 1;
    }
    50% {
        opacity: 0.5;
    }
    100% {
        opacity: 1;
    }
}
```

## Theming

### Light/Dark Theme Toggle

```rust
let (theme, set_theme) = create_signal(false); // false = light

App::new()
    .with_theme_styles(LIGHT_MSS, DARK_MSS, theme)
    .run(|_| Box::new(build_app(set_theme)));
```

### Dynamic Theme

```rust
let (theme_mss, set_theme_mss) = create_signal(initial_mss_string);

App::new()
    .with_dynamic_theme(theme_mss)
    .run(|_| Box::new(build_app()));

// Later: change theme at runtime
set_theme_mss.set(new_mss_string);
```

## Applying Classes in Code

```rust
use syngui::prelude::*; // imports WidgetExt trait

// Single class
Text::new("Title").class("heading")

// Multiple classes (space-separated)
Card::new().class("elevated primary")

// Vec of classes
widget.classes(vec!["card".into(), "selected".into()])
```

## MSS Internals

### StyleEngine

```rust
let stylesheet = parse_stylesheet_str(mss_content)?;
let engine = StyleEngine::new(stylesheet);

let ctx = StyleContext { element_type: "Button", classes: &["primary"], id: None, parent: None };
let style = engine.compute_style(&ctx);

// Read computed values
style.background_color()  // Option<mss::Color>
style.color()              // Option<mss::Color>
style.border_width()       // Option<f32>
style.opacity()            // Option<f32>
// ... etc
```

### ElementState

```rust
enum ElementState {
    Normal,
    Hover,
    Active,
    Focus,
    Selected,
    Disabled,
}
```

### MssFields

Elements store resolved MSS properties in `MssFields` struct. The `apply()` method reads from `ComputedStyle`:

```rust
let mut fields = MssFields::new();
fields.apply(&computed_style);
// fields.background_color, fields.color, fields.border_radius, etc.
```

### MSS → Core Color Conversion

MSS colors are `mss::Color` (u8 RGBA). Convert to core `Color`:

```rust
let mss_color = style.background_color().unwrap();
let color = Color::from_srgb(mss_color.r, mss_color.g, mss_color.b, mss_color.a as f32 / 255.0);
```

## Gradients

MSS supports CSS-like gradient functions as `background` values.

### Linear Gradient

```css
.hero {
    background: linear-gradient(135deg, #667eea, #764ba2);
}

/* Direction keywords */
.banner {
    background: linear-gradient(to right, #ff6b6b, #feca57);
}

/* Multiple color stops */
.rainbow {
    background: linear-gradient(90deg, #ef4444, #f97316, #eab308, #22c55e, #3b82f6, #8b5cf6);
}

/* Explicit stop positions */
.custom {
    background: linear-gradient(180deg, #000000 0%, #333333 30%, #ffffff 100%);
}
```

**Angle values:**
- `0deg` — bottom to top
- `90deg` — left to right
- `180deg` — top to bottom (default)
- `135deg` — diagonal top-left to bottom-right

**Direction keywords:** `to top`, `to right`, `to bottom`, `to left`, `to top right`, `to bottom left`, etc.

### Radial Gradient

```css
.spotlight {
    background: radial-gradient(circle at center, #ffffff, #3b82f6);
}

.glow {
    background: radial-gradient(ellipse at 30% 70%, rgba(255,0,0,0.8), transparent);
}
```

**Shape:** `circle` or `ellipse` (default).
**Position:** `at center` (default), `at 30% 70%`, `at left top`, etc.

### Conic Gradient

```css
.wheel {
    background: conic-gradient(from 0deg at center, red, yellow, green, blue, red);
}
```

### Programmatic Gradients

```rust
use syngui::core::{Gradient, ColorStop, Color, GradientShape};

DecoratedBox::new(Color::TRANSPARENT)
    .background_gradient(Gradient::Linear {
        angle_deg: 45.0,
        stops: vec![
            ColorStop::new(Color::from_hex("#FF6B6B"), 0.0),
            ColorStop::new(Color::from_hex("#4ECDC4"), 1.0),
        ],
    })
    .corner_radius([12.0; 4])
```

### Gradient Interaction with Other Properties

- **border-radius:** Gradients respect rounded corners via SDF clipping.
- **border:** Borders render on top of gradients.
- **opacity:** Applied to the gradient as a whole.
- **Transitions:** When a transition overrides background-color, the solid color takes priority over the gradient during the transition.
