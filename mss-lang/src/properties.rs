//! Реестр языка MSS: свойства с типами значений и описаниями, функции
//! значений, ключевые слова, единицы, псевдоклассы.
//!
//! Единственный источник имён свойств: syngui выводит из [`PROPERTIES`]
//! список известных свойств (предупреждение «не поддерживается») и
//! [`INHERITED_PROPERTIES`] (наследование в каскаде). Новое свойство
//! добавляется сюда — тогда его видят и каскад, и инструменты.

/// Тип значения свойства — для проверки и дополнения.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ValueKind {
    /// Цвет: `#rgb`, `#rrggbb`, `#rrggbbaa`, `rgb()`, `rgba()`, имя,
    /// `darken()`, `lighten()`.
    Color,
    /// Длина в `px` или число (= px). Другие единицы (`em`, `%`…) парсер
    /// принимает, но такие свойства их не применяют.
    Length,
    /// 1–4 длины: верх, право, низ, лево (как `padding` в CSS).
    Lengths,
    /// Размер: `px`, число, `%`, `auto`, `fit-content`, `max-content`,
    /// `min-content` (ширина, высота, скругление).
    Size,
    /// 1–4 размера (`border-radius`).
    Sizes,
    /// Число без единиц (или с `px` — читается так же).
    Number,
    /// Время: `200ms`, `0.3s`.
    Time,
    /// Угол: `45deg`, `0.5turn`, `1rad`, число — градусы.
    Angle,
    /// Одно из ключевых слов.
    Keyword(&'static [&'static str]),
    /// Цвет или градиент.
    Background,
    /// Тени через запятую: `x y [blur] [spread] цвет [inset]`.
    Shadow,
    /// Тень текста: `x y [blur] цвет`.
    TextShadow,
    /// Сокращение рамки: `ширина [стиль] [цвет]`.
    Border,
    /// Функции преобразования: `translate()`, `rotate()`, `scale()`…
    Transform,
    /// Точка преобразования: `center`, `top left`, `50% 50%`.
    TransformOrigin,
    /// Цепочка фильтров: `blur(4px) brightness(1.2)`.
    Filter,
    /// Семейство шрифта: имя или строка в кавычках.
    FontFamily,
    /// Насыщенность: `normal`, `bold`, `lighter`, `bolder`, 100…900.
    FontWeight,
    /// Высота строки: множитель или `px`.
    LineHeight,
    /// `свойство время [кривая] [задержка]`, через запятую.
    Transition,
    /// `имя время [кривая] [задержка] [повторы] [направление]…`.
    Animation,
    /// Имя `@keyframes`.
    AnimationName,
    /// Кривая: `ease-out`, `cubic-bezier()`, `steps()`, `spring()`…
    Easing,
    /// Список имён свойств через запятую или `all`.
    PropertyList,
    /// Цвета через пробел (можно строкой в кавычках).
    Colors,
    /// Одно или два числа (диапазон «от до»), с единицами или без.
    Range,
    /// Значение без проверки.
    Any,
}

impl ValueKind {
    /// Короткое имя типа для подсказок.
    pub fn label(self) -> &'static str {
        match self {
            ValueKind::Color => "цвет",
            ValueKind::Length => "длина",
            ValueKind::Lengths => "1–4 длины",
            ValueKind::Size => "размер",
            ValueKind::Sizes => "1–4 размера",
            ValueKind::Number => "число",
            ValueKind::Time => "время",
            ValueKind::Angle => "угол",
            ValueKind::Keyword(_) => "ключевое слово",
            ValueKind::Background => "цвет или градиент",
            ValueKind::Shadow => "тень",
            ValueKind::TextShadow => "тень текста",
            ValueKind::Border => "рамка",
            ValueKind::Transform => "преобразование",
            ValueKind::TransformOrigin => "точка преобразования",
            ValueKind::Filter => "фильтры",
            ValueKind::FontFamily => "шрифт",
            ValueKind::FontWeight => "насыщенность шрифта",
            ValueKind::LineHeight => "высота строки",
            ValueKind::Transition => "переход",
            ValueKind::Animation => "анимация",
            ValueKind::AnimationName => "имя @keyframes",
            ValueKind::Easing => "кривая",
            ValueKind::PropertyList => "список свойств",
            ValueKind::Colors => "цвета",
            ValueKind::Range => "число или диапазон",
            ValueKind::Any => "значение",
        }
    }

    /// Ключевые слова, которые подходят типу (для дополнения).
    pub fn keywords(self) -> &'static [&'static str] {
        match self {
            ValueKind::Keyword(k) => k,
            ValueKind::Size | ValueKind::Sizes => LENGTH_KEYWORDS,
            ValueKind::FontWeight => FONT_WEIGHTS,
            ValueKind::Easing => EASINGS,
            ValueKind::Background | ValueKind::Shadow | ValueKind::Filter => &["none"],
            ValueKind::Border => BORDER_STYLES,
            ValueKind::TransformOrigin => &["center", "top", "bottom", "left", "right"],
            ValueKind::PropertyList => &["all"],
            ValueKind::Animation | ValueKind::AnimationName => &["none"],
            _ => &[],
        }
    }
}

/// Свойство MSS.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct PropertySpec {
    pub name: &'static str,
    pub kind: ValueKind,
    /// Наследуется потомками (как `color` в CSS).
    pub inherited: bool,
    /// Краткое описание по-русски.
    pub doc: &'static str,
    /// Служебное: в дополнении не показывается.
    pub internal: bool,
}

use ValueKind as K;

const fn p(name: &'static str, kind: ValueKind, doc: &'static str) -> PropertySpec {
    PropertySpec {
        name,
        kind,
        inherited: false,
        doc,
        internal: false,
    }
}

/// Наследуемое свойство.
const fn i(name: &'static str, kind: ValueKind, doc: &'static str) -> PropertySpec {
    PropertySpec {
        name,
        kind,
        inherited: true,
        doc,
        internal: false,
    }
}

pub const BORDER_STYLES: &[&str] = &[
    "solid", "dashed", "dotted", "double", "groove", "ridge", "inset", "outset", "none", "hidden",
];
/// Ключевые слова размера ([`ValueKind::Size`]).
pub const LENGTH_KEYWORDS: &[&str] = &["auto", "fit-content", "max-content", "min-content"];
pub const FONT_WEIGHTS: &[&str] = &[
    "normal", "bold", "lighter", "bolder", "100", "200", "300", "400", "500", "600", "700", "800",
    "900",
];
const ALIGN: &[&str] = &[
    "start",
    "flex-start",
    "left",
    "top",
    "center",
    "end",
    "flex-end",
    "right",
    "bottom",
];
const CURSORS: &[&str] = &[
    "default",
    "auto",
    "pointer",
    "text",
    "move",
    "grab",
    "grabbing",
    "not-allowed",
    "crosshair",
    "col-resize",
    "row-resize",
    "ew-resize",
    "e-resize",
    "w-resize",
    "ns-resize",
    "s-resize",
    "n-resize",
    "nwse-resize",
    "se-resize",
    "nw-resize",
    "nesw-resize",
    "sw-resize",
    "ne-resize",
    "none",
];
const PARTICLE_PRESETS: &[&str] = &[
    "sparkle",
    "sparkles",
    "magic",
    "confetti",
    "fireworks",
    "sparks",
    "snow",
    "rain",
    "fire",
    "smoke",
    "bubbles",
    "hearts",
    "embers",
    "poof",
    "dust",
    "stars",
];
const PARTICLE_SHAPES: &[&str] = &[
    "circle", "dot", "square", "rect", "triangle", "star", "spark", "line", "ring", "bubble",
    "glow", "soft", "heart", "mixed", "confetti",
];
/// Первые слова `particle-emitter` (дальше — аргументы формы).
pub const PARTICLE_EMITTERS: &[&str] = &[
    "point", "center", "line", "rect", "area", "ring", "circle", "pointer", "cursor", "trail",
];

/// Кривые анимации по имени (`transition`, `animation`).
pub const EASINGS: &[&str] = &[
    "linear",
    "ease",
    "ease-in",
    "ease-out",
    "ease-in-out",
    "ease-in-sine",
    "ease-out-sine",
    "ease-in-out-sine",
    "ease-in-quad",
    "ease-out-quad",
    "ease-in-out-quad",
    "ease-in-cubic",
    "ease-out-cubic",
    "ease-in-out-cubic",
    "ease-in-quart",
    "ease-out-quart",
    "ease-in-out-quart",
    "ease-in-quint",
    "ease-out-quint",
    "ease-in-out-quint",
    "ease-in-expo",
    "ease-out-expo",
    "ease-in-out-expo",
    "ease-in-circ",
    "ease-out-circ",
    "ease-in-out-circ",
    "ease-in-back",
    "ease-out-back",
    "ease-in-out-back",
    "ease-in-elastic",
    "ease-out-elastic",
    "ease-in-out-elastic",
    "ease-in-bounce",
    "ease-out-bounce",
    "ease-in-out-bounce",
    "emphasized",
    "emphasized-decelerate",
    "emphasized-accelerate",
    "standard",
    "standard-decelerate",
    "standard-accelerate",
    "spring",
    "spring-bouncy",
];

/// Все свойства MSS.
pub const PROPERTIES: &[PropertySpec] = &[
    // Цвета и фон
    p("background", K::Background, "Фон: цвет или градиент (linear-/radial-/conic-gradient)."),
    p("background-color", K::Background, "Цвет фона (градиент тоже принимается)."),
    p("background-gradient", K::Background, "Градиент фона (то же, что градиент в background)."),
    i("color", K::Color, "Цвет текста и значков. Наследуется."),
    p("accent-color", K::Color, "Акцентный цвет виджета: отметка флажка, заполнение ползунка, выделение."),
    p("selection-color", K::Color, "Цвет выделения текста."),
    i("caret-color", K::Color, "Цвет текстового курсора. Наследуется."),
    p("opacity", K::Number, "Непрозрачность элемента с потомками: 0 — невидим, 1 — непрозрачен."),
    // Рамка
    p("border", K::Border, "Сокращение рамки: ширина, стиль, цвет (`1px solid #ccc`)."),
    p("border-top", K::Border, "Сокращение верхней рамки: ширина, стиль, цвет."),
    p("border-right", K::Border, "Сокращение правой рамки: ширина, стиль, цвет."),
    p("border-bottom", K::Border, "Сокращение нижней рамки: ширина, стиль, цвет."),
    p("border-left", K::Border, "Сокращение левой рамки: ширина, стиль, цвет."),
    p("border-width", K::Lengths, "Ширина рамки: 1–4 значения (верх, право, низ, лево)."),
    p("border-top-width", K::Length, "Ширина верхней рамки."),
    p("border-right-width", K::Length, "Ширина правой рамки."),
    p("border-bottom-width", K::Length, "Ширина нижней рамки."),
    p("border-left-width", K::Length, "Ширина левой рамки."),
    p("border-style", K::Keyword(BORDER_STYLES), "Стиль рамки; none/hidden убирают её."),
    p("border-top-style", K::Keyword(BORDER_STYLES), "Стиль верхней рамки."),
    p("border-right-style", K::Keyword(BORDER_STYLES), "Стиль правой рамки."),
    p("border-bottom-style", K::Keyword(BORDER_STYLES), "Стиль нижней рамки."),
    p("border-left-style", K::Keyword(BORDER_STYLES), "Стиль левой рамки."),
    p("border-color", K::Color, "Цвет рамки."),
    p("border-top-color", K::Color, "Цвет верхней рамки."),
    p("border-right-color", K::Color, "Цвет правой рамки."),
    p("border-bottom-color", K::Color, "Цвет нижней рамки."),
    p("border-left-color", K::Color, "Цвет левой рамки."),
    p("border-radius", K::Sizes, "Скругление углов: 1–4 значения (верх-лево, верх-право, низ-право, низ-лево)."),
    p("border-top-left-radius", K::Size, "Скругление верхнего левого угла."),
    p("border-top-right-radius", K::Size, "Скругление верхнего правого угла."),
    p("border-bottom-right-radius", K::Size, "Скругление нижнего правого угла."),
    p("border-bottom-left-radius", K::Size, "Скругление нижнего левого угла."),
    p("outline", K::Border, "Контур снаружи рамки (кольцо фокуса)."),
    p("outline-width", K::Length, "Толщина контура."),
    p("outline-color", K::Color, "Цвет контура."),
    p("outline-offset", K::Length, "Отступ контура от края элемента."),
    // Размеры и отступы
    p("width", K::Size, "Ширина: px, %, auto, fit-content, max-content, min-content."),
    p("height", K::Size, "Высота: px, %, auto, fit-content, max-content, min-content."),
    p("min-width", K::Size, "Наименьшая ширина."),
    p("max-width", K::Size, "Наибольшая ширина."),
    p("min-height", K::Size, "Наименьшая высота."),
    p("max-height", K::Size, "Наибольшая высота."),
    p("padding", K::Lengths, "Внутренние отступы: 1–4 значения (верх, право, низ, лево)."),
    p("padding-top", K::Length, "Внутренний отступ сверху."),
    p("padding-right", K::Length, "Внутренний отступ справа."),
    p("padding-bottom", K::Length, "Внутренний отступ снизу."),
    p("padding-left", K::Length, "Внутренний отступ слева."),
    p("margin", K::Lengths, "Внешние отступы: 1–4 значения (верх, право, низ, лево); учитывает родитель."),
    p("margin-top", K::Length, "Внешний отступ сверху."),
    p("margin-right", K::Length, "Внешний отступ справа."),
    p("margin-bottom", K::Length, "Внешний отступ снизу."),
    p("margin-left", K::Length, "Внешний отступ слева."),
    p("gap", K::Length, "Промежуток между детьми ряда, колонки, сетки."),
    p("flex", K::Number, "Сокращение: читается как flex-grow."),
    p("flex-grow", K::Number, "Доля свободного места по главной оси, которую забирает ребёнок."),
    p("flex-shrink", K::Number, "Во сколько раз охотнее ребёнок колонки отдаёт высоту при нехватке места."),
    p("justify-content", K::Keyword(ALIGN), "Выравнивание содержимого по главной оси: start, center, end."),
    p("align-items", K::Keyword(ALIGN), "Выравнивание детей по поперечной оси: start, center, end."),
    p("overflow", K::Keyword(&["visible", "hidden", "scroll"]), "Что делать с содержимым за границами: visible, hidden, scroll."),
    p("overflow-clip-margin", K::Length, "На сколько область обрезки прокрутки выходит за границы."),
    // Текст
    i("font-size", K::Length, "Размер шрифта. Наследуется."),
    i("font-weight", K::FontWeight, "Насыщенность шрифта: normal, bold, lighter, bolder, 100–900. Наследуется."),
    p("font-style", K::Keyword(&["normal", "italic", "oblique"]), "Начертание: normal, italic."),
    i("font-family", K::FontFamily, "Семейство шрифта. Наследуется."),
    i("line-height", K::LineHeight, "Высота строки: множитель (1.4) или px. Наследуется."),
    i("letter-spacing", K::Length, "Межбуквенный интервал. Наследуется."),
    i("text-align", K::Keyword(&["left", "center", "right"]), "Выравнивание текста по горизонтали. Наследуется."),
    i("text-vertical-align", K::Keyword(&["top", "center", "bottom"]), "Выравнивание текста по вертикали. Наследуется."),
    i("text-decoration", K::Keyword(&["none", "underline", "line-through"]), "Подчёркивание или зачёркивание. Наследуется."),
    i("text-transform", K::Keyword(&["none", "uppercase", "lowercase", "capitalize"]), "Регистр букв. Наследуется."),
    i("text-shadow", K::TextShadow, "Тень текста: смещение x y, размытие, цвет. Наследуется."),
    p("text-box-edge", K::Keyword(&["text", "normal"]), "`text` — вертикаль текста по метрикам шрифта (образец «Hg»), а не по строке."),
    p("line-clamp", K::Number, "Наибольшее число строк текста, дальше — многоточие."),
    p("clipboard-hint", K::Keyword(&["on", "off", "true", "false", "show", "none", "hidden"]), "TextField: значок с текстом буфера обмена при фокусе (on/off)."),
    // Значки
    p("icon-size", K::Length, "Размер значка."),
    p("icon-color", K::Color, "Цвет значка (без него — цвет текста)."),
    p("icon-color-selected", K::Color, "Цвет значка выбранного элемента."),
    p("icon-color-hover", K::Color, "Цвет значка при наведении."),
    p("icon-color-disabled", K::Color, "Цвет значка отключённого элемента."),
    p("icon-opacity", K::Number, "Непрозрачность значка 0…1."),
    PropertySpec {
        name: "-syn-icon-font-size",
        kind: K::Length,
        inherited: false,
        doc: "Служебное: font-size правила для значка, не наследуется.",
        internal: true,
    },
    // Перетекание поверхности
    p("flow-edge", K::Keyword(&["top", "right", "bottom", "left"]), "Край, которым панель «перетекает» в соседнюю поверхность (вогнутые ушки)."),
    p("flow-radius", K::Length, "Радиус вогнутых ушек у flow-edge."),
    p("flow-color", K::Color, "Цвет поверхности, в которую перетекает панель."),
    // Эффекты
    p("box-shadow", K::Shadow, "Тени: `x y [размытие] [растяжение] цвет [inset]`, через запятую."),
    p("glow", K::Shadow, "Свечение вокруг элемента (тени без смещения), через запятую."),
    p("filter", K::Filter, "Фильтры элемента: blur(), brightness(), grayscale()…"),
    p("backdrop-filter", K::Filter, "Фильтры фона за элементом (матовое стекло: blur())."),
    p("color-tint", K::Color, "Подкраска содержимого цветом."),
    p("noise", K::Number, "Зерно поверх элемента 0…1."),
    p("vignette", K::Number, "Затемнение краёв 0…1."),
    i("cursor", K::Keyword(CURSORS), "Указатель мыши над элементом. Наследуется."),
    // Преобразования
    p("transform", K::Transform, "Преобразования: translate(), rotate(), scale(), perspective()…; разворачивается в отдельные свойства."),
    p("transform-origin", K::TransformOrigin, "Точка, вокруг которой поворот и масштаб: center, top left, 50% 50%."),
    p("translate-x", K::Length, "Сдвиг по горизонтали."),
    p("translate-y", K::Length, "Сдвиг по вертикали."),
    p("translate-z", K::Length, "Сдвиг по глубине (3D)."),
    p("rotate", K::Angle, "Поворот в плоскости."),
    p("rotate-x", K::Angle, "3D-наклон вокруг горизонтали."),
    p("rotate-y", K::Angle, "3D-поворот вокруг вертикали."),
    p("rotate-z", K::Angle, "То же, что rotate."),
    p("scale", K::Number, "Масштаб по обеим осям."),
    p("scale-x", K::Number, "Масштаб по горизонтали."),
    p("scale-y", K::Number, "Масштаб по вертикали."),
    p("perspective", K::Length, "Перспектива 3D (по умолчанию — 4 × размер элемента)."),
    p("backface-visibility", K::Keyword(&["visible", "hidden"]), "hidden — обратная сторона повёрнутого элемента не видна."),
    p("box-reflect", K::Any, "Отражение: `below|above|left|right [зазор] [непрозрачность] [длина%]` или none."),
    p("background-rotate-x", K::Angle, "Наклон только подложки вокруг горизонтали."),
    p("background-rotate-y", K::Angle, "Поворот только подложки вокруг вертикали."),
    p("background-perspective", K::Length, "Перспектива наклона подложки."),
    // Fisheye
    p("magnification", K::Number, "Fisheye: во сколько раз растёт элемент под указателем (1 — выкл.)."),
    p("magnification-range", K::Number, "Fisheye: радиус влияния в размерах элемента."),
    p("magnification-falloff", K::Keyword(&["cosine", "cos", "gaussian", "gauss", "linear"]), "Fisheye: спад увеличения."),
    p("magnification-speed", K::Number, "Fisheye: скорость догоняния, 1/с."),
    // Частицы
    p("particle-preset", K::Keyword(PARTICLE_PRESETS), "ParticleEmitter: готовый набор параметров."),
    p("particle-rate", K::Number, "ParticleEmitter: частиц в секунду всегда."),
    p("particle-hover-rate", K::Number, "ParticleEmitter: частиц в секунду, пока указатель над элементом."),
    p("particle-burst", K::Number, "ParticleEmitter: частиц во всплеске."),
    p("particle-lifetime", K::Range, "ParticleEmitter: время жизни частицы (от до), с."),
    p("particle-speed", K::Range, "ParticleEmitter: скорость (от до), px/с."),
    p("particle-direction", K::Angle, "ParticleEmitter: направление вылета (0 — вправо, -90 — вверх)."),
    p("particle-spread", K::Angle, "ParticleEmitter: разброс направления."),
    p("particle-gravity", K::Range, "ParticleEmitter: тяготение `x y` px/с² (одно число — по вертикали)."),
    p("particle-drag", K::Number, "ParticleEmitter: сопротивление воздуха."),
    p("particle-size", K::Range, "ParticleEmitter: размер частицы (от до)."),
    p("particle-size-end", K::Number, "ParticleEmitter: множитель размера к концу жизни."),
    p("particle-color", K::Color, "ParticleEmitter: цвет частиц."),
    p("particle-colors", K::Colors, "ParticleEmitter: набор цветов через пробел (можно строкой в кавычках)."),
    p("particle-color-end", K::Color, "ParticleEmitter: цвет к концу жизни."),
    p("particle-shape", K::Keyword(PARTICLE_SHAPES), "ParticleEmitter: форма частиц."),
    p("particle-spin", K::Angle, "ParticleEmitter: вращение, до ± градусов в секунду."),
    p("particle-glow", K::Length, "ParticleEmitter: мягкое свечение вокруг частиц."),
    p("particle-wobble", K::Length, "ParticleEmitter: боковое покачивание."),
    p("particle-twinkle", K::Number, "ParticleEmitter: мерцание 0…1."),
    p("particle-emitter", K::Any, "ParticleEmitter: откуда летят: point [x% y%], line top|bottom|left|right, rect, ring, pointer."),
    p("particle-layer", K::Keyword(&["front", "back"]), "ParticleEmitter: над (front) или под (back) содержимым."),
    p("particle-max", K::Number, "ParticleEmitter: наибольшее число частиц."),
    // Переходы и анимации
    p("transition", K::Transition, "Плавная смена свойств: `свойство время [кривая] [задержка]`, через запятую."),
    p("transition-property", K::PropertyList, "Какие свойства анимируются при смене (all — все)."),
    p("transition-duration", K::Time, "Длительность перехода."),
    p("transition-timing-function", K::Easing, "Кривая перехода."),
    p("animation", K::Animation, "Сокращение анимации: `имя время [кривая] [задержка] [повторы] [направление]`."),
    p("animation-name", K::AnimationName, "Имя @keyframes."),
    p("animation-duration", K::Time, "Длительность одного цикла анимации."),
    p("animation-timing-function", K::Easing, "Кривая анимации."),
    p("animation-iteration-count", K::Any, "Число повторов или infinite."),
    p("animation-delay", K::Time, "Задержка перед началом анимации."),
    p("animation-direction", K::Keyword(&["normal", "reverse", "alternate", "alternate-reverse"]), "Направление повторов анимации."),
    p("animation-fill-mode", K::Keyword(&["none", "forwards", "backwards", "both"]), "Значения до начала и после конца анимации."),
    p("animation-play-state", K::Keyword(&["running", "paused"]), "Анимация идёт или на паузе."),
    // Прокрутка
    p("scrollbar-width", K::Length, "Толщина полосы прокрутки."),
    p("scrollbar-color", K::Color, "Цвет ползунка прокрутки."),
    p("scrollbar-thumb-hover-color", K::Color, "Цвет ползунка прокрутки при наведении."),
    p("scrollbar-track-color", K::Color, "Цвет дорожки прокрутки."),
    p("scrollbar-radius", K::Length, "Скругление ползунка прокрутки."),
    p("scrollbar-policy", K::Keyword(&["auto", "always", "never", "none", "hidden"]), "Когда показывать полосы прокрутки."),
    p("scrollbar-fade-delay", K::Number, "Через сколько секунд без прокрутки полоса гаснет."),
    // Разделители
    p("divider-thickness", K::Length, "SplitView: толщина разделителя."),
    p("gutter-color", K::Color, "Цвет промежутка-разделителя (SplitView)."),
    // Графики
    p("grid-color", K::Color, "Графики: цвет сетки."),
    p("axis-color", K::Color, "Графики: цвет осей."),
    p("axis-font-size", K::Length, "Графики: размер подписей осей."),
    p("title-font-size", K::Length, "Графики: размер заголовка."),
    p("legend-font-size", K::Length, "Графики: размер подписей легенды."),
    p("tooltip-background", K::Color, "Графики: фон всплывающей подсказки."),
    p("tooltip-border-color", K::Color, "Графики: рамка всплывающей подсказки."),
    p("label-color", K::Color, "Цвет подписей (графики, шкалы)."),
    p("label-font-size", K::Length, "Размер подписей (графики, шкалы)."),
    p("value-font-size", K::Length, "Размер подписи значения."),
    p("value-color", K::Color, "TickSlider: цвет подписи значения."),
    p("track-color", K::Color, "Цвет дорожки (шкала, ползунок, прогресс)."),
    p("needle-color", K::Color, "GaugeChart: цвет стрелки."),
    p("point-size", K::Length, "LineChart: размер точек."),
    p("tick-color", K::Color, "TickSlider: цвет делений."),
    p("tick-label-color", K::Color, "TickSlider: цвет подписей делений."),
    p("tick-length", K::Length, "TickSlider: длина делений."),
    p("tick-font-size", K::Length, "TickSlider: размер подписей делений."),
    // Виджеты
    p("button-size", K::Length, "SystemWindowControls: размер кнопок окна."),
    p("button-spacing", K::Length, "SystemWindowControls: промежуток между кнопками окна."),
    p("elevation", K::Number, "TopAppBar: высота тени."),
    p("shadow-color", K::Color, "TopAppBar: цвет тени."),
    p("item-height", K::Length, "ListView: высота строки."),
    p("segment-padding", K::Length, "SegmentedButton: внутренний отступ сегмента."),
    // Таблица
    p("header-bg", K::Color, "Таблица: фон заголовка."),
    p("header-color", K::Color, "Таблица: цвет текста заголовка."),
    p("header-font-size", K::Length, "Таблица: размер шрифта заголовка."),
    p("header-padding", K::Length, "Таблица: отступ в заголовке."),
    p("row-hover-bg", K::Color, "Таблица: фон строки при наведении."),
    p("row-selected-bg", K::Color, "Таблица: фон выбранной строки."),
    p("row-striped-bg", K::Color, "Таблица: фон чередующихся строк."),
    p("row-padding", K::Lengths, "Таблица: отступы строки, 1–4 значения."),
    p("row-padding-left", K::Length, "Таблица: отступ строки слева."),
    p("row-padding-top", K::Length, "Таблица: отступ строки сверху."),
    p("row-padding-right", K::Length, "Таблица: отступ строки справа."),
    p("row-padding-bottom", K::Length, "Таблица: отступ строки снизу."),
    p("cell-padding", K::Length, "Таблица: отступ в ячейке."),
    p("cell-font-size", K::Length, "Таблица: размер шрифта ячейки."),
    p("cell-min-width", K::Length, "Таблица: наименьшая ширина столбца."),
    p("cell-max-width", K::Length, "Таблица: наибольшая ширина столбца."),
    p("grid-alpha", K::Number, "Таблица: непрозрачность линий сетки 0…1."),
    // Редактор кода
    p("editor-bg", K::Color, "CodeEditor: фон."),
    p("editor-fg", K::Color, "CodeEditor: цвет текста."),
    p("editor-gutter-bg", K::Color, "CodeEditor: фон полосы номеров строк."),
    p("editor-gutter-fg", K::Color, "CodeEditor: цвет номеров строк."),
    p("editor-cursor", K::Color, "CodeEditor: цвет курсора."),
    p("editor-selection", K::Color, "CodeEditor: цвет выделения."),
    p("editor-current-line", K::Color, "CodeEditor: подсветка текущей строки."),
    p("editor-bracket-match", K::Color, "CodeEditor: подсветка парной скобки."),
    p("editor-whitespace", K::Color, "CodeEditor: цвет видимых пробелов."),
    p("editor-find-match", K::Color, "CodeEditor: найденное."),
    p("editor-find-current", K::Color, "CodeEditor: текущее найденное."),
    p("editor-error", K::Color, "CodeEditor: метка ошибки."),
    p("editor-warning", K::Color, "CodeEditor: метка предупреждения."),
    p("editor-info", K::Color, "CodeEditor: метка сведения."),
    p("editor-hint", K::Color, "CodeEditor: метка подсказки."),
    p("editor-popup-bg", K::Color, "CodeEditor: фон всплывающих окон."),
    p("editor-popup-border", K::Color, "CodeEditor: рамка всплывающих окон."),
    p("token-keyword", K::Color, "CodeEditor: ключевые слова."),
    p("token-keyword-control", K::Color, "CodeEditor: управляющие ключевые слова."),
    p("token-type", K::Color, "CodeEditor: типы."),
    p("token-type-builtin", K::Color, "CodeEditor: встроенные типы."),
    p("token-function", K::Color, "CodeEditor: функции."),
    p("token-function-macro", K::Color, "CodeEditor: макросы."),
    p("token-constant", K::Color, "CodeEditor: константы."),
    p("token-constant-builtin", K::Color, "CodeEditor: встроенные константы."),
    p("token-string", K::Color, "CodeEditor: строки."),
    p("token-string-special", K::Color, "CodeEditor: особые строки (экранирование, regex)."),
    p("token-number", K::Color, "CodeEditor: числа."),
    p("token-comment", K::Color, "CodeEditor: комментарии."),
    p("token-operator", K::Color, "CodeEditor: операторы."),
    p("token-punctuation", K::Color, "CodeEditor: знаки препинания."),
    p("token-variable", K::Color, "CodeEditor: переменные."),
    p("token-property", K::Color, "CodeEditor: поля и свойства."),
    p("token-attribute", K::Color, "CodeEditor: атрибуты."),
    p("token-tag", K::Color, "CodeEditor: теги."),
    p("token-namespace", K::Color, "CodeEditor: пространства имён."),
];

/// Свойство по имени.
pub fn property(name: &str) -> Option<&'static PropertySpec> {
    PROPERTIES.iter().find(|p| p.name == name)
}

/// Свойство известно MSS (переменные `--x` — тоже).
pub fn is_known_property(name: &str) -> bool {
    name.starts_with("--") || PROPERTIES.iter().any(|p| p.name == name)
}

const fn count_inherited() -> usize {
    let mut n = 0;
    let mut k = 0;
    while k < PROPERTIES.len() {
        if PROPERTIES[k].inherited {
            n += 1;
        }
        k += 1;
    }
    n
}

const INHERITED_ARR: [&str; count_inherited()] = {
    let mut out = [""; count_inherited()];
    let mut n = 0;
    let mut k = 0;
    while k < PROPERTIES.len() {
        if PROPERTIES[k].inherited {
            out[n] = PROPERTIES[k].name;
            n += 1;
        }
        k += 1;
    }
    out
};

/// Наследуемые свойства (из [`PROPERTIES`]).
pub const INHERITED_PROPERTIES: &[&str] = &INHERITED_ARR;

const NAMES_ARR: [&str; PROPERTIES.len()] = {
    let mut out = [""; PROPERTIES.len()];
    let mut k = 0;
    while k < PROPERTIES.len() {
        out[k] = PROPERTIES[k].name;
        k += 1;
    }
    out
};

/// Имена всех свойств (из [`PROPERTIES`]).
pub const PROPERTY_NAMES: &[&str] = &NAMES_ARR;

/// Ключевые слова каскада, допустимые у любого свойства.
pub const GLOBAL_KEYWORDS: &[(&str, &str)] = &[
    ("inherit", "Значение родителя."),
    ("initial", "Значение по умолчанию (сброс)."),
    (
        "unset",
        "Наследуемое — как inherit, остальное — как initial.",
    ),
];

/// Единицы длины.
pub const LENGTH_UNITS: &[&str] = &["px", "%", "em", "rem", "vw", "vh"];
/// Единицы времени.
pub const TIME_UNITS: &[&str] = &["ms", "s"];
/// Единицы угла.
pub const ANGLE_UNITS: &[&str] = &["deg", "rad", "turn"];

/// Псевдоклассы, которые знает каскад. Правило с другим пропускается.
pub const PSEUDO_CLASSES: &[(&str, &str)] = &[
    ("hover", "Указатель над элементом."),
    ("active", "Элемент нажат."),
    ("pressed", "Элемент нажат (то же, что :active)."),
    ("focus", "Элемент в фокусе ввода."),
    ("selected", "Элемент выбран (вкладка, строка списка)."),
    ("checked", "Флажок или переключатель включён."),
    ("disabled", "Элемент отключён."),
    ("window-maximized", "Окно развёрнуто."),
    ("window-fullscreen", "Окно во весь экран."),
    ("window-focused", "Окно активно."),
];

/// Псевдокласс известен каскаду.
pub fn is_known_pseudo(name: &str) -> bool {
    PSEUDO_CLASSES.iter().any(|(p, _)| *p == name)
}

/// Где функция значения уместна.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum FunctionGroup {
    /// Значение любого свойства: `var()`.
    Any,
    Color,
    Gradient,
    Transform,
    Filter,
    Easing,
}

/// Функция значения: `rgb()`, `var()`, `linear-gradient()`…
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct FunctionSpec {
    pub name: &'static str,
    pub group: FunctionGroup,
    /// Образец вызова.
    pub signature: &'static str,
    pub doc: &'static str,
}

const fn f(
    name: &'static str,
    group: FunctionGroup,
    signature: &'static str,
    doc: &'static str,
) -> FunctionSpec {
    FunctionSpec {
        name,
        group,
        signature,
        doc,
    }
}

use FunctionGroup as G;

/// Функции значений MSS.
pub const FUNCTIONS: &[FunctionSpec] = &[
    f("var", G::Any, "var(--имя[, по умолчанию])", "Значение переменной; второе — если переменной нет. Внутри rgba() и других функций не раскрывается."),
    f("rgb", G::Color, "rgb(r, g, b)", "Цвет из компонент 0–255."),
    f("rgba", G::Color, "rgba(r, g, b, a)", "Цвет с непрозрачностью a 0…1. Только числа: var() внутри не работает."),
    f("darken", G::Color, "darken(цвет, доля)", "Затемнить цвет на долю 0…1 (по умолчанию 0.1)."),
    f("lighten", G::Color, "lighten(цвет, доля)", "Осветлить цвет на долю 0…1 (по умолчанию 0.1)."),
    f("linear-gradient", G::Gradient, "linear-gradient(угол | to сторона, цвет [позиция], …)", "Линейный градиент."),
    f("radial-gradient", G::Gradient, "radial-gradient([circle | ellipse] [at x y], цвет [позиция], …)", "Радиальный градиент."),
    f("conic-gradient", G::Gradient, "conic-gradient([from угол] [at x y], цвет [позиция], …)", "Конический градиент."),
    f("translate", G::Transform, "translate(x[, y])", "Сдвиг."),
    f("translateX", G::Transform, "translateX(x)", "Сдвиг по горизонтали."),
    f("translateY", G::Transform, "translateY(y)", "Сдвиг по вертикали."),
    f("translateZ", G::Transform, "translateZ(z)", "Сдвиг по глубине."),
    f("translate3d", G::Transform, "translate3d(x, y, z)", "Сдвиг по трём осям."),
    f("scale", G::Transform, "scale(s[, sy])", "Масштаб."),
    f("scaleX", G::Transform, "scaleX(s)", "Масштаб по горизонтали."),
    f("scaleY", G::Transform, "scaleY(s)", "Масштаб по вертикали."),
    f("rotate", G::Transform, "rotate(угол)", "Поворот в плоскости."),
    f("rotateX", G::Transform, "rotateX(угол)", "3D-наклон вокруг горизонтали."),
    f("rotateY", G::Transform, "rotateY(угол)", "3D-поворот вокруг вертикали."),
    f("rotateZ", G::Transform, "rotateZ(угол)", "То же, что rotate."),
    f("perspective", G::Transform, "perspective(длина)", "Перспектива 3D."),
    f("blur", G::Filter, "blur(радиус)", "Размытие."),
    f("grayscale", G::Filter, "grayscale(доля)", "Обесцвечивание 0…1."),
    f("sepia", G::Filter, "sepia(доля)", "Сепия 0…1."),
    f("invert", G::Filter, "invert(доля)", "Инверсия 0…1."),
    f("brightness", G::Filter, "brightness(множитель)", "Яркость."),
    f("contrast", G::Filter, "contrast(множитель)", "Контраст."),
    f("hue-rotate", G::Filter, "hue-rotate(угол)", "Поворот оттенка."),
    f("saturate", G::Filter, "saturate(множитель)", "Насыщенность."),
    f("pixelate", G::Filter, "pixelate(размер)", "Пикселизация."),
    f("edge-detect", G::Filter, "edge-detect(сила)", "Выделение контуров."),
    f("chromatic-aberration", G::Filter, "chromatic-aberration(сдвиг)", "Хроматическая аберрация."),
    f("vignette", G::Filter, "vignette(сила)", "Затемнение краёв."),
    f("noise", G::Filter, "noise(сила)", "Зерно."),
    f("crt", G::Filter, "crt(сила)", "Эффект ЭЛТ-экрана."),
    f("wave", G::Filter, "wave(амплитуда, частота)", "Волна."),
    f("glitch", G::Filter, "glitch(сила)", "Сбой изображения."),
    f("dissolve", G::Filter, "dissolve(доля)", "Растворение."),
    f("swirl", G::Filter, "swirl(угол, радиус)", "Завихрение."),
    f("bulge", G::Filter, "bulge(сила)", "Выпуклость."),
    f("pinch", G::Filter, "pinch(сила)", "Вдавливание."),
    f("gradient-map", G::Filter, "gradient-map(цвет, цвет)", "Карта градиента по яркости."),
    f("duotone", G::Filter, "duotone(цвет, цвет)", "Двухцветное изображение."),
    f("silhouette", G::Filter, "silhouette(цвет)", "Силуэт одним цветом."),
    f("heat-haze", G::Filter, "heat-haze(сила, скорость)", "Марево."),
    f("directional-blur", G::Filter, "directional-blur(радиус, угол)", "Размытие в направлении."),
    f("motion-blur", G::Filter, "motion-blur(радиус, угол)", "Размытие движения."),
    f("radial-blur", G::Filter, "radial-blur(сила)", "Радиальное размытие."),
    f("zoom-blur", G::Filter, "zoom-blur(сила)", "То же, что radial-blur."),
    f("color-grade", G::Filter, "color-grade(…)", "Цветокоррекция."),
    f("hologram", G::Filter, "hologram(…)", "Голограмма."),
    f("x-ray", G::Filter, "x-ray(…)", "Рентген."),
    f("refraction", G::Filter, "refraction(…)", "Преломление."),
    f("lens-flare", G::Filter, "lens-flare(…)", "Блик объектива."),
    f("mask-reveal", G::Filter, "mask-reveal(…)", "Проявление по маске."),
    f("cubic-bezier", G::Easing, "cubic-bezier(x1, y1, x2, y2)", "Кривая Безье."),
    f("steps", G::Easing, "steps(n)", "Ступенчатая кривая."),
    f("spring", G::Easing, "spring([жёсткость[, затухание]])", "Пружина."),
];

/// Функция значения по имени (без учёта регистра у transform-функций).
pub fn function(name: &str) -> Option<&'static FunctionSpec> {
    FUNCTIONS
        .iter()
        .find(|f| f.name == name)
        .or_else(|| FUNCTIONS.iter().find(|f| f.name.eq_ignore_ascii_case(name)))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn names_are_unique_and_derived_lists_match() {
        let mut names: Vec<_> = PROPERTIES.iter().map(|p| p.name).collect();
        names.sort();
        let n = names.len();
        names.dedup();
        assert_eq!(names.len(), n, "повтор имени свойства");
        assert_eq!(PROPERTY_NAMES.len(), PROPERTIES.len());
        let mut inh = INHERITED_PROPERTIES.to_vec();
        inh.sort();
        assert_eq!(
            inh,
            [
                "caret-color",
                "color",
                "cursor",
                "font-family",
                "font-size",
                "font-weight",
                "letter-spacing",
                "line-height",
                "text-align",
                "text-decoration",
                "text-shadow",
                "text-transform",
                "text-vertical-align",
            ]
        );
        assert!(PROPERTIES.iter().all(|p| !p.doc.is_empty()));
    }

    #[test]
    fn lookups() {
        assert_eq!(property("gap").unwrap().kind, ValueKind::Length);
        assert!(is_known_property("--x"));
        assert!(!is_known_property("colour"));
        assert!(is_known_pseudo("hover"));
        assert!(!is_known_pseudo("focus-within"));
        assert_eq!(function("rotatey").unwrap().name, "rotateY");
    }
}
