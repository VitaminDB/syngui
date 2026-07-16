# Accessibility & Debug Tools

## Accessibility (a11y)

SYNGUI includes an accessibility layer for screen readers and assistive technology.

### AccessibilityInfo

```rust
struct AccessibilityInfo {
    role: Role,
    label: Option<String>,
    value: Option<String>,
    state: NodeState,
    properties: NodeProperties,
}
```

### Roles

```rust
enum Role {
    Button,
    Checkbox,
    RadioButton,
    TextField,
    Slider,
    ProgressBar,
    Tab,
    TabPanel,
    Dialog,
    Menu,
    MenuItem,
    List,
    ListItem,
    Tree,
    TreeItem,
    Table,
    Row,
    Cell,
    Image,
    Link,
    Group,
    Toolbar,
    Label,
    ScrollView,
    // ... more
}
```

### NodeState

```rust
struct NodeState {
    focused: bool,
    selected: bool,
    checked: Option<bool>,  // None = not checkable
    expanded: Option<bool>,
    disabled: bool,
}
```

### FocusManager

Manages keyboard focus traversal:

```rust
focus_manager.focus_next()      // Tab
focus_manager.focus_previous()  // Shift+Tab
focus_manager.set_focus(id)     // Focus specific element
focus_manager.current_focus()   // Currently focused element
```

## Debug Overlay

FPS counter, frame time, and draw call statistics:

```rust
App::new()
    .with_debug_overlay(true)   // Show FPS overlay
```

Displays:
- FPS (frames per second)
- Frame time (ms)
- Draw calls per frame
- Vertex count

## DevTools (Inspector)

Element inspector similar to browser DevTools:

```rust
App::new()
    .with_dev_tools(true)       // Enable DevTools
```

Toggle with **F12**. Features:

### Element Inspector
- Visual element tree hierarchy
- Click to select elements in the tree
- View element bounds, type, classes, and ID
- Highlight hovered element

### Styles Panel
- View computed MSS styles for selected element
- See resolved property values

### Profiler
- Frame timing analysis
- Layout/render/paint timing breakdown

### Event Log
- Live event stream
- Filter by event type
- View event targets and results
