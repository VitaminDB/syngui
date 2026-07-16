//! Pixel-perfect per-side border тесты.
//!
//! Несколько 5×5 grid'ов 80×80-ячеек без gap. Цель — визуально проверить
//! поведение `border-{left|top|right|bottom}-width: 1px` на стыках:
//!
//! - **Без border**: между ячейками не должно быть никаких линий.
//! - **Только одна сторона**: тонкая 1 px линия в нужном направлении,
//!   ровная толщина по всей длине, без halo.
//! - **Соседствующие стороны** (например, top на одной ячейке + bottom
//!   на соседней): на стыке должна быть **одна** 1 px линия, а не две.
//! - **Все 4 стороны**: ровная сетка 1 px, толщина одинаковая по всем
//!   сторонам каждой ячейки.

use syngui::prelude::*;

use super::{section_card, section_title};

/// Размер одной ячейки в logical-пикселях.
const CELL: f32 = 80.0;
/// Цвет фона ячейки — однотонный пастельный.
const CELL_BG: &str = "#E0E7FF";
/// Цвет бордера — высоко-контрастный к фону, чтобы артефакты были видны.
const BORDER_COLOR: &str = "#1E293B";

/// Палитра 25 пастельных цветов — для grid'а без border'а, чтобы паразитная
/// линия на стыке (если есть) была видна на однотонном фоне без шумового
/// перепада соседних ячеек.
const PALETTE: [&str; 25] = [
    "#FCA5A5", "#FCD34D", "#86EFAC", "#7DD3FC", "#C4B5FD",
    "#FDA4AF", "#FBBF24", "#34D399", "#38BDF8", "#A78BFA",
    "#F87171", "#F59E0B", "#10B981", "#0EA5E9", "#8B5CF6",
    "#EF4444", "#D97706", "#059669", "#0284C7", "#7C3AED",
    "#DC2626", "#B45309", "#047857", "#0369A1", "#6D28D9",
];

/// Описание per-side widths (logical px) для одной ячейки.
#[derive(Clone, Copy)]
struct Sides {
    left: f32,
    top: f32,
    right: f32,
    bottom: f32,
}

impl Sides {
    const fn none() -> Self { Self { left: 0.0, top: 0.0, right: 0.0, bottom: 0.0 } }
    const fn only_top() -> Self { Self { left: 0.0, top: 1.0, right: 0.0, bottom: 0.0 } }
    const fn only_bottom() -> Self { Self { left: 0.0, top: 0.0, right: 0.0, bottom: 1.0 } }
    const fn only_left() -> Self { Self { left: 1.0, top: 0.0, right: 0.0, bottom: 0.0 } }
    const fn only_right() -> Self { Self { left: 0.0, top: 0.0, right: 1.0, bottom: 0.0 } }
    const fn top_left() -> Self { Self { left: 1.0, top: 1.0, right: 0.0, bottom: 0.0 } }
    const fn bottom_right() -> Self { Self { left: 0.0, top: 0.0, right: 1.0, bottom: 1.0 } }
    const fn all() -> Self { Self { left: 1.0, top: 1.0, right: 1.0, bottom: 1.0 } }
}

fn cell(sides: Sides) -> impl Widget {
    let mut b = DecoratedBox::new()
        .style("background-color", Color::from_hex(CELL_BG))
        .style("width", CELL)
        .style("height", CELL)
        .style("border-color", Color::from_hex(BORDER_COLOR));
    if sides.left > 0.0 { b = b.style("border-left-width", sides.left); }
    if sides.top > 0.0 { b = b.style("border-top-width", sides.top); }
    if sides.right > 0.0 { b = b.style("border-right-width", sides.right); }
    if sides.bottom > 0.0 { b = b.style("border-bottom-width", sides.bottom); }
    b
}

fn grid_5x5(sides: Sides) -> impl Widget {
    Column::new()
        .gap(0.0)
        .cross_axis_alignment(CrossAxisAlignment::Start)
        .children((0..5).map(|_row| {
            Box::new(
                Row::new().gap(0.0).children((0..5).map(|_col| {
                    Box::new(cell(sides)) as Box<dyn Widget>
                })),
            ) as Box<dyn Widget>
        }))
}

fn labelled_grid(label: &str, sides: Sides) -> impl Widget {
    Column::new()
        .gap(8.0)
        .child(Text::new(label).class("label"))
        .child(grid_5x5(sides))
}

/// 5×5 grid пастельных цветов без border'а — артефакт на стыках виден
/// как тонкая линия более тёмного/светлого оттенка.
fn colored_grid_no_border() -> impl Widget {
    Column::new()
        .gap(0.0)
        .cross_axis_alignment(CrossAxisAlignment::Start)
        .children((0..5).map(|row| {
            Box::new(
                Row::new().gap(0.0).children((0..5).map(move |col| {
                    Box::new(
                        DecoratedBox::new()
                            .style("background-color", Color::from_hex(PALETTE[row * 5 + col]))
                            .style("width", CELL)
                            .style("height", CELL),
                    ) as Box<dyn Widget>
                })),
            ) as Box<dyn Widget>
        }))
}

pub fn build_border_test_section() -> impl Widget {
    section_card(
        Column::new()
            .gap(24.0)
            .child(section_title("Pixel-perfect grid (5×5, gap=0)"))
            .child(Text::new(
                "Каждая ячейка 80×80 px. На стыках должна быть ровная 1 px линия \
                 (или ничего, для случая «no border»). Двойная толщина или halo \
                 на стыках = паразитный артефакт.",
            ).class("label"))
            .child(
                Column::new()
                    .gap(8.0)
                    .child(Text::new("Цветной без border (контроль на стыках)").class("label"))
                    .child(colored_grid_no_border()),
            )
            .child(labelled_grid("Однотонный без border", Sides::none()))
            .child(labelled_grid("Только top", Sides::only_top()))
            .child(labelled_grid("Только bottom", Sides::only_bottom()))
            .child(labelled_grid("Только left", Sides::only_left()))
            .child(labelled_grid("Только right", Sides::only_right()))
            .child(labelled_grid("top + left", Sides::top_left()))
            .child(labelled_grid("bottom + right", Sides::bottom_right()))
            .child(labelled_grid("Все 4 стороны", Sides::all())),
    )
}
