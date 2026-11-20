//! Row без явной высоты внутри Reactive в Column ограниченной высоты берёт высоту
//! по содержимому и не отнимает место у соседа с flex-grow (synshell, syn-systeminfo conns).

use syngui::prelude::*;
use syngui::mss::StyleValue;
use syngui::testing::TestHarness;
use syngui::widgets::containers::virtual_list::{VirtualList, VirtualRow};

const MSS: &str = ".grow { flex-grow: 1; } .w92 { width: 92px; } .right { text-align: right; }
.thead { padding: 6px 10px; } .conns-thead { padding: 6px 28px 4px 28px; }
.th { font-size: 11px; font-weight: 600; } .conns-body { padding: 0px 18px 10px 18px; }";

fn build() -> TestHarness {
    build_with(false)
}

fn build_with(via_viewport: bool) -> TestHarness {
    let narrow = syngui::viewport::viewport_below(760.0);
    let head = Reactive::new(move || -> Vec<Box<dyn Widget>> {
        if via_viewport && narrow.get() {
            return vec![];
        }
        let r = Row::new()
            .gap(10.0)
            .child(
                DecoratedBox::new()
                    .style("width", StyleValue::px(8.0))
                    .style("height", StyleValue::px(1.0)),
            )
            .child(Text::new("Протокол").class("th").class("w92"))
            .child(Text::new("Куда").class("th").class("grow"))
            .child(Text::new("Длилось").class("th").class("w92").class("right"))
            .cross_axis_alignment(CrossAxisAlignment::Center)
            .class("thead")
            .class("conns-thead");
        vec![Box::new(r)]
    });
    let rows = Reactive::new(move || -> Vec<Box<dyn Widget>> {
        let rows: Vec<VirtualRow> = (0..50u64)
            .map(|i| VirtualRow::new(i, 0, move || Box::new(Text::new(format!("строка {i}"))) as Box<dyn Widget>))
            .collect();
        vec![Box::new(VirtualList::new(rows).estimated_row_height(46.0).class("grow"))]
    });
    let col = Column::new()
        .gap(0.0)
        .cross_axis_alignment(CrossAxisAlignment::Stretch)
        .child(head)
        .child(DecoratedBox::new().child(rows).class("grow").class("conns-body").class("list"))
        .class("grow");
    let page = Column::new()
        .gap(14.0)
        .cross_axis_alignment(CrossAxisAlignment::Stretch)
        .child(Text::new("Соединения"))
        .child(col);
    let mut h = TestHarness::new(Box::new(page));
    let engine = h.apply_mss(MSS);
    h.apply_styles(&engine);
    if via_viewport {
        // как в окне: первый кадр — до известной ширины, заголовок появляется пересборкой
        for _ in 0..3 {
            h.frame(Some(&engine), 1200.0, 760.0);
        }
    } else {
        h.layout(1200.0, 760.0);
    }
    h
}

#[test]
fn head_row_keeps_natural_height() {
    let h = build();
    let list = h.find_by_class("list")[0];
    let b = h.element_bounds(list);
    assert!(
        b.size.height > 600.0,
        "список под заголовком должен занять почти всю высоту, а получил {}",
        b.size.height
    );
}

#[test]
fn head_row_keeps_natural_height_after_rebuild() {
    let h = build_with(true);
    let list = h.find_by_class("list")[0];
    let b = h.element_bounds(list);
    let head = h.find_by_class("conns-thead");
    assert!(!head.is_empty(), "заголовок должен появиться после пересборки");
    let hb = h.element_bounds(head[0]);
    assert!(
        b.size.height > 600.0 && hb.size.height < 60.0,
        "заголовок {} по высоте, список {}",
        hb.size.height,
        b.size.height
    );
}
