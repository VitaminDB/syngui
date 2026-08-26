use crate::text::script::script_of;

/// Line-breaking class of a character, coarse enough for word wrapping.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum BreakClass {
    Space,
    Ideographic,
    NoBreakBefore,
    NoBreakAfter,
    Other,
}

const NO_BREAK_BEFORE: &[char] = &[
    '、', '。', '，', '．', '！', '？', '：', '；', '～',
    '」', '』', '】', '）', '］', '｝', '〉', '》', '〕', '〗', '〙', '〛', '〞', '〟',
    'ー', '々', '〻', '〜', 'ヽ', 'ヾ', 'ゝ', 'ゞ',
    'ぁ', 'ぃ', 'ぅ', 'ぇ', 'ぉ', 'っ', 'ゃ', 'ゅ', 'ょ', 'ゎ',
    'ァ', 'ィ', 'ゥ', 'ェ', 'ォ', 'ッ', 'ャ', 'ュ', 'ョ', 'ヮ', 'ヵ', 'ヶ',
    'ｰ', 'ｧ', 'ｨ', 'ｩ', 'ｪ', 'ｫ', 'ｬ', 'ｭ', 'ｮ', 'ｯ', '｡', '､', '｣',
];

const NO_BREAK_AFTER: &[char] = &[
    '「', '『', '【', '（', '［', '｛', '《', '〈', '〔', '〖', '〘', '〚', '〝', '｢',
];

fn is_cjk_punctuation(ch: char) -> bool {
    let c = ch as u32;
    matches!(c,
        0x3000..=0x303F
            | 0xFF00..=0xFF0F
            | 0xFF1A..=0xFF20
            | 0xFF3B..=0xFF40
            | 0xFF5B..=0xFF65)
}

/// Classifies `ch` for wrapping: CJK ideographs and syllables break between any
/// two of them, closers and small kana cling to the previous character, openers
/// to the next one. Everything outside the CJK blocks is `Other`.
pub fn break_class(ch: char) -> BreakClass {
    if ch == ' ' {
        return BreakClass::Space;
    }
    if NO_BREAK_BEFORE.contains(&ch) {
        return BreakClass::NoBreakBefore;
    }
    if NO_BREAK_AFTER.contains(&ch) {
        return BreakClass::NoBreakAfter;
    }
    if script_of(ch).is_some() && !is_cjk_punctuation(ch) {
        return BreakClass::Ideographic;
    }
    BreakClass::Other
}

/// Whether a line may break between `prev` and `ch` without a space. Never
/// true at the start of text; never affects text with no CJK characters.
pub fn breaks_before(prev: Option<char>, ch: char) -> bool {
    let Some(prev) = prev else {
        return false;
    };
    let p = break_class(prev);
    let c = break_class(ch);
    if p == BreakClass::NoBreakAfter || c == BreakClass::NoBreakBefore {
        return false;
    }
    matches!(p, BreakClass::Ideographic | BreakClass::NoBreakBefore)
        || matches!(c, BreakClass::Ideographic | BreakClass::NoBreakAfter)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn classes() {
        assert_eq!(break_class(' '), BreakClass::Space);
        assert_eq!(break_class('日'), BreakClass::Ideographic);
        assert_eq!(break_class('あ'), BreakClass::Ideographic);
        assert_eq!(break_class('한'), BreakClass::Ideographic);
        assert_eq!(break_class('Ａ'), BreakClass::Ideographic);
        assert_eq!(break_class('。'), BreakClass::NoBreakBefore);
        assert_eq!(break_class('」'), BreakClass::NoBreakBefore);
        assert_eq!(break_class('）'), BreakClass::NoBreakBefore);
        assert_eq!(break_class('ー'), BreakClass::NoBreakBefore);
        assert_eq!(break_class('ょ'), BreakClass::NoBreakBefore);
        assert_eq!(break_class('「'), BreakClass::NoBreakAfter);
        assert_eq!(break_class('（'), BreakClass::NoBreakAfter);
        assert_eq!(break_class('〒'), BreakClass::Other);
        assert_eq!(break_class('a'), BreakClass::Other);
        assert_eq!(break_class('.'), BreakClass::Other);
        assert_eq!(break_class('я'), BreakClass::Other);
    }

    #[test]
    fn never_breaks_at_start() {
        assert!(!breaks_before(None, '日'));
        assert!(!breaks_before(None, 'a'));
    }

    #[test]
    fn latin_text_is_untouched() {
        assert!(!breaks_before(Some('a'), 'b'));
        assert!(!breaks_before(Some('.'), 'a'));
        assert!(!breaks_before(Some(')'), '('));
        assert!(!breaks_before(Some('я'), 'з'));
    }

    #[test]
    fn ideographs_break_anywhere() {
        assert!(breaks_before(Some('日'), '本'));
        assert!(breaks_before(Some('あ'), 'い'));
        assert!(breaks_before(Some('한'), '국'));
        assert!(breaks_before(Some('日'), 'a'));
        assert!(breaks_before(Some('a'), '日'));
    }

    #[test]
    fn closers_and_small_kana_cling_to_previous() {
        assert!(!breaks_before(Some('日'), '。'));
        assert!(!breaks_before(Some('本'), '」'));
        assert!(!breaks_before(Some('ラ'), 'ー'));
        assert!(!breaks_before(Some('き'), 'ょ'));
        assert!(!breaks_before(Some('」'), '。'));
        assert!(breaks_before(Some('。'), '日'));
        assert!(breaks_before(Some('。'), 'a'));
        assert!(breaks_before(Some('ー'), 'メ'));
    }

    #[test]
    fn openers_cling_to_next() {
        assert!(!breaks_before(Some('「'), '日'));
        assert!(!breaks_before(Some('「'), '『'));
        assert!(!breaks_before(Some('（'), 'a'));
        assert!(breaks_before(Some('日'), '「'));
        assert!(breaks_before(Some('a'), '「'));
        assert!(breaks_before(Some('。'), '「'));
    }
}
