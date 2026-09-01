# Widget Catalog

All widgets follow the builder pattern: `Widget::new(args).method(value).method(value)`.

## Buttons

### Button

```rust
Button::new("Click me")
    .style(ButtonStyle::Primary)     // Primary | Secondary | Text | Danger
    .primary()                        // Shorthand for style(Primary)
    .secondary()                      // Shorthand
    .text_style()                     // Shorthand
    .danger()                         // Shorthand
    .disabled(true)                   // Disable interaction
    .width(120.0)                     // Fixed width
    .height(40.0)                     // Fixed height
    .active_index(signal, idx)        // Bind :selected state to signal
    .on_click(|| { ... })             // Click callback
    .on_click_at(|pos: Point| { ... }) // Click with position
```

### SegmentedButton

```rust
SegmentedButton::new(vec!["Day", "Week", "Month"])
    .selected(0)                      // Initially selected index
    .disabled(false)
    .on_change(|idx| { ... })         // Segment change callback
```

### ToolButton

```rust
ToolButton::new("\u{e88a}")          // Material Icon glyph
    .tooltip("Home")                  // Hover tooltip
    .text("Home")                     // Optional text label
    .active(true)                     // Highlighted state
    .disabled(false)
    .size(24.0)                       // Icon size
    .on_click(|| { ... })
```

### OptionButton

```rust
OptionButton::new("Toggle Option")
    .icon("\u{e88a}")
    .pressed_state(Arc::new(Mutex::new(false)))
    .disabled(false)
    .on_toggle(|pressed: bool| { ... })
```

## Input

### TextField

```rust
TextField::new()
    .text("initial value")
    .placeholder("Enter text...")
    .disabled(false)
    .read_only(false)                 // Can navigate but not edit
    .width(200.0)
    .prefix_icon("\u{e8b6}")          // Left icon
    .suffix_icon("\u{e5cd}")          // Right icon
    .prefix(widget)                   // Arbitrary left widget
    .suffix(widget)                   // Arbitrary right widget
    .on_change(|text: &str| { ... })
    .on_submit(|text: &str| { ... }) // Enter key
    .autofocus(true)                  // Grab keyboard focus on mount
    .clipboard_hint(true)             // Clipboard chip on focus (tap inserts);
                                      // also via MSS `clipboard-hint: on`
```

### MultilineTextEdit

Multi-line text editing with the same API as TextField.

### Checkbox

```rust
Checkbox::new()
    .with_checked(true)
    .label("Accept terms")
    .disabled(false)
    .on_change(|checked: bool| { ... })
```

### RadioButton / RadioGroup

```rust
let group = RadioGroup::new("size").selected("medium");

Column::new()
    .child(RadioButton::new("small", &group).label("Small"))
    .child(RadioButton::new("medium", &group).label("Medium"))
    .child(RadioButton::new("large", &group).label("Large"))
```

### Toggle (Switch)

```rust
Toggle::new()
    .on(false)
    .disabled(false)
    .on_change(|is_on: bool| { ... })
```

### Slider

```rust
Slider::new()
    .value(50.0)
    .range(0.0, 100.0)
    .step(1.0)
    .disabled(false)
    .width(200.0)
    .on_change(|value: f32| { ... })
```

### SpinBox

```rust
SpinBox::new()
    .value(10.0)
    .range(0.0, 100.0)
    .step(1.0)
    .decimal_places(0)
    .disabled(false)
    .width(100.0)
    .on_change(|value: f64| { ... })
```

### Dropdown

```rust
Dropdown::new()
    .item(DropdownItem { value: "a".into(), label: "Option A".into(), icon: None, disabled: false })
    .items(vec![...])
    .selected("a")
    .placeholder("Select...")
    .disabled(false)
    .width(200.0)
    .max_height(300.0)               // Popup max height
    .leading_icon("\u{e164}")
    .on_change(|value: &str| { ... })
```

### Combobox

Text field + dropdown combination with partial text matching. Same API pattern as Dropdown.

### Multiselect

Multiple selection from a list of checkboxes.

### Autocomplete

Text field with suggestion popup. Provides autocomplete as user types.

### DatePicker

```rust
DatePicker::new()                       // плейсхолдер и формат — из локали
    .today()                            // или .selected(Date::new(2026, 8, 20))
    .min_date(Date::today())
    .on_change(|date: Option<Date>| { ... })
```

Попап рисуется той же панелью, что и `Calendar`: русская локаль по умолчанию,
быстрый выбор месяца/года по клику на заголовок, обведённая сегодняшняя дата.

### TimePicker

```rust
TimePicker::new()
    .value(Time::new(14, 30))
    .on_change(|time: Time| { ... })
```

### ColorPicker

Color selection interface.

## Visual

### Text

```rust
Text::new("Hello world")
    .font_size(16.0)
    .bold()
    .color(Color::BLACK)
    .class("heading")
```

### Icon

```rust
Icon::new("\u{e88a}")                // Material Icons glyph
    .size(IconSize::Medium)           // Small(16) | Medium(24) | Large(32)
    .small()                          // Shorthand
    .custom_size(48.0)
    .color(Color::BLUE)
```

### Badge

```rust
Badge::new("3")                      // Text badge
Badge::dot()                         // Dot indicator (no text)
    .color(Color::RED)
    .text_color(Color::WHITE)
    .size(BadgeSize::Medium)          // Small(16) | Medium(20) | Large(24)
    .border_color(Color::WHITE)
    .border_width(2.0)
```

### Avatar

```rust
Avatar::new()
    .text("JD")                       // 1-2 character initials
    .size(40.0)                       // Diameter
    .color(Color::BLUE)               // Background
    .text_color(Color::WHITE)
```

### Card

```rust
Card::new()
    .elevation(4.0)                   // Shadow depth (0-24)
    .border_radius(12.0)
    .padding(16.0)
    .color(Color::WHITE)
    .child(content)
```

### Chip

Compact selectable tag/pill widget with optional close button.

### Image

```rust
Image::new("path/to/image.png")
    .fit(ImageFit::Cover)             // Cover | Contain | Fill | None
    .width(200.0)
    .height(150.0)
```

### Divider

```rust
Divider::horizontal()
Divider::vertical()
```

### ProgressBar

```rust
ProgressBar::new()
    .value(0.65)                      // 0.0 to 1.0
```

### CircularProgress

Animated circular loading spinner.

### RichText

```rust
RichText::new(vec![
    TextSpan::new("Hello ").color(Color::BLACK),
    TextSpan::new("world").color(Color::BLUE).bold(),
])
```

### Calendar

Сетка месяца с выбором даты. Без `.selected(...)` открывается на текущем месяце
с выделенной сегодняшней датой.

```rust
Calendar::new()
    .show_week_numbers(true)            // колонка номеров недель (ISO-8601)
    .on_select(|date: Date| { ... })

Calendar::new().locale(CalendarLocale::english())   // язык конкретного виджета
set_default_locale(CalendarLocale::german());       // язык всего приложения
```

Локали: `russian()` (по умолчанию), `english()`, `german()`, `french()`,
`spanish()`; `CalendarLocale::from_id("ru_RU.UTF-8")` и `detect()` — по
переменным окружения. Локаль задаёт названия месяцев и дней, первый день
недели, выходные и формат даты (`format_date`, `format_long`).

Клик по названию месяца или по году в заголовке открывает быстрый выбор
(сетка 12 месяцев / страница из 12 лет), стрелки листают месяц, год или
страницу лет — по текущему режиму.

**MSS** (общие для `Calendar` и `DatePicker`): `background`, `color`,
`border-color`, `accent-color`, `font-size` и переменные `--cal-panel-bg`,
`--cal-panel-border`, `--cal-muted-color`, `--cal-outside-color`,
`--cal-weekend-color`, `--cal-today-color`, `--cal-selected-color`,
`--cal-hover-bg`, `--cal-disabled-color`, `--cal-cell-size`, `--cal-radius`,
`--cal-font-size`.

### Accordion

Collapsible content sections.

### Canvas (2D Drawing)

```rust
Canvas::new(|ctx: &mut CanvasContext, elapsed: Duration| {
    ctx.fill_rect(Rect::new(Point::new(0.0, 0.0), Size::new(100.0, 50.0)), Color::RED);
    ctx.draw_line(Point::new(0.0, 0.0), Point::new(100.0, 100.0), Color::BLACK, 2.0);
    ctx.fill_circle(Point::new(50.0, 50.0), 20.0, Color::BLUE);
    ctx.stroke_circle(Point::new(50.0, 50.0), 30.0, Color::GREEN, 1.0);
    ctx.draw_polyline(&[p1, p2, p3], Color::BLACK, 1.0);
    ctx.fill_polygon(&[p1, p2, p3], Color::YELLOW);
    ctx.draw_rect(rect, Color::BLACK, 1.0);
    ctx.draw_arc(center, radius, start_angle, end_angle, color, width);
    ctx.draw_bezier(p0, p1, p2, p3, color, width);
})
.size(400.0, 300.0)
.animated(true)                       // Continuous redraw
```

### MapView (feature: `map`)

Geographic map with tile rendering and markers.

### MarkdownView (feature: `markdown`)

Renders markdown content with code highlighting, inline images and clickable links.

```rust
MarkdownView::new(source)                 // impl Into<String>
    .max_width(720.0)
    .with_copy_code(true)                 // copy button on code blocks
    .with_syntax_highlight(true)          // needs feature `markdown-syntax`
    .selectable(true)
    .base_url("/home/u/docs")             // resolve relative img/link paths
    .on_link_click(|url| { /* … */ })     // overrides default open_url
```

`base_url` resolves relative `![](img/a.png)` and `[text](page.md)` references
against a base directory or `http(s)`/`file://` prefix. Absolute URLs (`http://`,
`data:`, `file://`), absolute filesystem paths and in-document `#anchors` pass
through untouched; relative refs under a local base load as files, under an
`http(s)` base join into an absolute URL. Without `base_url` behaviour is
unchanged.

### VideoView (feature: `ffmpeg`)

Renders frames from an ffmpeg-backed `VideoPlayer` (file/stream playback).

```rust
VideoView::new(player)                // Arc<Mutex<VideoPlayer>>
    .fit(ImageFit::Contain)           // Contain | Cover | Fill | None
    .position_signal(pos_sig)         // RwSignal<f32>, seconds (output)
```

### FramesView (feature: `ffmpeg`)

In-memory frame-sequence player: `Vec<Arc<VideoFrame>>` + fps, no file or
decoder. One GPU texture per instance (frames swapped via `update_rgba`).
Click toggles play/pause. Used for generated video previews (LTX nodes).

```rust
FramesView::new(frames, 24.0)         // Arc<Vec<Arc<VideoFrame>>>, fps
    .fit(ImageFit::Contain)
    .autoplay(false)
    .loop_playback(true)
    .playing_signal(play_sig)         // RwSignal<bool>, bidirectional
    .position_signal(pos_sig)         // RwSignal<f32>, seconds; external
                                      // write while paused = seek
```

## Containers

### DecoratedBox

```rust
DecoratedBox::new(Color::WHITE)
DecoratedBox::styled()                // All from MSS
DecoratedBox::card()                  // White, rounded, shadow

    .background(Color::WHITE)
    .radius(8.0)                      // Uniform corner radius
    .corner_radius([8.0, 8.0, 0.0, 0.0]) // [TL, TR, BR, BL]
    .with_border(1.0, Color::GRAY)
    .border_left(2.0, Color::BLUE)    // Per-side borders
    .with_shadow(Color::BLACK.with_alpha(0.2), 8.0, 0.0, 2.0)
    .width(300.0)
    .height(200.0)
    .padding(16.0)
    .clip(true)                       // Clip children to bounds
    .child(content)
```

### Container

```rust
Container::new()
    .width(200.0)
    .height(100.0)
    .background(Color::BLUE)
    .padding(8.0)
    .shadow(shadow)
    .child(content)
```

### Animated (Transforms & Opacity)

```rust
Animated::new(widget)
    .translate_x(Animation::tween(Easing::EaseOutCubic).from(0.0).to(100.0).duration_ms(300))
    .translate_y(animation)
    .scale(animation)
    .scale_x(animation)
    .scale_y(animation)
    .rotate(animation)                // Degrees
    .opacity(animation)               // 0.0–1.0
    .repeat(true)                     // Infinite loop
    .repeat_mode(RepeatMode::PingPong(3))
    .origin(TransformOrigin::Center)  // TopLeft | Center | Custom(x, y)
```

### AnimatedSize

```rust
AnimatedSize::new(widget)
    .duration_ms(300)
    .easing(Easing::EaseOutCubic)
    .clip(true)
    .axis(AnimationAxis::Both)        // Width | Height | Both
```

### Carousel

```rust
Carousel::new()
    .child(page1)
    .child(page2)
    .child(page3)
    .current_page(0)
    .auto_play(true)
    .auto_play_interval_ms(5000)
    .show_indicators(true)
    .on_page_change(|idx| { ... })
```

### ShowIf (Conditional)

```rust
let (tab, set_tab) = create_signal(0usize);

Column::new()
    .child(ShowIf::new(0, tab).child(page_a))
    .child(ShowIf::new(1, tab).child(page_b))
```

### GestureDetector

Transparent gesture handling wrapper:

```rust
GestureDetector::new()
    .on_click(|| { ... })
    .on_click_at(|pos| { ... })
    .on_double_click(|| { ... })
    .on_hover_change(|hovered: bool| { ... })
    .on_mouse_down(|pos| { ... })
    .on_mouse_up(|pos| { ... })
    .cursor(CursorIcon::Pointer)
    .child(content)
```

### Page (Scrollable)

```rust
Page::new()
    .vertical()                       // ScrollDirection
    .horizontal()
    .both()
    .scrollbar_policy(ScrollbarPolicy::Auto) // Auto | Always | Never
    .scrollbar_width(8.0)
    .padding(EdgeInsets::all(24.0))
    .scroll_to(ScrollTarget::Top)     // Top | Bottom | Offset(f32)
    .child(content)
```

### ScrollView

Lower-level scrollable container:

```rust
ScrollView::new()
    .direction(ScrollDirection::Vertical)
    .child(content)
```

## Navigation

### TopAppBar

```rust
TopAppBar::new("App Title")
    .elevation(3.0)
    .height(56.0)
    .bg_color(Color::WHITE)
    .fg_color(Color::BLACK)
    .title_font_size(20.0)
    .shadow_color(Color::BLACK.with_alpha(0.1))
    .gap(8.0)
    .leading(back_button)             // Left widget
    .action(toggle)                   // Right widgets
    .action(badge)
```

### Toolbar

```rust
Toolbar::new()
    .with_title("Section Title")
    .height(48.0)
    .child(tool_button1)
    .child(tool_button2)
```

### Sidebar

```rust
Sidebar::new()
    .width(220.0)
    .header(logo_widget)
    .footer(version_widget)
    .section("Main")                  // Section header
    .item("Dashboard")               // Items
    .item_with_icon("Settings", "\u{e8b8}")
    .selected_signal(signal, set_signal)
    .on_select(|idx| { ... })
    .theme(dark_signal)               // Dark mode signal
    .item_height(40.0)
    .item_radius(8.0)
    .class("my-sidebar")
```

### TabBar / Tab / TabView

```rust
// Low-level: TabBar + Tab
let state: TabState = Arc::new(Mutex::new(0));

TabBar::new()
    .position(TabPosition::Top)       // Top | Bottom | Left | Right
    .tab(Tab::new("Tab 1", 0, state.clone()))
    .tab(Tab::new("Tab 2", 1, state.clone())
        .icon("\u{e88a}")
        .closable()
        .on_close(|| { ... })
    )

// High-level: TabView (tabs + content)
TabView::new()
    .tab("Tab 1", vec![Box::new(content1)])
    .tab_with_icon("Tab 2", "\u{e88a}", vec![Box::new(content2)])
```

### Router / RouterView

```rust
let router = Arc::new(Mutex::new(Router::new(
    vec!["home".into(), "settings".into()],
    "home",
)));

// Navigation
router.lock().unwrap().navigate("settings");
router.lock().unwrap().back();
router.lock().unwrap().forward();

// View
RouterView::new(router)
    .route("home", || Box::new(home_page()))
    .route("settings", || Box::new(settings_page()))
```

### Breadcrumb

```rust
Breadcrumb::new()
    .item("Home")
    .item("Products")
    .item("Details")
    .separator(" > ")
    .on_click(|idx| { ... })
```

### Pagination

```rust
Pagination::new()
    // ... page navigation controls
```

## Overlay

### Dialog

```rust
Dialog::new("Confirm Delete")
    .body("Are you sure?")
    .is_open(open_signal)
    .width(400.0)
    .action(DialogAction::new("Cancel", || { ... }))
    .action(DialogAction::new("Delete", || { ... }).primary())
    .on_close(|| { ... })
```

### Portal (Generic Overlay)

```rust
Portal::new()
    .is_open(open_signal)
    .modal(true)
    .backdrop(true)
    .backdrop_color(Color::BLACK.with_alpha(0.5))
    .width(500.0)
    .anchor(PortalAnchor::Center)     // Center | BottomEnd | TopEnd | BottomStart
    .on_close(|| { ... })
    .child(popup_content)
```

### FloatingWindow

Draggable floating window overlay.

### PopupMenu / MenuItem

```rust
PopupMenu::new()
    .item(MenuItem::new("Cut").shortcut("Ctrl+X"))
    .item(MenuItem::new("Copy").shortcut("Ctrl+C"))
    .item(MenuItem::separator())
    .item(MenuItem::new("Paste").shortcut("Ctrl+V"))
```

### ContextMenu

Right-click context menu.

### Draggable / DropArea

```rust
Draggable::new(drag_content)
    .drag_type("task")
    .payload(task_id.to_string())
    .label("Move task")

DropArea::new(drop_target_content)
    .accept_type("task")
    .on_drop(|data: DragData| { ... })
```

## Data

### ListView

```rust
// Static list
ListView::new(vec![
    ListItem::new("Item 1").secondary("Subtitle").icon("\u{e88a}"),
    ListItem::new("Item 2").trailing("$10"),
])
.item_height(48.0)
.selection_mode(SelectionMode::Single) // None | Single | Multiple
.selected(vec![0])
.on_select(|indices| { ... })

// Virtual scrolling (large lists)
ListView::virtual_new(10_000, |index| {
    ListItem::new(&format!("Item {index}"))
})
.item_height(48.0)
.buffer_size(5)                       // Items buffered outside viewport
```

### TableView

```rust
TableView::new(columns, data)
// Virtual scrolling variant:
TableView::virtual_new(columns, row_count, |index| vec!["col1".into(), "col2".into()])
```

### TreeView

```rust
TreeView::new(vec![
    TreeNode::new("Root")
        .child(TreeNode::new("Child 1"))
        .child(TreeNode::new("Child 2")
            .child(TreeNode::new("Grandchild"))
        ),
])
```

### PropertyGrid

```rust
PropertyGrid::new(vec![
    Property::new("Name", PropertyValue::String("Widget".into())),
    Property::new("Width", PropertyValue::Float(100.0)),
    Property::new("Visible", PropertyValue::Bool(true)),
])
```

## Feedback

### Tooltip

```rust
Tooltip::new(target_widget, "Tooltip text")
    .position(TooltipPosition::Top)   // Top | Bottom | Left | Right
```

### Snackbar

```rust
Snackbar::new("File saved successfully")
    .position(SnackbarPosition::Bottom)
```

### Notification

Хост рисует стек уведомлений в том углу, куда его поставил `Portal`;
одновременно видно не больше трёх карточек, остальные ждут в «колоде» за
ними (два верхних слоя выглядывают на 6 px со scale/opacity-ступенькой).

```rust
// Ctx живёт в контексте приложения, хост монтируется один раз.
let ctx = NotificationCtx::with_default_duration(15_000);

Portal::new()
    .is_open(always_open)
    .modal(false)
    .backdrop(false)
    .anchor(PortalAnchor::BottomEnd { margin_bottom: 72.0, margin_right: 16.0 })
    .child(NotificationHost::new(ctx.clone()).grow_up(true))

// Показать уведомление откуда угодно:
ctx.success("Сохранено");
ctx.show(
    NotificationItem::error("Не удалось загрузить")
        .message("Проверьте путь к модели")
        .duration_ms(8_000),
);
```

`grow_up(true)` — для нижнего якоря: свежая карточка появляется снизу,
прежние уезжают вверх, колода выглядывает над верхней карточкой (иначе
уходила бы за нижний край окна). Severity: `info` | `success` | `warning` |
`error`, каждому соответствует MSS-класс `.severity-*` с `accent-color`.

## Styling with Classes

Any widget can have MSS classes via `WidgetExt`:

```rust
use syngui::prelude::*; // imports WidgetExt

Text::new("Hello").class("heading")
Button::new("OK").class("primary-btn")
Column::new().class("card-body")
```

Multiple classes:

```rust
widget.class("card elevated")     // Space-separated
widget.classes(vec!["a".into(), "b".into()])
```

Debug naming (for DevTools inspector):

```rust
widget.named("header-section")
```
