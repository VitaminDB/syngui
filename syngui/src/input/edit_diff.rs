//! Разница двух состояний строки как поток правок у каретки: сколько символов
//! удалить перед ней и что вставить.
//!
//! Нужна веб-агенту ввода (`app/web_text_agent.rs`): браузер и экранная
//! клавиатура правят скрытый `<input>` (набор, composition IME, автозамена),
//! а виджету уходит эквивалент — `Backspace` × [`EditDiff::removed`] и
//! `CharInput` на каждый символ [`EditDiff::inserted`]. Каретка агента
//! всегда в конце, поэтому правка привязана к концу строки: общий префикс не
//! трогается, а всё после него удаляется и набирается заново. Общий суффикс
//! намеренно не ищется: «end ␣» → «end.␣» (точка по двойному пробелу Gboard)
//! у каретки виджета — это Backspace + «. », а не вставка точки перед пробелом.

/// Правка, переводящая старое значение в новое.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct EditDiff {
    /// Сколько символов (char) удалить перед кареткой.
    pub removed: usize,
    /// Что вставить после удаления.
    pub inserted: String,
}

impl EditDiff {
    pub fn is_empty(&self) -> bool {
        self.removed == 0 && self.inserted.is_empty()
    }
}

/// Считает правку, превращающую `old` в `new`.
pub fn edit_diff(old: &str, new: &str) -> EditDiff {
    let old_chars: Vec<char> = old.chars().collect();
    let new_chars: Vec<char> = new.chars().collect();
    let prefix = old_chars
        .iter()
        .zip(&new_chars)
        .take_while(|(a, b)| a == b)
        .count();
    EditDiff {
        removed: old_chars.len() - prefix,
        inserted: new_chars[prefix..].iter().collect(),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn diff(old: &str, new: &str) -> (usize, &'static str) {
        let d = edit_diff(old, new);
        (d.removed, Box::leak(d.inserted.into_boxed_str()))
    }

    #[test]
    fn append_char() {
        assert_eq!(diff("", "a"), (0, "a"));
        assert_eq!(diff("hel", "hell"), (0, "l"));
    }

    #[test]
    fn backspace() {
        assert_eq!(diff("hello", "hell"), (1, ""));
        assert_eq!(diff("a", ""), (1, ""));
    }

    #[test]
    fn word_delete() {
        assert_eq!(diff("hello world", "hello "), (5, ""));
    }

    #[test]
    fn autocorrect_on_commit() {
        // Gboard: "helo" → "hello " при подтверждении слова пробелом.
        assert_eq!(diff("helo", "hello "), (1, "lo "));
    }

    #[test]
    fn composition_replace() {
        assert_eq!(diff("прив", "привет"), (0, "ет"));
        assert_eq!(diff("teh", "the"), (2, "he"));
    }

    #[test]
    fn double_space_period() {
        assert_eq!(diff("end ", "end. "), (1, ". "));
    }

    #[test]
    fn unchanged_and_multibyte() {
        assert!(edit_diff("abc", "abc").is_empty());
        assert_eq!(diff("日本", "日本語"), (0, "語"));
        assert_eq!(diff("日本語", "日本"), (1, ""));
    }

    #[test]
    fn edit_in_the_middle_is_replayed_from_the_end() {
        assert_eq!(diff("ab", "axb"), (1, "xb"));
        assert_eq!(diff("axb", "ab"), (2, "b"));
    }
}
