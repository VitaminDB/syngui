# Creating Custom Widgets

## Widget / Element Pattern

Every widget in SYNGUI consists of two parts:

1. **Widget** (struct implementing `Widget` trait) — immutable description, lightweight
2. **Element** (struct implementing `Element` trait) — stateful counterpart, handles layout/render/events

## Widget Trait

```rust
pub trait Widget: Send + 'static {
    fn create_element(&self) -> Box<dyn Element>;
    fn can_update(&self, other: &dyn Any) -> bool;
    fn as_any(&self) -> &dyn Any;
    fn as_any_mut(&mut self) -> &mut dyn Any;
    fn mount(&self, tree: &mut ElementTree, parent_id: ElementId);
    fn child_widgets(&self) -> Vec<&dyn Widget>;  // Default: empty
}
```

## Element Trait

```rust
pub trait Element: Send + 'static {
    // Lifecycle
    fn update(&mut self, widget: &dyn Widget, ctx: &mut UpdateContext);
    fn mount(&mut self, tree: &mut ElementTree);

    // Layout
    fn layout(&mut self, constraints: Constraints) -> Size;
    fn layout_hint(&self) -> LayoutHint;  // Default: Center
    fn explicit_dimensions(&self) -> (Option<f32>, Option<f32>);
    fn min_max_dimensions(&self) -> (Option<f32>, Option<f32>, Option<f32>, Option<f32>);
    fn margin(&self) -> EdgeInsets;

    // Rendering
    fn build_display_list(&self, list: &mut DisplayList, clip: Rect);
    fn post_build_display_list(&self, list: &mut DisplayList, clip: Rect);

    // Events
    fn handle_event(&mut self, event: &Event, ctx: &mut EventContext) -> EventResult;

    // Animation
    fn animate(&mut self, dt: Duration) -> bool;  // Return true if still animating

    // Identity
    fn id(&self) -> ElementId;
    fn set_id(&mut self, id: ElementId);
    fn bounds(&self) -> Rect;
    fn set_position(&mut self, pos: Point);
    fn children(&self) -> &[ElementId];
    fn hit_test(&self, point: Point) -> bool;

    // Styling
    fn element_type_name(&self) -> &str;
    fn get_classes(&self) -> &[String];
    fn set_classes(&mut self, classes: Vec<String>);
    fn apply_computed_style(&mut self, style: &ComputedStyle);

    // Dirty tracking
    fn mark_dirty(&mut self, flags: DirtyFlags);
    fn clear_dirty(&mut self, flags: DirtyFlags);
    fn is_dirty(&self, flags: DirtyFlags) -> bool;
    fn needs_repaint(&self) -> bool;

    // Dynamic rebuild
    fn needs_rebuild(&self) -> bool;   // Default: false
    fn build_children(&self) -> Vec<Box<dyn Widget>>;  // Default: empty
    fn clear_rebuild(&mut self);       // Default: no-op

    // Visibility
    fn is_visible(&self) -> bool;      // Default: true
    fn clip_content(&self) -> bool;    // Default: false
}
```

## Minimal Custom Widget Example

```rust
use syngui::prelude::*;

// 1. Widget (immutable description)
struct ColorBox {
    color: Color,
    width: f32,
    height: f32,
}

impl ColorBox {
    fn new(color: Color) -> Self {
        Self { color, width: 100.0, height: 100.0 }
    }

    fn size(mut self, w: f32, h: f32) -> Self {
        self.width = w;
        self.height = h;
        self
    }
}

impl Widget for ColorBox {
    fn create_element(&self) -> Box<dyn Element> {
        Box::new(ColorBoxElement {
            id: ElementId::new(),
            color: self.color,
            width: self.width,
            height: self.height,
            bounds: Rect::zero(),
            dirty: DirtyFlags::all(),
        })
    }

    fn can_update(&self, other: &dyn Any) -> bool {
        other.is::<ColorBox>()
    }

    fn as_any(&self) -> &dyn Any { self }
    fn as_any_mut(&mut self) -> &mut dyn Any { self }
    fn mount(&self, _tree: &mut ElementTree, _parent_id: ElementId) {}
}

// 2. Element (stateful counterpart)
struct ColorBoxElement {
    id: ElementId,
    color: Color,
    width: f32,
    height: f32,
    bounds: Rect,
    dirty: DirtyFlags,
}

impl Element for ColorBoxElement {
    fn update(&mut self, widget: &dyn Widget, _ctx: &mut UpdateContext) {
        if let Some(w) = widget.as_any().downcast_ref::<ColorBox>() {
            self.color = w.color;
            self.width = w.width;
            self.height = w.height;
        }
    }

    fn layout(&mut self, constraints: Constraints) -> Size {
        let size = constraints.constrain(Size::new(self.width, self.height));
        self.bounds = Rect::new(self.bounds.origin, size);
        size
    }

    fn build_display_list(&self, list: &mut DisplayList, _clip: Rect) {
        list.push_rect(self.bounds, self.color, [0.0; 4]);
    }

    fn handle_event(&mut self, _event: &Event, _ctx: &mut EventContext) -> EventResult {
        EventResult::Ignored
    }

    fn animate(&mut self, _dt: Duration) -> bool { false }
    fn bounds(&self) -> Rect { self.bounds }
    fn id(&self) -> ElementId { self.id }
    fn set_id(&mut self, id: ElementId) { self.id = id; }
    fn set_position(&mut self, pos: Point) { self.bounds = Rect::new(pos, self.bounds.size()); }
    fn children(&self) -> &[ElementId] { &[] }
    fn element_type_name(&self) -> &str { "ColorBox" }
    fn get_classes(&self) -> &[String] { &[] }
    fn mark_dirty(&mut self, flags: DirtyFlags) { self.dirty.insert(flags); }
    fn clear_dirty(&mut self, flags: DirtyFlags) { self.dirty.remove(flags); }
    fn is_dirty(&self, flags: DirtyFlags) -> bool { self.dirty.contains(flags) }
    fn hit_test(&self, point: Point) -> bool { self.bounds.contains(point) }
}
```

## Widget with Children

For compositional widgets, manage children via `mount()` and `child_widgets()`:

```rust
struct MyPanel {
    children: Vec<Box<dyn Widget>>,
}

impl MyPanel {
    fn new() -> Self { Self { children: Vec::new() } }

    fn child(mut self, widget: impl Widget + 'static) -> Self {
        self.children.push(Box::new(widget));
        self
    }
}

impl Widget for MyPanel {
    fn mount(&self, tree: &mut ElementTree, parent_id: ElementId) {
        for child in &self.children {
            let elem = child.create_element();
            let child_id = tree.insert(elem, Some(parent_id));
            child.mount(tree, child_id);
        }
    }

    fn child_widgets(&self) -> Vec<&dyn Widget> {
        self.children.iter().map(|c| c.as_ref() as &dyn Widget).collect()
    }

    // ... rest of Widget impl
}

struct MyPanelElement {
    id: ElementId,
    bounds: Rect,
    child_ids: Vec<ElementId>,
    // ...
}

impl Element for MyPanelElement {
    fn layout_hint(&self) -> LayoutHint {
        LayoutHint::Column { gap: 8.0, cross_align: CrossAxisAlignment::Start,
            padding_left: 16.0, padding_top: 16.0,
            padding_right: 16.0, padding_bottom: 16.0 }
    }

    fn children(&self) -> &[ElementId] { &self.child_ids }

    // ElementTree handles layout and rendering of children
    // based on layout_hint()
    // ...
}
```

## Dynamic Child Rebuild

For widgets that rebuild children in response to state changes:

```rust
impl Element for MyDynamicElement {
    fn needs_rebuild(&self) -> bool {
        self.data_changed
    }

    fn build_children(&self) -> Vec<Box<dyn Widget>> {
        self.items.iter().map(|item| {
            Box::new(Text::new(item)) as Box<dyn Widget>
        }).collect()
    }

    fn clear_rebuild(&mut self) {
        self.data_changed = false;
    }
}
```

The ElementTree calls `rebuild_if_needed()` before layout, which:
1. Walks the tree calling `needs_rebuild()` on each element
2. If true, calls `build_children()` and replaces the subtree
3. Calls `clear_rebuild()` after
4. Re-applies MSS styles

## MSS Integration

To support MSS styling, implement `apply_computed_style()`:

```rust
impl Element for MyElement {
    fn element_type_name(&self) -> &str { "MyWidget" }

    fn set_classes(&mut self, classes: Vec<String>) {
        self.classes = classes;
    }

    fn get_classes(&self) -> &[String] { &self.classes }

    fn apply_computed_style(&mut self, style: &ComputedStyle) {
        self.mss.apply(style); // MssFields helper
    }
}
```

Then style in MSS:

```css
MyWidget {
    background-color: #FFFFFF;
    border-radius: 8px;
    padding: 16px;
}

MyWidget:hover {
    background-color: #F5F5F5;
}
```

## IntoWidget Trait

Widgets can accept closures as children via `IntoWidget`:

```rust
// Closures auto-wrap in Reactive
Column::new()
    .child(move || Text::new(&format!("Value: {}", sig.get())))
```

This works because `.child<M>(impl IntoWidget<M>)` accepts:
- Any `Widget` directly
- `Fn() -> impl Widget` (auto-wrapped in `Reactive`)
