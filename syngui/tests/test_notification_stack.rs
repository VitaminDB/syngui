//! Геометрия стека уведомлений: якорь снизу-справа и направление колоды.
//!
//! Регрессия: хост писался под верхний якорь — колода переполнения
//! выглядывала ПОД нижней карточкой. При `PortalAnchor::BottomEnd` это
//! уводило её за нижнюю кромку окна; `grow_up(true)` разворачивает колоду
//! вверх, за верхнюю карточку.

use std::time::Duration;

use syngui::core::{Point, Rect, Size};
use syngui::prelude::*;
use syngui::render::DisplayList;
use syngui::testing::*;
use syngui::widgets::containers::Stack;
use syngui::widgets::feedback::{NotificationCtx, NotificationHost, NotificationItem};
use syngui::widgets::overlay::portal::{Portal, PortalAnchor};

const MARGIN_BOTTOM: f32 = 72.0;
const MARGIN_RIGHT: f32 = 16.0;
/// Минимальная ширина «настоящей» карточки: отсекает 3px-акцентную полосу
/// и прочую мелочь из display-list'а.
const CARD_MIN_W: f32 = 80.0;

/// Кладёт `count` уведомлений в хост снизу-справа и возвращает нарисованные
/// прямоугольники карточек сверху вниз.
fn card_rects(count: usize, grow_up: bool, viewport: Size) -> Vec<Rect> {
    let ctx = NotificationCtx::with_default_duration(60_000);
    for i in 0..count {
        ctx.show(NotificationItem::info(format!("Уведомление {i}")));
    }

    let always_open = use_signal(true);
    let widget = Stack::new().clip(false).child(
        Portal::new()
            .is_open(always_open)
            .modal(false)
            .backdrop(false)
            .anchor(PortalAnchor::BottomEnd {
                margin_bottom: MARGIN_BOTTOM,
                margin_right: MARGIN_RIGHT,
            })
            .child(NotificationHost::new(ctx.clone()).grow_up(grow_up)),
    );

    let mut harness = TestHarness::new(Box::new(widget));
    harness.layout(viewport.width, viewport.height);
    // Несколько тиков: первый забирает items в active, остальные докручивают
    // fade-in (карточки с opacity <= 0.01 не рисуются вовсе).
    for _ in 0..20 {
        harness.tree.animate(harness.root_id, Duration::from_millis(16));
        harness.layout(viewport.width, viewport.height);
    }

    let mut list = DisplayList::new();
    list.set_surface_size(viewport);
    harness.tree.build_display_list(
        harness.root_id,
        &mut list,
        Rect::new(Point::zero(), viewport),
    );

    let mut rects: Vec<Rect> = list
        .commands()
        .iter()
        .filter_map(|cmd| match cmd {
            syngui::render::DrawCommand::Rect { rect, .. } if rect.size.width >= CARD_MIN_W => {
                Some(*rect)
            }
            _ => None,
        })
        .collect();
    rects.sort_by(|a, b| a.origin.y.total_cmp(&b.origin.y));
    rects
}

fn viewport() -> Size {
    Size::new(1200.0, 800.0)
}

/// Одиночный тост стоит в правом нижнем углу с заданными отступами.
#[test]
fn single_toast_sits_in_bottom_right_corner() {
    let vp = viewport();
    let rects = card_rects(1, true, vp);
    assert_eq!(rects.len(), 1, "ожидалась одна карточка, получено {rects:?}");
    let r = rects[0];
    assert!(
        (r.origin.y + r.size.height - (vp.height - MARGIN_BOTTOM)).abs() < 1.0,
        "нижняя кромка должна отстоять от низа на {MARGIN_BOTTOM}, получилось {r:?}"
    );
    assert!(
        (r.origin.x + r.size.width - (vp.width - MARGIN_RIGHT)).abs() < 1.0,
        "правая кромка должна отстоять от правого края на {MARGIN_RIGHT}, получилось {r:?}"
    );
}

/// Одновременно видно не больше трёх карточек, они не перекрываются и
/// растут вверх от нижнего края.
#[test]
fn at_most_three_cards_stack_upwards() {
    let vp = viewport();
    let rects = card_rects(3, true, vp);
    assert_eq!(rects.len(), 3, "три уведомления — три карточки: {rects:?}");
    for pair in rects.windows(2) {
        let (upper, lower) = (pair[0], pair[1]);
        assert!(
            upper.origin.y + upper.size.height <= lower.origin.y + 0.5,
            "карточки не должны перекрываться: {upper:?} и {lower:?}"
        );
    }
    let last = rects[2];
    assert!(
        (last.origin.y + last.size.height - (vp.height - MARGIN_BOTTOM)).abs() < 1.0,
        "нижняя карточка прижата к нижнему отступу, получилось {last:?}"
    );

    // Пятое и шестое уведомления уходят в колоду — полноразмерных карточек
    // по-прежнему три (колода уже и рисуется со scale).
    let many = card_rects(6, true, vp);
    let full_width: Vec<Rect> = many
        .iter()
        .copied()
        .filter(|r| (r.size.width - last.size.width).abs() < 0.5)
        .collect();
    assert_eq!(full_width.len(), 3, "видимых карточек должно остаться три: {many:?}");
}

/// Колода переполнения выглядывает НАД верхней карточкой и не вылезает за
/// пределы окна.
#[test]
fn overflow_deck_peeks_above_and_stays_in_window() {
    let vp = viewport();
    let rects = card_rects(5, true, vp);
    assert!(rects.len() > 3, "ожидалась колода за тремя карточками: {rects:?}");

    let widest = rects
        .iter()
        .map(|r| r.size.width)
        .fold(0.0_f32, f32::max);
    let top_visible = rects
        .iter()
        .filter(|r| (r.size.width - widest).abs() < 0.5)
        .map(|r| r.origin.y)
        .fold(f32::MAX, f32::min);
    let deck_top = rects
        .iter()
        .filter(|r| r.size.width < widest - 0.5)
        .map(|r| r.origin.y)
        .fold(f32::MAX, f32::min);

    assert!(
        deck_top < top_visible - 0.5,
        "колода должна выглядывать выше верхней карточки: колода {deck_top}, верх {top_visible}"
    );
    for r in &rects {
        assert!(
            r.origin.y >= -0.5 && r.origin.y + r.size.height <= vp.height + 0.5,
            "карточка {r:?} вышла за окно высотой {}",
            vp.height
        );
    }
}

/// Режим по умолчанию (верхний якорь) не изменился: колода уходит ВНИЗ.
#[test]
fn default_mode_keeps_deck_below() {
    let vp = viewport();
    let rects = card_rects(5, false, vp);
    let widest = rects.iter().map(|r| r.size.width).fold(0.0_f32, f32::max);
    let bottom_visible = rects
        .iter()
        .filter(|r| (r.size.width - widest).abs() < 0.5)
        .map(|r| r.origin.y + r.size.height)
        .fold(f32::MIN, f32::max);
    let deck_bottom = rects
        .iter()
        .filter(|r| r.size.width < widest - 0.5)
        .map(|r| r.origin.y + r.size.height)
        .fold(f32::MIN, f32::max);
    assert!(
        deck_bottom > bottom_visible - 0.5,
        "в обычном режиме колода остаётся под нижней карточкой: колода {deck_bottom}, низ {bottom_visible}"
    );
}
