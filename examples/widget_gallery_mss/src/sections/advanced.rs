//! Раздел «Продвинутые виджеты»: граф, ползунок с делениями, сегментный
//! прогресс, смахиваемый список, виртуальный список на 10 000 строк, строка
//! меню, экранная клавиатура, области панорамирования и просмотра картинки,
//! волна звука, пропорции и масштаб, док-панели.
//!
//! Цвета — только из переменных темы (styles/pages/advanced.mss). У GraphView
//! цвета текста и рёбер — `Color::TRANSPARENT` = «из темы».

use std::sync::Arc;

use syngui::audio::AudioBuffer;
use syngui::prelude::*;
use syngui::signal::use_signal;
use syngui::widgets::containers::{PanZoomViewport, VirtualList, VirtualRow};
use syngui::widgets::visual::{SegmentState, SegmentedProgressBar, StaticWaveform};
use syngui::widgets::*;

use super::{label, rtext, section_card, section_title};

const PIC_CITY: &str = "examples/widget_gallery_mss/pics/city.jpg";
const PIC_NATURE: &str = "examples/widget_gallery_mss/pics/nature.jpg";
const PIC_ABSTRACT: &str = "examples/widget_gallery_mss/pics/abstract.jpg";

pub fn build_advanced_section() -> impl Widget {
    Column::new()
        .gap(20.0)
        .cross_axis_alignment(CrossAxisAlignment::Stretch)
        .child(menu_bar_card())
        .child(graph_card())
        .child(sliders_card())
        .child(
            Row::new()
                .gap(20.0)
                .cross_axis_alignment(CrossAxisAlignment::Stretch)
                .child(grow(dismissible_card()))
                .child(grow(virtual_list_card())),
        )
        .child(keyboard_card())
        .child(viewports_card())
        .child(waveform_card())
        .child(aspect_scale_card())
        .child(dock_card())
}

/// Растягиваемая ячейка ряда (класс `grow` — flex-grow: 1).
fn grow(child: impl Widget + 'static) -> impl Widget {
    DecoratedBox::new().class("grow").child(child)
}

/// Карточка демо: заголовок, пояснение и содержимое.
fn demo_card(title: &str, hint: &str, body: impl Widget + 'static) -> impl Widget {
    section_card(
        Column::new()
            .gap(12.0)
            .cross_axis_alignment(CrossAxisAlignment::Stretch)
            .child(section_title(title))
            .child(label(hint))
            .child(body),
    )
}

// ─── MenuBar ────────────────────────────────────────────────────────────────

fn menu_bar_card() -> impl Widget {
    let picked = use_signal(String::from("—"));
    let wrap = use_signal(true);
    demo_card(
        "MenuBar",
        "Строка меню приложения: щелчок раскрывает меню, при открытом меню наведение на соседний \
         заголовок переключает его. Пункты строятся при каждом открытии — «Перенос строк» \
         показывает текущее состояние.",
        Column::new()
            .gap(10.0)
            .cross_axis_alignment(CrossAxisAlignment::Stretch)
            .child(
                DecoratedBox::new().class("adv-menubar-host").child(
                    MenuBar::new()
                        .menu("Файл", || {
                            vec![
                                MenuItem::new("new", "Новый").shortcut("Ctrl+N"),
                                MenuItem::new("open", "Открыть…").shortcut("Ctrl+O"),
                                MenuItem::new("recent", "Недавние").children(vec![
                                    MenuItem::new("recent.1", "notes.md"),
                                    MenuItem::new("recent.2", "main.rs"),
                                    MenuItem::new("recent.3", "Cargo.toml"),
                                ]),
                                MenuItem::separator(),
                                MenuItem::new("save", "Сохранить").shortcut("Ctrl+S"),
                                MenuItem::new("save_as", "Сохранить как…").shortcut("Ctrl+Shift+S"),
                                MenuItem::separator(),
                                MenuItem::new("quit", "Выход").shortcut("Ctrl+Q"),
                            ]
                        })
                        .menu("Правка", || {
                            vec![
                                MenuItem::new("undo", "Отменить").shortcut("Ctrl+Z"),
                                MenuItem::new("redo", "Повторить")
                                    .shortcut("Ctrl+Y")
                                    .disabled(true),
                                MenuItem::separator(),
                                MenuItem::new("cut", "Вырезать").shortcut("Ctrl+X"),
                                MenuItem::new("copy", "Копировать").shortcut("Ctrl+C"),
                                MenuItem::new("paste", "Вставить").shortcut("Ctrl+V"),
                            ]
                        })
                        .menu("Вид", move || {
                            let mark = if wrap.get_untracked() { "✓ " } else { "    " };
                            vec![
                                MenuItem::new("wrap", format!("{mark}Перенос строк"))
                                    .shortcut("Alt+Z"),
                                MenuItem::new("zoom_in", "Крупнее").shortcut("Ctrl++"),
                                MenuItem::new("zoom_out", "Мельче").shortcut("Ctrl+-"),
                            ]
                        })
                        .menu("Справка", || {
                            vec![MenuItem::new("about", "О программе")]
                        })
                        .on_select(move |id| {
                            if id == "wrap" {
                                wrap.update(|w| *w = !*w);
                            }
                            picked.set(id.to_string());
                        })
                        .build(),
                ),
            )
            .child(rtext("adv-result", move || {
                format!(
                    "Выбран пункт: {}   ·   перенос строк: {}",
                    picked.get(),
                    if wrap.get() { "вкл." } else { "выкл." }
                )
            })),
    )
}

// ─── GraphView ──────────────────────────────────────────────────────────────

/// Строка узла цветом темы (`Color::TRANSPARENT` — `color` из MSS).
fn line(s: &str) -> Vec<(String, Color)> {
    vec![(s.to_string(), Color::TRANSPARENT)]
}

fn graph_card() -> impl Widget {
    let t = Color::TRANSPARENT;
    let nodes = vec![
        GraphNode::new(
            1,
            vec![line("fn classify(n: i32) -> &str"), line("  if n < 0")],
        )
        .accent(t),
        GraphNode::new(2, vec![line("  return \"отрицательное\"")]),
        GraphNode::new(3, vec![line("  if n == 0")]),
        GraphNode::new(4, vec![line("  return \"ноль\"")]),
        GraphNode::new(5, vec![line("  let mut i = 2"), line("  while i * i <= n")]),
        GraphNode::new(
            6,
            vec![
                line("    if n % i == 0"),
                line("      return \"составное\""),
            ],
        ),
        GraphNode::new(7, vec![line("    i += 1")]),
        GraphNode::new(8, vec![line("  return \"простое\"")]),
        GraphNode::new(9, vec![line("}  // выход")]).accent(t),
    ];
    let edges = vec![
        GraphEdge::new(1, 2, t),
        GraphEdge::new(1, 3, t),
        GraphEdge::new(3, 4, t),
        GraphEdge::new(3, 5, t),
        GraphEdge::new(5, 6, t),
        GraphEdge::new(5, 8, t),
        GraphEdge::new(6, 7, t),
        GraphEdge::new(7, 5, t),
        GraphEdge::new(6, 9, t),
        GraphEdge::new(2, 9, t),
        GraphEdge::new(4, 9, t),
        GraphEdge::new(8, 9, t),
    ];
    let selected = use_signal(None::<u64>);
    let opened = use_signal(0u32);
    demo_card(
        "GraphView",
        "Граф потока управления функции: послойная раскладка, стрелки, обратное ребро цикла. \
         Перетаскивание — сдвиг, колесо — прокрутка, Ctrl+колесо — масштаб, щелчок — выбор узла, \
         двойной щелчок — «открыть». Мини-карта появляется, когда граф не влезает.",
        Column::new()
            .gap(10.0)
            .cross_axis_alignment(CrossAxisAlignment::Stretch)
            .child(
                DecoratedBox::new().class("adv-graph-host").child(
                    GraphView::new(nodes, edges)
                        .root(1)
                        .on_select(move |id| selected.set(Some(id)))
                        .on_activate(move |_| opened.update(|n| *n += 1))
                        .class("adv-graph"),
                ),
            )
            .child(rtext("adv-result", move || {
                let sel = selected
                    .get()
                    .map(|id| format!("блок #{id}"))
                    .unwrap_or_else(|| "ничего".into());
                format!(
                    "Выбрано: {sel}   ·   открыто двойным щелчком: {} раз",
                    opened.get()
                )
            })),
    )
}

// ─── TickSlider + SegmentedProgressBar ──────────────────────────────────────

fn sliders_card() -> impl Widget {
    let volume = use_signal(60.0f32);
    let quality = use_signal(2.0f32);
    let done = use_signal(7.0f32);

    let quality_name = |v: f32| {
        match v.round() as i32 {
            0 => "Низкое",
            1 => "Среднее",
            2 => "Высокое",
            _ => "Ультра",
        }
        .to_string()
    };

    demo_card(
        "TickSlider · SegmentedProgressBar",
        "Ползунок с делениями и подписями, привязка к делениям. Сегментный прогресс — шаги \
         задачи: заполненные, частично, исключённые, отметки событий (пульсирует непросмотренная).",
        Column::new()
            .gap(18.0)
            .cross_axis_alignment(CrossAxisAlignment::Stretch)
            .child(
                Row::new()
                    .gap(32.0)
                    .cross_axis_alignment(CrossAxisAlignment::Start)
                    .child(
                        Column::new()
                            .gap(6.0)
                            .child(label("Громкость, шаг 10, подписи каждые 25"))
                            .child(
                                TickSlider::new().horizontal()
                                    .range(0.0, 100.0)
                                    .step(10.0)
                                    .value(60.0)
                                    .ticks(vec![0.0, 25.0, 50.0, 75.0, 100.0])
                                    .tick_labels(|v| format!("{v:.0}"))
                                    .show_value_label(true)
                                    .value_formatter(|v| format!("{v:.0} %"))
                                    .width(320.0)
                                    .on_change(move |v| volume.set(v))
                                    .class("adv-tick"),
                            )
                            .child(rtext("adv-result", move || format!("Громкость: {:.0} %", volume.get()))),
                    )
                    .child(
                        Column::new()
                            .gap(6.0)
                            .child(label("Качество — 4 положения, привязка к делениям"))
                            .child(
                                TickSlider::new().horizontal()
                                    .range(0.0, 3.0)
                                    .value(2.0)
                                    .tick_count(4)
                                    .snap_to_ticks(true)
                                    .tick_labels(move |v| quality_name(v))
                                    .width(320.0)
                                    .on_change(move |v| quality.set(v))
                                    .class("adv-tick"),
                            )
                            .child(rtext("adv-result", move || format!("Качество: {}", quality_name(quality.get())))),
                    )
                    .child(
                        Column::new()
                            .gap(6.0)
                            .child(label("Вертикальный"))
                            .child(
                                TickSlider::new().horizontal()
                                    .vertical()
                                    .range(0.0, 10.0)
                                    .value(7.0)
                                    .tick_count(6)
                                    .height(120.0)
                                    .class("adv-tick"),
                            ),
                    ),
            )
            .child(
                Column::new()
                    .gap(6.0)
                    .child(label("Прогресс по шагам: ползунок задаёт, сколько из 12 шагов готово"))
                    .child(
                        TickSlider::new().horizontal()
                            .range(0.0, 12.0)
                            .step(0.5)
                            .value(7.0)
                            .tick_count(13)
                            .width(480.0)
                            .on_change(move |v| done.set(v))
                            .class("adv-tick"),
                    )
                    .child(rtext_widget(move || {
                        let d = done.get();
                        let fr: Vec<f32> = (0..12).map(|i| (d - i as f32).clamp(0.0, 1.0)).collect();
                        Box::new(SegmentedProgressBar::from_fractions(&fr).class("adv-segments"))
                    }))
                    .child(rtext("adv-result", move || format!("Готово {:.1} из 12 шагов", done.get()))),
            )
            .child(
                Column::new()
                    .gap(6.0)
                    .child(label("Все состояния сегментов"))
                    .child(
                        SegmentedProgressBar::new(vec![
                            SegmentState::Filled,
                            SegmentState::Filled,
                            SegmentState::Partial(0.5),
                            SegmentState::Empty,
                            SegmentState::Excluded,
                            SegmentState::Alert { pulse: false },
                            SegmentState::Alert { pulse: true },
                            SegmentState::Empty,
                            SegmentState::Disabled,
                            SegmentState::Disabled,
                        ])
                        .class("adv-segments"),
                    )
                    .child(Text::new(
                        "готово · готово · половина · пусто · исключено · событие · новое событие · пусто · недоступно ×2",
                    ).class("adv-caption")),
            ),
    )
}

/// Пересобираемый виджет (обёртка над `super::rx`).
fn rtext_widget(f: impl Fn() -> Box<dyn Widget> + Send + Sync + 'static) -> Reactive {
    super::rx(f)
}

// ─── Dismissible ────────────────────────────────────────────────────────────

const NOTES: &[(&str, &str)] = &[
    ("Сборка прошла", "fast-release за 41 с"),
    ("Новое письмо", "Отчёт за ноябрь"),
    ("Напоминание", "Созвон в 15:00"),
    ("Обновление", "syngui 0.9 готов к установке"),
    ("Батарея", "Осталось 18 %"),
];

fn dismissible_card() -> impl Widget {
    let items = use_signal((0..NOTES.len()).collect::<Vec<usize>>());
    let last = use_signal(String::from("—"));
    demo_card(
        "Dismissible",
        "Смахните уведомление мышью или пальцем вбок: дальше трети ширины или броском — улетает \
         и удаляется из списка, иначе возвращается пружиной.",
        Column::new()
            .gap(10.0)
            .cross_axis_alignment(CrossAxisAlignment::Stretch)
            .child(super::rx(move || {
                let v = items.get();
                let mut col = Column::new()
                    .gap(8.0)
                    .cross_axis_alignment(CrossAxisAlignment::Stretch);
                if v.is_empty() {
                    col = col.child(Text::new("Все уведомления убраны").class("adv-caption"));
                }
                for i in v {
                    let (title, body) = NOTES[i];
                    col = col.child(
                        Dismissible::new(
                            DecoratedBox::new().class("adv-note").child(
                                Column::new()
                                    .gap(2.0)
                                    .child(Text::new(title).class("adv-note-title"))
                                    .child(Text::new(body).class("adv-note-body")),
                            ),
                        )
                        .direction(DismissDirection::Both)
                        .on_dismiss(move |dir| {
                            items.update(|v| v.retain(|&x| x != i));
                            let side = if dir == SwipeDirection::Left {
                                "влево"
                            } else {
                                "вправо"
                            };
                            last.set(format!("«{title}» — {side}"));
                        }),
                    );
                }
                Box::new(col)
            }))
            .child(
                Row::new()
                    .gap(12.0)
                    .cross_axis_alignment(CrossAxisAlignment::Center)
                    .child(
                        Button::new("Вернуть все")
                            .on_click(move || items.set((0..NOTES.len()).collect())),
                    )
                    .child(rtext("adv-result", move || {
                        format!("Убрано: {}", last.get())
                    })),
            ),
    )
}

// ─── VirtualList ────────────────────────────────────────────────────────────

const NAMES: &[&str] = &[
    "Анна",
    "Борис",
    "Вера",
    "Глеб",
    "Дина",
    "Егор",
    "Жанна",
    "Захар",
    "Ирина",
    "Кирилл",
];

fn virtual_list_card() -> impl Widget {
    let rows: Vec<VirtualRow> = (0..10_000u64)
        .map(|i| {
            VirtualRow::new(i, 0, move || {
                let name = NAMES[(i as usize * 7) % NAMES.len()];
                let mut col = Column::new().gap(2.0).child(
                    Row::new()
                        .gap(10.0)
                        .child(Text::new(format!("#{:05}", i + 1)).class("adv-row-num"))
                        .child(Text::new(name).class("adv-row-title")),
                );
                // каждая пятая строка выше — высоты разные
                if i % 5 == 0 {
                    col = col.child(
                        Text::new("Строка с пояснением: высота измеряется на лету")
                            .class("adv-row-sub"),
                    );
                }
                Box::new(DecoratedBox::new().class("adv-row").child(col)) as Box<dyn Widget>
            })
        })
        .collect();
    demo_card(
        "VirtualList",
        "10 000 строк разной высоты: собираются только видимые (и запас вокруг), \
         поэтому прокрутка не зависит от длины списка.",
        DecoratedBox::new().class("adv-vlist-host").child(
            VirtualList::new(rows)
                .gap(4.0)
                .estimated_row_height(36.0)
                .scrollbar_policy(ScrollbarPolicy::Auto),
        ),
    )
}

// ─── OnScreenKeyboard ───────────────────────────────────────────────────────

fn keyboard_card() -> impl Widget {
    let kb = KeyboardState::new("");
    let submitted = use_signal(String::from("—"));
    let osk = on_screen_keyboard(kb, KeyboardLayout::text_ru_en("Готово"))
        .on_submit(move |q| {
            submitted.set(if q.is_empty() {
                "(пусто)".into()
            } else {
                q.to_string()
            })
        })
        .gap(6.0)
        .stretch(44.0);
    let ctl = osk.controller();
    demo_card(
        "OnScreenKeyboard",
        "Экранная клавиатура для пульта, мыши и сенсора: раскладки RU/EN и цифры, Shift, \
         стрелки на клавиатуре двигают фокус, Enter — нажать. KeyboardController даёт те же \
         действия снаружи — кнопки ниже набирают текст через него.",
        Column::new()
            .gap(10.0)
            .cross_axis_alignment(CrossAxisAlignment::Stretch)
            .child(input_field(kb.text, "Поиск фильмов…", false, true))
            .child(DecoratedBox::new().class("adv-osk-host").child(osk.build()))
            .child(
                Row::new()
                    .gap(10.0)
                    .cross_axis_alignment(CrossAxisAlignment::Center)
                    .child({
                        let c = ctl.clone();
                        Button::new("Набрать «syngui»")
                            .class("secondary")
                            .on_click(move || {
                                for ch in "syngui".chars() {
                                    c.type_char(ch);
                                }
                            })
                    })
                    .child({
                        let c = ctl.clone();
                        Button::new("Стереть всё")
                            .class("secondary")
                            .on_click(move || c.apply(&KeyAction::Clear))
                    })
                    .child(rtext("adv-result", move || {
                        format!("Отправлено: {}", submitted.get())
                    })),
            ),
    )
}

// ─── PanZoomViewport + ImageViewport ────────────────────────────────────────

fn sticker(title: &str, body: &str) -> impl Widget {
    DecoratedBox::new().class("adv-sticker").child(
        Column::new()
            .gap(4.0)
            .child(Text::new(title).class("adv-note-title"))
            .child(Text::new(body).class("adv-note-body")),
    )
}

fn viewports_card() -> impl Widget {
    let zoom = use_signal(1.0f32);
    let pan = use_signal(Point::new(24.0, 24.0));

    let cmd = use_signal((0u64, ImageViewCommand::Fit));
    let turns = use_signal(0i32);
    let info = use_signal(ImageViewInfo::default());
    let send = move |c: ImageViewCommand| {
        cmd.update(|(seq, cur)| {
            *seq += 1;
            *cur = c;
        })
    };

    let board = Column::new()
        .gap(24.0)
        .child(
            Row::new()
                .gap(24.0)
                .child(sticker("Идея", "Галерея виджетов"))
                .child(sticker("Сделать", "Раздел «Продвинутые»"))
                .child(sticker("Готово", "Редакторы")),
        )
        .child(
            Row::new()
                .gap(24.0)
                .child(
                    Image::new(PIC_NATURE)
                        .fit(ImageFit::Cover)
                        .class("adv-board-pic"),
                )
                .child(sticker(
                    "Заметка",
                    "Колесо — масштаб,\nлевая кнопка — сдвиг",
                ))
                .child(
                    Image::new(PIC_ABSTRACT)
                        .fit(ImageFit::Cover)
                        .class("adv-board-pic"),
                ),
        );

    demo_card(
        "PanZoomViewport · ImageViewport",
        "Слева — бесконечное полотно: колесо — масштаб к курсору, перетаскивание — сдвиг, точечная \
         сетка масштабируется вместе с содержимым. Справа — просмотр картинки: «100 %» — пиксель \
         в пиксель, двойной щелчок — вписать ↔ 1:1, поворот четвертями.",
        Row::new()
            .gap(16.0)
            .cross_axis_alignment(CrossAxisAlignment::Stretch)
            .child(
                grow(
                    Column::new()
                        .gap(8.0)
                        .cross_axis_alignment(CrossAxisAlignment::Stretch)
                        .child(
                            DecoratedBox::new().class("adv-viewport-host").child(
                                PanZoomViewport::new()
                                    .grid(true)
                                    .grid_step(24.0)
                                    .pan_button(MouseButton::Left)
                                    .zoom(zoom)
                                    .pan(pan)
                                    .zoom_range(0.3, 3.0)
                                    .child(board)
                                    .class("adv-panzoom"),
                            ),
                        )
                        .child(
                            Row::new()
                                .gap(8.0)
                                .cross_axis_alignment(CrossAxisAlignment::Center)
                                .child(Button::new("−").class("secondary").on_click(move || zoom.update(|z| *z = (*z / 1.25).max(0.3))))
                                .child(Button::new("+").class("secondary").on_click(move || zoom.update(|z| *z = (*z * 1.25).min(3.0))))
                                .child(Button::new("Сброс").class("secondary").on_click(move || {
                                    zoom.set(1.0);
                                    pan.set(Point::new(24.0, 24.0));
                                }))
                                .child(rtext("adv-result", move || format!("Масштаб {:.0} %", zoom.get() * 100.0))),
                        ),
                ),
            )
            .child(
                grow(
                    Column::new()
                        .gap(8.0)
                        .cross_axis_alignment(CrossAxisAlignment::Stretch)
                        .child(DecoratedBox::new().class("adv-viewport-host").child(super::rx(move || {
                            let (seq, c) = cmd.get();
                            Box::new(
                                ImageViewport::new(PIC_CITY)
                                    .command(seq, c)
                                    .quarter_turns(turns.get())
                                    .info(info)
                                    .class("adv-imageview"),
                            )
                        })))
                        .child(
                            Row::new()
                                .gap(8.0)
                                .cross_axis_alignment(CrossAxisAlignment::Center)
                                .child(Button::new("−").class("secondary").on_click(move || send(ImageViewCommand::ZoomOut)))
                                .child(Button::new("+").class("secondary").on_click(move || send(ImageViewCommand::ZoomIn)))
                                .child(Button::new("Вписать").class("secondary").on_click(move || send(ImageViewCommand::Fit)))
                                .child(Button::new("100 %").class("secondary").on_click(move || send(ImageViewCommand::Actual)))
                                .child(Button::new("Повернуть").class("secondary").on_click(move || turns.update(|t| *t = (*t + 1) % 4)))
                                .child(rtext("adv-result", move || {
                                    let i = info.get();
                                    if i.ready {
                                        format!("{:.0} %  ·  {}×{}", i.scale * 100.0, i.natural.0, i.natural.1)
                                    } else {
                                        "загрузка…".into()
                                    }
                                })),
                        ),
                ),
            ),
    )
}

// ─── StaticWaveform ─────────────────────────────────────────────────────────

/// Синтетическая «запись» 4 с: три фразы с огибающей и паузами.
fn synth_audio() -> Arc<AudioBuffer> {
    let sr = 8_000u32;
    let n = (sr * 4) as usize;
    let mut pcm = Vec::with_capacity(n);
    let mut seed = 0x2545_F491u32;
    for i in 0..n {
        let t = i as f32 / sr as f32;
        // фразы: 0.2–1.3 с, 1.6–2.5 с, 2.8–3.8 с
        let env = [(0.2f32, 1.3f32), (1.6, 2.5), (2.8, 3.8)]
            .iter()
            .map(|&(a, b)| {
                if t < a || t > b {
                    0.0
                } else {
                    let x = (t - a) / (b - a);
                    (x * std::f32::consts::PI).sin().powf(0.6)
                        * (0.55 + 0.45 * (t * 9.0).sin().abs())
                }
            })
            .fold(0.0f32, f32::max);
        seed ^= seed << 13;
        seed ^= seed >> 17;
        seed ^= seed << 5;
        let noise = (seed as f32 / u32::MAX as f32) * 2.0 - 1.0;
        let tone = (t * 220.0 * std::f32::consts::TAU).sin() * 0.6
            + (t * 330.0 * std::f32::consts::TAU).sin() * 0.3;
        pcm.push((tone * 0.7 + noise * 0.3) * env + noise * 0.01);
    }
    Arc::new(AudioBuffer {
        pcm: pcm.into(),
        sample_rate: sr,
        channels: 1,
    })
}

fn waveform_card() -> impl Widget {
    let buf = synth_audio();
    let progress = use_signal(0.35f32);
    let fmt = |s: f32| format!("0:{:04.1}", s);
    demo_card(
        "StaticWaveform",
        "Волна записи по RMS-корзинам (данные синтетические, 4 с): сыгранная часть — акцентом, \
         щелчок или перетаскивание по волне — перемотка.",
        Column::new()
            .gap(8.0)
            .cross_axis_alignment(CrossAxisAlignment::Stretch)
            .child(super::rx(move || {
                Box::new(
                    StaticWaveform::new()
                        .pcm(buf.clone())
                        .bins(160)
                        .height(72.0)
                        .progress(progress.get())
                        .on_seek(move |p| progress.set(p))
                        .class("adv-wave"),
                )
            }))
            .child(
                Row::new()
                    .gap(12.0)
                    .cross_axis_alignment(CrossAxisAlignment::Center)
                    .child(
                        Button::new("В начало")
                            .class("secondary")
                            .on_click(move || progress.set(0.0)),
                    )
                    .child(rtext("adv-result", move || {
                        format!("{} / {}", fmt(progress.get() * 4.0), fmt(4.0))
                    })),
            ),
    )
}

// ─── AspectRatio + ScaleBox ─────────────────────────────────────────────────

fn ratio_frame(ratio: f32, caption: &str, pic: &'static str) -> impl Widget {
    Column::new()
        .gap(6.0)
        .cross_axis_alignment(CrossAxisAlignment::Center)
        .child(
            AspectRatio::new(ratio)
                .max_height(230.0)
                .child(Image::new(pic).fit(ImageFit::Cover).class("adv-ratio-pic"))
                .class("adv-ratio"),
        )
        .child(Text::new(caption).class("adv-caption"))
}

fn aspect_scale_card() -> impl Widget {
    let scale = use_signal(1.4f32);
    demo_card(
        "AspectRatio · ScaleBox",
        "AspectRatio держит пропорции при любой ширине (сузьте окно). ScaleBox увеличивает \
         ребёнка с учётом раскладки — соседи расступаются, картинка остаётся чёткой.",
        Column::new()
            .gap(16.0)
            .cross_axis_alignment(CrossAxisAlignment::Stretch)
            .child(
                Row::new()
                    .gap(16.0)
                    .cross_axis_alignment(CrossAxisAlignment::Start)
                    .child(DecoratedBox::new().class("grow-3").child(ratio_frame(
                        16.0 / 9.0,
                        "16 : 9 — видео",
                        PIC_CITY,
                    )))
                    .child(DecoratedBox::new().class("grow-2").child(ratio_frame(
                        1.0,
                        "1 : 1 — аватар",
                        PIC_NATURE,
                    )))
                    .child(grow(ratio_frame(
                        9.0 / 16.0,
                        "9 : 16 — экран телефона",
                        PIC_ABSTRACT,
                    ))),
            )
            .child(
                Column::new()
                    .gap(8.0)
                    .child(label("Масштаб средней плитки"))
                    .child(
                        TickSlider::new()
                            .horizontal()
                            .range(0.5, 2.0)
                            .step(0.1)
                            .value(1.4)
                            .ticks(vec![0.5, 1.0, 1.5, 2.0])
                            .tick_labels(|v| format!("×{v:.1}"))
                            .width(320.0)
                            .on_change(move |v| scale.set(v))
                            .class("adv-tick"),
                    )
                    .child(super::rx(move || {
                        let s = scale.get();
                        Box::new(
                            Row::new()
                                .gap(12.0)
                                .cross_axis_alignment(CrossAxisAlignment::Center)
                                .child(
                                    DecoratedBox::new()
                                        .class("adv-tile")
                                        .child(Text::new("A").class("adv-tile-text")),
                                )
                                .child(
                                    ScaleBox::new(
                                        DecoratedBox::new()
                                            .class("adv-tile adv-tile-accent")
                                            .child(Text::new("B").class("adv-tile-text-on")),
                                    )
                                    .scale(s)
                                    .smooth(14.0),
                                )
                                .child(
                                    DecoratedBox::new()
                                        .class("adv-tile")
                                        .child(Text::new("C").class("adv-tile-text")),
                                )
                                .child(Text::new(format!("×{s:.1}")).class("adv-result")),
                        )
                    })),
            ),
    )
}

// ─── DockArea ───────────────────────────────────────────────────────────────

fn dock_card() -> impl Widget {
    let state = DockState::new(DockSpec::row(
        DockSpec::tabs(["files"]),
        DockSpec::column(
            DockSpec::tabs(["editor", "preview"]),
            DockSpec::tabs(["console", "problems"]),
            0.62,
        ),
        0.26,
    ));
    let closed = use_signal(Vec::<String>::new());
    demo_card(
        "DockArea",
        "Док-панели как в IDE: перетащите вкладку в другую группу (в центр — добавить, к краю — \
         разрезать группу), к краю всей области — во всю высоту. Вкладки закрываются крестиком.",
        Column::new()
            .gap(10.0)
            .cross_axis_alignment(CrossAxisAlignment::Stretch)
            .child(
                DecoratedBox::new().class("adv-dock-host").child(
                    DockArea::new(state)
                        .title(|p| {
                            match p {
                                "files" => "Файлы",
                                "editor" => "main.rs",
                                "preview" => "Просмотр",
                                "console" => "Консоль",
                                "problems" => "Проблемы",
                                other => other,
                            }
                            .to_string()
                        })
                        .closable(|p| p != "files")
                        .on_close(move |p| closed.update(|v| v.push(p.to_string())))
                        .content(|p| -> Box<dyn Widget> {
                            let lines: &[&str] = match p {
                                "files" => &[
                                    "▾ src",
                                    "    main.rs",
                                    "    lib.rs",
                                    "▸ examples",
                                    "  Cargo.toml",
                                ],
                                "editor" => &[
                                    "fn main() {",
                                    "    println!(\"Привет из док-панели\");",
                                    "}",
                                ],
                                "preview" => &["Привет из док-панели"],
                                "console" => &[
                                    "$ cargo run",
                                    "   Compiling demo v0.1.0",
                                    "    Finished in 0.84s",
                                    "Привет из док-панели",
                                ],
                                "problems" => &["Нет проблем ✓"],
                                _ => &[""],
                            };
                            let mut col = Column::new().gap(4.0);
                            for l in lines {
                                col = col.child(Text::new(*l).class("adv-dock-line"));
                            }
                            Box::new(DecoratedBox::new().class("adv-dock-body").child(col))
                        })
                        .empty(|| Box::new(Text::new("Все панели закрыты").class("adv-caption")))
                        .build(),
                ),
            )
            .child(
                Row::new()
                    .gap(12.0)
                    .cross_axis_alignment(CrossAxisAlignment::Center)
                    .child(
                        Button::new("Вернуть раскладку")
                            .class("secondary")
                            .on_click(move || {
                                state.set_spec(DockSpec::row(
                                    DockSpec::tabs(["files"]),
                                    DockSpec::column(
                                        DockSpec::tabs(["editor", "preview"]),
                                        DockSpec::tabs(["console", "problems"]),
                                        0.62,
                                    ),
                                    0.26,
                                ));
                                closed.set(Vec::new());
                            }),
                    )
                    .child(rtext("adv-result", move || {
                        let c = closed.get();
                        if c.is_empty() {
                            "Закрытых вкладок нет".into()
                        } else {
                            format!("Закрыто: {}", c.join(", "))
                        }
                    })),
            ),
    )
}
