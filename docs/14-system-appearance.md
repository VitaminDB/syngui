# 14. Системное оформление

Приложение с собственным титлбаром (CSD) выглядит инородно, если игнорирует
настройки рабочего стола. syngui отдаёт три независимых куска этих настроек:

| Что | Модуль | Источник |
|---|---|---|
| Светлая/тёмная схема, акцент, контраст, reduced-motion | `syngui::appearance` | XDG-портал `org.freedesktop.appearance`, конфиги DE, winit `ThemeChanged` |
| Набор, порядок и вид кнопок окна | `syngui::appearance::decorations` | `kwinrc` + тема Aurorae, `gsettings button-layout` |
| Размытие фона за прозрачным окном | `syngui::window::backdrop` | `ext-background-effect-v1`, `org_kde_kwin_blur`, `_KDE_NET_WM_BLUR_BEHIND_REGION` |

Фичи: `system-theme` (портал через zbus), `system-blur` (Wayland-протоколы +
x11rb). Без них модули остаются, но читают только конфиги DE и не размывают
фон.

## Светлая/тёмная схема и акцент

```rust
use syngui::appearance::{read_system_appearance, SystemAppearance};

let appearance = use_signal(read_system_appearance());   // до первого кадра

App::new()
    .with_dynamic_theme(theme_mss)
    .with_system_appearance(appearance)                  // дальше держит сам фреймворк
    .run(...);
```

`with_system_appearance` наполняет сигнал до построения дерева виджетов и
обновляет его при каждом изменении настроек: на Linux — по сигналу
`SettingChanged` портала (поток `syngui-appearance`, значение возвращается в
главный поток через `run_on_main_thread`), на Windows/macOS — по
`WindowEvent::ThemeChanged`.

```rust
pub struct SystemAppearance {
    pub color_scheme: ColorScheme,   // NoPreference | Dark | Light
    pub accent: Option<Color>,       // None — DE акцент не сообщает
    pub high_contrast: bool,
    pub reduced_motion: bool,
}
```

Акцент приходит из портала в sRGB 0..1 и хранится как обычный `Color`
(`Color::from_srgb_f32`); обратно в палитру — через `Color::to_hex()`.
Производные цвета удобно считать теми же средствами: `lighten`/`darken` и
`readable_on()` (чёрный или белый — тот, что контрастнее на акценте).

Почему не winit: `Window::theme()` на X11 всегда `None`, а на Wayland отражает
только ту тему CSD, которую приложение выставило само. Портал же работает и в
X11-сессии, и во flatpak.

Отладочные override'ы: `SYNGUI_COLOR_SCHEME=dark|light`,
`SYNGUI_ACCENT_COLOR=#RRGGBB` — при них слежение не запускается.

## Кнопки окна

```rust
use syngui::widgets::overlay::SystemWindowControls;

Row::new()
    .child(SystemWindowControls::left())
    .child(title)
    .child(SystemWindowControls::right())
```

Виджет сам читает раскладку рабочего стола и рисует только те кнопки, которые
приложение умеет выполнять (свернуть / развернуть / закрыть). На KDE с темой
Aurorae состояния кнопок растеризуются из SVG темы (`active-center`,
`hover-center`, `pressed-center`, `inactive-center`) — то есть выглядят точно
как у остальных окон. В остальных окружениях рисуется встроенный вектор,
который красится из MSS (`color`, `background-color`).

Полезные методы: `.decorations(...)` (готовый снимок вместо чтения диска),
`.button_size(px)`, `.spacing(px)`, `.active(bool)`, `.maximized(bool)`.

Виджет занимает всю высоту, которую ему даёт родитель, и центрирует кнопки
внутри — иначе группа получается ростом с кнопку и липнет к верхнему краю
титлбара. SVG растеризуется ровно в физические пиксели кнопки (масштаб экрана
берётся из окна и пересчитывается при переезде на другой монитор): текстуры
грузятся без mip-уровней, поэтому «запас» разрешения с последующим уменьшением
даёт рваные края.

Состояние окна для `.active()`/`.maximized()` удобно брать из сигнала:

```rust
let window_state = use_signal(syngui::window::WindowState::default());
App::new().with_window_state(window_state)   // maximized / fullscreen / focused
```

Прочитать настройки отдельно — `read_system_decorations()`:

```rust
pub struct SystemDecorations {
    pub layout: DecorationLayout,      // left / right — порядок кнопок
    pub metrics: DecorationMetrics,    // размер, зазор, отступы, выравнивание заголовка
    pub style: DecorationStyle,        // Aurorae(тема) | Native
}
```

## Ресайз frameless-окна

Перетаскивание за титлбар (`WindowDragRegion`) — лишь половина
собственной рамки: у окна без декораций нет и зон захвата для изменения
размера. `WindowResizeRegion` добавляет их сам: полоса шириной `inset`
вдоль каждого края (и угловые зоны 24px) показывает курсор-стрелку, а
нажатие левой кнопкой вызывает `EventContext::start_window_resize(dir)` —
фреймворк передаёт направление в `winit::Window::drag_resize_window`, и
дальше окно тянет оконная система, как обычную рамку.

```rust
use syngui::widgets::overlay::WindowResizeRegion;

WindowResizeRegion::new()
    .inset(24.0)          // = padding прозрачного «воздуха» вокруг шелла
    .child(DecoratedBox::new().class("window-backdrop").child(shell))
```

Дочерние элементы получают события первыми, поэтому кнопки у самого края
продолжают работать. Когда окно развёрнуто или в полноэкранном режиме,
зона отключается (по флагам `:window-maximized` / `:window-fullscreen`),
иначе она легла бы на содержимое у края экрана.

## Размытие фона

```rust
use syngui::window::BackdropConfig;

let backdrop = use_signal(BackdropConfig::frosted());  // blur + контраст

App::new()
    .transparent(true)                 // без этого эффект не виден
    .background(Color::rgba(0.0, 0.0, 0.0, 0.0))
    .with_backdrop(backdrop)
    .run(...);
```

Сигнал читается эффектом, поэтому размытие включается и выключается на лету.
Фон панелей при этом должен быть полупрозрачным — эффект виден ровно там, где
сквозь окно что-то просвечивает.

**Форма области важна.** У окна с CSD поверхность обычно больше видимой части:
вокруг «шелла» остаётся прозрачная рамка под тень и resize-захват. Если
размывать всю поверхность, вокруг окна повиснет мутный прямоугольник, поэтому
область задаётся явно:

```rust
BackdropConfig::frosted().with_shell(30.0, 20.0)   // отступ и радиус в логических px
```

Скругление собирается из горизонтальных полос (`wl_region` умеет только
прямоугольники). Регион живёт в координатах поверхности, поэтому фреймворк
переустанавливает его на каждый resize — приложению достаточно менять отступ и
радиус, когда окно разворачивается.

Протокол выбирается сам: KWin 6.7+ рекламирует стандартный
`ext_background_effect_manager_v1` (регион задаётся явно — пустой означает
«эффекта нет»), более старые версии — `org_kde_kwin_blur` и
`org_kde_kwin_contrast`, X11 — свойство `_KDE_NET_WM_BLUR_BEHIND_REGION`. Если
композитор не умеет ничего из этого, `set_backdrop` возвращает `false`, и окно
остаётся просто прозрачным.
