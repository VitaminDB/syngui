pub(crate) mod axis;
pub(crate) mod legend;
pub(crate) mod series;
pub(crate) mod tooltip;

use crate::widget::context::TextMeasure;
use std::sync::Arc;

pub(crate) fn estimate_text_width(
    text: &str,
    font_size: f32,
    tm: Option<&Arc<dyn TextMeasure>>,
) -> f32 {
    tm.map(|tm| tm.measure_text_width(text, font_size, text.chars().count()))
        .unwrap_or_else(|| text.chars().count() as f32 * font_size * 0.6)
}
