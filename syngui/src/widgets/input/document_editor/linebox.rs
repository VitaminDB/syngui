//! Строчная раскладка инлайн-текста блока.
//!
//! Зеркалит перенос `FontAtlas::shape_text` (жадный по словам,
//! `split_inclusive(' ')`), как это уже делает selection_map у MarkdownView:
//! каждый видимый сегмент замеряется через [`TextMeasure`] и рисуется
//! отдельной командой `push_text_*`, так что перенос батчера не срабатывает.
//! Сегменты хранят байтовые диапазоны своих ранов — на них ложатся каретка
//! и выделение следующих этапов.

use crate::widget::context::TextMeasure;

use super::model::{InlineText, LinkTarget};
use super::style::DocStyle;

/// Визуальный стиль сегмента (плоский снимок для рисования).
#[derive(Clone, Debug)]
pub struct SegStyle {
    pub font_size: f32,
    pub bold: bool,
    pub italic: bool,
    pub strike: bool,
    pub code: bool,
    pub link: Option<LinkTarget>,
}

/// Непрерывный кусок текста на одной строке.
#[derive(Clone, Debug)]
pub struct Seg {
    /// X относительно левого края области текста.
    pub x: f32,
    pub width: f32,
    pub text: String,
    /// Индекс исходного рана в `InlineText`.
    pub run_idx: usize,
    /// Байтовое смещение начала сегмента внутри рана.
    pub byte_start: usize,
    pub style: SegStyle,
}

/// Одна визуальная строка.
#[derive(Clone, Debug)]
pub struct LineBox {
    /// Y верхней кромки строки относительно верха области текста.
    pub y: f32,
    pub height: f32,
    pub segs: Vec<Seg>,
}

#[derive(Clone, Debug, Default)]
pub struct InlineLayout {
    pub lines: Vec<LineBox>,
    pub height: f32,
}

/// Раскладывает раны блока в строки под ширину `max_width`.
///
/// `base_size` — базовый кегль блока (у заголовков свой), `force_bold` —
/// принудительный жирный (заголовки). Высота строки едина для блока:
/// `base_size * line_height` — так каретка и выделение остаются простыми.
pub fn layout_inline_text(
    text: &InlineText,
    base_size: f32,
    force_bold: bool,
    max_width: f32,
    style: &DocStyle,
    tm: &dyn TextMeasure,
) -> InlineLayout {
    let line_h = style.line_h(base_size);
    let max_width = max_width.max(base_size); // Защита от вырожденной ширины.
    let mut lines: Vec<LineBox> = vec![LineBox { y: 0.0, height: line_h, segs: Vec::new() }];
    let mut x = 0.0f32;

    let new_line = |lines: &mut Vec<LineBox>, x: &mut f32| {
        let y = lines.len() as f32 * line_h;
        lines.push(LineBox { y, height: line_h, segs: Vec::new() });
        *x = 0.0;
    };

    for (run_idx, run) in text.0.iter().enumerate() {
        let seg_style = SegStyle {
            font_size: if run.style.code { style.code_font_size } else { base_size },
            bold: run.style.bold || force_bold,
            italic: run.style.italic,
            strike: run.style.strike,
            code: run.style.code,
            link: run.style.link.clone(),
        };
        let measure = |t: &str| -> f32 {
            tm.measure_text_width_styled(
                t,
                seg_style.font_size,
                t.chars().count(),
                seg_style.bold,
                None,
            )
        };

        // Разбивка по жёстким переносам: каждый '\n' начинает новую строку.
        let mut byte_pos = 0usize;
        let mut first_part = true;
        for part in run.text.split('\n') {
            if !first_part {
                new_line(&mut lines, &mut x);
                byte_pos += 1; // Сам '\n'.
            }
            first_part = false;
            if part.is_empty() {
                continue;
            }

            let part_w = measure(part);
            if x + part_w <= max_width {
                // Часть помещается целиком.
                push_seg(&mut lines, &mut x, part, part_w, run_idx, byte_pos, &seg_style);
                byte_pos += part.len();
                continue;
            }

            // Пословный перенос; пробел остаётся на конце слова.
            let mut word_start = byte_pos;
            for word in part.split_inclusive(' ') {
                let ww = measure(word);
                if x + ww > max_width && x > 0.0 {
                    new_line(&mut lines, &mut x);
                }
                push_seg(&mut lines, &mut x, word, ww, run_idx, word_start, &seg_style);
                word_start += word.len();
            }
            byte_pos += part.len();
        }
    }

    let height = lines.len() as f32 * line_h;
    InlineLayout { lines, height }
}

fn push_seg(
    lines: &mut Vec<LineBox>,
    x: &mut f32,
    text: &str,
    width: f32,
    run_idx: usize,
    byte_start: usize,
    style: &SegStyle,
) {
    let line = lines.last_mut().expect("минимум одна строка");
    // Смежные сегменты одного рана на одной строке сливаем — меньше команд.
    if let Some(last) = line.segs.last_mut() {
        if last.run_idx == run_idx
            && last.byte_start + last.text.len() == byte_start
            && (last.x + last.width - *x).abs() < 0.5
        {
            last.text.push_str(text);
            last.width += width;
            *x += width;
            return;
        }
    }
    line.segs.push(Seg {
        x: *x,
        width,
        text: text.to_string(),
        run_idx,
        byte_start,
        style: style.clone(),
    });
    *x += width;
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::widget::context::TextMeasure;

    /// Моноширинная метрика: 10px на символ.
    struct Mono;
    impl TextMeasure for Mono {
        fn measure_text_width(&self, _t: &str, _fs: f32, chars: usize) -> f32 {
            chars as f32 * 10.0
        }
        fn hit_test_char(&self, text: &str, _fs: f32, x: f32) -> usize {
            ((x / 10.0).round() as usize).min(text.chars().count())
        }
    }

    fn style() -> DocStyle {
        DocStyle { line_height: 2.0, ..DocStyle::default() }
    }

    #[test]
    fn short_text_single_line() {
        let t = InlineText::plain("привет мир");
        let l = layout_inline_text(&t, 10.0, false, 500.0, &style(), &Mono);
        assert_eq!(l.lines.len(), 1);
        assert_eq!(l.height, 20.0);
        assert_eq!(l.lines[0].segs.len(), 1);
        assert_eq!(l.lines[0].segs[0].text, "привет мир");
    }

    #[test]
    fn word_wrap() {
        // 10 символов на строку (100px). «привет мир небо» → «привет » | «мир небо».
        let t = InlineText::plain("привет мир небо");
        let l = layout_inline_text(&t, 10.0, false, 100.0, &style(), &Mono);
        assert_eq!(l.lines.len(), 2);
        let line_text = |i: usize| -> String {
            l.lines[i].segs.iter().map(|s| s.text.as_str()).collect()
        };
        assert_eq!(line_text(0), "привет ");
        assert_eq!(line_text(1), "мир небо");
    }

    #[test]
    fn hard_break_forces_line() {
        let t = InlineText::plain("раз\nдва");
        let l = layout_inline_text(&t, 10.0, false, 500.0, &style(), &Mono);
        assert_eq!(l.lines.len(), 2);
        // Байтовые смещения учитывают '\n'.
        assert_eq!(l.lines[1].segs[0].byte_start, "раз\n".len());
    }

    #[test]
    fn empty_text_has_one_line() {
        let t = InlineText::default();
        let l = layout_inline_text(&t, 10.0, false, 500.0, &style(), &Mono);
        assert_eq!(l.lines.len(), 1);
        assert!(l.lines[0].segs.is_empty());
        assert_eq!(l.height, 20.0);
    }

    #[test]
    fn byte_offsets_track_words() {
        let t = InlineText::plain("aa bb cc");
        let l = layout_inline_text(&t, 10.0, false, 30.0, &style(), &Mono);
        // По одному слову на строку: "aa " | "bb " | "cc".
        assert_eq!(l.lines.len(), 3);
        assert_eq!(l.lines[0].segs[0].byte_start, 0);
        assert_eq!(l.lines[1].segs[0].byte_start, 3);
        assert_eq!(l.lines[2].segs[0].byte_start, 6);
    }
}
