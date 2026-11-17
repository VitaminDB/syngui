//! `Flex` с переносом меряет элементы по содержимому, как flex-wrap в CSS:
//! SegmentedButton (заполняет доступную ширину, если она ограничена) не
//! растягивается на всю строку, а длинный текст по-прежнему переносится
//! по ширине строки (регрессия 1c1cbf3: все элементы получали ширину строки
//! и сегменты в панели инструментов становились во всю ширину, каждый на
//! своей строке).

use syngui::prelude::*;
use syngui::testing::*;
use syngui::widgets::buttons::Segment;
use syngui::widgets::*;

const MSS: &str = "
.box { width: 900px; }
.pill { padding: 4px; }
";

fn harness(widget: impl Widget + 'static) -> TestHarness {
    let mut h = TestHarness::new(Box::new(widget));
    h.apply_mss(MSS);
    h.layout(1000.0, 600.0);
    h
}

#[test]
fn segmented_takes_content_width_in_wrap() {
    let h = harness(
        DecoratedBox::new().class("box").child(
            Flex::row()
                .wrap()
                .gap(16.0)
                .child(
                    SegmentedButton::new(vec![Segment::new("День"), Segment::new("Неделя")])
                        .class("seg1"),
                )
                .child(
                    SegmentedButton::new(vec![Segment::new("Один"), Segment::new("Два")])
                        .class("seg2"),
                ),
        ),
    );
    let a = h.element_bounds(h.find_by_class("seg1")[0]);
    let b = h.element_bounds(h.find_by_class("seg2")[0]);
    assert!(a.size.width < 450.0, "сегменты растянуты: {a:?}");
    assert_eq!(a.origin.y, b.origin.y, "сегменты должны стоять в одной строке");
}

#[test]
fn long_text_still_wraps_to_line_width() {
    let long = "очень длинный текст ".repeat(40);
    let h = harness(
        DecoratedBox::new().class("box").child(
            Flex::row()
                .wrap()
                .child(DecoratedBox::new().class("pill").child(Text::new(long).class("txt"))),
        ),
    );
    let t = h.element_bounds(h.find_by_class("txt")[0]);
    assert!(t.size.width <= 900.0 + 0.5, "текст вылез за строку: {t:?}");
    assert!(t.size.height > 30.0, "текст должен переноситься: {t:?}");
}
