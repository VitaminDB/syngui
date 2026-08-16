# SYNGUI — Руководство по разработке

> **Приоритет развития фреймворка**: MSS-стилизация и синтаксический сахар (`use_signal`, `use_effect`, `use_context`, `provide_context`, `create_memo`).
> Все новые виджеты и свойства должны управляться через MSS. Прямые свойства (`.background()`, `.padding()`) — только для удобства, MSS имеет приоритет.

---

## Содержание

1. [Быстрый старт](#1-быстрый-старт)
2. [Архитектура приложения](#2-архитектура-приложения)
3. [Реактивная система (Signals)](#3-реактивная-система-signals)
4. [Макрос mgui!](#4-макрос-mgui)
5. [MSS-стилизация](#5-mss-стилизация)
6. [Каталог виджетов](#6-каталог-виджетов)
7. [Анимации](#7-анимации)
8. [Паттерны и рецепты](#8-паттерны-и-рецепты)
9. [Android-поддержка](#9-android-поддержка)
10. [Системное оформление](#10-системное-оформление)

---

## 1. Быстрый старт

### Минимальное приложение

```rust
use syngui::prelude::*;
use syngui::widgets::*;

fn main() {
    App::new()
        .title("Hello SYNGUI")
        .size(400, 300)
        .run(|_| Box::new(Text::new("Hello, World!")));
}
```

### Приложение с состоянием и стилями

```rust
use syngui::prelude::*;
use syngui::widgets::*;

const STYLES: &str = include_str!("../styles/app.mss");

fn main() {
    let count = use_signal(0i32);
    provide_context(count);

    App::new()
        .title("Counter")
        .size(400, 300)
        .with_styles_str(STYLES)
        .run(|_| Box::new(build_ui()));
}

fn build_ui() -> impl Widget {
    let count = use_context::<RwSignal<i32>>();

    mgui! {
        Column::new().gap(16.0).class("root") => [
            move || {
                let c = count.get();
                Text::new(&format!("Count: {c}")).class("counter-text")
            },
            Button::new("Increment")
                .on_click(move || count.set(count.get_untracked() + 1))
                .class("btn-primary"),
        ]
    }
}
```

---

## 2. Архитектура приложения

### Структура проекта (эталон: app/calculator)

```
my_app/
├── Cargo.toml
├── src/
│   ├── main.rs          # точка входа: вызов lib::run_desktop()
│   └── lib.rs           # вся логика + UI
└── styles/
    └── app.mss          # MSS-стили
```

### Cargo.toml

```toml
[package]
name = "my-app"
version = "0.1.0"
edition = "2021"

[features]
default = ["desktop"]
desktop = ["syngui/clipboard"]
android = ["syngui/android"]

[dependencies]
syngui = { path = "../../syngui" }

[target.'cfg(target_os = "android")'.dependencies]
android_logger = "0.14"
log = "0.4"
```

### Точка входа

**main.rs** — минимальный:
```rust
fn main() {
    my_app::run_desktop();
}
```

**lib.rs** — основная логика:
```rust
use syngui::prelude::*;
use syngui::widgets::*;

const STYLES: &str = include_str!("../styles/app.mss");

pub fn run_desktop() {
    // 1. Создаём состояние
    let ctx = AppState::new();
    provide_context(ctx);

    // 2. Запускаем приложение
    App::new()
        .title("My App")
        .size(800, 600)
        .min_size(400, 300)
        .vsync(true)
        .gpu_backend(GpuBackend::Auto)
        .gpu_power(GpuPowerPreference::LowPower)
        .with_styles_str(STYLES)
        .with_debug_overlay(false)
        .run(|_| Box::new(build_root()));
}
```

### AppBuilder — полный API

| Метод | Описание |
|-------|----------|
| `.title(str)` | Заголовок окна |
| `.size(w, h)` | Начальный размер |
| `.min_size(w, h)` | Минимальный размер |
| `.background(Color)` | Цвет фона окна |
| `.vsync(bool)` | V-Sync |
| `.frame_limit(fps)` | Программный кап FPS для анимационных и debug-overlay redraw'ов (0 = без капа; типовые: 30/60/120). Сигнал/событийные redraw'ы остаются мгновенными. |
| `.gpu_backend(GpuBackend)` | `Auto`, `Vulkan`, `Gl`, `Dx12`, `Metal` |
| `.gpu_power(GpuPowerPreference)` | `HighPerformance`, `LowPower` |
| `.maximized(bool)` | Запуск в полноэкранном режиме |
| `.with_styles(path)` | Загрузить MSS из файла |
| `.with_styles_str(content)` | Загрузить MSS из строки |
| `.with_theme_styles(light, dark, signal)` | Двойная тема |
| `.with_additional_styles(path)` | Добавить дополнительные стили |
| `.with_additional_styles_str(content)` | Добавить стили из строки |
| `.with_dynamic_theme(signal)` | Динамическая смена темы |
| `.with_font_family(family)` | Шрифт по умолчанию |
| `.with_icon_font(data)` | Иконочный шрифт (Material Icons) |
| `.with_debug_overlay(bool)` | FPS-оверлей |
| `.with_dev_tools(bool)` | DevTools (F12) |
| `.with_android_app(app)` | Android event loop |
| `.run(|ctx| Box::new(widget))` | Запуск |

---

## 3. Реактивная система (Signals)

### Основные функции (синтаксический сахар)

```rust
// Создание сигнала (read-write)
let count = use_signal(0i32);           // → RwSignal<i32>
let name = use_signal(String::new());   // → RwSignal<String>

// Чтение (с подпиской — внутри Reactive/замыкания)
let value = count.get();

// Чтение (без подписки — в обработчиках событий)
let value = count.get_untracked();

// Запись (уведомляет подписчиков)
count.set(42);

// Обновление на месте
count.update(|v| *v += 1);

// Производное значение (мемоизация)
let doubled = create_memo(move || count.get() * 2);
let d = doubled.get(); // автоподписка

// Эффект (побочное действие при изменении зависимостей)
create_effect(move || {
    let c = count.get();
    log::info!("Count changed: {c}");
});

// Эффект с очисткой
create_effect_with_cleanup(move || {
    let c = count.get();
    // ... setup
    Some(Box::new(move || { /* cleanup */ }) as Box<dyn Fn()>)
});
```

### Контекст (Dependency Injection)

```rust
// Регистрация (до App::run)
provide_context(my_state);

// Получение (в любом месте дерева виджетов)
let state = use_context::<MyState>();
let state = try_use_context::<MyState>(); // Option<MyState>
```

### Паттерн: Copy-контекст с сигналами

```rust
#[derive(Clone, Copy)]
struct AppState {
    items: RwSignal<Vec<String>>,
    selected: RwSignal<Option<usize>>,
    loading: RwSignal<bool>,
}

impl AppState {
    fn new() -> Self {
        Self {
            items: use_signal(vec![]),
            selected: use_signal(None),
            loading: use_signal(false),
        }
    }
}
```

**Почему Copy**: `RwSignal<T>` реализует `Copy` (это просто ID слота). Структура с сигналами тоже `Copy` — можно передавать по значению в замыкания без `clone()`.

### Реактивные замыкания

Замыкание `move || { ... }` внутри дерева виджетов автоматически оборачивается в `Reactive`:

```rust
Column::new()
    .child(move || {
        // Вызывается заново при изменении любого .get() сигнала
        let name = name_signal.get();
        Text::new(&name)
    })
    .child(Text::new("Статичный текст")) // НЕ реактивный
```

### Правила:

| Контекст | Использовать | Причина |
|----------|-------------|---------|
| Внутри `move \|\|` замыкания UI | `.get()` | Автоподписка на изменения |
| В обработчике `on_click` | `.get_untracked()` | Обработчик не перестраивает UI |
| В `create_effect` | `.get()` | Эффект должен отслеживать |
| При записи | `.set(value)` | Уведомляет подписчиков |

---

## 4. Макрос mgui!

Макрос `mgui!` — декларативный синтаксис для построения деревьев виджетов:

```rust
mgui! {
    Parent::new() => [
        Child1::new(),
        Child2::new() => [
            GrandChild::new(),
        ],
        move || { reactive_child() },
    ]
}
```

Оператор `=>` добавляет дочерние виджеты. Эквивалент:
```rust
Parent::new()
    .child(Child1::new())
    .child(Child2::new().child(GrandChild::new()))
    .child(move || reactive_child())
```

---

## 5. MSS-стилизация

### Приоритет: MSS > прямые свойства

MSS (MGUI Style Sheets) — CSS-подобный язык стилей. **Все визуальные свойства** должны задаваться через MSS. Прямые builder-методы виджетов — только для layout и поведения.

### Загрузка стилей

```rust
// Из строки (рекомендуется — compile-time embedding)
const STYLES: &str = include_str!("../styles/app.mss");
App::new().with_styles_str(STYLES)

// Из файла
App::new().with_styles("styles/app.mss")
```

### Синтаксис MSS

```css
/* Переменные */
:root {
    --primary: #3B82F6;
    --bg: #1a1a2e;
    --text: #e8e8e8;
    --radius: 8px;
}

/* По типу виджета */
Button {
    border-radius: var(--radius);
    font-size: 16px;
    padding: 12px 24px;
    transition: background 150ms ease;
}

/* По классу */
.card {
    background: #ffffff;
    border-radius: 12px;
    padding: 16px;
    box-shadow: 0 2px 8px rgba(0,0,0,0.1);
}

/* Составные селекторы */
Button.primary { background: var(--primary); }
Button.primary:hover { background: #2563EB; }
Button.primary:pressed { background: #1D4ED8; }
Button.primary:disabled { opacity: 0.5; }

/* Псевдоклассы — состояние элемента */
:hover, :active, :focus, :selected, :checked, :disabled, :pressed

/* Псевдоклассы — состояние ОКНА (глобальные, синхронизируются с winit) */
:window-maximized   /* окно сейчас maximized */
:window-fullscreen  /* окно сейчас в borderless fullscreen */
:window-focused     /* окно сейчас имеет фокус */

/* Пример: «настоящий» maximize в frameless-окне (без padding и rounded) */
.window-backdrop {
    padding: 30px;
    transition: padding 200ms ease;
}
.window-backdrop:window-maximized { padding: 0; }
.shell {
    border-radius: 12px;
    box-shadow: 0 12px 36px rgba(0,0,0,0.12);
    transition: border-radius 200ms ease, box-shadow 200ms ease;
}
.shell:window-maximized { border-radius: 0; box-shadow: none; }

/* Вложенность (nesting) */
.sidebar {
    background: #1e1e2e;
    .item { padding: 8px 16px; }
    .item:hover { background: #2a2a3e; }
}
```

### Применение классов в коде

```rust
Button::new("OK").class("primary")
Text::new("Title").class("heading")
Column::new().class("sidebar")

// Несколько классов
Button::new("Delete").class("btn").class("danger")
// или
Button::new("Delete").classes(vec!["btn", "danger"])
```

### Все MSS-свойства

| Свойство | Описание | Пример |
|----------|----------|--------|
| `background` | Фон (цвет, градиент) | `#FF0000`, `var(--bg)` |
| `color` | Цвет текста/переднего плана | `#ffffff` |
| `border` | Граница (shorthand) | `1px solid #ccc` |
| `border-radius` | Скругление углов | `8px`, `50%` |
| `border-color` | Цвет границы | `#333` |
| `border-width` | Толщина границы | `2px` |
| `padding` | Внутренний отступ | `16px`, `8px 16px` |
| `width` / `height` | Размеры | `200px`, `100%` |
| `font-size` | Размер шрифта | `16px` |
| `font-weight` | Жирность | `400`, `700`, `bold` |
| `font-family` | Семейство шрифтов | `"Inter"` |
| `text-align` | Выравнивание текста | `left`, `center`, `right` |
| `text-decoration` | Декорация текста | `underline` |
| `opacity` | Прозрачность | `0.5` |
| `box-shadow` | Тень | `0 2px 8px rgba(0,0,0,0.2)` |
| `text-shadow` | Тень текста (offset + blur + color) | `2 2 4 rgba(0,0,0,0.6)`. `blur_radius` рендерится gaussian'ом в шейдере (multi-tap); `blur=0` идёт по pixel-identical fast-path'у. Emoji не блюрятся в v1. |
| `cursor` | Курсор мыши | `pointer`, `text` |
| `overflow` | Обрезка содержимого | `hidden`, `visible` |
| `gap` | Промежуток между детьми | `8px` |
| `transition` | Анимация перехода | `background 200ms ease` |
| `accent-color` | Акцентный цвет | `#3B82F6` |

### MSS element type names

Имена виджетов для селекторов по типу:

| Виджет (Rust) | Имя в MSS |
|---------------|-----------|
| `Button` | `Button` |
| `Text` | `Text` |
| `TextField` | `TextField` |
| `Checkbox` | `Checkbox` |
| `Toggle` | `Toggle` |
| `RadioButton` | `RadioButton` |
| `Slider` | `Slider` |
| `Dropdown` | `Dropdown` |
| `Container` | `Container` |
| `DecoratedBox` | `DecoratedBox` |
| `TransformBox` | `TransformBox` |
| `Card` | `Card` |
| `Row` | `Row` |
| `Column` | `Column` |
| `Grid` | `Grid` |
| `Stack` | `Stack` |
| `Flex` | `Flex` |
| `ListView` | `ListView` |
| `ListItem` | `ListItem` |
| `TableView` | `TableView` |
| `TabBar` | `TabBar` |
| `Tab` | `Tab` |
| `Sidebar` | `Sidebar` |
| `Toolbar` | `Toolbar` |
| `TopAppBar` | `TopAppBar` |
| `Dialog` | `Dialog` |
| `Tooltip` | `Tooltip` |
| `Icon` | `Icon` |
| `Image` | `Image` |
| `Avatar` | `Avatar` |
| `Badge` | `Badge` |
| `Chip` | `Chip` |
| `ProgressBar` | `ProgressBar` |
| `CircularProgress` | `CircularProgress` |
| `Divider` | `Divider` |
| `ScrollView` | `ScrollView` |
| `Accordion` | `Accordion` |

### Каскад и наследование

MSS реализует CSS-подобный каскад:

**Комбинаторы селекторов.** Работают `descendant` (пробел), `child` (`>`), `+`, `~`, группы через запятую и универсальный `*`:

```css
.card .title { font-size: 18px; }  /* любой потомок */
.card > Button { ... }              /* прямой ребёнок */
.title + .subtitle { ... }           /* соседний */
*, .reset { margin: 0; }             /* все элементы */
```

**Наследуемые свойства.** Набор CSS-подобный (не все свойства): `color`, `font-family`, `font-size`, `font-weight`, `letter-spacing`, `text-align`, `text-vertical-align`, `text-decoration`, `text-transform`, `text-shadow`, `cursor`, `line-height`. Применяются автоматически на всех потомках, если свои правила не переопределили:

```css
.app {
    color: #111;
    font-family: "Inter";
    font-size: 14px;
}
/* Все Text/Button/... внутри .app подхватят color/font без явного указания */
```

Остальные свойства (`padding`, `background`, `border`, `width` и т.п.) **не** наследуются — как в CSS.

**Ключевые слова.** Поддерживаются `inherit`, `initial`, `unset`:

- `inherit` — взять значение у родителя для этого свойства (работает даже для не-наследуемых свойств).
- `initial` — сбросить к «нет значения» (эквивалент отсутствия правила).
- `unset` — `inherit` для наследуемых свойств, `initial` для остальных.

```css
.reset-color { color: inherit; }
.no-padding { padding: unset; }
```

---

## 6. Каталог виджетов

### Классификация: прямые свойства vs MSS

**Прямые свойства** — builder-методы виджета (layout, поведение, размеры).
**MSS** — визуальные стили через CSS-классы.

> **Правило**: Для визуальных свойств (цвета, шрифты, скругления, тени) используйте MSS. Прямые свойства — только для layout и callbacks.

---

### Кнопки

#### Button
```rust
Button::new("Label")
    .style(ButtonStyle::Primary)    // Primary, Secondary, Text, Danger
    .disabled(bool)
    .width(f32) / .height(f32)
    .on_click(|| { ... })
    .on_click_at(|pos| { ... })
    .active_index(signal, index)
    .class("my-btn")
```
**Прямые**: `style`, `disabled`, `width`, `height`, `on_click`, `active_index`
**MSS**: `background`, `color`, `border-radius`, `font-size`, `font-weight`, `padding`, `cursor`, `transition`, `:hover`, `:pressed`, `:disabled`

#### ToolButton
```rust
ToolButton::new("icon_name")
    .tooltip("Hint")
    .disabled(bool)
    .on_click(|| { ... })
    .class("tool-btn")
```
**Прямые**: `tooltip`, `disabled`, `on_click`
**MSS**: `background`, `color`, `border-radius`, `padding`

#### SegmentedButton
```rust
SegmentedButton::new(vec!["One", "Two", "Three"])
    .selected(signal)
    .on_select(|index| { ... })
    .class("segment")
```
**Прямые**: `selected`, `on_select`
**MSS**: `background`, `color`, `border-radius`, `:selected`

#### OptionButton
```rust
OptionButton::new("Toggle me")
    .checked(bool)
    .on_change(|checked| { ... })
    .class("option")
```
**Прямые**: `checked`, `on_change`
**MSS**: `background`, `color`, `:checked`

---

### Ввод данных (Input)

#### TextField
```rust
TextField::new()
    .text("initial")
    .placeholder("Type here...")
    .disabled(bool) / .read_only(bool)
    .width(f32)
    .prefix(widget) / .suffix(widget)
    .prefix_icon("search") / .suffix_icon("clear")
    .on_change(|text| { ... })
    .on_submit(|text| { ... })
    .class("input")
```
**Прямые**: `text`, `placeholder`, `disabled`, `read_only`, `width`, `prefix`, `suffix`, `on_change`, `on_submit`
**MSS**: `background`, `color`, `border`, `border-radius`, `font-size`, `padding`, `:focus`, `:disabled`

#### MultilineTextEdit
```rust
MultilineTextEdit::new()
    .text("content")
    .placeholder("Enter text...")
    .width(f32) / .height(f32)
    .read_only(bool)
    .on_change(|text| { ... })
    .class("editor")
```
**Прямые**: `text`, `placeholder`, `width`, `height`, `read_only`, `on_change`
**MSS**: `background`, `color`, `border`, `font-size`, `padding`

#### Checkbox
```rust
Checkbox::new("Accept terms")
    .checked(bool)
    .disabled(bool)
    .on_change(|checked| { ... })
    .class("check")
```
**Прямые**: `checked`, `disabled`, `label`, `on_change`
**MSS**: `color`, `accent-color`, `:checked`, `:disabled`

#### Toggle
```rust
Toggle::new()
    .checked(bool)
    .label("Dark mode")
    .disabled(bool)
    .on_change(|on| { ... })
    .class("toggle")
```
**Прямые**: `checked`, `label`, `disabled`, `on_change`
**MSS**: `accent-color`, `:checked`, `:disabled`

#### RadioButton / RadioGroup
```rust
RadioGroup::new(vec!["A", "B", "C"])
    .selected(index)
    .on_change(|index| { ... })
    .class("radio")
```
**Прямые**: `selected`, `on_change`
**MSS**: `color`, `accent-color`, `:selected`

#### Slider
```rust
Slider::new()
    .value(0.5)
    .range(0.0, 1.0)
    .step(0.01)
    .width(200.0)
    .disabled(bool)
    .on_change(|val| { ... })
    .class("slider")
```
**Прямые**: `value`, `range`, `step`, `width`, `disabled`, `on_change`
**MSS**: `accent-color`, `background`

#### SpinBox
```rust
SpinBox::new()
    .value(10)
    .range(0, 100)
    .step(1)
    .width(120.0)
    .on_change(|val| { ... })
```
**Прямые**: `value`, `range`, `step`, `width`, `on_change`
**MSS**: `background`, `color`, `border`

#### Dropdown
```rust
Dropdown::new(vec![
    DropdownItem::new("Option A"),
    DropdownItem::new("Option B"),
])
    .selected(index)
    .placeholder("Select...")
    .width(200.0)
    .on_change(|index| { ... })
    .class("dropdown")
```
**Прямые**: `selected`, `placeholder`, `width`, `on_change`
**MSS**: `background`, `color`, `border`, `border-radius`

#### Combobox
```rust
Combobox::new(vec!["Apple", "Banana", "Cherry"])
    .text("initial")
    .placeholder("Search...")
    .width(200.0)
    .on_change(|text| { ... })
    .on_select(|index| { ... })
```
**Прямые**: `text`, `placeholder`, `width`, `on_change`, `on_select`
**MSS**: `background`, `color`, `border`

#### Multiselect
```rust
Multiselect::new(vec!["Tag1", "Tag2", "Tag3"])
    .selected(vec![0, 2])
    .width(300.0)
    .on_change(|indices| { ... })
```
**Прямые**: `selected`, `width`, `on_change`
**MSS**: `background`, `border`

#### Autocomplete
```rust
Autocomplete::new()
    .suggestions(vec!["rust", "react", "ruby"])
    .placeholder("Search...")
    .width(250.0)
    .on_change(|text| { ... })
    .on_select(|item| { ... })
```
**Прямые**: `suggestions`, `placeholder`, `width`, `on_change`, `on_select`
**MSS**: `background`, `color`, `border`

#### DatePicker
```rust
DatePicker::new()
    .value(Date::new(2026, 3, 20))
    .on_change(|date| { ... })
    .width(200.0)
```
**Прямые**: `value`, `on_change`, `width`
**MSS**: `background`, `color`, `border`

#### TimePicker
```rust
TimePicker::new()
    .value(Time::new(14, 30))
    .on_change(|time| { ... })
    .width(150.0)
```
**Прямые**: `value`, `on_change`, `width`
**MSS**: `background`, `color`, `border`

#### ColorPicker
```rust
ColorPicker::new()
    .value(ColorValue::hex("#3B82F6"))
    .on_change(|color| { ... })
```
**Прямые**: `value`, `on_change`
**MSS**: `border`

---

### Контейнеры (Layout)

#### Column
```rust
Column::new()
    .gap(16.0)
    .cross_axis_alignment(CrossAxisAlignment::Stretch)
    .width(f32) / .height(f32)
    .clip(bool)
    .child(widget)
    .class("column")
```
**Прямые**: `gap`, `cross_axis_alignment`, `width`, `height`, `clip`
**MSS**: `background`, `padding`, `gap`, `border`

#### Row
```rust
Row::new()
    .gap(8.0)
    .main_axis_alignment(MainAxisAlignment::SpaceBetween)
    .cross_axis_alignment(CrossAxisAlignment::Center)
    .width(f32) / .height(f32)
    .clip(bool)
    .child(widget)
    .class("row")
```
**Прямые**: `gap`, `main_axis_alignment`, `cross_axis_alignment`, `width`, `height`, `clip`
**MSS**: `background`, `padding`, `gap`, `border`

#### Grid
```rust
Grid::new(4)     // 4 колонки
    .gap(10.0)
    .width(f32) / .height(f32)
    .child(widget)
    .class("grid")
```
**Прямые**: `columns`, `gap`, `width`, `height`
**MSS**: `background`, `padding`, `gap`

#### Flex
```rust
Flex::new(FlexDirection::Row)
    .gap(8.0)
    .wrap(true)
    .main_axis_alignment(MainAxisAlignment::Center)
    .cross_axis_alignment(CrossAxisAlignment::Stretch)
    .child(widget)
    .class("flex")
```
**Прямые**: `direction`, `gap`, `wrap`, `main_axis_alignment`, `cross_axis_alignment`
**MSS**: `background`, `padding`, `gap`

#### Stack
```rust
Stack::new()
    .fit(StackFit::Expand)
    .child(widget)  // слои друг на друге
    .class("stack")
```
**Прямые**: `fit`
**MSS**: `background`

#### Flex-grow (заполнение свободного места)
Flex задаётся через MSS-свойство `flex-grow` (CSS-совместимое поведение в Row/Column).
Удобный шаблон — завести классы `.grow*` в stylesheet приложения:

```mss
.grow   { flex-grow: 1; }
.grow-2 { flex-grow: 2; }
.grow-3 { flex-grow: 3; }
```

Применение в коде:
```rust
Row::new()
    .child(Text::new("Label"))
    .child(TextField::new().class("grow"))    // одиночный expand
    .child(Button::new("OK"))

Row::new()
    .child(panel_a.class("grow-2"))
    .child(panel_b.class("grow"))             // пропорции 2:1
```

Для runtime-значений (flex из переменной) — inline-style:
```rust
widget.style("flex-grow", StyleValue::Number(factor))
```

Занять всю ширину/высоту родителя — через MSS:
```mss
.fill-w { width: 100%; }
.fill-h { height: 100%; }
```

#### Padding
```rust
Padding::all(16.0)
Padding::symmetric(8.0, 16.0)     // vertical, horizontal
Padding::only(left, top, right, bottom)
    .child(widget)
```
**Прямые**: `all`, `symmetric`, `only`, `left`, `top`, `right`, `bottom`
**Только layout** — без визуальных свойств. Для стилизации используйте MSS `padding` на родителе.

#### Center
```rust
Center::new(widget)
```
**Только layout** — без свойств.

#### Container
```rust
Container::new()
    .width(f32) / .height(f32)
    .background(Color)
    .padding(f32)
    .shadow(Shadow { ... })
    .child(widget)
    .class("container")
```
**Прямые**: `width`, `height`, `background`, `padding`, `shadow`
**MSS**: `background`, `padding`, `border`, `border-radius`, `box-shadow`
**MSS имеет приоритет** над прямыми `background` и `padding`.

#### DecoratedBox
```rust
DecoratedBox::new(Color::from_hex("#1a1a2e"))
    .border_radius(12.0)
    .clip(true)
    .child(widget)
    .class("decorated")
```
**Прямые**: `background` (конструктор), `border_radius`, `clip`
**MSS**: `background`, `border`, `border-radius`, `box-shadow`, `padding`
**MSS имеет приоритет** над конструктором.

#### Card
```rust
Card::new()
    .elevation(4.0)
    .border_radius(12.0)
    .padding(16.0)
    .color(Color::WHITE)
    .child(widget)
    .class("card")
```
**Прямые**: `elevation`, `border_radius`, `padding`, `color`
**MSS**: `background`, `border-radius`, `padding`, `box-shadow`

#### SplitView
```rust
SplitView::new(left_widget, right_widget)
    .direction(SplitDirection::Horizontal)
    .initial_ratio(0.3)
    .min_size(200.0)
```
**Прямые**: `direction`, `initial_ratio`, `min_size`
**MSS**: `background`

#### TransformBox
```rust
let active = use_signal(true);
let pos = use_signal(Point::zero());
let size = use_signal(Size::new(200.0, 120.0));
let rot = use_signal(0.0_f32);

TransformBox::new()
    .active(active)               // RwSignal<bool> — toggle handles
    .position(pos)                // RwSignal<Point> — bind offset
    .size_signal(size)            // RwSignal<Size> — bind size
    .rotation(rot)                // RwSignal<f32> — bind rotation (degrees)
    .initial_size(200.0, 120.0)   // starting dimensions
    .min_size(40.0, 40.0)         // minimum resize
    .resizable(true)              // 8 resize handles
    .rotatable(true)              // rotation handle above top-center
    .moveable(true)               // drag body to move
    .child(widget)
```
**Прямые**: `resizable`, `rotatable`, `moveable`, `initial_size`, `min_size`
**Сигналы**: `active`, `position`, `size_signal`, `rotation`
**MSS**: `--tb-border-color`, `--tb-border-width`, `--tb-handle-size`, `--tb-handle-color`, `--tb-handle-border-color`

Interactive Figma-like selection handles. 8 resize handles at corners/edges,
rotation handle above top-center, body drag for move. All capabilities
independently toggleable. Handles rotate with content, cursors adapt to rotation angle.

#### Page (Scroll container)
```rust
Page::new()
    .scrollbar_policy(ScrollbarPolicy::Auto)
    .child(widget)
    .class("page")
```
**Прямые**: `scrollbar_policy`
**MSS**: `background`

#### ScrollView
```rust
ScrollView::new()
    .direction(ScrollDirection::Vertical)
    .child(widget)
    .class("scroll")
```
**Прямые**: `direction`
**MSS**: `background`

#### Carousel
```rust
Carousel::new()
    .child(image1)
    .child(image2)
    .child(image3)
    .class("carousel")
```
**MSS**: `background`

#### ShowIf
```rust
ShowIf::new(|| condition_signal.get(), widget)
```
**Только логика** — показывает/скрывает дочерний виджет.

#### GestureDetector
```rust
GestureDetector::new()
    .on_tap(|| { ... })
    .on_double_tap(|| { ... })
    .on_long_press(|| { ... })
    .child(widget)
```
**Только поведение** — без визуальных свойств.

#### Reactive
```rust
Reactive::new(move || {
    let items = items_signal.get();
    items.iter().map(|item| {
        Box::new(Text::new(item)) as Box<dyn Widget>
    }).collect()
})
```
**Только логика** — перестраивает детей при изменении сигналов. Обычно используется неявно через замыкания `move || { ... }`.

#### Named
```rust
Named::new("debug-label", widget)
```
**Только отладка** — имя видно в DevTools.

---

### Визуальные виджеты

#### Text
```rust
Text::new("Hello")
    .class("heading")
```
**Прямые**: только текст (конструктор)
**MSS**: `color`, `font-size`, `font-weight`, `font-family`, `text-align`, `text-decoration`

#### Icon
```rust
Icon::new("settings")
    .size(IconSize::Medium)     // Small(18), Medium(24), Large(36), Custom(f32)
    .color(Color::WHITE)
    .class("icon")
```
**Прямые**: `size`, `color`
**MSS**: `color`, `font-size`

#### Image
```rust
Image::new("path/to/image.png")
    .width(200.0) / .height(150.0)
    .fit(ImageFit::Cover)       // Cover, Contain, Fill, None
    .class("img")
```
**Прямые**: `width`, `height`, `fit`, `src`
**MSS**: `border-radius`, `opacity`

#### Avatar
```rust
Avatar::new("path/to/photo.png")
    .size(48.0)
    .fallback("JD")     // инициалы если нет изображения
    .class("avatar")
```
**Прямые**: `size`, `fallback`, `src`
**MSS**: `border`, `border-radius`

#### Badge
```rust
Badge::new("3")
    .size(BadgeSize::Small)
    .class("badge")
```
**Прямые**: `size`
**MSS**: `background`, `color`, `font-size`

#### Chip
```rust
Chip::new("Tag")
    .deletable(true)
    .on_delete(|| { ... })
    .class("chip")
```
**Прямые**: `deletable`, `on_delete`
**MSS**: `background`, `color`, `border-radius`, `padding`

#### Card
(см. раздел Контейнеры)

#### Divider
```rust
Divider::new()
    .direction(DividerDirection::Horizontal)
    .class("divider")
```
**Прямые**: `direction`
**MSS**: `background`, `height`

#### ProgressBar
```rust
ProgressBar::new()
    .value(0.65)            // 0.0 — 1.0
    .width(300.0)
    .class("progress")
```
**Прямые**: `value`, `width`
**MSS**: `background`, `accent-color`, `border-radius`, `height`

#### CircularProgress
```rust
CircularProgress::new()
    .value(0.7)             // None = indeterminate
    .size(48.0)
    .class("spinner")
```
**Прямые**: `value`, `size`
**MSS**: `color`, `accent-color`

#### Canvas
```rust
Canvas::new(|ctx, elapsed| {
    ctx.fill_rect(0.0, 0.0, 100.0, 50.0, Color::RED);
    ctx.draw_line(0.0, 0.0, 100.0, 50.0, Color::WHITE, 2.0);
    ctx.fill_circle(50.0, 25.0, 20.0, Color::BLUE);
})
.size(300.0, 200.0)
.animated(true)             // перерисовывать каждый кадр
```
**Прямые**: `size`, `animated`, рисующее замыкание
**MSS**: нет — рисование через замыкание.

#### RichText / TextSpan
```rust
RichText::new(vec![
    TextSpan::new("Bold ").bold(),
    TextSpan::new("and "),
    TextSpan::new("colored").color(Color::RED),
])
```
**Прямые**: `spans`
**MSS**: `font-size`, `color` (базовые)

#### Calendar
```rust
Calendar::new()
    .selected(Date::new(2026, 3, 20))
    .on_select(|date| { ... })
```
**Прямые**: `selected`, `on_select`
**MSS**: `background`, `color`, `accent-color`

#### Accordion
```rust
Accordion::new(vec![
    AccordionSection::new("Section 1", content1),
    AccordionSection::new("Section 2", content2),
])
```
**Прямые**: секции
**MSS**: `background`, `border`, `color`

#### MarkdownView (feature: "markdown")
```rust
MarkdownView::new("# Hello\n\nSome **bold** text\n\n```rust\nfn main() {}\n```")
    .max_width(600.0)
    .with_syntax_highlight(true)   // требует feature `markdown-syntax`
    .with_copy_code(true)          // кнопка copy на каждом code-блоке
    .class("markdown-view")

// Альтернатива: получить готовый редактор тем же вызовом
let editor: Box<dyn Widget> = MarkdownView::new(src).with_editable(true);
```
**Прямые**: `max_width`, `with_syntax_highlight`, `with_copy_code`, `with_highlighter(Arc<dyn CodeHighlighter>)`, `with_editable(bool) → Box<dyn Widget>` (возвращает MarkdownEditor)
**Парсер**: pulldown-cmark + GFM (tables, tasklists, strikethrough, footnotes); пост-обработка — heading anchors (slug-id) и autolinks (`http(s)://...`)
**MSS**: `color`, `font-size`, `line-height`, `width`; кастомные `--md-heading-*`, `--md-code-*`, `--md-code-block-*`, `--md-quote-*`, `--md-list-*`, `--md-table-*`, `--md-hr-*`, `--md-link-color`, `--md-bullet-color`, `--md-checkbox-*`, `--md-block-spacing`, `--md-strikethrough-color`, `--md-image-*`, `--md-footnote-color`, `--md-footnote-divider-color`, `--md-copy-bg`, `--md-copy-bg-hover`, `--md-copy-color`, `--md-copy-radius`, `--md-copy-size`, `--md-copy-margin`, `--md-copy-flash-bg`

#### MarkdownEditor (feature: "markdown")
```rust
let text = use_signal(String::from("# Hello"));
MarkdownEditor::new(text)
    .show_toolbar(true)
    .syntax_highlight(true)
    .copy_code(true)
    .rows(14)
    .split_ratio(0.5)
    .class("markdown-editor")
```
Composite-виджет: тулбар + body. Тулбар переключает `EditorMode::{Edit, Preview, Split}`. Edit — `MultilineTextEdit` с `soft_wrap`; Preview — `ScrollView<MarkdownView>`; Split — `SplitView`.
**Прямые**: `mode(RwSignal<EditorMode>)`, `initial_mode`, `show_toolbar`, `syntax_highlight`, `copy_code`, `line_numbers`, `rows`, `split_ratio`, `on_change`
**MSS**: `.markdown-editor`, `.markdown-editor .toolbar`, `.markdown-editor .editor-pane`, `.markdown-editor .preview-pane`, `.markdown-editor .preview-md`, `.markdown-editor .split-pane`

#### MapView (feature: "map")
```rust
MapView::new()
    .center(55.75, 37.62)
    .zoom(12)
    .markers(vec![MapMarker::new(55.75, 37.62, "Moscow")])
    .tile_provider(TileProvider::OpenStreetMap)
```
**Прямые**: `center`, `zoom`, `markers`, `tile_provider`
**MSS**: `border`

---

### Данные (Data)

#### ListView
```rust
// Обычный список
ListView::new(items.iter().map(|i| ListItem::new(i)).collect())
    .selection_mode(SelectionMode::Single)
    .on_select(|indices| { ... })
    .width(300.0) / .height(400.0)
    .class("list")

// Виртуальный список (для тысяч элементов)
ListView::virtual_new(10000, |index| {
    ListItem::new(&format!("Item {index}"))
})
.item_height(48.0)
.buffer_size(5)
```
**Прямые**: `selection_mode`, `on_select`, `width`, `height`, `item_height`, `buffer_size`
**MSS**: `background`, `border`

#### ListItem
```rust
ListItem::new("Title")
    .subtitle("Description")
    .leading(Icon::new("folder"))
    .trailing(Icon::new("chevron_right"))
    .on_click(|| { ... })
    .class("list-item")
```
**Прямые**: `subtitle`, `leading`, `trailing`, `on_click`
**MSS**: `background`, `color`, `padding`, `:hover`, `:selected`

#### TableView
```rust
TableView::new(
    vec![
        TableColumn::new("Name").width(ColumnWidth::Flex(2.0)),
        TableColumn::new("Age").width(ColumnWidth::Fixed(80.0)),
    ],
    data,  // Vec<Vec<String>>
)
.selection_mode(SelectionMode::Multi)
.on_select(|rows| { ... })
.width(600.0) / .height(400.0)

// Виртуальная таблица
TableView::virtual_new(columns, 100000, |row_index| {
    vec!["col1".into(), "col2".into()]
})
```
**Прямые**: `columns`, `data`, `selection_mode`, `on_select`, `width`, `height`
**MSS**: `background`, `border`, `color`

#### TreeView
```rust
TreeView::new(vec![
    TreeNode::new("Root")
        .children(vec![
            TreeNode::new("Child 1"),
            TreeNode::new("Child 2"),
        ]),
])
.on_select(|node| { ... })
```
**Прямые**: `nodes`, `on_select`
**MSS**: `background`, `color`

#### PropertyGrid
```rust
PropertyGrid::new(vec![
    Property::new("Name", PropertyValue::Text("John".into())),
    Property::new("Age", PropertyValue::Number(30.0)),
    Property::new("Active", PropertyValue::Bool(true)),
])
.on_change(|prop, val| { ... })
```
**Прямые**: `properties`, `on_change`
**MSS**: `background`, `border`, `color`

---

### Навигация (Navigation)

#### TabBar / Tab / TabView
```rust
// Только вкладки (кнопки)
TabBar::new(vec![
    Tab::new("Tab 1"),
    Tab::new("Tab 2").icon("settings"),
])
.selected(signal)
.on_select(|index| { ... })
.class("tabs")

// Вкладки + содержимое
TabView::new(signal, vec![
    TabViewPage::new("Tab 1", content1),
    TabViewPage::new("Tab 2", content2),
])
.tab_position(TabPosition::Top)
```
**Прямые**: `selected`, `on_select`, `tab_position`
**MSS**: `background`, `color`, `border`, `:selected`

#### Toolbar
```rust
Toolbar::new("Title")
    .leading(Icon::new("menu"))
    .action(ToolButton::new("search"))
    .action(ToolButton::new("more_vert"))
    .class("toolbar")
```
**Прямые**: `title`, `leading`, `action`
**MSS**: `background`, `color`, `border`, `box-shadow`, `padding`

#### TopAppBar
```rust
TopAppBar::new("Page Title")
    .leading(ToolButton::new("arrow_back").on_click(|| { ... }))
    .action(ToolButton::new("share"))
    .class("appbar")
```
**Прямые**: `title`, `leading`, `action`
**MSS**: `background`, `color`, `box-shadow`

#### Sidebar
```rust
Sidebar::new(vec![
    SidebarItem::new("Home").icon("home"),
    SidebarItem::new("Settings").icon("settings"),
])
.selected(signal)
.on_select(|index| { ... })
.width(250.0)
.class("sidebar")
```
**Прямые**: `selected`, `on_select`, `width`
**MSS**: `background`, `color`, `border`, `:hover`, `:selected` (+ кастомные MSS-свойства)

#### Router / RouterView
```rust
Router::new(vec![
    ("/", || Box::new(HomePage::new())),
    ("/settings", || Box::new(SettingsPage::new())),
])
.view()  // → RouterView
```
**Прямые**: маршруты
**MSS**: нет

#### Breadcrumb
```rust
Breadcrumb::new(vec!["Home", "Products", "Details"])
    .on_click(|index| { ... })
    .class("breadcrumb")
```
**Прямые**: `items`, `on_click`
**MSS**: `color`, `font-size`

#### Pagination
```rust
Pagination::new()
    .total(100)
    .page_size(10)
    .current(signal)
    .on_change(|page| { ... })
    .class("pagination")
```
**Прямые**: `total`, `page_size`, `current`, `on_change`
**MSS**: `color`, `accent-color`

---

### Оверлеи (Overlay)

#### Dialog
```rust
Dialog::new()
    .title("Confirm")
    .body("Are you sure?")
    .action(DialogAction::new("Cancel").on_click(|| { ... }))
    .action(DialogAction::new("OK").primary().on_click(|| { ... }))
    .is_open(signal)
    .width(400.0)
    .on_close(|| { ... })
    .class("dialog")
```
**Прямые**: `title`, `body`, `action`, `is_open`, `width`, `on_close`
**MSS**: `background`, `border-radius`, `box-shadow`, `padding`

#### AlertDialog / ConfirmDialog
```rust
AlertDialog::new("Error", "Something went wrong")
    .is_open(signal)

ConfirmDialog::new("Delete?", "This cannot be undone")
    .is_open(signal)
    .on_confirm(|| { ... })
    .on_cancel(|| { ... })
```
Упрощённые обёртки над Dialog.

#### FloatingWindow
```rust
FloatingWindow::new()
    .title("Properties")
    .is_open(signal)
    .position(100.0, 100.0)
    .size(300.0, 200.0)
    .modal(true)          // блокирует события вне окна (overlay-pipeline)
    .child(content)
    .class("floating")
```
**Прямые**: `title`, `is_open`, `position`, `size`, `modal`
**MSS**: `background`, `border`, `box-shadow`

#### Portal
```rust
Portal::new()
    .is_open(signal)
    .modal(true)
    .child(content)
```
**Прямые**: `is_open`, `modal`
**MSS**: `background` (для backdrop)

#### PopupMenu / MenuItem / ContextMenu
```rust
PopupMenu::new(vec![
    MenuItem::new("Cut").shortcut("Ctrl+X").on_click(|| { ... }),
    MenuItem::separator(),
    MenuItem::new("Copy").on_click(|| { ... }),
])
.is_open(signal)

ContextMenu::new(target_widget, vec![
    MenuItem::new("Edit"),
    MenuItem::new("Delete"),
])
```
**Прямые**: `items`, `is_open`
**MSS**: `background`, `border`, `border-radius`, `box-shadow`

#### Draggable / DropArea
```rust
Draggable::new(widget)
    .data(DragData::Text("hello".into()))
    .class("draggable")

DropArea::new(widget)
    .on_drop(|data| { ... })
    .class("drop-zone")
```
**Прямые**: `data`, `on_drop`
**MSS**: `background`, `border` (+ `:active` для drop zone)

---

### Обратная связь (Feedback)

#### Tooltip
```rust
Tooltip::new(widget, "Hint text")
    .position(TooltipPosition::Top)
    .class("tooltip")
```
**Прямые**: `position`, `text`
**MSS**: `background`, `color`, `font-size`, `border-radius`, `padding`

#### Snackbar
```rust
Snackbar::new("Operation complete")
    .is_open(signal)
    .duration_ms(3000)
    .position(SnackbarPosition::Bottom)
    .action("Undo", || { ... })
    .class("snackbar")
```
**Прямые**: `is_open`, `duration_ms`, `position`, `action`
**MSS**: `background`, `color`, `border-radius`

#### NotificationHost
```rust
NotificationHost::new(signal)
    .position(NotificationPosition::TopRight)
    .class("notifications")
```
**Прямые**: `position`
**MSS**: `background`, `color`

---

### Анимация

#### Animated
```rust
Animated::new(widget)
    .scale(Animation::tween(Easing::EaseOutBack)
        .from(0.5).to(1.0).duration_ms(800).build())
    .opacity(Animation::tween(Easing::EaseOutQuad)
        .from(0.0).to(1.0).duration_ms(600).build())
    .translate_x(Animation::spring().stiffness(300.0).build())
    .translate_y(animation)
    .rotate(animation)
    .origin(TransformOrigin::Center)
    .repeat(RepeatMode::Loop)
```
**Прямые**: `scale`, `opacity`, `translate_x`, `translate_y`, `rotate`, `origin`, `repeat`
**MSS**: нет — анимации через код.

#### AnimatedSize
```rust
AnimatedSize::new()
    .duration_ms(300)
    .easing(Easing::EaseInOut)
    .axis(AnimationAxis::Both)
    .child(widget)
```
**Прямые**: `duration_ms`, `easing`, `axis`
**MSS**: нет

---

### Графики (Charts)

#### LineChart
```rust
LineChart::new(vec![
    Series::new("Revenue")
        .data(vec![DataPoint::new(1.0, 100.0), ...])
        .style(SeriesStyle::new().color(Color::BLUE)),
])
.x_axis(AxisConfig::new().label("Month"))
.y_axis(AxisConfig::new().label("$"))
.legend(LegendPosition::TopRight)
.tooltip(TooltipConfig::default())
.width(600.0) / .height(400.0)
```

#### BarChart
```rust
BarChart::new(vec![
    BarSeries::new("Sales", vec![10.0, 20.0, 30.0]),
])
.mode(BarMode::Grouped)
.orientation(BarOrientation::Vertical)
.width(500.0) / .height(300.0)
```

#### PieChart
```rust
PieChart::new(vec![
    PieSlice::new("A", 30.0).color(Color::RED),
    PieSlice::new("B", 70.0).color(Color::BLUE),
])
.label_position(PieLabelPosition::Outside)
.width(300.0) / .height(300.0)
```

#### GaugeChart
```rust
GaugeChart::new(0.75)
    .segments(vec![
        GaugeSegment::new(0.0, 0.3, Color::RED),
        GaugeSegment::new(0.3, 0.7, Color::YELLOW),
        GaugeSegment::new(0.7, 1.0, Color::GREEN),
    ])
    .width(200.0) / .height(200.0)
```

#### RadarChart
```rust
RadarChart::new(
    vec![RadarIndicator::new("Speed", 100.0), ...],
    vec![RadarSeries::new("Player 1", vec![80.0, 90.0, ...])],
)
.grid_shape(RadarGridShape::Polygon)
.width(300.0) / .height(300.0)
```

**Все графики**: прямые свойства для данных и конфигурации. MSS — `background`, `border` (минимально).

---

## 7. Анимации

### Tween (временная)
```rust
Animation::tween(Easing::EaseOutCubic)
    .from(0.0)
    .to(1.0)
    .duration_ms(500)
    .delay_ms(100)
    .build()
```

### Spring (физическая)
```rust
Animation::spring()
    .stiffness(300.0)
    .damping(20.0)
    .mass(1.0)
    .from(0.0)
    .to(1.0)
    .build()
```

### Sequence (последовательная)
```rust
Animation::sequence(vec![
    Animation::tween(Easing::EaseIn).from(0.0).to(1.0).duration_ms(300).build(),
    Animation::tween(Easing::EaseOut).from(1.0).to(0.5).duration_ms(200).build(),
])
```

### Easing-функции
`Linear`, `EaseIn`, `EaseOut`, `EaseInOut`, `EaseInQuad`, `EaseOutQuad`, `EaseInOutQuad`, `EaseInCubic`, `EaseOutCubic`, `EaseInOutCubic`, `EaseInQuart`, `EaseOutQuart`, `EaseInBack`, `EaseOutBack`, `EaseInOutBack`, `EaseInBounce`, `EaseOutBounce`, `EaseInElastic`, `EaseOutElastic`, `CubicBezier(f32,f32,f32,f32)`

### CSS Transitions (через MSS)
```css
Button {
    transition: background 150ms ease, transform 100ms ease;
}
```

---

## 8. Паттерны и рецепты

### Паттерн: структура приложения (эталон — Calculator)

```rust
use syngui::prelude::*;
use syngui::widgets::*;

const STYLES: &str = include_str!("../styles/app.mss");

// 1. Copy-контекст с сигналами
#[derive(Clone, Copy)]
struct AppCtx {
    items: RwSignal<Vec<String>>,
    selected: RwSignal<Option<usize>>,
}

impl AppCtx {
    fn new() -> Self {
        Self {
            items: use_signal(vec!["Item 1".into(), "Item 2".into()]),
            selected: use_signal(None),
        }
    }
}

// 2. Точка входа
pub fn run_desktop() {
    let ctx = AppCtx::new();
    provide_context(ctx);

    App::new()
        .title("My App")
        .size(800, 600)
        .with_styles_str(STYLES)
        .run(|_| Box::new(build_root()));
}

// 3. UI-функции возвращают impl Widget
fn build_root() -> impl Widget {
    mgui! {
        Column::new().class("root") => [
            build_header(),
            DecoratedBox::new().class("grow") => [
                build_content(),
            ],
        ]
    }
}

fn build_header() -> impl Widget {
    let ctx = use_context::<AppCtx>();
    Toolbar::new("My App")
        .action(ToolButton::new("add").on_click(move || {
            ctx.items.update(|items| items.push("New".into()));
        }))
        .class("header")
}

fn build_content() -> impl Widget {
    let ctx = use_context::<AppCtx>();

    // Реактивный список
    move || {
        let items = ctx.items.get();
        let selected = ctx.selected.get();

        mgui! {
            Column::new().gap(4.0).class("content") => [
                ..items.iter().enumerate().map(|(i, item)| {
                    let is_selected = selected == Some(i);
                    ListItem::new(item)
                        .on_click(move || ctx.selected.set(Some(i)))
                        .class(if is_selected { "item-selected" } else { "item" })
                })
            ]
        }
    }
}
```

### Паттерн: триггер анимации

```rust
// Используем счётчик-сигнал для перезапуска анимации
let trigger = use_signal(0u32);

// В UI: читаем trigger для зависимости
move || {
    let _t = trigger.get();  // подписка
    let value = display.get();
    Animated::new(Text::new(&value))
        .scale(Animation::tween(Easing::EaseOutBack)
            .from(0.5).to(1.0).duration_ms(800).build())
}

// В обработчике: инкрементируем для перезапуска
trigger.set(trigger.get_untracked().wrapping_add(1));
```

### Паттерн: условный рендеринг

```rust
let show_details = use_signal(false);

move || {
    if show_details.get() {
        Card::new()
            .child(Text::new("Details here"))
            .class("details")
    } else {
        Text::new("Click to expand").class("placeholder")
    }
}
```

### Паттерн: форма

```rust
let name = use_signal(String::new());
let email = use_signal(String::new());
let submitting = use_signal(false);

mgui! {
    Column::new().gap(12.0).class("form") => [
        TextField::new()
            .placeholder("Name")
            .on_change(move |t| name.set(t))
            .class("field"),
        TextField::new()
            .placeholder("Email")
            .on_change(move |t| email.set(t))
            .class("field"),
        move || {
            let busy = submitting.get();
            Button::new(if busy { "Saving..." } else { "Submit" })
                .disabled(busy)
                .on_click(move || {
                    submitting.set(true);
                    // ... async submit
                })
                .class("btn-submit")
        },
    ]
}
```

### Паттерн: тёмная/светлая тема

```css
/* light.mss */
:root { --bg: #ffffff; --text: #1a1a1a; --primary: #3B82F6; }

/* dark.mss */
:root { --bg: #1a1a2e; --text: #e8e8e8; --primary: #60A5FA; }
```

```rust
let theme = use_signal(false); // false = light

App::new()
    .with_theme_styles(
        include_str!("../styles/light.mss"),
        include_str!("../styles/dark.mss"),
        theme,
    )
    .run(|_| Box::new(build_root()));
```

---

## 9. Android-поддержка

### Точка входа

```rust
#[cfg(target_os = "android")]
#[no_mangle]
fn android_main(app: syngui::app::AndroidApp) {
    android_logger::init_once(
        android_logger::Config::default()
            .with_max_level(log::LevelFilter::Info),
    );

    let ctx = AppCtx::new();
    provide_context(ctx);

    App::new()
        .title("My App")
        .vsync(true)
        .gpu_backend(GpuBackend::Gl)         // OpenGL для Android
        .gpu_power(GpuPowerPreference::LowPower)
        .with_android_app(app)
        .with_styles_str(STYLES)
        .run(|_| Box::new(build_root()));
}
```

### Отличия от Desktop

| | Desktop | Android |
|---|---------|---------|
| GPU | `GpuBackend::Auto` | `GpuBackend::Gl` |
| Размер окна | `.size(w, h)` | Полноэкранный (не задаётся) |
| Точка входа | `fn main()` | `#[no_mangle] fn android_main(AndroidApp)` |
| Логгирование | `env_logger` | `android_logger` (logcat) |
| Feature | `desktop` | `android` |

### Cargo.toml для Android

```toml
[package.metadata.android]
package = "com.example.myapp"
build_targets = ["aarch64-linux-android"]

[package.metadata.android.sdk]
min_sdk_version = 28
target_sdk_version = 34
```

---

## 10. Системное оформление

Три независимых источника настроек рабочего стола; подробности —
`docs/14-system-appearance.md`.

### Светлая/тёмная схема и акцент DE

```rust
use syngui::appearance::read_system_appearance;

let appearance = use_signal(read_system_appearance());   // стартуем сразу в системной схеме

App::new()
    .with_dynamic_theme(theme_mss)
    .with_system_appearance(appearance)   // дальше обновляет фреймворк
    .run(...);

// в эффекте темы
let system = appearance.get();
if system.is_dark() { /* тёмная палитра */ }
if let Some(accent) = system.accent { /* accent.to_hex(), .lighten(), .readable_on() */ }
```

Источник на Linux — XDG-портал `org.freedesktop.appearance` (feature
`system-theme`), fallback — `kdeglobals` / `gsettings`; на Windows и macOS —
winit `ThemeChanged`. `Window::theme()` из winit на Linux бесполезен.

### Кнопки окна в системном виде

```rust
use syngui::widgets::overlay::SystemWindowControls;

Row::new()
    .child(SystemWindowControls::left())
    .child(title)
    .child(SystemWindowControls::right())
```

Раскладка — из `kwinrc` / `gsettings`, внешний вид на KDE — из SVG темы
Aurorae (все состояния кнопки), иначе — встроенный вектор по MSS-цветам.

### Размытие фона (feature `system-blur`)

```rust
// Область эффекта = «шелл»: отступ под тень и радиус углов, иначе композитор
// размоет и прозрачную рамку вокруг окна.
let backdrop = use_signal(syngui::window::BackdropConfig::frosted().with_shell(30.0, 20.0));
let window_state = use_signal(syngui::window::WindowState::default());

App::new()
    .transparent(true)
    .with_backdrop(backdrop)          // переключается на лету
    .with_window_state(window_state)  // maximized / fullscreen / focused
    .run(...);
```

Работает поверх `ext-background-effect-v1` (KWin 6.7+), `org_kde_kwin_blur`
или X11-свойства `_KDE_NET_WM_BLUR_BEHIND_REGION` — что найдётся.

---

## Краткая справка

### Импорты

```rust
use syngui::prelude::*;           // Core types, Color, Event, etc.
use syngui::widgets::*;           // All widgets
use syngui::animation::{Animation, Easing};  // Animation API
use syngui::mgui;                 // mgui! macro
```

### Сигнальная система (шпаргалка)

| Функция | Что делает |
|---------|-----------|
| `use_signal(val)` | Создаёт `RwSignal<T>` |
| `create_memo(\|\| expr)` | Производное значение |
| `create_effect(\|\| { ... })` | Побочный эффект |
| `create_effect_with_cleanup(\|\| { ... })` | Эффект + cleanup |
| `provide_context(val)` | Внедрение зависимости |
| `use_context::<T>()` | Получение зависимости |
| `.get()` | Чтение + подписка |
| `.get_untracked()` | Чтение без подписки |
| `.set(val)` | Запись + уведомление |
| `.update(\|v\| ...)` | Мутация на месте |

### Ключевые правила

1. **MSS — главный**: Все визуальные свойства через MSS. Прямые `.background()` — только fallback.
2. **Copy-контекст**: Оборачивайте состояние в `#[derive(Clone, Copy)]` структуру с `RwSignal` полями.
3. **`get()` в UI, `get_untracked()` в handlers**: Иначе обработчик подпишется на лишние пересборки.
4. **Функции вместо классов**: UI строится функциями `fn build_*() -> impl Widget`.
5. **`include_str!`** для MSS: Compile-time embedding, без runtime IO.
6. **`mgui!` макрос**: Декларативный синтаксис с `=>` для вложенных виджетов.
