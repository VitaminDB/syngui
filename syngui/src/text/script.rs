/// Writing systems that need a CJK fallback face and ideographic line breaking.
#[derive(Clone, Copy, PartialEq, Eq, Hash, Debug)]
pub enum Script {
    Han,
    Kana,
    Hangul,
}

const SCRIPT_RANGES: &[(u32, u32, Script)] = &[
    (0x1100, 0x11FF, Script::Hangul),
    (0x2E80, 0x2EFF, Script::Han),
    (0x2F00, 0x2FDF, Script::Han),
    (0x3000, 0x303F, Script::Han),
    (0x3040, 0x309F, Script::Kana),
    (0x30A0, 0x30FF, Script::Kana),
    (0x3130, 0x318F, Script::Hangul),
    (0x31F0, 0x31FF, Script::Kana),
    (0x3400, 0x4DBF, Script::Han),
    (0x4E00, 0x9FFF, Script::Han),
    (0xA960, 0xA97F, Script::Hangul),
    (0xAC00, 0xD7AF, Script::Hangul),
    (0xD7B0, 0xD7FF, Script::Hangul),
    (0xF900, 0xFAFF, Script::Han),
    (0xFF00, 0xFF65, Script::Han),
    (0xFF66, 0xFF9F, Script::Kana),
    (0x20000, 0x3134F, Script::Han),
];

/// Script of `ch` when it lies in a CJK block (CJK punctuation and fullwidth
/// forms count as Han); `None` for every other character.
pub fn script_of(ch: char) -> Option<Script> {
    let c = ch as u32;
    if c < SCRIPT_RANGES[0].0 {
        return None;
    }
    SCRIPT_RANGES
        .iter()
        .find(|&&(lo, hi, _)| c >= lo && c <= hi)
        .map(|&(_, _, script)| script)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn latin_and_cyrillic_have_no_script() {
        for ch in ['a', 'Z', '0', ' ', 'я', 'Ж', 'é', '€'] {
            assert_eq!(script_of(ch), None, "{ch:?}");
        }
    }

    #[test]
    fn han_ranges() {
        for ch in ['日', '本', '語', '\u{3400}', '\u{4DBF}', '\u{F900}', '\u{2E80}', '\u{2F00}', '\u{20000}', '\u{3134F}'] {
            assert_eq!(script_of(ch), Some(Script::Han), "{ch:?}");
        }
    }

    #[test]
    fn cjk_punctuation_and_fullwidth_map_to_han() {
        for ch in ['。', '、', '「', '」', '\u{3000}', '！', '，', 'Ａ', '\u{FF65}'] {
            assert_eq!(script_of(ch), Some(Script::Han), "{ch:?}");
        }
    }

    #[test]
    fn kana_ranges() {
        for ch in ['あ', 'ん', 'ア', 'ン', 'ー', '\u{31F0}', 'ｱ', '\u{FF9F}'] {
            assert_eq!(script_of(ch), Some(Script::Kana), "{ch:?}");
        }
    }

    #[test]
    fn hangul_ranges() {
        for ch in ['한', '국', '\u{1100}', '\u{3130}', '\u{A960}', '\u{D7B0}', '\u{D7FF}'] {
            assert_eq!(script_of(ch), Some(Script::Hangul), "{ch:?}");
        }
    }

    #[test]
    fn range_boundaries_are_inclusive() {
        assert_eq!(script_of('\u{4E00}'), Some(Script::Han));
        assert_eq!(script_of('\u{9FFF}'), Some(Script::Han));
        assert_eq!(script_of('\u{AC00}'), Some(Script::Hangul));
        assert_eq!(script_of('\u{D7AF}'), Some(Script::Hangul));
        assert_eq!(script_of('\u{2FE0}'), None);
        assert_eq!(script_of('\u{31FF}'), Some(Script::Kana));
        assert_eq!(script_of('\u{3200}'), None);
    }
}
