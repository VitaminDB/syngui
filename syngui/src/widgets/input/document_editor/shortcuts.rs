//! Markdown-шорткаты набора.
//!
//! Блочные: `# `…`###### `, `- `/`* `, `N. `, `[] `, `> `, ` ``` `, `---` —
//! проверяются после ввода пробела (или замыкающего символа) в начале
//! параграфа и превращают его в блок соответствующего типа.
//! Инлайновые: `**b**`, `*i*`, `~~s~~`, `` `code` `` — срабатывают на вводе
//! замыкающего маркера, снимают маркеры из текста и стилизуют содержимое.

use super::edit::style_range;
use super::model::{InlineStyle, InlineText};

/// Во что превращается параграф блочным шорткатом.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum BlockShortcut {
    Heading(u8),
    Bullet,
    Numbered(u64),
    Todo,
    Quote,
    CodeBlock,
    Divider,
    Toggle,
}

/// Распознаёт блочный шорткат по префиксу параграфа (плоский текст до
/// каретки). Возвращает (шорткат, длина съедаемого префикса в байтах).
pub fn block_shortcut(head: &str) -> Option<(BlockShortcut, usize)> {
    // Заголовки: 1–6 решёток + пробел.
    let hashes = head.bytes().take_while(|b| *b == b'#').count();
    if hashes >= 1 && hashes <= 6 && head.len() == hashes + 1 && head.ends_with(' ') {
        return Some((BlockShortcut::Heading(hashes as u8), head.len()));
    }
    match head {
        "- " | "* " | "+ " => return Some((BlockShortcut::Bullet, head.len())),
        "[] " | "[ ] " => return Some((BlockShortcut::Todo, head.len())),
        "> " => return Some((BlockShortcut::Quote, head.len())),
        ">> " => return Some((BlockShortcut::Toggle, head.len())),
        "```" => return Some((BlockShortcut::CodeBlock, head.len())),
        "---" => return Some((BlockShortcut::Divider, head.len())),
        _ => {}
    }
    // Нумерация: `N. ` / `N) `.
    if head.ends_with(". ") || head.ends_with(") ") {
        let digits = &head[..head.len() - 2];
        if !digits.is_empty() && digits.len() <= 9 && digits.bytes().all(|b| b.is_ascii_digit()) {
            if let Ok(n) = digits.parse::<u64>() {
                return Some((BlockShortcut::Numbered(n), head.len()));
            }
        }
    }
    None
}

/// Инлайн-шорткаты. `caret` — позиция сразу после только что введённого
/// символа. При срабатывании маркеры удаляются, содержимое стилизуется,
/// возвращается новая позиция каретки.
pub fn try_inline_shortcut(text: &mut InlineText, caret: usize) -> Option<usize> {
    let plain = text.text();
    let head = &plain[..caret.min(plain.len())];

    type Setter = fn(&mut InlineStyle);
    // Порядок важен: `**` раньше `*`.
    let rules: [(&str, Setter); 4] = [
        ("**", |s| s.bold = true),
        ("*", |s| s.italic = true),
        ("~~", |s| s.strike = true),
        ("`", |s| s.code = true),
    ];

    for (marker, setter) in rules {
        let m_len = marker.len();
        if !head.ends_with(marker) {
            continue;
        }
        // Ищем открывающий маркер левее закрывающего.
        let before_close = &head[..head.len() - m_len];
        let Some(open_rel) = before_close.rfind(marker) else { continue };
        let inner_start = open_rel + m_len;
        let inner = &before_close[inner_start..];
        // Содержимое непустое, без переводов строк, не начинается с пробела
        // (иначе `2 * 3 * 4` превращалось бы в курсив).
        if inner.is_empty()
            || inner.starts_with(' ')
            || inner.ends_with(' ')
            || inner.contains('\n')
        {
            continue;
        }
        // `**` не должен ложно срабатывать как `*` по хвосту `**`.
        if marker == "*" && before_close[..open_rel].ends_with('*') {
            continue;
        }
        // Удаляем маркеры (сначала правый — смещения левого не плывут).
        super::edit::text_delete(text, head.len() - m_len, head.len());
        super::edit::text_delete(text, open_rel, inner_start);
        // Стилизуем содержимое.
        let styled_start = open_rel;
        let styled_end = open_rel + inner.len();
        style_range(text, styled_start, styled_end, &|s| setter(s));
        return Some(styled_end);
    }
    None
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn block_shortcuts() {
        assert_eq!(block_shortcut("# "), Some((BlockShortcut::Heading(1), 2)));
        assert_eq!(block_shortcut("### "), Some((BlockShortcut::Heading(3), 4)));
        assert_eq!(block_shortcut("- "), Some((BlockShortcut::Bullet, 2)));
        assert_eq!(block_shortcut("7. "), Some((BlockShortcut::Numbered(7), 3)));
        assert_eq!(block_shortcut("[] "), Some((BlockShortcut::Todo, 3)));
        assert_eq!(block_shortcut("```"), Some((BlockShortcut::CodeBlock, 3)));
        assert_eq!(block_shortcut("---"), Some((BlockShortcut::Divider, 3)));
        assert_eq!(block_shortcut("тек "), None);
        assert_eq!(block_shortcut("####### "), None);
    }

    #[test]
    fn inline_bold() {
        let mut t = InlineText::plain("это **жирно**");
        let caret = t.len_bytes();
        let new = try_inline_shortcut(&mut t, caret).unwrap();
        assert_eq!(t.text(), "это жирно");
        assert_eq!(new, "это жирно".len());
        let bold: String =
            t.0.iter().filter(|r| r.style.bold).map(|r| r.text.as_str()).collect();
        assert_eq!(bold, "жирно");
    }

    #[test]
    fn inline_italic_not_confused_with_bold() {
        let mut t = InlineText::plain("**почти*");
        // Хвост `*` после `**` — открывающий маркер сместился бы на `*`
        // внутри `**`: не срабатываем.
        assert!(try_inline_shortcut(&mut t, t.len_bytes()).is_none());

        let mut t = InlineText::plain("тут *курсив*");
        let new = try_inline_shortcut(&mut t, t.len_bytes()).unwrap();
        assert_eq!(t.text(), "тут курсив");
        assert_eq!(new, "тут курсив".len());
        assert!(t.0.iter().any(|r| r.style.italic));
    }

    #[test]
    fn inline_code_and_strike() {
        let mut t = InlineText::plain("см. `код`");
        try_inline_shortcut(&mut t, t.len_bytes()).unwrap();
        assert!(t.0.iter().any(|r| r.style.code && r.text == "код"));

        let mut t = InlineText::plain("~~зачёркнуто~~");
        try_inline_shortcut(&mut t, t.len_bytes()).unwrap();
        assert!(t.0.iter().any(|r| r.style.strike && r.text == "зачёркнуто"));
    }

    #[test]
    fn inline_rejects_spaces() {
        let mut t = InlineText::plain("2 * 3 *");
        assert!(try_inline_shortcut(&mut t, t.len_bytes()).is_none());
        assert_eq!(t.text(), "2 * 3 *");
    }
}
