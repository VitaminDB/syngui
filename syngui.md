# syngui — руководство для генерации кода

Скилл для локальной модели. Всё ниже **проверено по исходникам** репозитория
`syngui` (коммит `435d792`) и по эталонному приложению **synthos**.

**Корни путей в этом документе:**

| Обозначение | Реальный путь |
|---|---|
| `syngui/src/...` | `/home/master/Projects/2027/syngui/syngui/src/...` |
| `docs/...` | `/home/master/Projects/2027/syngui/docs/...` |
| `synthos: src/...` | `/home/master/Projects/2027/synthos/src/...` |
| `synthos: styles/...` | `/home/master/Projects/2027/synthos/styles/...` |
| `gallery: ...` | `/home/master/Projects/2027/syngui/app/widget_gallery_mss/...` |

> ⚠️ **Файлы в `docs/01..15-*.md` частично устарели.** Там встречается
> `create_signal`, `Signal`/`SetSignal`, `Button::primary()`, `Text::font_size()`,
> `DecoratedBox::new(Color)` — **всего этого в коде нет**. Источник истины —
> исходники и этот файл.

---

## 0. Тридцать правил, которые нельзя нарушать

Если сомневаешься — перечитай этот список. 90 % ошибок генерации — отсюда.

1. **Сигнал один тип: `RwSignal<T>`**, создаётся `use_signal(initial)`.
   Никаких `create_signal`, `Signal<T>`, `SetSignal<T>`.
2. **`use_signal` — НЕ хук React.** Каждый вызов создаёт *новый* слот навсегда.
   Никогда не вызывай его внутри реактивного замыкания / билдера `Reactive` —
   это утечка и потеря состояния. Сигналы создаются один раз: в `build_context()`,
   в конструкторе Ctx-структуры, до `.run(...)`.
3. **Читать сигналы можно только с главного потока.** `get()` из фонового
   потока = паника. Писать (`set` / `set_always`) можно откуда угодно — вызов
   сам маршалится. `update()` — только главный поток.
4. **Реактивность даёт только замыкание.** `Text::new(sig.get())` внутри
   статичного дерева обновится один раз. Нужно `.child(move || Text::new(...))`
   или `Reactive::new(|| vec![...])`.
5. **`Text` не имеет `.font_size()`.** Размер, цвет, выравнивание, перенос —
   через MSS-класс. У `Text` есть только: `selectable`, `color`, `font_weight`,
   `bold`, `dark_mode`, `max_lines`, `elide`.
6. **`Button` не имеет `.primary()` / `.danger()` / `.secondary()`.** Только
   `.class("btn-primary")`.
7. **`DecoratedBox::new()` — без аргументов.** Ни цвета, ни радиуса в коде:
   всё через MSS. Это главный «div» фреймворка (623 использования в synthos).
8. **`DecoratedBox` не кликается.** Клик — `GestureDetector::new().on_click(..).child(..)`.
9. **`.class("a b")` разбивается на два класса ТОЛЬКО** у `WidgetExt::class`
   (виджеты без собственного `class`) и у `Button`. У `Column`, `Row`, `Flex`,
   `ScrollView`, `ListView`, `TreeView`, `SplitView`, `Sidebar`, `CodeEditor`,
   `GestureDetector`, `Positioned`, `PanZoom`, `Terminal`, чартов и др.
   собственный `.class()` кладёт строку **целиком**. Пиши `.class("a").class("b")`.
10. **`WidgetExt::class()` возвращает `StyledWidget<W>`** — после него методы
    самого виджета недоступны. Ставь `.class()` **последним** в цепочке.
11. **Заполнить свободное место** = MSS `flex-grow: 1`, обычно классом `.grow`.
    Никакого `Expanded` — его удалили.
12. **Растянуть по поперечной оси** = `.cross_axis_alignment(CrossAxisAlignment::Stretch)`
    у родителя или `Stack::new().fit(StackFit::Expand)`.
13. **MSS ≈ CSS, но:** нет `!important`, нет `@media`, нет `calc()`,
    нет `display`/`position`/`z-index`/`justify-content`/`align-items`,
    **не работают единицы `em`, `rem`, `vw`, `vh`** (парсятся и молча теряются).
    Живут только `px`, `%` и безразмерные числа.
14. **`:root { }` — ТОЛЬКО переменные.** Обычные свойства там игнорируются.
15. **`var()` должен стоять в начале значения.** `rgba(255, var(--x), 0, 1)`
    не раскроется. `border: 1px solid var(--c)` — раскроется (шорхенд разбирается).
16. **Один псевдокласс на селектор.** `Button:hover:focus` не сработает как в CSS.
17. **Псевдоклассы:** `:hover`, `:active` (= `:pressed`), `:focus`, `:selected`,
    `:checked`, `:disabled` + оконные `:window-maximized`, `:window-fullscreen`,
    `:window-focused`. Больше нет ничего (`:nth-child`, `:not()` — нет).
18. **Inline-стиль (`.style(...)`) перекрывает всё** в базовом слое, но
    **не участвует** в hover/active/focus-слоях.
19. **`create_memo` не кеширует** — это просто отложенное замыкание, считается
    на каждый `.get()`. Не клади туда тяжёлое.
20. **Колбэки — `FnMut(..) + Send + 'static`**; билдеры детей — `Fn() -> W + Send + Sync + 'static`.
21. **`mgui!` — синтаксический сахар над `.child()`.** Контейнер обязан иметь
    `.child()`. Разделитель — запятая, дети в `=> [ ... ]`.
22. Внутри `Reactive::new` возвращай **`Vec<Box<dyn Widget>>`**, приводя типы:
    `vec![Box::new(x) as Box<dyn Widget>]` или объявив `|| -> Vec<Box<dyn Widget>>`.
23. **Диалоги/меню/поповеры = оверлеи** (`Portal`, `PopupMenu`, `ContextMenu`,
    `Dialog`, `FloatingWindow`), они кладутся в `Stack` на верхний уровень
    и позиционируются сами.
24. **Иконка — это строка-глиф** из шрифта иконок: `Icon::new(MI_SETTINGS)`,
    где `MI_SETTINGS: &str = "\u{E8B8}"`. Шрифт подключается
    `.with_icon_font(syngui::text::icon_fonts::material::FONT_DATA)`.
25. **`use_context::<T>()` паникует, если контекст не выдан.** Безопасный
    вариант — `try_use_context::<T>()`. Контекст — thread-local (главный поток).
26. **Async:** `spawn(async move { ... })` (фича `tokio`), возврат в UI —
    только через `run_on_main_thread(move || sig.set(...))` или прямой
    `sig.set(..)` (он сам маршалится).
27. **`Widget: Any`, `Element: Send`.** Свой виджет обязан реализовать
    `create_element/can_update/as_any/as_any_mut/mount`.
28. **Layout — Flutter-подобный:** родитель даёт `Constraints`, ребёнок
    возвращает `Size`. Нет абсолютного позиционирования, кроме `Positioned`
    внутри `Stack`/`PanZoom` и оверлеев.
29. **Неизвестное MSS-свойство молча игнорируется** (в лог уходит один
    `warn` на свойство: `[MSS] Свойство '…' не поддерживается`).
    Проверяй имена по таблице раздела 6.
30. **F12 в native-сборке открывает DevTools-инспектор** всегда, даже без
    `.with_dev_tools(true)` (`syngui/src/app/event_handling.rs:383`).

---

## 1. Быстрый старт

### 1.1. Минимальное приложение

```rust
use syngui::prelude::*;

fn main() {
    App::new()
        .title("Hello syngui")
        .size(400, 300)
        .run(|_ctx| Box::new(Text::new("Hello, World!")));
}
```

`run` принимает `FnMut(&BuildContext) -> Box<dyn Widget>` — замыкание строит
корень **после** инициализации GPU и рантайма сигналов.

### 1.2. Приложение со стилями, состоянием и контекстом

```rust
use syngui::mgui;
use syngui::prelude::*;

const STYLES: &str = include_str!("../styles/app.mss");

#[derive(Clone, Copy)]
struct AppCtx {
    count: RwSignal<i32>,
    dark: RwSignal<bool>,
}

fn main() {
    // Сигналы создаются ОДИН раз, до run().
    let ctx = AppCtx {
        count: use_signal(0i32),
        dark: use_signal(false),
    };

    App::new()
        .title("Counter")
        .size(480, 320)
        .with_styles_str(STYLES)
        .run(move |_| {
            provide_context(ctx);
            Box::new(build_ui())
        });
}

fn build_ui() -> impl Widget {
    let ctx = use_context::<AppCtx>();

    mgui! {
        Column::new().gap(16.0).center().class("root") => [
            move || Text::new(format!("Count: {}", ctx.count.get())).class("counter"),
            Button::new("Increment")
                .on_click(move || ctx.count.set(ctx.count.get_untracked() + 1))
                .class("btn-primary"),
        ]
    }
}
```

```css
/* styles/app.mss */
:root { --accent: #4f8cff; }

.root     { background: #16181d; padding: 24px; }
.counter  { color: #e8eaf0; font-size: 28px; font-weight: bold; }

.btn-primary {
    background: var(--accent);
    color: #fff;
    padding: 8px 16px;
    border-radius: 8px;
    transition: background 150ms ease-out;
    &:hover    { background: #6ea3ff; }
    &:disabled { background: #555; }
}
```

### 1.3. Cargo.toml

```toml
[dependencies]
syngui = { path = "../syngui/syngui", default-features = false, features = [
    "msdf", "effects", "winit", "clipboard", "i18n",
    "material-icons", "tokio", "markdown", "image",
] }
```

`default = ["msdf", "effects", "clipboard", "winit", "wayland-dnd", "i18n"]`
(`syngui/Cargo.toml:157`).

Полный список фич — раздел 17.

### 1.4. Импорты

```rust
use syngui::prelude::*;    // почти всё нужное
use syngui::widgets::*;    // остальные виджеты
use syngui::mgui;          // макрос (он #[macro_export], путь от корня крейта)
```

`prelude` (`syngui/src/lib.rs:69`) содержит: `App`/`AppBuilder`/`GpuBackend`/
`GpuPowerPreference`, `Widget`/`WidgetExt`/`Element`/`ElementId`/`ElementTree`,
`Text`, `Center`, `Elide`, сигналы (`use_signal`, `RwSignal`, `create_memo`,
`Memo`, `create_effect`, `use_effect(+_with_cleanup)`, `dispose_effect`,
`EffectId`), контекст (`provide_context`, `use_context`, `try_use_context`),
`run_on_main_thread`, `spawn`, `use_async`, `tr!`/`trn!`/`Lang`,
`viewport_size`/`viewport_below`, `Color`/`Point`/`Size`/`Rect`/`EdgeInsets`,
`Event`/`Key`/`MouseButton`/`Modifiers`/`EventResult`, MSS-типы
и большинство виджетов.

**Нет в prelude — импортируй явно** (проще всего `use syngui::widgets::*;`):
`GestureDetector`, `ShowIf`, `Named`, `Page`, `ScrollbarPolicy`,
`ScrollPhysics`, `ScrollTarget`, `VirtualFlex`, `PanZoomViewport`,
`Positioned`, `TransformBox`, `Animated`, `AnimatedSize`, `RepeatMode`,
`TransformOrigin`, `AnimationAxis`, `IntoWidget`, `Sidebar`, `Breadcrumb`,
`Stepper`, `MultilineTextEdit`, `CodeEditor`, `MarkdownView`,
`MarkdownEditor`, `Terminal`, `PopupPanel`, `PopupAnchor`, `PortalAnchor`,
`SystemWindowControls`, `WindowControl`, `WindowDragRegion`,
`WindowResizeRegion`, `CalendarLocale`, `DropInfo`, `TreeNodeDecoration`,
`SortDirection`, `ColumnWidth`, `ParticleSystem`, все чарты.

Совсем точечно (не покрыто и `widgets::*`):
`use syngui::widgets::buttons::Segment;`,
`use syngui::core::sync::Mutex;` (wasm-совместимый),
`use syngui::async_runtime::{spawn, run_on_main_thread};`,
`use syngui::appearance::{read_system_appearance, SystemAppearance};`,
`use syngui::window::{BackdropConfig, WindowState};`,
`use syngui::text::icon_fonts::material;`,
`use syngui::effects::*;` (для `filter` из Rust).

---

## 2. Архитектура и ментальная модель

```
Widget  (иммутабельное описание, пересоздаётся каждый билд)
   │ create_element() / can_update()
Element (состояние, живёт между кадрами: layout, ввод, анимации)
   │ layout(Constraints) -> Size   ← measure_recursive (сверху вниз)
   │ set_position(Point)           ← position_recursive
   │ build_display_list(&mut DisplayList, clip)
DisplayList (GPU-независимые команды)
   │ Batcher: группировка по шейдеру/текстуре/клипу
Batches → wgpu (Vulkan / Metal / DX12 / GL / WebGPU), один render pass
```

Файлы: `syngui/src/widget/widget.rs`, `syngui/src/widget/element.rs`,
`syngui/src/widget/tree/mod.rs`, `syngui/src/render/display_list/mod.rs`.

### Диффинг

`can_update(&other)` решает, можно ли переиспользовать элемент. Обычная
реализация — `other.is::<Self>()`. Если да — `element.update(widget, ctx)`,
если нет — старый элемент уничтожается, создаётся новый.

Практическое следствие: при пересборке `Reactive` состояние (скролл, курсор в
поле, раскрытая выпадашка) **сохраняется**, если тип виджета на той же позиции
не изменился.

### DirtyFlags (`syngui/src/widget/dirty.rs`)

| Флаг | Значение |
|---|---|
| `LAYOUT` | пересчитать раскладку |
| `RENDER` | пересобрать display list |
| `PAINT` | перерисовать |
| `STATE` | изменилось внутреннее состояние |
| `CHILDREN` | изменились дети |
| `ANIMATION` | активна анимация |

---

## 3. Реактивность

Файл: `syngui/src/signal.rs`. Экспорт: `syngui::prelude`.

### 3.1. RwSignal

```rust
let count: RwSignal<i32> = use_signal(0);

count.get();            // читает + подписывает текущий Reactive/effect
count.get_untracked();  // читает без подписки — для обработчиков событий
count.set(5);           // пишет, если значение != старого (нужен PartialEq + Send)
count.set_always(5);    // пишет безусловно (нужен Send)
count.update(|v| *v += 1);  // мутация на месте, ТОЛЬКО главный поток
```

* `RwSignal<T>` — `Copy`, `Send`, `Sync`; это просто индекс слота. Можно свободно
  копировать в замыкания.
* `T: Clone + 'static`.
* `set` / `set_always` из фонового потока автоматически уходят в
  `run_on_main_thread` (`syngui/src/signal.rs:178`).
* Внутри `update(|v| ...)` **нельзя читать этот же сигнал** — будет понятная
  паника. Читай до вызова.
* `update` можно вкладывать в другие сигналы/`tr!`/уведомления: значение
  вынимается из слота, `RUNTIME` не заимствован.

**Типичная ошибка:**

```rust
// ❌ паника: get() из фонового потока
spawn(async move { let v = sig.get(); });

// ✅ снять значение ДО spawn
let v = sig.get_untracked();
spawn(async move { use_value(v); });

// ✅ или вернуться на главный поток
spawn(async move {
    run_on_main_thread(move || { let v = sig.get_untracked(); });
});
```

### 3.2. Мемо

```rust
let total = create_memo(move || items.get().iter().sum::<i32>());
total.get();
```

`Memo<T>` — обёртка над `Box<dyn Fn() -> T>`; **кеша нет**, считается на каждый
`get()` (`syngui/src/signal.rs:342`). Использовать как удобное именованное
производное значение, не как оптимизацию.

### 3.3. Эффекты

```rust
let id: EffectId = create_effect(move || {
    let q = query.get();          // авто-подписка
    tracing::info!(%q, "query changed");
});

use_effect_with_cleanup(move || {
    let running = Arc::new(AtomicBool::new(true));
    let flag = running.clone();
    std::thread::spawn(move || {
        while flag.load(Ordering::Relaxed) {
            std::thread::sleep(Duration::from_secs(1));
            seconds.set(seconds_value_from_atomic());   // set() потокобезопасен
        }
    });
    Some(Box::new(move || running.store(false, Ordering::Relaxed)) as Box<dyn Fn()>)
});

dispose_effect(id);   // ручное снятие
```

* `use_effect` == `create_effect`, `use_effect_with_cleanup` == `create_effect_with_cleanup`.
* Эффект **выполняется сразу** при создании, затем при изменении зависимостей.
* Если эффект создан во время сборки элемента — он привязан к его scope и
  умирает вместе с ним (`element_effects`, `syngui/src/signal.rs:498`).
  **Поэтому эффекты, которые должны жить всё приложение, ставят вне страниц** —
  см. `synthos: src/lib.rs:200` (`install_run_watcher` вынесен из `view()`
  именно по этой причине).
* Не вызывай `tr!` внутри `create_effect` для локализации UI: подпишется
  эффект, а не элемент.

### 3.4. Реактивные поддеревья

```rust
// 1) Замыкание как ребёнок — авто-обёртка в Reactive
Column::new().child(move || Text::new(format!("{}", sig.get())))

// 2) Явный Reactive для списка / ветвления
Reactive::new(|| -> Vec<Box<dyn Widget>> {
    let ctx = use_context::<HuggingFaceCtx>();
    match ctx.list_state.get() {
        ListLoadState::Loading => vec![Box::new(loading_skeleton())],
        ListLoadState::Error   => vec![Box::new(error_view())],
        _                      => vec![Box::new(list_view())],
    }
})
```

Эталон: `synthos: src/pages/huggingface/list_panel.rs:25`.

`IntoWidget<Marker>` (`syngui/src/widgets/containers/reactive.rs:14`) даёт две
реализации: любой `Widget` и любое `Fn() -> W + Send + Sync + 'static`.
Поэтому `.child(...)` принимает и виджет, и замыкание.

### 3.5. Контекст

```rust
#[derive(Clone, Copy)]           // Copy — если внутри только RwSignal
struct AppCtx { theme: RwSignal<String>, router: /* Arc<Mutex<Router>> → Clone */ }

provide_context(ctx);            // один раз, внутри run(...)
let ctx = use_context::<AppCtx>();        // паникует, если не выдан
let ctx = try_use_context::<AppCtx>();    // Option<AppCtx>
remove_context::<AppCtx>();
```

Хранилище — thread-local type-map (`syngui/src/context_provider.rs`).
Эталон структуры: `synthos: src/context.rs` (`AppCtx`, `GeneralCtx`,
`AppearanceCtx`, `VoiceFabCtx`, …) и `synthos: src/lib.rs:340` (`build_context`).

### 3.6. Async

```rust
use syngui::async_runtime::{run_on_main_thread, spawn};

spawn(async move {                       // фича "tokio"
    let data = fetch().await;
    run_on_main_thread(move || result.set(data));
});
```

`use_async` (фича `tokio`, `syngui/src/async_hook.rs`):

```rust
let (data, loading) = use_async(move || {
    let p = profile.get();               // авто-подписка → рефетч
    async move { fetch_data(&p).await }
});
// data:    RwSignal<Option<T>>  (прошлое значение сохраняется на время загрузки)
// loading: RwSignal<bool>
```

Эталон реальной загрузки с ретраями/паузой: `synthos: src/pages/huggingface/download.rs:269`.

### 3.7. Размер вьюпорта (адаптив)

```rust
use syngui::prelude::{viewport_size, viewport_below};

let narrow = viewport_below(900.0);      // RwSignal<bool>, меняется на пересечении порога
Column::new().child(move || if narrow.get() { compact() } else { wide() })
```

`viewport_below` заводит собственный сигнал+эффект — вызывать **один раз при
сборке компонента**, не внутри часто перезапускаемого замыкания
(`syngui/src/viewport.rs:41`).

---

## 4. Композиция дерева

### 4.1. `mgui!`

```rust
mgui! {
    Column::new().gap(12.0).class("card") => [
        Text::new("Заголовок").class("h1"),
        Row::new().gap(8.0) => [
            Button::new("OK").class("btn-primary"),
            Button::new("Отмена"),
        ],
        move || Text::new(format!("{}", sig.get())),
    ]
}
```

Раскрытие (`syngui/src/widgets/macros.rs:9`): `X => [a, b]` → `X.child(a).child(b)`.
Вложенность и замыкания работают, потому что `.child()` принимает `IntoWidget`.

`children!` — пакетное боксирование:

```rust
Row::new().children(children![Text::new("A"), Text::new("B")])
// == vec![Box::new(..) as Box<dyn Widget>, ...]
```

### 4.2. Классы и inline-стили

```rust
use syngui::prelude::*;   // тянет WidgetExt

Text::new("Title").class("heading")                 // StyledWidget<Text>
Text::new("x").classes(vec!["a".into(), "b".into()])
widget.style("flex-grow", StyleValue::Number(2.0))  // inline
widget.style("width", StyleValue::percent(100.0))
widget.style("background-color", Color::from_hex("#333").unwrap())
```

`StyleValue` (`syngui/src/mss/value.rs`): `Color`, `Length(f32, Unit)`,
`String`, `Number`, `Var`, `VarWithFallback`, `List`, `Gradient`,
`Inherit`, `Initial`, `Unset`, `None`.
Хелперы: `StyleValue::px(v)`, `StyleValue::percent(v)`;
`From<f32>` → px, `From<Color>` → цвет.

`.named("debug-name")` (`WidgetExt::named`) оборачивает в `Named` — виджет
виден под этим именем в DevTools.

---

## 5. Layout

### 5.1. Constraints

```rust
Constraints::new(min_w, max_w, min_h, max_h)
Constraints::tight(size)     // min == max
Constraints::loose(size)     // min = 0
Constraints::expand()        // бесконечность
c.constrain(size) / c.loosen() / c.is_tight() / c.has_bounded_width()
```

### 5.2. Контейнеры

```rust
Column::new()
    .gap(12.0)
    .main_axis_alignment(MainAxisAlignment::Start)      // Start End Center SpaceBetween SpaceAround SpaceEvenly
    .cross_axis_alignment(CrossAxisAlignment::Stretch)  // Start End Center Stretch Baseline
    .center()                                            // = центр по обеим осям
    .clip(true)
    .width(200.0).height(100.0)
    .child(a).children(vec![b, c])
    .class("panel")

Row::new()   // тот же API (без .expand())
Flex::new().direction(FlexDirection::Row).wrap().gap(8.0)   // wrap-раскладка
Grid::new(3).gap(8.0).row_gap(12.0).col_gap(8.0).masonry(true)
Stack::new().fit(StackFit::Expand).clip(false).child(bg).child(overlay)
Padding::all(16.0).child(x)
Padding::symmetric(16.0, 8.0).child(x)     // h, v
Padding::only(8.0, 16.0, 8.0, 0.0).child(x) // l, t, r, b
Center::new().child(x)
DecoratedBox::new().clip(true).class("card").child(x)   // «div»: весь вид из MSS
SplitView::new(left, right)
    .direction(SplitDirection::Horizontal)
    .initial_ratio(0.3).min_size(200.0).divider_width(4.0)
    .ratio_signal(sig)                     // двусторонняя привязка (persist!)
Positioned::new(x).at(120.0, 40.0)          // внутри Stack / PanZoom
ShowIf::new(index, selected_signal).child(x)
```

### 5.3. Как что-то растянуть — шпаргалка

| Задача | Решение |
|---|---|
| Занять свободное место по главной оси | MSS `flex-grow: 1` → `.class("grow")` |
| Пропорции 2:1 | классы `.grow-2` и `.grow` |
| Растянуть по поперечной оси | `.cross_axis_alignment(CrossAxisAlignment::Stretch)` |
| Растянуть по обеим осям | `Stack::new().fit(StackFit::Expand)` |
| Занять всю ширину/высоту родителя | MSS `width: 100%` / `height: 100%` |
| Центрировать | `Center::new().child(..)` или `.center()` |
| Прижать вправо | `Row` + распорка `.class("grow")` перед элементом |

Обязательные хелперы в MSS каждого приложения (`synthos: styles/base/layout.mss`):

```css
.grow   { flex-grow: 1; }
.grow-2 { flex-grow: 2; }
.grow-3 { flex-grow: 3; }
```

### 5.4. Скролл

```rust
ScrollView::new()
    .vertical()                       // .horizontal() .both() .direction(ScrollDirection::…)
    .scrollbar_policy(ScrollbarPolicy::Auto)   // Auto | Always | Never
    .scrollbar_width(8.0)
    .center_content(true)
    .class("chat-scroll")             // ВНИМАНИЕ: принимает &str, класс не разбивается
    .child(content)

Page::new()                            // ScrollView + физика инерции + scroll_to
    .vertical()
    .physics(ScrollPhysics::default())            // friction, bounce…
    .scroll_to(ScrollTarget::Bottom)              // Top | Bottom | Offset(f32)
    .child(content)

// виртуализованные сетка/флекс: строится только видимое окно
VirtualFlex::grid(3, item_count, |i| Box::new(card(i)))          // (cols, count, builder)
VirtualFlex::flex(240.0, item_count, |i| Box::new(card(i)))      // (min_item_width, count, builder)
    .gap(12.0).estimated_item_height(180.0).scrollbar_policy(ScrollbarPolicy::Auto)
```

MSS-свойства скроллбара: `scrollbar-width`, `scrollbar-color`,
`scrollbar-thumb-hover-color`, `scrollbar-track-color`, `scrollbar-radius`,
`scrollbar-policy`, `scrollbar-fade-delay` (см. `synthos: styles/components/scrollbars.mss`).

---

## 6. MSS — стилизация

Файлы: `syngui/src/mss/` (`parser/`, `cascade.rs`, `style_engine.rs`,
`fields.rs`, `inheritance.rs`, `value.rs`).

### 6.1. Подключение

```rust
App::new()
    .with_styles_str(include_str!("../styles/app.mss"))
    .with_styles("styles/app.mss")                        // из файла
    .with_additional_styles_str(EXTRA)                    // домешать
    .with_theme_styles(LIGHT, DARK, dark_signal)          // две темы + переключатель
    .with_dynamic_theme(theme_mss_signal)                 // MSS целиком из RwSignal<String>
```

Эталонная сборка большого стиля из кусков — `synthos: src/styles.rs`
(`concat!(include_str!(...), "\n", ...)`, один компонент = один файл).
Динамическая тема: `synthos: src/lib.rs:487` — эффект пересобирает
`theme_mss` из палитры + акцента системы + пользовательских шрифтов.

### 6.2. Что совместимо с CSS

Совместимо и работает как ожидается:

* синтаксис правил, комментарии `/* */`;
* селекторы: тип (`Button`), класс (`.card`), id (`#main`), `*`,
  составные (`Button.danger`), потомок (`.panel Text`), прямой ребёнок
  (`.panel > Text`), соседи `+` и `~`, группы через запятую;
* вложенность через `&` (`&:hover`, `& .child`, `&.mod`, `&> x`);
* переменные `--x` в `:root` + `var(--x)` и `var(--x, fallback)`;
* наследование, ключевые слова `inherit` / `initial` / `unset`;
* специфичность `(a, b, c)` и порядок правил при равенстве;
* `transition` (шорхенд и по свойствам) со всем набором easing;
* `@keyframes` + `animation-*`;
* цвета `#rgb`-hex 6/8, `rgb()`, `rgba()`, именованные;
* градиенты `linear-gradient`, `radial-gradient`, `conic-gradient`;
* `filter` / `backdrop-filter`, `box-shadow` (в т.ч. `inset` и списком),
  `opacity`, `mix-blend-mode`, `overflow`, `cursor`.

### 6.3. Чем MSS отличается от CSS — исчерпывающе

| Отличие | Подробность |
|---|---|
| Нет `!important` | приоритет = специфичность → порядок → inline |
| Нет `@media`, `@supports`, `@import` | адаптив делается кодом: `viewport_below()` |
| Нет `calc()`, `clamp()`, `min()`, `max()` | считай в Rust и клади через `.style()` |
| Единицы `em`, `rem`, `vw`, `vh` парсятся, но **не резолвятся** | реально работают только `px`, `%`, число |
| `%` работает **только** у `width/height/min-*/max-*` | `padding: 5%` → игнор |
| `:root` — только переменные | `:root { padding: … }` не применится ни к чему |
| Один псевдокласс на цепочку | `A:hover:focus` — не сработает |
| Нет `:nth-child`, `:not()`, `::before/::after` | |
| Нет `display`, `position`, `float`, `z-index` | раскладку задают виджеты |
| Нет `justify-content`, `align-items`, `flex-direction` | это `.main_axis_alignment()` и т.д. в коде |
| Есть `flex-grow`, но нет `flex-shrink`/`flex-basis` | |
| `var()` только в начале значения | `rgba(0,0,var(--x),1)` — не раскроется |
| Неизвестное свойство молча игнорируется | один `log::warn!` на свойство |
| Оконные псевдоклассы | `:window-maximized`, `:window-fullscreen`, `:window-focused` |
| Кастомные свойства виджетов | `icon-size`, `caret-color`, `editor-bg`, `token-*`, `row-hover-bg`… |
| Дополнительные визуальные свойства | `glow`, `color-tint`, `noise`, `vignette`, `outline-*`, `line-clamp` |

### 6.4. Полный список поддерживаемых свойств

Источник истины: `KNOWN_PROPERTIES`, `syngui/src/mss/fields.rs:14`.

**Фон и цвет:**
`background`, `background-color`, `color`, `border-color`, `accent-color`

**Рамки:**
`border`, `border-width`, `border-style`, `border-radius`,
`border-{left,top,right,bottom}-width`,
`border-{top,right,bottom,left}-style`,
`border-{left,top,right,bottom}-color`,
`border-{top-left,top-right,bottom-right,bottom-left}-radius`

**Размеры:** `width`, `height`, `min-width`, `max-width`, `min-height`, `max-height`
(значения: `px`, `%`, `auto`, `fit-content`, `max-content`, `min-content`)

**Отступы:** `padding`, `padding-{left,right,top,bottom}`,
`margin`, `margin-{left,top,right,bottom}`, `gap`

**Текст:** `font-size`, `font-weight`, `font-family`, `line-height`,
`text-align`, `text-vertical-align`, `text-decoration`, `letter-spacing`,
`text-transform` (`uppercase|lowercase|capitalize|none`), `text-shadow`, `line-clamp`

**Иконки:** `icon-size`, `icon-color`, `icon-color-selected`,
`icon-color-hover`, `icon-color-disabled`, `icon-opacity`

**Поля ввода:** `selection-color`, `caret-color`, `clipboard-hint` (`on|off`)

**Трансформации:** `transform`, `transform-origin`, `translate-x`, `translate-y`,
`rotate`, `scale`, `scale-x`, `scale-y`

**Визуал:** `opacity`, `cursor`, `box-shadow`, `overflow`,
`filter`, `backdrop-filter`, `mix-blend-mode`,
`outline`, `outline-width`, `outline-color`, `outline-offset`,
`glow`, `color-tint`, `noise`, `vignette`

**Анимации:** `transition`, `transition-property`, `transition-duration`,
`transition-timing-function`, `animation`, `animation-name`,
`animation-duration`, `animation-timing-function`, `animation-iteration-count`,
`animation-delay`, `animation-direction`, `animation-fill-mode`,
`animation-play-state`

**Раскладка:** `flex-grow`

**Чарты:** `grid-color`, `axis-color`, `axis-font-size`, `title-font-size`,
`legend-font-size`, `tooltip-background`, `tooltip-border-color`,
`label-color`, `label-font-size`, `value-font-size`, `track-color`,
`needle-color`, `point-size`, `grid-alpha`

**Разделитель/скроллбар:** `divider-thickness`, `scrollbar-width`,
`scrollbar-color`, `scrollbar-thumb-hover-color`, `scrollbar-track-color`,
`scrollbar-radius`, `scrollbar-policy`, `scrollbar-fade-delay`

**CodeEditor:** `editor-bg`, `editor-fg`, `editor-gutter-bg`, `editor-gutter-fg`,
`editor-cursor`, `editor-selection`, `editor-current-line`,
`editor-bracket-match`, `editor-whitespace`, `editor-find-match`,
`editor-find-current`, `gutter-color`

**Подсветка синтаксиса:** `token-keyword`, `token-keyword-control`,
`token-type`, `token-type-builtin`, `token-function`, `token-function-macro`,
`token-constant`, `token-constant-builtin`, `token-string`,
`token-string-special`, `token-number`, `token-comment`, `token-operator`,
`token-punctuation`, `token-variable`, `token-property`, `token-attribute`,
`token-tag`, `token-namespace`

**Таблицы/списки:** `header-bg`, `header-color`, `header-font-size`,
`header-padding`, `row-hover-bg`, `row-selected-bg`, `row-striped-bg`,
`row-padding`, `row-padding-{left,top,right,bottom}`, `cell-padding`,
`cell-font-size`, `cell-min-width`, `cell-max-width`

### 6.5. Наследование

Наследуются (`syngui/src/mss/inheritance.rs:4`) **только**:

`color`, `font-family`, `font-size`, `font-weight`, `letter-spacing`,
`text-align`, `text-vertical-align`, `text-decoration`, `text-transform`,
`text-shadow`, `cursor`, `line-height`, `caret-color`
— плюс **все пользовательские `--переменные`**.

Всё остальное (`padding`, `background`, `border-radius`, `width`, …) —
не наследуется.

Практика synthos: всё равно объявлять базовые правила по типу виджета
(`synthos: styles/base/reset.mss`):

```css
Text { color: var(--text); font-size: 14px; }
Icon { color: var(--text-muted); icon-size: 20px; }
TextField { width: 240px; background-color: var(--bg-search); … }
```

### 6.6. Каскад — точный порядок

`syngui/src/mss/cascade.rs:152` (`apply_styles_to_tree`), DFS сверху вниз:

1. Стартовая база = унаследованные свойства родителя.
2. Кандидаты берутся из индекса по классу/типу/catch-all.
3. Подходящие правила сортируются по `(a, b, c)` специфичности,
   при равенстве — по порядку в таблице стилей.
4. Правила **без** псевдокласса пишутся в слой `base`.
   Правила с `:hover` / `:active` / `:focus` / `:selected` / `:checked` —
   в свои слои. Оконные псевдоклассы пишутся в `base`, если флаг окна
   совпадает, иначе игнорируются.
5. **Inline-стили применяются последними — только к `base`.**
6. Каждый псевдослой = `base` + свои декларации (`merge_layer`).
7. Дальше вниз передаётся `extract_inherited(base)`.

Следствия:
* inline-стиль побеждает любой селектор;
* inline-стиль не отменяет `:hover` (тот пересобирается из base + hover);
* transition/keyframes настраиваются из `base`.

### 6.7. Переменные, темы

```css
:root {
    --primary: #EE5E48;
    --text: #1C1D22;
    --radius: 10px;
    /* var() внутри rgba() НЕ работает — храни полный rgba */
    --glass-card-bg: rgba(255, 255, 255, 0.62);
}

.btn { background: var(--primary); border-radius: var(--radius); }
.btn-alt { color: var(--maybe-missing, #888); }
```

Порядок блоков `:root` = приоритет: переменные хранятся в HashMap, поздние
перекрывают ранние (используется в synthos для наложения системного акцента
поверх темы — `synthos: src/lib.rs:497`).

### 6.8. Кастомные `--переменные`, которые читают виджеты

Это не «просто переменные» — виджет достаёт их из `ComputedStyle` напрямую.

**Popup-часть `Dropdown`, `Combobox`, `Multiselect`, `Autocomplete`, `PopupMenu`,
`ContextMenu`:**
`--popup-background`, `--popup-color`, `--popup-border`, `--popup-accent`,
`--popup-hover-background`, `--popup-hover-color`,
`--popup-selected-background`, `--popup-selected-color`,
`--popup-submenu-arrow-color`, `--popup-min-width`, `--popup-min-height`,
`--popup-max-height`

**`MarkdownView`** (`syngui/src/widgets/visual/markdown_view/widget.rs`):
`--md-h1-size`…`--md-h6-size`, `--md-heading-color`, `--md-heading-spacing`,
`--md-block-spacing`, `--md-link-color`, `--md-bullet-color`,
`--md-list-indent`, `--md-code-bg`, `--md-code-color`, `--md-code-font-size`,
`--md-code-padding-h`, `--md-code-radius`, `--md-code-block-bg`,
`--md-code-block-color`, `--md-code-block-padding`, `--md-code-block-radius`,
`--md-quote-bg`, `--md-quote-border-color`, `--md-quote-border-width`,
`--md-quote-padding-left`, `--md-quote-padding-v`, `--md-quote-radius`,
`--md-quote-text-color`, `--md-table-header-bg`, `--md-table-header-color`,
`--md-table-border-color`, `--md-table-stripe-bg`, `--md-hr-color`,
`--md-hr-thickness`, `--md-image-height`, `--md-image-placeholder-bg`,
`--md-image-placeholder-color`, `--md-checkbox-color`,
`--md-checkbox-check-color`, `--md-strikethrough-color`,
`--md-footnote-color`, `--md-footnote-divider-color`,
`--md-copy-bg`, `--md-copy-bg-hover`, `--md-copy-color`, `--md-copy-size`,
`--md-copy-radius`, `--md-copy-margin`, `--md-copy-flash-bg`

**`DocumentEditor`** (`syngui/src/widgets/input/document_editor/style.rs`):
`--doc-text-color`, `--doc-text-size`, `--doc-line-height`, `--doc-padding`,
`--doc-max-content-width`, `--doc-indent`, `--doc-block-spacing`,
`--doc-child-spacing`, `--doc-h1-size`…`--doc-h6-size`, `--doc-heading-color`,
`--doc-link-color`, `--doc-link-missing-color`, `--doc-muted-color`,
`--doc-number-color`, `--doc-bullet-color`, `--doc-checkbox-color`,
`--doc-checkbox-check-color`, `--doc-code-bg`, `--doc-code-color`,
`--doc-code-font-size`, `--doc-code-block-bg`, `--doc-code-block-color`,
`--doc-code-block-padding`, `--doc-quote-border-color`, `--doc-divider-color`,
`--doc-table-border-color`, `--doc-table-header-bg`, `--doc-embed-bg`,
`--doc-embed-border-color`, `--doc-media-bg`, `--doc-menu-bg`,
`--doc-menu-border`, `--doc-menu-sel-bg`, `--doc-caret-color`,
`--doc-selection-color`, `--doc-callout-padding`, `--doc-toggle-chevron-color`

**`Calendar` / `DatePicker`:** `--cal-panel-bg`, `--cal-panel-border`,
`--cal-cell-size`, `--cal-radius`, `--cal-font-size`, `--cal-hover-bg`,
`--cal-selected-color`, `--cal-today-color`, `--cal-weekend-color`,
`--cal-outside-color`, `--cal-muted-color`, `--cal-disabled-color`

**`TransformBox`:** `--tb-border-color`, `--tb-border-width`,
`--tb-handle-color`, `--tb-handle-border-color`, `--tb-handle-size`

**`Tab` / `TabBar`:** `--tab-indicator-height`, `--tab-indicator-inset`, `--tab-fill`

### 6.9. Имена типов для селекторов

Полный список (`element_type_name()` по всему дереву):

`Animated`, `AnimatedSize`, `Autocomplete`, `Avatar`, `Badge`, `BarChart`,
`Breadcrumb`, `BuildingOverlay`, `Button`, `Calendar`, `Card`, `Carousel`,
`Center`, `Checkbox`, `Chip`, `CircularProgress`, `CodeEditor`, `ColorPicker`,
`Column`, `Combobox`, `ContextMenu`, `DatePicker`, `DecoratedBox`, `Dialog`,
`Divider`, `Draggable`, `DropArea`, `Dropdown`, `Flex`, `FloatingWindow`,
`FramesView`, `GaugeChart`, `GestureDetector`, `Grid`, `HeatOverlay`, `Icon`,
`Image`, `LineChart`, `ListView`, `MapView`, `MarkdownEditor`, `MarkdownView`,
`MultilineTextEdit`, `Multiselect`, `Named`, `Notification`, `OptionButton`,
`Padding`, `Page`, `Pagination`, `PanZoomViewport`, `PieChart`, `PopupMenu`,
`PopupPanel`, `Portal`, `Positioned`, `ProgressBar`, `PropertyGrid`,
`RadarChart`, `RadioButton`, `Reactive`, `RichText`, `RouterView`, `Row`,
`ScrollView`, `SegmentedButton`, `SegmentedProgressBar`, `ShowIf`, `Sidebar`,
`Slider`, `Snackbar`, `SpinBox`, `SplitView`, `Stack`, `StaticWaveform`,
`Stepper`, `SystemWindowControls`, `Tab`, `TabBar`, `TableView`, `Terminal`,
`Text`, `TextField`, `TickSlider`, `TimePicker`, `Toggle`, `Toolbar`,
`ToolButton`, `Tooltip`, `TopAppBar`, `TransformBox`, `TreeView`, `VideoView`,
`VirtualFlex`, `VirtualSpacer`, `WindowControl`, `WindowDragRegion`,
`WindowResizeRegion`

Внутренние типы `DocumentEditor`: `document-editor`, `doc-text-row`,
`doc-code-block`, `doc-table`, `doc-divider`, `doc-chrome`, `doc-embed-card`,
`doc-media-card`, `doc-media-player`.

### 6.10. Transition и keyframes

```css
Button {
    background-color: #2196F3;
    transition: background-color 200ms ease-out;
}
Button:hover { background-color: #1E88E5; }

.card {
    transition: background-color 300ms ease-in-out,
                opacity 200ms ease-out,
                box-shadow 200ms linear;
}
.all { transition: all 300ms ease; }

@keyframes pulse {
    0%   { opacity: 1; }
    50%  { opacity: 0.5; }
    100% { opacity: 1; }
}
.blink {
    animation-name: pulse;
    animation-duration: 1200ms;
    animation-iteration-count: infinite;
    animation-timing-function: ease-in-out;
}
```

Easing: `linear`, `ease`, `ease-in`, `ease-out`, `ease-in-out`,
`ease-in|out|in-out-{sine,quad,cubic,quart,quint,expo,circ,back,elastic,bounce}`,
`cubic-bezier(x1,y1,x2,y2)`, `steps(n)`.

Анимируются: цвета, `opacity`, размеры/отступы, `box-shadow`, `glow`,
`noise`, `vignette`, `filter`.

### 6.11. Градиенты

```css
.hero    { background: linear-gradient(135deg, #667eea, #764ba2); }
.bar     { background: linear-gradient(to right, #ff6b6b, #feca57); }
.stops   { background: linear-gradient(180deg, #000 0%, #333 30%, #fff 100%); }
.spot    { background: radial-gradient(circle at center, #fff, #3b82f6); }
.wheel   { background: conic-gradient(from 0deg at center, red, yellow, red); }
```

`0deg` — снизу вверх, `90deg` — слева направо, `180deg` — сверху вниз (дефолт).
Градиент уважает `border-radius`; рамка рисуется поверх.

---

## 7. Каталог виджетов

Импорт: `use syngui::prelude::*;` (основное) или
`use syngui::widgets::*;` / точечно `use syngui::widgets::input::dropdown::Dropdown;`.

### 7.1. Текст и иконки

```rust
Text::new(impl Into<String>)
    .color(Color) .bold() .font_weight(700) .selectable(true)
    .max_lines(2) .elide(Elide::Middle)          // Elide::End (дефолт) | Middle
    .dark_mode(dark_color, Arc<Mutex<bool>>)
// размер/выравнивание/перенос — только MSS

RichText::new()
    .span("bold ", |s| s.bold().color(c))
    .text("plain")
    .default_color(c).default_font_size(14.0).line_height(20.0).wrap(true)

Icon::new(MI_SETTINGS)          // строка-глиф; вид через MSS icon-size / icon-color
```

### 7.2. Кнопки (`syngui/src/widgets/buttons/`)

```rust
Button::new("OK")
    .icon(MI_CHECK) .leading_icon(..) .trailing_icon(..)
    .disabled(false)
    .active_index(sig_usize, 2)             // «выбран», если sig == 2
    .on_click(|| {})
    .on_click_at(|p: Point| {})
    .on_click_with_bounds(|r: Rect| {})     // для привязки поповера
    .class("btn-primary")                   // &str, разбивается по пробелам

ToolButton::new(MI_DELETE)                  // иконочная кнопка
    .tooltip("Удалить").text("подпись").active(true).disabled(false)
    .press_passthrough()                    // не съедать press у родителя
    .on_click(|| {}) .on_click_at(..) .on_click_with_bounds(|p, r| {})

// Segment НЕ в prelude: use syngui::widgets::buttons::Segment;
SegmentedButton::new(vec![Segment::new("A"), Segment::with_icon("B", MI_X)])
    .selected(0).on_change(|i| {})   // ещё есть Segment::icon_only(MI_X)

OptionButton::new("Toggle").icon(..).pressed_state(Arc<Mutex<bool>>).on_toggle(|on| {})
```

### 7.3. Ввод (`syngui/src/widgets/input/`)

```rust
TextField::new()                    // или TextField::with_text("...")
    .text("v").placeholder("Поиск").width(240.0)
    .disabled(false).read_only(false).obscure(true)
    .prefix(w).suffix(w).prefix_icon(MI_SEARCH).suffix_icon(..)
    .on_prefix_click(|| {})
    .helper_text("подсказка").error("ошибка")
    .autofocus(true).clipboard_hint(true)
    .input_filter(|c: char| c.is_ascii_digit())
    .on_filter_reject(|c| {})
    .on_change(|t: &str| {}) .on_submit(|t: &str| {}) .on_escape(|| {})
    .submit_on_focus_lost(true)

MultilineTextEdit::new()
    .text(..).placeholder(..).rows(4).max_rows(12).auto_height(true)
    .soft_wrap(true).show_line_numbers(false).read_only(false)
    .submit_on_enter(true).on_submit(|t| {}).on_change(|t| {})

Checkbox::new().with_checked(true).label("Согласен").on_change(|b| {})
Toggle::new().on(true).on_change(|b| {})
RadioGroup::new("group-id").selected("a");
RadioButton::new("a", &group).label("A")

Slider::new().value(0.5).range(0.0, 1.0).step(0.01)
    .vertical().bipolar().show_value(2).value_width(48.0).width(200.0)
    .on_change(|v: f32| {})

TickSlider::new().ticks(vec![0.0, 0.5, 1.0]).tick_count(5)
    .tick_labels(|v| format!("{v:.0}")).snap_to_ticks(true)
    .show_value_label(true).value_formatter(|v| format!("{v:.1}×"))

SpinBox::new().value(1.0).range(0.0, 10.0).step(0.5).decimal_places(2).on_change(|v: f64| {})

Dropdown::with_items(vec![DropdownItem::new("id", "Метка").icon(MI_X)])
    .selected("id").placeholder("Выбор").width(200.0).max_height(320.0)
    .leading_icon(MI_LIST).on_change(|v: &str| {})

Combobox::new(items).text("").placeholder("").popup_min_width(240.0).on_change(|s| {})
Multiselect::new(items).selected(vec![0, 2]).with_autocomplete(true).max_visible(8).on_change(|ids: &[usize]| {})
Autocomplete::new(vec!["a".into()]).min_chars(2).on_select(|s| {}).on_change(|s| {})

DatePicker::new().today().min_date(d).max_date(d).show_week_numbers(true)
    .locale(CalendarLocale::russian()).on_change(|d: Option<Date>| {})
TimePicker::new().selected(Time::new(9, 30)).use_24h(true).on_change(|t: Option<Time>| {})
ColorPicker::new().color(ColorValue::new(255, 0, 0)).show_alpha(true).on_change(|c| {})

CodeEditor::new()                     // фича "code-editor"
    .text(src).language(lang).auto_detect_language(path)
    .read_only(false).show_line_numbers(true).soft_wrap(false)
    .tab_width(4).insert_spaces(true).size_limit_mb(16)
    .command_signal(cmd_sig).state_signal(state_sig)
    .on_change(|c| {}).on_save(|s| {}).on_cursor(|i| {})

DocumentEditor::new()                 // фича "document-editor" (Notion-стиль)
    .markdown(src).read_only(false).handle(&handle)
    .slash_items(items).on_slash_custom(|q| {})
    .links(provider).media(resolver).embeds(factory)
    .on_context_menu(|p| {}).on_drop_file(..).on_change(|| {})
// Ручка: модель, история undo/redo и выделение блоков живут в ней —
// переживают пересоздание элемента (смена вкладки/страницы).
handle.serialize(); handle.revision(); handle.selected();
handle.history_state()                // RwSignal<(можно отменить, можно повторить)>
handle.block_selection()              // RwSignal<Vec<BlockId>> — выделенные блоки
handle.queue_op(DocOp::Undo)          // Redo, Copy, Cut, Paste, SelectBlocks(ids),
                                      // InsertMarkdown, Duplicate, Delete, Move{down}, …
```

Выделение блоков в `DocumentEditor`: Ctrl+клик — переключить блок,
Shift+клик — диапазон, клик по ручке ⋮⋮ без переноса — блок, протяжка из
пустого места — рамка; повторный Ctrl+A — все блоки, Esc — снять, ↑/↓ —
шаг по блокам. Над выделением Delete/Backspace удаляют, Ctrl+C/X/V
работают с блоками как markdown (Ctrl+V без выделения: одна строка — в
текст, несколько — блоками). `CharInput` при Ctrl/Cmd (кроме AltGr)
приложение не шлёт — в русской раскладке Ctrl+Z иначе печатал «я».

### 7.4. Данные

```rust
ListView::new(vec![ListItem::new("Строка").secondary("под").icon(MI_X).trailing("12")])
    .selection_mode(SelectionMode::Single)     // None | Single | Multiple
    .selected(vec![0]).selected_signal(sig, 0)
    .item_height(48.0).buffer_size(8)
    .item_widget(builder)                      // кастомный рендер строки
    .on_select(|i| {}).on_reach_top(..)
ListView::virtual_new(count, |i| ListItem::new(format!("row {i}")))   // виртуализация

TableView::new(columns, rows)
    .sortable(true).striped(true).row_height(28.0).header_height(32.0)
    .keyboard_nav(true).cell_cursor(true).text_selection(true).editable(true)
    .table_id("t1").column_widths_state(..).column_visibility_state(..)
    .on_row_click(|r| {}).on_cell_select(|r, c| {}).on_sort(|c, d| {})
    .on_cell_edit(..).on_row_double_click(..)
TableView::virtual_new(columns, row_count, |i| vec![format!("{i}"), "…".into()])
TableColumn::fixed("Имя", 180.0) / ::flex("Путь", 1.0) / ::new("X")
    .min_width(80.0).max_width(400.0).resizable(true).sortable(true)
    .align(ColumnAlign::Right).cell_renderer(..).sort_key(..)

TreeView::new(vec![
    TreeNode::branch("src", "src", vec![TreeNode::leaf("m", "main.rs")])
        .icon(MI_FOLDER).expanded(true).badge(color).label_color(c).strikethrough(true),
])
.indent(16.0).item_height(24.0).show_lines(true)
.selection_mode(SelectionMode::Single).selected(vec!["m".into()])
.on_select(|id| {}).on_toggle(|id, open| {})

PropertyGrid::new()
    .property(Property::text("name", "value"))
    .property(Property::number("size", 12.0))
    .property(Property::boolean("on", true))
    .property(Property::color("tint", Color::WHITE))
    .property(Property::choice("mode", vec!["a".into()], 0))
    .editable(true).label_width(120.0).on_change(|i, v| {})
```

### 7.5. Навигация

```rust
let router = Arc::new(Mutex::new(Router::new(
    vec!["home".to_string(), "settings".to_string()], "home")));

RouterView::new(router.clone())
    .route("home", || Box::new(home_page()))
    .route("settings", || Box::new(settings_page()))
    .handle_back(true)

// вне вида
router.lock().unwrap().navigate("settings");
router.lock().unwrap().back();  // .forward() .can_go_back() .current() .history()

TabBar::new().position(TabPosition::Top).tab(Tab::new("Файл", 0, &tab_state).icon(..).closable().badge("3"))
Sidebar::new().header(w).footer(w).child(w).class("rail")
Toolbar::with_title("Заголовок").child(w).height(48.0)
TopAppBar::new("Title").leading(w).action(w).height(56.0).gap(8.0)
Breadcrumb::new().item("Домой").icon_item(MI_X, "Папка").separator("/").on_click(|i| {})
Pagination::new(total_pages, current).max_visible(7).on_page_change(|p| {})
Stepper::new().step("Шаг 1", None).current(0).allow_navigation(true).on_step_click(|i| {})
```

### 7.6. Визуальные

```rust
Card::new().child(w)
Avatar::new().text("АВ").size(36.0)
Badge::new("12").small()            // BadgeSize::{Small,Medium,Large}; Badge::dot()
Chip::new("тег").icon(..).selected(true).deletable().on_click(..).on_delete(..)
Divider::horizontal().length(120.0).indent(8.0)   // ::vertical()
ProgressBar::with_value(0.4).indeterminate().show_percentage()
CircularProgress::with_value(0.4).size(24.0).stroke_width(3.0).indeterminate()
SegmentedProgressBar::from_bools(&[true, false]).with_disabled_from(3)

Image::new("assets/x.png")
Image::from_url("https://…")              // фича image-network
Image::from_bytes("key", bytes)
Image::from_rgba("key", w, h, rgba)
    .fit(ImageFit::Cover)                 // Contain | Cover | Fill | None
    .tint(Color::WHITE).placeholder(true)

Canvas::new(|ctx: &mut CanvasContext, t: f32| { /* Path, Paint, LineCap… */ })
    .size(200.0, 120.0).animated(true).background(Color::TRANSPARENT)

Calendar::new().selected(Date::today()).locale(CalendarLocale::russian()).on_select(|d| {})
ParticleSystem::confetti(token).count(120).palette(vec![..])

MarkdownView::new(src)                    // фича "markdown"
    .selectable(true).with_copy_code(true).with_syntax_highlight(true)
    .with_syntax_theme("base16-ocean.dark").base_url("…").max_width(760.0)
    .on_link_click(|url| { let _ = syngui::open_url(url); })
MarkdownEditor::new(text_sig).mode(mode_sig).show_toolbar(true).split_ratio(0.5)

Terminal::new()                           // фича "terminal": PTY + VT100
    .command("bash").args(["-l"]).cwd("/home/u").env("TERM", "xterm-256color")
    .font_family("monospace").font_size(13.0).line_height(1.25)
    .attach(session).command_signal(cmd_sig).autofocus(true).class("term")
VideoView / video_player_view(..)         // фича "ffmpeg"
FramesView::new(frames, fps).playing_signal(..).position_signal(..).autoplay(true)
MapView::new().center(lat, lng).zoom(12).provider(TileProvider::osm())
    .marker(MapMarker::new(lat, lng).label("X").pulse())   // фича "map"
```

### 7.7. Чарты (`syngui/src/widgets/charts/`)

```rust
LineChart::new().series(Series::new(..)).x_axis(AxisConfig::..).y_axis(..)
    .legend(LegendPosition::Top).tooltip(true).animate(true).zoom(true)
    .title("…").mark_lines(&[0.0]).size(600.0, 320.0).on_point_click(..)
BarChart / PieChart / RadarChart / GaugeChart — аналогично.
```
Цвета/сетка/шрифты — через MSS: `grid-color`, `axis-color`, `axis-font-size`,
`legend-font-size`, `label-color`, `tooltip-background`, `point-size` и т.д.

### 7.8. Обратная связь и оверлеи

```rust
// Тултип оборачивает виджет
Tooltip::new(child, "текст").position(TooltipPosition::Below).delay_ms(400).max_width(280.0)
Tooltip::rich(child, content_widget)

// Уведомления: один хост на приложение
let notif = NotificationCtx::with_default_duration(15_000);
notif.success("Готово"); notif.error("Ошибка"); notif.info(..); notif.warning(..);
notif.show(NotificationItem::info("Заголовок").message("текст").duration_ms(4000));
NotificationHost::new(notif.clone()).grow_up(true).class("notifications")

Snackbar::new("Сохранено", show_sig).action("Отменить", || {}).duration_ms(4000)
    .position(SnackbarPosition::BottomCenter)

// Диалог
Dialog::new("Удалить?")
    .icon(MI_DELETE).body("Действие необратимо").width(420.0)
    .is_open(open_sig)
    .action(DialogAction::new("Отмена", move || open_sig.set(false)))
    .action(DialogAction::new("Удалить", move || { do_it(); open_sig.set(false); }).primary())
    .on_close(move || open_sig.set(false))
AlertDialog::new("Title", "Message", open_sig)
set_dialog_labels("ОК", "Отмена");     // глобальные подписи (иначе — из каталога i18n)

// Универсальный оверлей (эталон synthos для всех модалок)
Portal::new()
    .is_open(open_sig).modal(true).backdrop(true).backdrop_color(c)
    .anchor(PortalAnchor::Center)      // Center | BottomEnd{..} | TopEnd{..} | BottomStart{..}
    .close_on_outside_click(true).width(520.0)
    .child(card_widget)
    .on_close(|| {})

// Меню
PopupMenu::new().items(items).is_open(open).position(pos_sig)
    .anchor(PopupAnchor::BottomStart).anchor_rect(rect_sig).min_width(200.0)
    .on_select(|id: &str| {})
ContextMenu::new().items(vec![MenuItem::new("del", "Удалить").icon(MI_DELETE).shortcut("Del")])
    .on_select(|id| {}).child(inner)
MenuItem::separator();  MenuItem::new(..).children(vec![..])   // подменю

PopupPanel::new().is_open(open).anchor_rect(rect_sig).anchor(PopupAnchor::BottomEnd)
    .min_width(240.0).max_height(420.0).class("popup").child(w).on_close(|| {})

FloatingWindow::new("Заголовок")
    .is_open(open).position(pos_sig).size(Size::new(600.0, 400.0)).size_signal(sz)
    .closable(true).minimizable(true).is_minimized(min_sig)
    .with_resizable(true).drag_on_body(true).modal(false).centered()
    .child(w).on_close(|| {})
```

### 7.9. Жесты, DnD, окно

```rust
GestureDetector::new()
    .on_click(|| {}).on_click_at(|p| {}).on_click_with_bounds(|p, r| {})
    .on_double_click(|| {}).on_hover_change(|h| {})
    .on_mouse_down(|p| {}).on_mouse_up(|p| {})
    .on_back(|| true)                   // Android back / Escape
    .cursor(CursorIcon::Pointer)
    .child(w)

Draggable::new("task", payload_json).label("Move").threshold(6.0)
    .on_click(|| {}).on_double_click(|| {}).child(w)
DropArea::new().accept_types(vec!["task".into()])
    .on_drop(|d: DragData| {}).on_drop_positioned(|i: DropInfo| {})
    .on_drag_enter(|| {}).on_drag_leave(|| {}).placeholder("Бросьте сюда").child(w)

// use syngui::widgets::containers::PanZoomViewport;
PanZoomViewport::new()
    .pan(pan_sig).zoom(zoom_sig).zoom_range(0.2, 4.0).zoom_speed(1.1)
    .grid(true).grid_step(24.0).pan_button(MouseButton::Middle)
    .on_background_click(|screen, world| {}).on_background_context_menu(..)
    .child(canvas)

// frameless-окно
WindowDragRegion::new().child(titlebar)
WindowResizeRegion::new().inset(24.0).enabled(true).child(shell)
SystemWindowControls::right().button_size(16.0).spacing(8.0).active(true).maximized(m)
WindowControl::close() / ::minimize() / ::toggle_maximize()
```

### 7.10. Анимационные обёртки

```rust
Animated::new(child)
    .opacity(Animation::tween(Easing::EaseOut).from(0.0).to(1.0).duration_ms(300))
    .translate_x(Animation::tween(Easing::EaseOutCubic).from(-40.0).to(0.0).duration_ms(400))
    .scale(Animation::spring().from(0.9).to(1.0).stiffness(300.0).damping(18.0))
    .rotate(..).origin(TransformOrigin::Center)
    .repeat(true).repeat_mode(RepeatMode::PingPong(0))   // 0 = бесконечно

AnimatedSize::new(child).duration_ms(250).easing(Easing::EaseOutCubic)
    .clip(true).axis(AnimationAxis::Height)

Carousel::new().child(a).child(b).current_page(0)
    .auto_play(true).auto_play_interval_ms(4000).show_indicators(true).on_page_change(|i| {})
```

---

## 8. События и ввод

`syngui/src/input/events.rs`, `syngui/src/widget/tree/event.rs`.

```rust
enum Event {
    MouseMove(Point),
    MouseDown { button: MouseButton, position: Point },
    MouseUp   { button: MouseButton, position: Point },
    MouseWheel { delta: f32, delta_x: f32, position: Point },
    DoubleClick { button: MouseButton, position: Point },
    KeyDown(Key), KeyUp(Key), CharInput(char),
    TouchStart/TouchMove/TouchEnd { id: u64, position: Point },
    FocusGained, FocusLost,
    Resized { width: u32, height: u32 }, CloseRequested,
    DragStart/DragMove/DragEnter/DragLeave/Drop/DragEnd { .. },
    BackPressed, Custom(String),
}

enum EventResult { Ignored, Handled, Captured }
enum MouseButton { Left, Right, Middle, Back, Forward, Other(u16) }
```

Диспетчеризация: сначала верхний оверлей, затем DFS от корня; самый глубокий
попавший элемент получает событие первым, `Ignored` всплывает к родителю.
`Captured` перехватывает мышь до отпускания кнопки.

`EventContext` (`syngui/src/widget/context.rs`) — что доступно в `handle_event`:

```rust
ctx.modifiers                        // Modifiers { ctrl, shift, alt, meta }
ctx.capture()
ctx.set_cursor(CursorIcon::Text)
ctx.register_overlay(bounds, modal) / ctx.unregister_overlay()
ctx.start_drag(DragData::new("type", "payload", id).with_label("…"))
ctx.copy_to_clipboard("x") / ctx.paste_from_clipboard()
ctx.scroll_into_view(rect)
ctx.start_window_resize(dir)
ctx.set_virtual_keyboard_visible(true)   // Android IME
ctx.viewport_size()
ctx.measure_text_width(text, font_size, chars)
```

**Глобальные хоткеи** делаются прозрачной обёрткой-виджетом — эталон
`synthos: src/components/event_hook.rs` + `src/search/mod.rs:340`:

```rust
EventHook::new()
    .on_key_down(move |key, mods| {
        if matches!(key, Key::K | Key::F) && mods.ctrl { search.toggle(); KeyReply::Handled }
        else { KeyReply::Ignore }
    })
    .child(app_shell)
```

**Функциональные клавиши в web:**

```rust
App::new().capture_function_keys(FunctionKeys::of(&[Key::F2, Key::F11]))
syngui::input::set_captured_function_keys(FunctionKeys::ALL);
```
Иначе F5/F11/F12 остаются браузеру (на native настройка не действует).

---

## 9. Анимации (Rust API)

`syngui/src/animation/`.

```rust
Animation::tween(Easing::EaseOutCubic).from(0.0).to(100.0).duration_ms(300).delay_ms(0)
Animation::spring().from(0.0).to(1.0).stiffness(200.0).damping(20.0).mass(1.0)
Animation::Constant(1.0)

a.current_value(); a.set_target(v); a.tick(dt) -> bool; a.is_complete(); a.reset();
```

Easing: 30+ вариантов (`Linear`, `EaseIn|Out|InOut` × `Sine|Quad|Cubic|Quart|Quint|Expo|Circ|Back|Elastic|Bounce`,
`CubicBezier(..)`, `Steps(n)`), плюс `CSS_EASE*`.

Практика: для состояний (`hover`, `selected`) — MSS `transition`;
для входа/выхода и «живых» элементов — `Animated` / `AnimatedSize`;
для сложного — `Canvas::new(|ctx, t| ..).animated(true)`.

---

## 10. Эффекты

MSS: `filter`, `backdrop-filter` (список функций слева направо), плюс
самостоятельные `glow`, `color-tint`, `noise`, `vignette`, `mix-blend-mode`,
`outline-*`, `box-shadow`, `opacity`.

```css
.glass {
    background-color: rgba(255,255,255,0.12);
    backdrop-filter: blur(16px);
    border: 1px solid rgba(255,255,255,0.18);
    border-radius: 16px;
}
.photo  { filter: grayscale(100%) brightness(0.9); transition: filter 300ms ease-out; }
.photo:hover { filter: grayscale(0%) brightness(1.0); }
.retro  { filter: crt(0.4) noise(0.15) vignette(0.6) chromatic-aberration(2px); }
```

Функции `filter`: `blur`, `grayscale`, `sepia`, `invert`, `brightness`,
`contrast`, `hue-rotate`, `saturate`, `color-grade`, `gradient-map`, `duotone`,
`silhouette`, `wave`, `swirl`, `bulge`/`pinch`, `heat-haze`, `refraction`,
`noise`, `vignette`, `crt`, `pixelate`, `edge-detect`,
`chromatic-aberration`, `glitch`, `dissolve`, `hologram`/`x-ray`,
`lens-flare`, `mask-reveal`, `directional-blur`, `radial-blur`/`zoom-blur`.

Rust API (в `build_display_list` своего виджета):

```rust
use syngui::effects::*;
list.push_effect_layer(chain(vec![blur(3.0), vignette(0.6, 0.3)]), bounds);
// … команды рисования …
list.pop_effect_layer();
```

Производительность: каждый слой — offscreen-таргет; `backdrop-filter` самый
дорогой. Предпочитай `opacity`/`box-shadow` как отдельные свойства — у них
оптимизированный путь без offscreen.

---

## 11. Локализация (i18n)

Фича `i18n` (в default). Модуль `syngui/src/i18n/`, документация `docs/15-i18n.md`.

```rust
syngui::i18n::register_catalogs(&[include_str!("../i18n/en.lang"),
                                  include_str!("../i18n/ru.lang")]);
syngui::i18n::set_language(syngui::i18n::system_language());

App::new().run(|_| Box::new(DecoratedBox::new().child(move || {
    syngui::i18n::subscribe();       // корень перестраивается при смене языка
    build_root()
})));

Text::new(tr!("app.title"))
Text::new(tr!("greeting", name = "Анна"))
Text::new(trn!("files.count", 5))
try_tr("dynamic.key")                // Option<String>
```

Формат `.lang`:

```
@tag = "ru"
@name = "Русский"
@plural = "east-slavic"

app.title = "Мой редактор"
greeting  = "Привет, {name}!"
files.count.one  = "{n} файл"
files.count.few  = "{n} файла"
files.count.many = "{n} файлов"
```

Правила:
* `tr!` **не делает виджет реактивным** — строка резолвится при сборке;
  живое переключение даёт `Reactive`/`.child(|| ..)` вокруг.
* Не вызывай `tr!` в `create_effect` для UI — подпишется эффект, не элемент.
* `tr()` можно звать из воркеров; `set_language` из воркера маршалится сам.
* Встроенные каталоги виджетов — 14 языков, приложение может переопределять
  ключи своим каталогом.

Эталон: `synthos: src/i18n/mod.rs` (14 каталогов + effect
`GeneralCtx.language → set_language`).

---

## 12. Окно, системное оформление, frameless

```rust
App::new()
    .title("App").size(1280, 720).min_size(960, 640).maximized(true)
    .frameless().transparent(true).background(Color::from_srgb(0,0,0,0.0))
    .decorations(false).fullscreen(false).position(x, y).size_ratio(0.8, 0.8)
    .vsync(true).frame_limit(120).staging_belt(true)
    .gpu_backend(GpuBackend::Auto)          // Auto|Vulkan|Gl|Dx12|Metal
    .gpu_power(GpuPowerPreference::LowPower)
    .with_font_family("Inter").with_font_url("fonts/Inter.ttf")
    .with_fallback_font_url("fonts/NotoSansCJK.otf").with_emoji_font_url(..)
    .with_icon_font(syngui::text::icon_fonts::material::FONT_DATA)
    .with_system_appearance(appearance_sig)
    .with_backdrop(backdrop_sig)
    .with_window_state(window_state_sig)
    .with_window_icon_png(bytes).with_tray(tray_cfg).with_single_instance("id")
    .with_splash(png).splash_size(400, 300).splash_min_duration(800)
    .with_debug_overlay(false).with_dev_tools(false)
    .capture_function_keys(FunctionKeys::NONE)
    .double_click_interval(Duration::from_millis(400))
    .add_window(cfg, build_fn)              // второе окно
    .run(|_| Box::new(root()));
```

Системная тема/акцент (`syngui/src/appearance/`, фича `system-theme`):

```rust
let appearance = use_signal(syngui::appearance::read_system_appearance());
// SystemAppearance { color_scheme: NoPreference|Dark|Light, accent: Option<Color>,
//                    high_contrast: bool, reduced_motion: bool }
App::new().with_system_appearance(appearance)
```
Отладка: `SYNGUI_COLOR_SCHEME=dark|light`, `SYNGUI_ACCENT_COLOR=#RRGGBB`.

Размытие фона (`system-blur`):

```rust
let backdrop = use_signal(BackdropConfig::frosted().with_shell(30.0, 20.0));
App::new().transparent(true).with_backdrop(backdrop)
```
`with_shell(inset, radius)` обязателен для CSD-окна с прозрачным «воздухом»
вокруг шелла — иначе композитор размоет всю поверхность.

Состояние окна для MSS-псевдоклассов:

```rust
let ws = use_signal(syngui::window::WindowState::default()); // maximized/fullscreen/focused
App::new().with_window_state(ws)
```

```css
.shell { border-radius: 20px; padding: 30px; transition: all 180ms ease-out; }
.shell:window-maximized { border-radius: 0; padding: 0; }
```

Эталон целиком: `synthos: src/lib.rs:79` + `synthos: styles/layout/shell.mss`
+ `synthos: src/components/titlebar.rs`.

---

## 13. Собственный виджет

Минимальный «прозрачный» виджет-обёртка — эталон
`synthos: src/components/event_hook.rs` (перехват клавиш + публикация bounds).

```rust
use std::any::Any;
use std::time::Duration;
use syngui::core::{Point, Rect, Size};
use syngui::input::{Event, EventResult};
use syngui::layout::Constraints;
use syngui::mss::{ComputedStyle, MssFields};
use syngui::render::DisplayList;
use syngui::widget::context::EventContext;
use syngui::widget::{
    DirtyFlags, Element, ElementId, ElementTree, LayoutHint, StyledElement,
    UpdateContext, Widget,
};
use syngui::widgets::containers::IntoWidget;

pub struct MyBox { child: Option<Box<dyn Widget>> }

impl MyBox {
    pub fn new() -> Self { Self { child: None } }
    pub fn child<M>(mut self, c: impl IntoWidget<M>) -> Self {
        self.child = Some(c.into_widget()); self
    }
}

impl Widget for MyBox {
    fn create_element(&self) -> Box<dyn Element> {
        Box::new(MyBoxElement {
            id: ElementId::new(),
            has_child: self.child.is_some(),
            bounds: Rect::zero(),
            classes: Vec::new(),
            dirty_flags: DirtyFlags::LAYOUT | DirtyFlags::RENDER,
            mss: MssFields::new(),
        })
    }
    fn can_update(&self, other: &dyn Any) -> bool { other.is::<Self>() }
    fn as_any(&self) -> &dyn Any { self }
    fn as_any_mut(&mut self) -> &mut dyn Any { self }
    fn mount(&self, tree: &mut ElementTree, parent_id: ElementId) {
        if let Some(child) = &self.child {
            let el = child.create_element();
            let id = tree.insert_with_type_id(el, Some(parent_id), child.as_any().type_id());
            child.mount(tree, id);
        }
    }
    fn child_widgets(&self) -> Vec<&dyn Widget> {
        self.child.as_ref().map(|c| vec![c.as_ref()]).unwrap_or_default()
    }
}

struct MyBoxElement {
    id: ElementId, has_child: bool, bounds: Rect,
    classes: Vec<String>, dirty_flags: DirtyFlags, mss: MssFields,
}

impl Element for MyBoxElement {
    fn update(&mut self, widget: &dyn Widget, _ctx: &mut UpdateContext) {
        if let Some(w) = widget.as_any().downcast_ref::<MyBox>() {
            self.has_child = w.child.is_some();
        }
    }
    fn layout(&mut self, c: Constraints) -> Size {
        let w = if c.max_width.is_finite()  { c.max_width }  else { 0.0 };
        let h = if c.max_height.is_finite() { c.max_height } else { 0.0 };
        self.bounds = Rect::new(self.bounds.origin, Size::new(w, h));
        self.bounds.size
    }
    fn build_display_list(&self, list: &mut DisplayList, _clip: Rect) {
        if let Some(bg) = self.mss.background_color {
            // border_radius_resolved(reference_size, default) — резолвит % в px
            let radius = self.mss.border_radius_resolved(self.bounds.size.height, 0.0);
            list.push_rect(self.bounds, bg, radius);
        }
    }
    fn handle_event(&mut self, _e: &Event, _ctx: &mut EventContext) -> EventResult {
        EventResult::Ignored
    }
    fn animate(&mut self, _dt: Duration) -> bool { false }
    fn children(&self) -> &[ElementId] { &[] }
    fn bounds(&self) -> Rect { self.bounds }
    fn set_position(&mut self, p: Point) { self.bounds.origin = p; }
    fn mark_dirty(&mut self, f: DirtyFlags) { self.dirty_flags |= f; }
    fn clear_dirty(&mut self, f: DirtyFlags) { self.dirty_flags.remove(f); }
    fn is_dirty(&self, f: DirtyFlags) -> bool { self.dirty_flags.contains(f) }
    fn id(&self) -> ElementId { self.id }
    fn set_id(&mut self, id: ElementId) { self.id = id; }
    fn mount(&mut self, _tree: &mut ElementTree) {}
    fn element_type_name(&self) -> &str { "MyBox" }
    fn layout_hint(&self) -> LayoutHint {
        LayoutHint::Padding { left: 0.0, top: 0.0, right: 0.0, bottom: 0.0 }
    }
    fn set_classes(&mut self, c: Vec<String>) { self.classes = c; self.mark_dirty(DirtyFlags::RENDER); }
    fn get_classes(&self) -> &[String] { &self.classes }
    fn reset_mss_styles(&mut self) { self.mss.reset(); }
    fn apply_computed_style(&mut self, s: &ComputedStyle) {
        self.mss.apply(s);
        self.mark_dirty(DirtyFlags::LAYOUT | DirtyFlags::RENDER);
    }
    fn apply_transition_styles(
        &mut self, base: &ComputedStyle, hover: Option<&ComputedStyle>,
        active: Option<&ComputedStyle>, focus: Option<&ComputedStyle>,
        selected: Option<&ComputedStyle>, _checked: Option<&ComputedStyle>,
    ) { self.mss.apply_transitions(base, hover, active, focus, selected); }
}

impl StyledElement for MyBoxElement {
    fn apply_style(&mut self, _s: &ComputedStyle) { self.mark_dirty(DirtyFlags::RENDER); }
    fn classes(&self) -> &[String] { &self.classes }
    fn set_classes(&mut self, c: Vec<String>) { self.classes = c; }
}
```

**LayoutHint** (`syngui/src/widget/element.rs`) определяет, как дерево
раскладывает детей: `Center` (дефолт), `Column{..}`, `Row{..}`, `Stack`,
`Padding{..}`, `Grid{..}`, `Scroll{..}`, `Split{..}`, `AnimatedSize`,
`Container{..}`, `Loose`, `Portal{..}`, `FloatingWindow{..}`, `Flex{..}`,
`HorizontalPages`, `Positioned{x,y}`, `PanZoom`.

**DisplayList — что можно рисовать** (`syngui/src/render/display_list/mod.rs`):

```rust
list.push_rect(rect, color, [r; 4]);
list.push_rect_bordered(rect, color, radius, Border { .. });
list.push_rect_per_side_border(..);
list.push_gradient_rect(rect, gradient, radius);
list.push_text(text, rect, color, font_size);
list.push_text_centered / push_text_aligned / push_text_singleline / push_text_styled(..);
list.push_image(..);
list.push_shadow(..) / push_inner_shadow(..) / push_glow_shadow(..) / push_outline(..);
list.push_text_selection(..) / push_text_cursor(..);
list.push_clip(rect) / push_clip_rounded(rect, radius) / pop_clip();
list.push_transform(t) / pop_transform();
list.push_opacity(a) / pop_opacity();
list.push_effect_layer(effect, bounds) / pop_effect_layer();
list.begin_overlay() / begin_overlay_absolute() / end_overlay();
list.push_canvas(vertices, indices) / push_line_strip(points, color, width);
```

Доступность: `fn accessibility_info(&self) -> Option<AccessibilityInfo>`
(`syngui/src/a11y/`, фича `accessibility` — AccessKit).

---

## 14. Тестирование

Фича `testing`, `syngui/src/testing/mod.rs`. GPU не нужен.

```rust
use syngui::testing::*;

let mut h = TestHarness::new(Box::new(build_ui()));
h.layout(1280.0, 720.0);
h.apply_mss("Button { padding: 8px 16px; } .btn { background: #f00; }");
h.send_events(&click_at(Point::new(40.0, 24.0)));
h.send_events(&type_text("hello"));
h.send_events(&press_key(Key::Enter));
h.rebuild();

let ids = h.find_by_class("btn");
let b   = h.element_bounds(ids[0]);
let mss = h.element_mss(ids[0]);
assert_size!(h.root_size(), 1280.0, 720.0);
assert_bounds!(b, 0.0, 0.0, 120.0, 32.0);
```

Если тесты запускаются в нескольких потоках — вызови
`syngui::signal::allow_signal_reads_on_this_thread()` в начале теста.

---

## 15. Платформы

**Android** — `#[no_mangle] fn android_main(app: AndroidApp)`,
`.with_android_app(app)`, `.gpu_backend(GpuBackend::Gl)`; фича `android`.
Эталон: `synthos: src/lib.rs:145`. Клавиатура — `ctx.set_virtual_keyboard_visible(..)`;
кнопка «назад» — `Event::BackPressed` / `GestureDetector::on_back`.

**WASM** — цель `wasm32-unknown-unknown`; шрифты не берутся из системы,
задавай `.with_font_url(..)` и `.with_fallback_font_url(..)`;
`capture_function_keys` определяет, какие F-клавиши забирает приложение;
кэш тайлов карты — IndexedDB. Экранная клавиатура — тот же
`ctx.set_virtual_keyboard_visible(..)`, что на Android: скрытый `<input>`
рядом с canvas (`app/web_text_agent.rs`) берёт фокус, набор возвращается в
canvas синтетическими клавишами (`input::edit_diff`); `set_secret_keyboard(true)`
у поля пароля — `type="password"`. Хост-странице нужна
`interactive-widget=resizes-content` в viewport-мете, иначе canvas не сжимается
под клавиатуру. Поле под клавиатурой (web и Android): сначала прокрутка ближайшего
скролл-контейнера, кадром позже — сдвиг всего дерева `tree.keyboard_pan` (аналог
`adjustPan`), если поле всё ещё не видно; сдвиг снимается с закрытием клавиатуры. `syngui::core::sync::Mutex` на wasm — без блокировок
(однопоточная реализация), поэтому пиши `syngui::core::sync::Mutex`, а не `std::sync::Mutex`,
если код должен собираться под web.

---

## 16. Эталонные паттерны synthos

Приложение: `/home/master/Projects/2027/synthos` (~100 kLOC на syngui).
Частота виджетов там: `DecoratedBox` 623, `Text` 621, `Column` 320, `Row` 262,
`Reactive` 199, `ToolButton` 107, `Icon` 103, `Button` 95, `Stack` 56,
`GestureDetector` 44, `ScrollView` 34.

### 16.1. Точка входа (`src/lib.rs:44` `run_desktop`)

```
logging::init()
→ build_context()  — создать ВСЕ сигналы, собрать AppCtx, поставить effect'ы темы
→ i18n::install(ctx.general)
→ App::new()…with_styles_str(styles::styles()).with_dynamic_theme(theme_mss)
     .with_system_appearance(..).with_backdrop(..).with_window_state(..)
     .run(move |_| {
         provide_context(ctx.clone());
         provide_context(…);          // по контексту на подсистему
         install_*_autosave();        // effect'ы persist
         restore_last_view(&startup_cfg);
         Box::new(DecoratedBox::new().class("grow").child(move || {
             syngui::i18n::subscribe();
             build_app()
         }))
     })
```

Уроки:
* конфиг читается **снимком до** установки автосейва (иначе автосейв успевает
  перезаписать файл стартовыми значениями);
* глобальные effect'ы ставятся в `run`, не в `view()` страницы — иначе умрут
  вместе с пересборкой страницы (`src/lib.rs:200`);
* весь UI обёрнут в один `Reactive` ради живой смены языка.

### 16.2. Оболочка приложения (`src/lib.rs:953` `build_app`)

```rust
let shell = search::hotkey_scope(mgui! {
    DecoratedBox::new().class("window-backdrop") => [
        DecoratedBox::new().clip(true).class("shell") => [
            Column::new().gap(0.0).cross_axis_alignment(CrossAxisAlignment::Stretch) => [
                titlebar::view(),
                DecoratedBox::new().class("grow").child(mgui! {
                    Row::new().gap(0.0).cross_axis_alignment(CrossAxisAlignment::Stretch) => [
                        components::nav_rail::view(),
                        DecoratedBox::new().class("grow").child(routes),
                    ]
                }),
                DecoratedBox::new().class("window-statusbar"),
            ]
        ]
    ]
});

mgui! {
    Stack::new().clip(false) => [
        shell,
        components::template_picker::view(),
        pages::syn_chat::archive_dialog::view(),
        components::voice_fab::view(),
        search::panel::view(),
        notification_view,
    ]
}
```

Паттерн: **шелл + все глобальные оверлеи в одном `Stack` на верхнем уровне.**

### 16.3. Кликабельная карточка с контекстным меню (`src/components/chat_item.rs`)

```rust
let card = DecoratedBox::new().class(if selected { "conversation-item selected" }
                                    else        { "conversation-item" })
    .child(Row::new().gap(12.0).cross_axis_alignment(CrossAxisAlignment::Center)
        .child(Avatar::new().text(initials).size(36.0).class(tone))
        .child(DecoratedBox::new().class("grow").child(meta_col))
        .child(ToolButton::new(MI_DELETE)
            .tooltip(tr!("chat.item.delete"))
            .on_click(move || on_delete(&id))
            .class("chat-item-trailing-delete")));

let clickable = GestureDetector::new().on_click(move || on_select(&id)).child(card);

ContextMenu::new()
    .items(vec![MenuItem::new("delete", tr!("chat.item.delete")).icon(MI_DELETE)])
    .on_select(move |a| if a == "delete" { on_delete(&id) })
    .child(clickable)
```

Уроки: `DecoratedBox` даёт вид, `GestureDetector` — клик, `ContextMenu` —
правую кнопку; `ToolButton` внутри перехватывает клик раньше родителя;
`.class("grow")` растягивает средний блок.

### 16.4. Строка настроек и карточка (`src/pages/settings/widgets.rs`)

```rust
pub fn row_frame(icon, title, desc, control: Box<dyn Widget>) -> Box<dyn Widget> {
    let inner = Row::new().gap(16.0).cross_axis_alignment(CrossAxisAlignment::Center)
        .child(DecoratedBox::new().class("settings-row-icon-wrap")
            .child(Center::new().child(Icon::new(icon).class("settings-row-icon"))))
        .child(DecoratedBox::new().class("grow").child(
            Column::new().gap(2.0).cross_axis_alignment(CrossAxisAlignment::Start)
                .child(Text::new(title).class("settings-row-title"))
                .child(Text::new(desc).class("settings-row-desc"))))
        .children(vec![control]);

    Box::new(DecoratedBox::new().class("settings-row")
        .child(Padding::symmetric(24.0, 18.0).child(inner)))
}
```

Уроки: переиспользуемые «слоты» принимают `Box<dyn Widget>`;
`.children(vec![control])` — способ добавить готовый бокс.

### 16.5. Реактивная ветка загрузки (`src/pages/huggingface/list_panel.rs:25`)

См. раздел 3.4. Ключевое: аннотация `|| -> Vec<Box<dyn Widget>>` избавляет от
ручных `as Box<dyn Widget>` в каждой ветке.

### 16.6. Модалка через `Portal` (`src/pages/settings/skills_dialog.rs:34`)

```rust
// сигнал-«есть что показать» → is_open
create_effect(move || {
    let has = ctx.skills_dialog.get().is_some();
    if is_open.get_untracked() != has { is_open.set(has); }
});

Portal::new()
    .is_open(is_open).modal(true).backdrop(true).anchor(PortalAnchor::Center)
    .child(Reactive::new(|| -> Vec<Box<dyn Widget>> { /* карточка по типу диалога */ }))
```

### 16.7. Persist через один effect (`src/lib.rs:546` `install_config_autosave`)

Один `create_effect`, который читает `.get()` у всех persist-сигналов
(подписка) и пишет `config.json`. Любое изменение любого сигнала → сохранение.

### 16.8. Динамическая тема (`src/lib.rs:487`)

```rust
create_effect(move || {
    let base  = active_theme(appearance, theme_key).to_mss();
    let mut extra = String::new();
    if appearance.use_system_accent.get() {
        if let Some(accent) = appearance.system.get().accent {
            extra.push_str(&theme_data::accent_override_mss(accent, is_dark));
        }
    }
    theme_mss.set(format!(
        "{base}\n{extra}\n:root {{\n  --code-editor-font-size: {es:.0}px;\n}}\n"
    ));
});
```

Уроки: тема — обычная строка MSS в `RwSignal<String>`; поздние `:root`
перекрывают ранние; пользовательские шрифты/масштабы прокидываются
переменными, а виджеты берут их из глобальных правил
(`synthos: styles/base/reset.mss`, блоки `CodeEditor {}`, `PopupMenu {}`).

### 16.9. Организация MSS

* `styles/base/variables.mss` — только токены (`:root`);
* `styles/base/reset.mss` — глобальные правила по типам (`Text`, `Icon`,
  `TextField`, `Toggle`, `Checkbox`, `SpinBox`, `Dropdown`, `Slider`,
  `MultilineTextEdit`, `CodeEditor`, `PopupMenu`);
* `styles/base/layout.mss` — `.grow`, `.grow-2`, `.grow-3`;
* `styles/components/<component>.mss` — один компонент = один файл;
* `styles/layout/<screen>.mss` — каркасы экранов;
* всё склеивается `concat!(include_str!(..))` в `src/styles.rs`.

**Всегда задавай глобальное правило по типу виджета**, иначе виджет живёт на
своих хардкод-дефолтах и «выпадает» из тёмной темы. Особенно: `Toggle`,
`Checkbox`, `Slider`, `Dropdown`, `SpinBox`, `PopupMenu`, `CodeEditor`.

---

## 17. Фичи Cargo (`syngui/Cargo.toml:156`)

| Фича | Что даёт |
|---|---|
| `winit` | окно и `App` (в default) |
| `msdf` | MSDF-рендер текста (в default) |
| `effects` = `blur` + `shadow` | эффекты (в default) |
| `clipboard` | буфер обмена на desktop (в default) |
| `wayland-dnd` | DnD файлов на Wayland (в default) |
| `i18n` | каталоги, `tr!`/`trn!` (в default) |
| `color-emoji` | цветные эмодзи |
| `system-theme` | светлая/тёмная + акцент через XDG-портал |
| `system-blur` | размытие фона композитором (KWin/X11) |
| `wayland`, `x11`, `android` | бэкенды winit |
| `tokio` | `spawn`, `use_async` |
| `accessibility` | AccessKit (AT-SPI/UIA/NSAccessibility) |
| `markdown`, `markdown-syntax`, `markdown-links` | `MarkdownView` |
| `document-editor` | `DocumentEditor` (Notion-стиль) |
| `links` | `syngui::open_url` через системный браузер |
| `map`, `map-native` | `MapView` |
| `image`, `image-network`, `svg` | картинки, сеть, SVG |
| `audio` | захват микрофона, `AudioWaveform` |
| `ffmpeg` | видео-плеер (тянет `audio`) |
| `terminal` | `Terminal` (PTY + VT100) |
| `code-editor` (`-mvp`/`-all` — алиасы) | `CodeEditor` |
| `material-icons`, `font-awesome` | встроенные шрифты иконок |
| `tray`, `single-instance`, `splash` | трей, одна копия, сплэш |
| `debug` = `inspector` | DevTools |
| `testing` | `TestHarness` без GPU |

---

## 18. Шпаргалка ошибок компиляции

| Симптом | Причина и исправление |
|---|---|
| `no method named 'font_size' found for struct 'Text'` | размер — только MSS: `.class("h1")` + `.h1 { font-size: 24px; }` |
| `no method named 'primary'` у `Button` | `.class("btn-primary")` |
| `expected 0 arguments, found 1` у `DecoratedBox::new` | `DecoratedBox::new()` без аргументов |
| `no method named 'on_click' for DecoratedBox` | оберни в `GestureDetector::new().on_click(..).child(box)` |
| `no method named 'gap' for StyledWidget<Column>` | `.class()` поставлен раньше; перенеси его в конец цепочки |
| `expected &str, found String` в `.class(..)` | у `Button`/`ScrollView`/`PopupPanel`/`SegmentedProgressBar` `class` берёт `&str` → `.class(&s)` |
| `the trait 'IntoWidget<_>' is not implemented` | замыкание должно быть `Fn() -> impl Widget + Send + Sync + 'static`; не захватывай `!Send` |
| `expected 'Box<dyn Widget>', found ...` в `Reactive` | аннотируй билдер: `|| -> Vec<Box<dyn Widget>>` |
| `cannot move out of ... captured variable` | сигналы `Copy` — копируй их в локальные `let` перед `move`; `Arc` — `.clone()` до замыкания |
| `RwSignal::<T>::get() called from non-main thread` | сними значение до `spawn` или оберни в `run_on_main_thread` |
| `called from inside this signal's own update()` | не читай тот же сигнал внутри его `update` |
| Стиль «не применяется» | проверь имя свойства по разделу 6.4; проверь единицы (`em/rem/vh` не работают); проверь, что не пишешь свойство в `:root` |
| Класс не совпал | контейнерный `.class("a b")` создал один класс `"a b"` — пиши `.class("a").class("b")` |
| `:hover` перестал работать после `.style(..)` | inline не участвует в псевдослоях, но перекрывает base — задай hover через класс |
| Состояние сбрасывается при каждом обновлении | `use_signal` вызван внутри реактивного билдера — вынеси наружу |
| `use_context` паникует | контекст не выдан до первого использования или выдан на другом потоке; используй `try_use_context` |

---

## 19. Чек-лист перед выдачей кода

1. Все сигналы созданы **вне** реактивных замыканий.
2. Всё, что должно обновляться, — внутри `move || ...` / `Reactive::new`.
3. Свойства вида/типографики — в MSS, а не в билдерах.
4. Каждому используемому классу есть правило в `.mss`.
5. Имена MSS-свойств сверены с разделом 6.4; единицы только `px` / `%`.
6. Для каждого нестандартного виджета (`Toggle`, `Checkbox`, `Dropdown`,
   `Slider`, `PopupMenu`, `CodeEditor`) есть глобальное правило по типу.
7. `.class()` стоит последним у виджетов без собственного `class`;
   у контейнеров многоклассовость сделана цепочкой.
8. Заполнение места — `.class("grow")`; хелперы `.grow*` объявлены в MSS.
9. Клики — `GestureDetector` / `ToolButton` / `Button`, не `DecoratedBox`.
10. Оверлеи (диалоги, меню, тосты) вынесены в общий `Stack` верхнего уровня.
11. Фоновая работа: `spawn` + `set()`/`run_on_main_thread`, чтений сигналов
    из воркеров нет.
12. Долгоживущие `create_effect` поставлены в `run(...)`, а не в `view()`.
13. Все пользовательские строки — через `tr!`, корень обёрнут в `Reactive`
    с `i18n::subscribe()`.
14. Нужные фичи Cargo включены (`markdown`, `code-editor`, `terminal`,
    `tokio`, `material-icons`, …).
15. Код проверен `cargo check`.
