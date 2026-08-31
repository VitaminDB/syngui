//! Содержимое прокручиваемой области не должно исчезать.
//!
//! Пропадало так: колесо разгоняло инерцию, у края включалась «резинка»
//! (bounce), и если очередной кадр просаживался (подгрузка данных, тяжёлая
//! перерисовка), пружина отскока раскачивалась вместо затухания. Смещение
//! прокрутки уезжало на тысячи пикселей, содержимое уходило далеко за
//! пределы области — и отсечение выбрасывало его из кадра целиком.

use std::time::Duration;

use syngui::core::{Point, Rect, Size};
use syngui::input::Event;
use syngui::prelude::*;
use syngui::render::{DisplayList, DrawCommand};
use syngui::testing::TestHarness;
use syngui::widgets::containers::Page;

const MSS: &str = r#"
.grow { flex-grow: 1; }
.sheet { width: 794px; background-color: #ffffff; }
"#;

const VIEW_W: f32 = 1200.0;
const VIEW_H: f32 = 800.0;

/// Лист «квитанции» в прокручиваемой странице — как на экране показаний.
fn build(pages: usize, rows: usize) -> TestHarness {
    let mut col = Column::new()
        .gap(24.0)
        .cross_axis_alignment(CrossAxisAlignment::Stretch);
    for p in 0..pages {
        let mut sheet = Column::new().cross_axis_alignment(CrossAxisAlignment::Stretch);
        for r in 0..rows {
            sheet = sheet.child(Text::new(format!("строка {p}.{r}")));
        }
        col = col.child(
            Row::new()
                .cross_axis_alignment(CrossAxisAlignment::Start)
                .child(DecoratedBox::new().class("grow"))
                .child(DecoratedBox::new().class("sheet").child(sheet))
                .child(DecoratedBox::new().class("grow")),
        );
    }

    // Page намеренно не корень дерева: отсечение содержимого считается
    // только для вложенных узлов, а на экране показаний страница всегда
    // лежит внутри колонки с тулбаром.
    let root = Column::new()
        .expand()
        .child(DecoratedBox::new().class("grow").child(
            Page::new().child(Padding::all(20.0).child(col)),
        ));
    let mut h = TestHarness::new(Box::new(root));
    let engine = h.apply_mss(MSS);
    h.apply_styles(&engine);
    h.layout(VIEW_W, VIEW_H);
    h
}

/// Кадр приложения: анимации → layout → display list. Возвращает число
/// текстовых команд и вертикальный сдвиг содержимого.
fn frame(h: &mut TestHarness, dt: Duration) -> (usize, f32) {
    let root = h.root_id;
    h.tree.animate(root, dt);
    h.layout(VIEW_W, VIEW_H);

    let mut dl = DisplayList::new();
    dl.set_scale_factor(1.0);
    dl.set_surface_size(Size::new(VIEW_W, VIEW_H));
    h.tree
        .build_display_list(root, &mut dl, Rect::new(Point::zero(), Size::new(VIEW_W, VIEW_H)));

    let cmds = dl.commands();
    let texts = cmds
        .iter()
        .filter(|c| matches!(c, DrawCommand::Text { .. }))
        .count();
    let ty = cmds
        .iter()
        .find_map(|c| match c {
            DrawCommand::PushTransform(t) => Some(t.m32),
            _ => None,
        })
        .unwrap_or(0.0);
    (texts, ty)
}

fn wheel(h: &mut TestHarness, delta: f32) {
    h.send_event(&Event::MouseWheel {
        delta,
        delta_x: 0.0,
        position: Point::new(VIEW_W / 2.0, VIEW_H / 2.0),
    });
}

/// Долгая прокрутка в обе стороны на ровных кадрах.
#[test]
fn content_survives_long_scrolling() {
    let mut h = build(4, 40);
    let (at_top, _) = frame(&mut h, Duration::from_millis(16));
    assert!(at_top > 0, "на нулевом смещении текст должен рисоваться");

    let mut empty_steps = Vec::new();
    for step in 0..400 {
        wheel(&mut h, if step % 80 < 40 { -50.0 } else { 50.0 });
        let (n, _) = frame(&mut h, Duration::from_millis(16));
        if n == 0 {
            empty_steps.push(step);
        }
    }
    assert!(empty_steps.is_empty(), "контент пропал на шагах {empty_steps:?}");
}

/// Разгон к краю плюс просевший кадр — тот самый случай.
///
/// Быстрая прокрутка вверх из середины набирает большую скорость инерции,
/// на краю она уходит в «резинку», а следующий кадр приходит через четверть
/// секунды. Пружина отскока обязана погасить это, а не разогнать: смещение
/// прокрутки должно остаться в пределах области, содержимое — в кадре.
#[test]
fn content_survives_stalled_frames_at_the_edge() {
    let mut h = build(4, 40);
    frame(&mut h, Duration::from_millis(16));

    // Уезжаем в середину документа.
    for _ in 0..10 {
        wheel(&mut h, -120.0);
        frame(&mut h, Duration::from_millis(16));
    }

    // Резко вверх — колесо крутится пачкой, инерция копится.
    for _ in 0..8 {
        wheel(&mut h, 120.0);
    }

    let mut worst_shift = 0.0f32;
    let mut empty_steps = Vec::new();
    for step in 0..60 {
        // Кадр просел: 250 мс вместо 16.
        let dt = if step < 4 {
            Duration::from_millis(250)
        } else {
            Duration::from_millis(16)
        };
        let (n, ty) = frame(&mut h, dt);
        worst_shift = worst_shift.max(ty.abs());
        if n == 0 {
            empty_steps.push(step);
        }
    }

    assert!(
        worst_shift < 400.0,
        "содержимое уехало на {worst_shift:.1} px — «резинка» раскачалась"
    );
    assert!(
        empty_steps.is_empty(),
        "содержимое исчезло на шагах {empty_steps:?} (сдвиг до {worst_shift:.1} px)"
    );
}

/// Разворот прокрутки должен срабатывать с первого оборота.
///
/// Ломалось так: у верхнего края включалась «резинка», и пока она
/// доигрывала, каждый кадр анимации переписывал смещение своей траекторией.
/// Обороты колеса вниз в это время пропадали — прокрутка «схватывалась»
/// только с третьего раза. Вторая половина той же болезни — инерция:
/// встречный импульс усреднялся со старой скоростью и не менял знак.
#[test]
fn reverse_after_hitting_the_top_works_on_first_notch() {
    let mut h = build(4, 40);
    frame(&mut h, Duration::from_millis(16));

    // Вниз, затем вверх до упора — на краю запускается «резинка».
    for _ in 0..6 {
        wheel(&mut h, -120.0);
        frame(&mut h, Duration::from_millis(16));
    }
    for _ in 0..10 {
        wheel(&mut h, 120.0);
        frame(&mut h, Duration::from_millis(16));
    }

    let (_, ty_before) = frame(&mut h, Duration::from_millis(16));

    // Один оборот вниз обязан сдвинуть содержимое сразу же.
    wheel(&mut h, -120.0);
    let (_, ty_after) = frame(&mut h, Duration::from_millis(16));

    let shift = ty_before - ty_after;
    assert!(
        shift > 50.0,
        "первый оборот вниз почти не сдвинул содержимое: было {ty_before:.1}, стало {ty_after:.1}"
    );
}

/// У верхнего края колесо должно упираться намертво, без дрожания.
///
/// Раньше инерция последнего оборота уводила содержимое за край, «резинка»
/// возвращала его обратно — и каждая попытка крутить вверх на упоре качала
/// лист на несколько пикселей туда-сюда.
#[test]
fn wheel_at_the_top_edge_does_not_jitter() {
    let mut h = build(4, 40);
    frame(&mut h, Duration::from_millis(16));

    // Уходим вниз и возвращаемся вверх до упора.
    for _ in 0..6 {
        wheel(&mut h, -120.0);
        frame(&mut h, Duration::from_millis(16));
    }
    for _ in 0..12 {
        wheel(&mut h, 120.0);
        frame(&mut h, Duration::from_millis(16));
    }

    // Дальше крутим вверх, стоя на упоре: содержимое обязано стоять.
    let mut shifts = Vec::new();
    for _ in 0..30 {
        wheel(&mut h, 120.0);
        let (_, ty) = frame(&mut h, Duration::from_millis(16));
        shifts.push(ty);
    }

    let worst = shifts.iter().fold(0.0f32, |acc, t| acc.max(t.abs()));
    assert!(
        worst < 0.5,
        "лист дрожит у верхнего края: смещения {:?}",
        &shifts[..shifts.len().min(8)]
    );
}
