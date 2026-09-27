# 3D, частицы и «рыбий глаз»

Три возможности, из которых собирается док syndesktop, но полезные в любом
интерфейсе: 3D-трансформации плоских элементов (в том числе через MSS),
система частиц с пресетами и настройкой из MSS и контейнер с увеличением под
указателем.

## 3D-трансформации

Элемент остаётся плоским, как `transform: rotateY()` в CSS: его поддерево
рисуется в текстуру слоя, а слой выводится четырёхугольником, углы которого
спроецированы с перспективой. Углы передаются GPU в однородных координатах
`[X, Y, W]`, поэтому текстура интерполируется перспективно-корректно — без
«излома» по диагонали. Вращение применяется после 2D-трансформаций
(`translate`, `rotate`, `scale`) вокруг `transform-origin`; туда же ставится
точка схода перспективы.

### MSS

```css
.card {
    rotate-x: 20deg;            /* наклон вокруг горизонтали: низ — к зрителю */
    rotate-y: -30deg;           /* поворот вокруг вертикали: правый край — от зрителя */
    translate-z: 12px;          /* к зрителю (с перспективой — крупнее) */
    perspective: 400px;         /* расстояние до зрителя; по умолчанию 4 × размер элемента, 0 — без перспективы */
    backface-visibility: hidden;/* не рисовать, повёрнутый обратной стороной */
    transform-origin: center bottom;
    transition: rotate-y 600ms ease-out-back;
}
.card:hover { rotate-y: 180deg; }

/* то же функциями */
.card { transform: perspective(400px) rotateX(20deg) rotateY(-30deg) translateZ(12px); }
```

Углы — `deg`, `rad`, `turn` или число (градусы); `rotate-z` — то же, что `rotate`.
Все четыре свойства анимируются переходами и `@keyframes`:

```css
@keyframes flip {
    from { rotate-y: 0deg; }
    to { rotate-y: 360deg; }
}
.coin { animation: flip 1.2s ease-in-out infinite; }
```

### Отражение

```css
.icon { box-reflect: below 3px 0.3 45%; }   /* сторона, зазор, непрозрачность, длина (доля размера) */
```

Стороны: `below`, `above`, `left`, `right`. Отражение затухает к дальнему краю и
работает вместе с 3D-поворотом (отражается уже повёрнутый элемент).

### Наклон только подложки

Контейнеры, которые рисуют подложку через `MssFields::paint_box` (сейчас —
`Fisheye`), умеют наклонять в 3D только фон, тени и рамку, оставляя содержимое
ровным — 3D-«полка» дока:

```css
.shelf {
    background: linear-gradient(180deg, #ffffff40, #ffffff12);
    background-rotate-x: 56deg;
    background-perspective: 420px;
    transform-origin: center bottom;
}
```

### Rust

```rust
Animated::new(card)
    .rotate_y(Animation::tween(Easing::EaseInOutCubic).from(0.0).to(180.0).duration_ms(700).build())
    .rotate_x(Animation::spring().from(-15.0).to(0.0).into())
    .translate_z(Animation::tween(Easing::EaseOutCubic).from(-40.0).to(0.0).build())
    .perspective(500.0)
    .backface_visible(false);

// Вручную в своём элементе:
list.push_projected_layer(bounds, Some(&Transform3D { rotate_y: 30.0, ..Default::default() }), None);
// … содержимое …
list.pop_effect_layer();
```

`Transform3D`, `Reflection`, `ProjectedQuad` и `project_layer` — в
`syngui::core::transform3d`; эффект — `Effect::Projected` (см. [13-effects.md](13-effects.md)).

Стоимость: каждый повёрнутый элемент — один слой (текстура размера окна из пула) и
один проход вывода; плоские элементы ничего не платят.

## Частицы: `ParticleEmitter`

Эмиттер — обёртка вокруг содержимого или самостоятельный слой. Непрерывный поток,
всплески, поток только при наведении и «след» за указателем; частицы рисуются
поверх содержимого (`particle-layer: back` — под ним).

```rust
// Всплеск звёзд при каждом изменении счётчика.
ParticleEmitter::new().preset(ParticlePreset::Stars).burst(30).burst_token(clicks.get()).child(button)

// Искры, пока указатель над значком.
ParticleEmitter::new().preset(ParticlePreset::Sparkle).hover_rate(16.0).child(icon)

// Снег по всему окну.
Stack::new().fit(StackFit::Expand).child(content).child(ParticleEmitter::new().preset(ParticlePreset::Snow).rate(20.0))
```

Пресеты: `sparkle`, `magic`, `confetti`, `fireworks`, `sparks`, `snow`, `rain`,
`fire`, `smoke`, `bubbles`, `hearts`, `embers`, `poof` (облачко, как при удалении
значка из дока macOS), `dust`, `stars`. Формы: `circle`, `square`, `triangle`,
`star`, `spark` (чёрточка по скорости), `ring`, `glow` (мягкое пятно), `heart`,
`mixed`.

Всё настраивается и из MSS — свойства поверх пресета:

```css
.fx {
    particle-preset: fire;
    particle-rate: 0;              /* в секунду всегда */
    particle-hover-rate: 18;       /* … пока указатель над элементом (нужна обёртка .child) */
    particle-burst: 40;            /* во всплеске (burst_token) */
    particle-lifetime: 0.6s 1.2s;
    particle-speed: 20 60;         /* px/с */
    particle-direction: -90deg;    /* 0 — вправо, -90 — вверх */
    particle-spread: 60deg;
    particle-gravity: 0 -20;       /* px/с² */
    particle-drag: 1;
    particle-size: 2px 6px;
    particle-size-end: 0.2;        /* множитель к концу жизни */
    particle-color: var(--accent);
    particle-colors: "#fff #ffe9a8 #a8d8ff";
    particle-color-end: #ff000000;
    particle-shape: star;
    particle-spin: 180deg;         /* до ± градусов в секунду */
    particle-glow: 6px;
    particle-wobble: 6px;
    particle-twinkle: 0.5;
    particle-emitter: rect;        /* point [x% y%] | line top|bottom|left|right | rect | ring | pointer */
    particle-layer: front;
    particle-max: 300;
}
```

Кругами и свечением частицы рисуются SDF-прямоугольниками и тенями (дёшево,
сглаженно), фигуры — тесселированными многоугольниками. Пока частиц нет и поток
выключен, эмиттер не встаёт в тик анимаций. Старый `ParticleSystem`
(конфетти/искры/фейерверк по счётчику) остался как есть.

## «Рыбий глаз»: `Fisheye` и `ScaleBox`

`ScaleBox` — ребёнок в естественном размере, увеличенный вместе с местом, которое
он занимает: соседи расступаются, рисунок масштабируется трансформацией (картинки
и SVG остаются чёткими — мелкие SVG растеризуются минимум в 192 px), события мыши
переводятся в координаты ребёнка.

```rust
Row::new().child(ScaleBox::new(icon).scale(1.5)).child(other)
```

`Fisheye` — ряд или колонка таких коробок, элементы плавно вырастают под
указателем, как значки дока macOS. Увеличение считается по расстоянию до центров
в неувеличенной раскладке (без обратной связи — ничего не «дрожит»), каждый
элемент догоняет цель экспоненциально.

```rust
Fisheye::new()
    .zoom(1.8)                 // во сколько раз растёт элемент под указателем
    .range(3.0)                // радиус влияния — в размерах элемента
    .falloff(Falloff::Cosine)  // Cosine | Gaussian | Linear
    .speed(16.0)               // скорость догоняния, 1/с
    .overflow(true)            // растут поверх ряда, высота полосы постоянна
    .cross_axis_alignment(CrossAxisAlignment::End) // растут вверх (док снизу)
    .vertical(false)
    .on_hover(|hit| { /* (индекс, прямоугольник после увеличения) — подпись, якорь меню */ })
    .children(icons)
```

MSS: `magnification`, `magnification-range`, `magnification-falloff`,
`magnification-speed`, `gap`, `padding`, фон/рамка/тени (с `background-rotate-x`).

С `overflow(true)` ряд сам ловит указатель над выросшими элементами (его область
попадания расширена в сторону роста), а клики по ним доходят до ребёнка.

## Отладка без экрана

`syngui-layer` (оболочка syndesktop) рендерит в PNG без композитора
(`SYNGUI_LAYER_HEADLESS=1600x900 SYNGUI_LAYER_DUMP=dir`), эффекты и 3D в таких
снимках тоже видны. Сценарий указателя:
`SYNGUI_LAYER_SCRIPT="1200 move syndesktop-dock 700,150; 1500 click …; 1600 down …; 1700 up …; 2000 leave …"`.
`SYNGUI_TRACE_HIT=1` печатает путь попадания каждого нажатия.
