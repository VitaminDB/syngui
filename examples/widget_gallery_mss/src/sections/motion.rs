//! Перетекания: анимации в духе Material 3 Expressive, где интерфейс не
//! подменяет состояния скачком, а переливается между ними — панель
//! подстраивается под вкладку, карточки вырастают из края и уходят обратно,
//! соседи сдвигаются, тема перекрашивается плавно.

use syngui::animation::Easing;
use syngui::buttons::Segment;
use syngui::containers::Keyed;
use syngui::prelude::*;

use super::{label, section_card, section_title};

mod mi {
    pub const DASHBOARD: &str = "\u{E871}";
    pub const MUSIC: &str = "\u{E405}";
    pub const SPEED: &str = "\u{E9E4}";
    pub const CALENDAR: &str = "\u{EBCC}";
    pub const SUNNY: &str = "\u{E430}";
    pub const MEMORY: &str = "\u{E322}";
    pub const THERMO: &str = "\u{E1FF}";
    pub const PLAY: &str = "\u{E037}";
    pub const NEXT: &str = "\u{E044}";
    pub const PREV: &str = "\u{E045}";
    pub const BELL: &str = "\u{E7F4}";
    pub const CLOSE: &str = "\u{E5CD}";
    pub const APPS: &str = "\u{E5C3}";
    pub const SEARCH: &str = "\u{E8B6}";
    pub const SETTINGS: &str = "\u{E8B8}";
    pub const POWER: &str = "\u{E8AC}";
}

pub fn build_motion_section() -> impl Widget {
    section_card(
        Column::new()
            .gap(28.0)
            .child(section_title("Перетекания"))
            .child(intro())
            .child(build_tabs_demo())
            .child(build_popup_demo())
            .child(build_notifications_demo())
            .child(build_size_demo())
            .child(build_easing_demo())
            .child(build_theme_demo()),
    )
}

fn intro() -> impl Widget {
    Column::new().gap(4.0).child(label(
        "Состояния не подменяются скачком: содержимое переливается, размер \
         догоняет новое содержимое на пружине, карточки вырастают из края и \
         уходят обратно, соседи съезжаются. Кривые — emphasized и spring из MSS.",
    ))
}

fn demo(title: &str, hint: &str, body: impl Widget + 'static) -> impl Widget {
    Column::new()
        .gap(10.0)
        .child(Text::new(title).class("demo-title"))
        .child(label(hint))
        .child(body)
}

fn code(s: &str) -> impl Widget {
    DecoratedBox::new().child(Text::new(s).class("demo-code-text")).class("demo-code")
}

// ── Вкладки: панель подстраивается под содержимое ────────────────────────────

fn build_tabs_demo() -> impl Widget {
    let tab = use_signal(0usize);
    let panel = super::rx(move || {
        let key = tab.get();
        Box::new(
            AnimatedSwitcher::new(key as u64, move || match key {
                0 => Box::new(dashboard_tab()),
                1 => Box::new(media_tab()),
                _ => Box::new(performance_tab()),
            })
            .slide(28.0, 0.0)
            .duration_ms(320)
            .exit_duration_ms(180)
            .size_spring(Some((380.0, 36.0)))
            .class("motion-panel-body"),
        )
    });
    demo(
        "Панель с вкладками",
        "AnimatedSwitcher: старая вкладка уезжает и растворяется, новая въезжает, а карточка перетекает к её размеру на пружине. Направление — по порядку вкладок.",
        Column::new()
            .gap(12.0)
            .cross_axis_alignment(CrossAxisAlignment::Start)
            .child(
                SegmentedButton::new(vec![
                    Segment::with_icon("Обзор", mi::DASHBOARD),
                    Segment::with_icon("Медиа", mi::MUSIC),
                    Segment::with_icon("Нагрузка", mi::SPEED),
                ])
                .selected(0)
                .on_change(move |i| tab.set(i)),
            )
            .child(DecoratedBox::new().child(panel).class("motion-panel"))
            .child(code(
                "AnimatedSwitcher::new(tab as u64, move || content(tab))\n    .slide(28.0, 0.0).size_spring(Some((380.0, 36.0)))",
            )),
    )
}

fn stat(icon: &str, value: &str, caption: &str) -> impl Widget {
    Row::new()
        .gap(10.0)
        .cross_axis_alignment(CrossAxisAlignment::Center)
        .child(Icon::new(icon).class("motion-stat-icon"))
        .child(
            Column::new()
                .gap(0.0)
                .child(Text::new(value).class("motion-stat-value"))
                .child(Text::new(caption).class("label")),
        )
}

fn dashboard_tab() -> impl Widget {
    let mut grid = Column::new().gap(4.0);
    let mut head = Row::new().gap(4.0);
    for d in ["Пн", "Вт", "Ср", "Чт", "Пт", "Сб", "Вс"] {
        head = head.child(DecoratedBox::new().child(Text::new(d).class("motion-cal-head")).class("motion-cal-cell"));
    }
    grid = grid.child(head);
    let mut day = 1;
    for _ in 0..5 {
        let mut row = Row::new().gap(4.0);
        for _ in 0..7 {
            let cls = if day == 21 { "motion-cal-cell motion-cal-today" } else { "motion-cal-cell" };
            let txt = if day <= 31 { day.to_string() } else { String::new() };
            row = row.child(DecoratedBox::new().child(Text::new(txt).class("motion-cal-day")).class(cls));
            day += 1;
        }
        grid = grid.child(row);
    }
    Row::new()
        .gap(20.0)
        .cross_axis_alignment(CrossAxisAlignment::Start)
        .child(
            Column::new()
                .gap(14.0)
                .child(stat(mi::SUNNY, "15 °C", "Ясно"))
                .child(stat(mi::MEMORY, "5,4 ГиБ", "Память"))
                .child(stat(mi::CALENDAR, "21 июня", "Суббота")),
        )
        .child(grid)
}

fn media_tab() -> impl Widget {
    Row::new()
        .gap(18.0)
        .cross_axis_alignment(CrossAxisAlignment::Center)
        .child(DecoratedBox::new().child(Icon::new(mi::MUSIC).class("motion-cover-icon")).class("motion-cover"))
        .child(
            Column::new()
                .gap(6.0)
                .child(Text::new("Bad Apple!! feat. nomico").class("motion-stat-value"))
                .child(label("Alstroemeria Records · 3:37"))
                .child(ProgressBar::new().value(0.42).class("motion-progress"))
                .child(
                    Row::new()
                        .gap(6.0)
                        .child(ToolButton::new(mi::PREV))
                        .child(ToolButton::new(mi::PLAY))
                        .child(ToolButton::new(mi::NEXT)),
                ),
        )
}

fn ring(value: f32, caption: &str, icon: &str) -> impl Widget {
    Column::new()
        .gap(6.0)
        .cross_axis_alignment(CrossAxisAlignment::Center)
        .child(
            Stack::new()
                .child(CircularProgress::new().value(value).size(72.0).stroke_width(6.0).class("motion-ring"))
                .child(
                    Column::new()
                        .main_axis_alignment(MainAxisAlignment::Center)
                        .cross_axis_alignment(CrossAxisAlignment::Center)
                        .child(Icon::new(icon).class("motion-ring-icon"))
                        .style("width", 72.0_f32)
                        .style("height", 72.0_f32),
                ),
        )
        .child(Text::new(format!("{}%", (value * 100.0).round() as i32)).class("motion-stat-value"))
        .child(label(caption))
}

fn performance_tab() -> impl Widget {
    Row::new()
        .gap(28.0)
        .child(ring(0.54, "GPU", mi::THERMO))
        .child(ring(0.41, "CPU", mi::SPEED))
        .child(ring(0.23, "Память", mi::MEMORY))
        .child(ring(0.67, "Диск", mi::DASHBOARD))
}

// ── Карточка из панели ───────────────────────────────────────────────────────

fn build_popup_demo() -> impl Widget {
    let open = use_signal(false);
    let bar = DecoratedBox::new()
        .child(
            Row::new()
                .gap(8.0)
                .cross_axis_alignment(CrossAxisAlignment::Center)
                .child(ToolButton::new(mi::APPS).on_click(move || open.set(!open.get_untracked())).class("motion-bar-btn"))
                .child(Text::new("Нажмите значок — карточка вырастет из панели").class("motion-bar-text").class("grow"))
                .child(ToolButton::new(mi::SEARCH).class("motion-bar-btn"))
                .child(ToolButton::new(mi::SETTINGS).class("motion-bar-btn")),
        )
        .class("motion-bar");
    let card = Presence::signal(open, || {
        Box::new(
            DecoratedBox::new()
                .child(
                    Column::new()
                        .gap(8.0)
                        .child(Text::new("Меню запуска").class("motion-stat-value"))
                        .child(menu_row(mi::APPS, "Все приложения"))
                        .child(menu_row(mi::SEARCH, "Поиск"))
                        .child(menu_row(mi::SETTINGS, "Параметры"))
                        .child(menu_row(mi::POWER, "Завершение работы")),
                )
                .class("motion-popup popup-flow-top")
                .style("width", 260.0_f32),
        )
    })
    .enter(Motion::fade().slide(0.0, -8.0))
    .exit(Motion::fade().slide(0.0, -6.0))
    .collapse(AnimationAxis::Height)
    .origin(TransformOrigin::Custom(0.5, 0.0))
    .duration_ms(280)
    .easing(Easing::EMPHASIZED_DECELERATE)
    .initial(false);
    demo(
        "Карточка из панели",
        "Presence с collapse(Height): карточка раскрывается от края панели и складывается обратно; flow-edge: top в MSS делает её углы у панели вогнутыми — карточка перетекает в панель.",
        Column::new()
            .gap(0.0)
            .cross_axis_alignment(CrossAxisAlignment::Start)
            .child(bar)
            .child(Row::new().child(card).style("padding-left", 16.0_f32))
            .child(
                code(".motion-popup.popup-flow-top { flow-edge: top; flow-radius: 14px; border-top-left-radius: 0; … }\nPresence::signal(open, || card()).collapse(AnimationAxis::Height)")
                    .style("margin-top", 12.0_f32),
            ),
    )
}

fn menu_row(icon: &str, text: &str) -> impl Widget {
    DecoratedBox::new()
        .child(
            Row::new()
                .gap(10.0)
                .cross_axis_alignment(CrossAxisAlignment::Center)
                .child(Icon::new(icon).class("motion-menu-icon"))
                .child(Text::new(text).class("motion-menu-text")),
        )
        .class("motion-menu-row")
}

// ── Уведомления: въезд, уход, сдвиг соседей ──────────────────────────────────

#[derive(Clone)]
struct Note {
    id: u64,
    title: String,
    body: String,
    alive: bool,
}

fn build_notifications_demo() -> impl Widget {
    let notes = use_signal(Vec::<Note>::new());
    let next = use_signal(1u64);
    let titles = ["Снимок экрана сохранён", "Обновление готово", "Новое сообщение", "Сборка завершена", "Батарея 20 %"];
    let bodies = ["~/Pictures/Screenshot.png", "Перезапустите, чтобы применить", "Алексей: «Смотри, что получилось»", "syngui: 0 ошибок, 9 предупреждений", "Подключите зарядное устройство"];
    let add = move || {
        let n = next.get_untracked();
        next.set(n + 1);
        let i = (n as usize - 1) % titles.len();
        notes.update(|v| v.insert(0, Note { id: n, title: titles[i].into(), body: bodies[i].into(), alive: true }));
    };
    let remove_oldest = move || {
        notes.update(|v| {
            if let Some(last) = v.iter_mut().rev().find(|n| n.alive) {
                last.alive = false;
            }
        });
    };
    let shuffle = move || {
        notes.update(|v| {
            if v.len() > 1 {
                let first = v.remove(0);
                v.push(first);
            }
        });
    };
    let list = super::rx(move || {
        let v = notes.get();
        let mut col = Column::new().gap(8.0);
        if v.is_empty() {
            col = col.child(label("Пусто. Нажмите «Показать»."));
        }
        for n in v {
            let id = n.id;
            let version = (n.alive as u64) | (id << 1);
            col = col.child(Keyed::new(id, version, move || {
                let card = DecoratedBox::new()
                    .child(
                        Row::new()
                            .gap(10.0)
                            .cross_axis_alignment(CrossAxisAlignment::Start)
                            .child(Icon::new(mi::BELL).class("motion-note-icon"))
                            .child(
                                Column::new()
                                    .gap(2.0)
                                    .child(Text::new(n.title.clone()).class("motion-stat-value"))
                                    .child(Text::new(n.body.clone()).class("label"))
                                    .class("grow"),
                            )
                            .child(
                                ToolButton::new(mi::CLOSE)
                                    .on_click(move || notes.update(|v| v.iter_mut().filter(|x| x.id == id).for_each(|x| x.alive = false)))
                                    .class("motion-note-close"),
                            ),
                    )
                    .class("motion-note")
                    .style("width", 340.0_f32);
                let presence = Presence::new(n.alive, card)
                    .enter(Motion::fade().slide(48.0, 0.0).scale(0.96))
                    .exit(Motion::fade().slide(48.0, 0.0))
                    .collapse(AnimationAxis::Height)
                    .origin(TransformOrigin::Custom(1.0, 0.5))
                    .duration_ms(320)
                    .on_exit_complete(move || notes.update(|v| v.retain(|x| x.id != id)));
                Box::new(AnimatedPosition::new(presence))
            }));
        }
        Box::new(col)
    });
    demo(
        "Стопка уведомлений",
        "Presence въезжает справа и уходит со схлопыванием высоты, AnimatedPosition везёт соседей на новое место на пружине, Keyed сохраняет карточки при перестановке.",
        Column::new()
            .gap(12.0)
            .cross_axis_alignment(CrossAxisAlignment::Start)
            .child(
                Row::new()
                    .gap(8.0)
                    .child(Button::new("Показать").class("primary").on_click(add))
                    .child(Button::new("Убрать старое").on_click(remove_oldest))
                    .child(Button::new("Переставить").on_click(shuffle)),
            )
            .child(DecoratedBox::new().child(list).class("motion-notes-area"))
            .child(code(
                "Keyed::new(id, version, move || Box::new(AnimatedPosition::new(\n    Presence::new(alive, card).enter(Motion::fade().slide(48.0, 0.0)).collapse(AnimationAxis::Height)\n        .on_exit_complete(move || remove(id)))))",
            )),
    )
}

// ── Размер: пружина против tween ─────────────────────────────────────────────

fn build_size_demo() -> impl Widget {
    let wide = use_signal(false);
    let box_of = move |cls: &'static str| {
        super::rx(move || {
            let w = if wide.get() { 320.0 } else { 120.0 };
            let h = if wide.get() { 64.0 } else { 120.0 };
            Box::new(DecoratedBox::new().style("width", w).style("height", h).class(cls))
        })
    };
    demo(
        "Размер: пружина и MSS",
        "AnimatedSize с .spring() подхватывает новую цель без рывка, даже если нажать посреди движения. Правый бокс настроен из темы: transition: size 320ms spring(420, 40).",
        Column::new()
            .gap(12.0)
            .cross_axis_alignment(CrossAxisAlignment::Start)
            .child(Button::new("Переключить размер").class("primary").on_click(move || wide.set(!wide.get_untracked())))
            .child(
                Row::new()
                    .gap(24.0)
                    .cross_axis_alignment(CrossAxisAlignment::Start)
                    .child(
                        Column::new()
                            .gap(6.0)
                            .child(AnimatedSize::new(box_of("motion-box-a")).duration_ms(320).easing(Easing::EMPHASIZED))
                            .child(label("tween · emphasized")),
                    )
                    .child(
                        Column::new()
                            .gap(6.0)
                            .child(AnimatedSize::new(box_of("motion-box-b")).spring(420.0, 40.0))
                            .child(label(".spring(420, 40)")),
                    )
                    .child(
                        Column::new()
                            .gap(6.0)
                            .child(AnimatedSize::new(box_of("motion-box-c")).class("motion-size-mss"))
                            .child(label("MSS: transition: size … spring()")),
                    ),
            ),
    )
}

// ── Кривые ───────────────────────────────────────────────────────────────────

fn build_easing_demo() -> impl Widget {
    let curves: [(&str, Easing); 6] = [
        ("emphasized", Easing::EMPHASIZED),
        ("emphasized-decelerate", Easing::EMPHASIZED_DECELERATE),
        ("emphasized-accelerate", Easing::EMPHASIZED_ACCELERATE),
        ("standard", Easing::STANDARD),
        ("spring (мягкая)", Easing::SPRING_SMOOTH),
        ("spring-bouncy", Easing::SPRING_BOUNCY),
    ];
    let mut row = Flex::new().gap(16.0).wrap();
    for (name, e) in curves {
        row = row.child(curve_card(name, e));
    }
    demo(
        "Кривые Material 3 и пружины",
        "В MSS: transition: opacity 300ms emphasized; transition: size 400ms spring(300, 22). Точка внизу ходит по той же кривой.",
        row,
    )
}

fn curve_card(name: &str, e: Easing) -> impl Widget {
    let canvas = Canvas::new(move |ctx, t| {
        let (w, h) = (160.0, 100.0);
        let pad = 8.0;
        let accent = ctx.mss_accent().unwrap_or(Color::rgb(0.3, 0.5, 0.9));
        let muted = ctx.mss_color().unwrap_or(Color::rgb(0.5, 0.5, 0.5)).with_alpha(0.25);
        ctx.set_color(muted);
        ctx.set_stroke_width(1.0);
        ctx.draw_polyline(&[(pad, h - pad - 16.0), (w - pad, h - pad - 16.0)]);
        ctx.draw_polyline(&[(pad, pad), (w - pad, pad)]);
        let pts: Vec<(f32, f32)> = (0..=48)
            .map(|i| {
                let x = i as f32 / 48.0;
                let y = e.apply(x);
                (pad + x * (w - 2.0 * pad), h - pad - 16.0 - y * (h - 2.0 * pad - 16.0))
            })
            .collect();
        ctx.set_color(accent);
        ctx.set_stroke_width(2.0);
        ctx.draw_polyline(&pts);
        // Точка: цикл 1,6 с — движение туда, пауза, обратно.
        let cycle = (t % 1.6) / 1.6;
        let p = if cycle < 0.45 { e.apply(cycle / 0.45) } else if cycle < 0.55 { 1.0 } else if cycle < 1.0 { 1.0 - e.apply((cycle - 0.55) / 0.45) } else { 0.0 };
        ctx.set_color(accent);
        ctx.fill_circle(pad + p * (w - 2.0 * pad), h - 6.0, 5.0);
    })
    .size(160.0, 100.0)
    .animated(true)
    .class("motion-curve");
    Column::new().gap(4.0).child(canvas).child(label(name))
}

// ── Плавная смена темы ───────────────────────────────────────────────────────

fn build_theme_demo() -> impl Widget {
    demo(
        "Плавная смена темы",
        "App::with_theme_transition(ms): при замене таблицы стилей цвета всех элементов перетекают, а не меняются скачком. Переключатель — в шапке галереи, здесь — та же кнопка.",
        Row::new()
            .gap(8.0)
            .child(Button::new("Светлая ↔ тёмная").class("primary").on_click(|| {
                let ctx = use_context::<crate::GalleryCtx>();
                ctx.toggle_dark();
            }))
            .child(code("App::new().with_dynamic_theme(theme_mss).with_theme_transition(360)")),
    )
}
