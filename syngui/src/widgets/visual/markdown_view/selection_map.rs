use crate::core::{Point, Rect};
use crate::widget::context::TextMeasure;

#[derive(Clone, Debug)]
pub struct SelectableRun {
    pub rect: Rect,
    pub visible_text: String,
    pub font_size: f32,
    pub font_family: Option<String>,
    #[allow(dead_code)]
    pub bold: bool,
    pub block_id: u32,
    pub line_id: u32,
    pub link: Option<String>,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct SelPos {
    pub run_idx: usize,
    pub byte_in_run: usize,
}

impl SelPos {
    pub const ZERO: Self = SelPos { run_idx: 0, byte_in_run: 0 };

    pub fn key(self) -> (usize, usize) {
        (self.run_idx, self.byte_in_run)
    }
}

pub fn hit_test(runs: &[SelectableRun], pos: Point, tm: &dyn TextMeasure) -> SelPos {
    if runs.is_empty() {
        return SelPos::ZERO;
    }

    let mut line_indices: Vec<Vec<usize>> = Vec::new();
    let mut current_line: Option<u32> = None;
    for (i, r) in runs.iter().enumerate() {
        if Some(r.line_id) == current_line {
            line_indices.last_mut().unwrap().push(i);
        } else {
            current_line = Some(r.line_id);
            line_indices.push(vec![i]);
        }
    }

    let mut best_line = 0usize;
    let mut best_dy = f32::INFINITY;
    for (li, idxs) in line_indices.iter().enumerate() {
        let first = &runs[idxs[0]];
        let top = first.rect.origin.y;
        let bottom = top + first.rect.size.height;
        let dy = if pos.y < top {
            top - pos.y
        } else if pos.y > bottom {
            pos.y - bottom
        } else {
            0.0
        };
        if dy < best_dy {
            best_dy = dy;
            best_line = li;
        }
    }

    let line = &line_indices[best_line];
    let mut best_run = line[0];
    let mut best_dx = f32::INFINITY;
    for &idx in line {
        let r = &runs[idx];
        let left = r.rect.origin.x;
        let right = left + r.rect.size.width;
        let dx = if pos.x < left {
            left - pos.x
        } else if pos.x > right {
            pos.x - right
        } else {
            0.0
        };
        if dx < best_dx {
            best_dx = dx;
            best_run = idx;
        }
    }

    let run = &runs[best_run];
    let x_local = (pos.x - run.rect.origin.x).clamp(0.0, run.rect.size.width);
    let char_idx = tm.hit_test_char_styled(
        &run.visible_text,
        run.font_size,
        x_local,
        run.font_family.as_deref(),
    );
    let byte = char_idx_to_byte(&run.visible_text, char_idx);
    SelPos { run_idx: best_run, byte_in_run: byte }
}

fn char_idx_to_byte(text: &str, char_idx: usize) -> usize {
    text.char_indices()
        .nth(char_idx)
        .map(|(b, _)| b)
        .unwrap_or(text.len())
}

pub fn extract_selection_text(runs: &[SelectableRun], a: SelPos, b: SelPos) -> String {
    if runs.is_empty() {
        return String::new();
    }
    let (start, end) = if a.key() <= b.key() { (a, b) } else { (b, a) };
    if start == end {
        return String::new();
    }

    let last = runs.len().saturating_sub(1);
    let start_run = start.run_idx.min(last);
    let end_run = end.run_idx.min(last);

    let mut out = String::new();
    let mut prev_block: Option<u32> = None;
    let mut prev_line: Option<u32> = None;

    for ri in start_run..=end_run {
        let run = &runs[ri];
        let lo = if ri == start_run { start.byte_in_run } else { 0 };
        let hi = if ri == end_run { end.byte_in_run } else { run.visible_text.len() };
        let lo = lo.min(run.visible_text.len());
        let hi = hi.min(run.visible_text.len());
        if hi <= lo {
            continue;
        }

        if let (Some(pb), Some(pl)) = (prev_block, prev_line) {
            if pb != run.block_id {
                out.push_str("\n\n");
            } else if pl != run.line_id {
                out.push('\n');
            }
        }

        out.push_str(&run.visible_text[lo..hi]);
        prev_block = Some(run.block_id);
        prev_line = Some(run.line_id);
    }
    out
}

pub fn select_all_pos(runs: &[SelectableRun]) -> (SelPos, SelPos) {
    if runs.is_empty() {
        return (SelPos::ZERO, SelPos::ZERO);
    }
    let last = runs.len() - 1;
    let last_byte = runs[last].visible_text.len();
    (
        SelPos { run_idx: 0, byte_in_run: 0 },
        SelPos { run_idx: last, byte_in_run: last_byte },
    )
}

pub fn word_boundaries_in_run(text: &str, byte_offset: usize) -> (usize, usize) {
    if text.is_empty() {
        return (0, 0);
    }
    let bo = byte_offset.min(text.len());
    let Some(ch_at) = text[bo..].chars().next() else {
        return (bo, bo);
    };

    if !is_word_char(ch_at) {
        return (bo, bo + ch_at.len_utf8());
    }

    let mut start = bo;
    for (i, c) in text[..bo].char_indices().rev() {
        if is_word_char(c) {
            start = i;
        } else {
            break;
        }
    }

    let mut end = text.len();
    for (i, c) in text[bo..].char_indices() {
        if !is_word_char(c) {
            end = bo + i;
            break;
        }
    }
    (start, end)
}

fn is_word_char(c: char) -> bool {
    c.is_alphanumeric() || c == '_'
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::core::Size;

    fn run(rect: Rect, text: &str, block: u32, line: u32) -> SelectableRun {
        SelectableRun {
            rect,
            visible_text: text.to_string(),
            font_size: 14.0,
            font_family: None,
            bold: false,
            block_id: block,
            line_id: line,
            link: None,
        }
    }

    fn rect(x: f32, y: f32, w: f32, h: f32) -> Rect {
        Rect::new(Point::new(x, y), Size::new(w, h))
    }

    #[test]
    fn extract_within_single_run() {
        let runs = vec![run(rect(0.0, 0.0, 100.0, 18.0), "hello world", 0, 0)];
        let s = extract_selection_text(
            &runs,
            SelPos { run_idx: 0, byte_in_run: 0 },
            SelPos { run_idx: 0, byte_in_run: 5 },
        );
        assert_eq!(s, "hello");
    }

    #[test]
    fn extract_swaps_anchor_and_focus_when_reversed() {
        let runs = vec![run(rect(0.0, 0.0, 100.0, 18.0), "hello world", 0, 0)];
        let s = extract_selection_text(
            &runs,
            SelPos { run_idx: 0, byte_in_run: 6 },
            SelPos { run_idx: 0, byte_in_run: 11 },
        );
        assert_eq!(s, "world");
    }

    #[test]
    fn extract_two_runs_same_line_no_separator() {
        let runs = vec![
            run(rect(0.0, 0.0, 50.0, 18.0), "alpha ", 0, 0),
            run(rect(50.0, 0.0, 50.0, 18.0), "beta", 0, 0),
        ];
        let s = extract_selection_text(
            &runs,
            SelPos { run_idx: 0, byte_in_run: 0 },
            SelPos { run_idx: 1, byte_in_run: 4 },
        );
        assert_eq!(s, "alpha beta");
    }

    #[test]
    fn extract_two_lines_same_block_inserts_lf() {
        let runs = vec![
            run(rect(0.0, 0.0, 100.0, 18.0), "first", 0, 0),
            run(rect(0.0, 18.0, 100.0, 18.0), "second", 0, 1),
        ];
        let s = extract_selection_text(
            &runs,
            SelPos { run_idx: 0, byte_in_run: 0 },
            SelPos { run_idx: 1, byte_in_run: 6 },
        );
        assert_eq!(s, "first\nsecond");
    }

    #[test]
    fn extract_across_blocks_inserts_blank_line() {
        let runs = vec![
            run(rect(0.0, 0.0, 100.0, 18.0), "para1", 0, 0),
            run(rect(0.0, 24.0, 100.0, 18.0), "para2", 1, 1),
        ];
        let s = extract_selection_text(
            &runs,
            SelPos { run_idx: 0, byte_in_run: 0 },
            SelPos { run_idx: 1, byte_in_run: 5 },
        );
        assert_eq!(s, "para1\n\npara2");
    }

    #[test]
    fn extract_zero_range_returns_empty_string() {
        let runs = vec![run(rect(0.0, 0.0, 50.0, 18.0), "abc", 0, 0)];
        let s = extract_selection_text(
            &runs,
            SelPos { run_idx: 0, byte_in_run: 1 },
            SelPos { run_idx: 0, byte_in_run: 1 },
        );
        assert_eq!(s, "");
    }

    #[test]
    fn select_all_returns_first_zero_to_last_end() {
        let runs = vec![
            run(rect(0.0, 0.0, 50.0, 18.0), "abc", 0, 0),
            run(rect(0.0, 18.0, 50.0, 18.0), "defg", 0, 1),
        ];
        let (a, b) = select_all_pos(&runs);
        assert_eq!(a, SelPos::ZERO);
        assert_eq!(b, SelPos { run_idx: 1, byte_in_run: 4 });
    }

    #[test]
    fn word_boundaries_basic() {
        let (s, e) = word_boundaries_in_run("hello world", 2);
        assert_eq!(&"hello world"[s..e], "hello");
    }

    #[test]
    fn word_boundaries_at_punctuation_returns_single_char() {
        let (s, e) = word_boundaries_in_run("hello world", 5);
        assert_eq!(&"hello world"[s..e], " ");
    }

    #[test]
    fn word_boundaries_unicode() {
        let text = "Привет мир";
        let (s, e) = word_boundaries_in_run(text, 4);
        assert_eq!(&text[s..e], "Привет");
    }
}
