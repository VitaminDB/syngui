pub fn find_match(text: &str, byte: usize) -> Option<(usize, usize)> {
    let text_len = text.len();
    if text_len == 0 {
        return None;
    }

    let candidates: [Option<usize>; 2] = [
        if byte < text_len { Some(byte) } else { None },
        if byte > 0 {
            Some(prev_char_boundary(text, byte))
        } else {
            None
        },
    ];

    for cand in candidates.iter().flatten().copied() {
        let ch = match text[cand..].chars().next() {
            Some(c) => c,
            None => continue,
        };
        let (open, close, forward) = match ch {
            '(' => ('(', ')', true),
            '[' => ('[', ']', true),
            '{' => ('{', '}', true),
            ')' => ('(', ')', false),
            ']' => ('[', ']', false),
            '}' => ('{', '}', false),
            _ => continue,
        };
        if let Some(m) = scan_match(text, cand, open, close, forward) {
            return Some((cand, m));
        }
    }
    None
}

fn scan_match(text: &str, start: usize, open: char, close: char, forward: bool) -> Option<usize> {
    let mut depth: i32 = 0;
    if forward {
        let after = &text[start + open.len_utf8()..];
        let base = start + open.len_utf8();
        for (i, ch) in after.char_indices() {
            if ch == open {
                depth += 1;
            } else if ch == close {
                if depth == 0 {
                    return Some(base + i);
                }
                depth -= 1;
            }
        }
    } else {
        let mut byte = start;
        while byte > 0 {
            byte = prev_char_boundary(text, byte);
            let ch = match text[byte..].chars().next() {
                Some(c) => c,
                None => return None,
            };
            if ch == close {
                depth += 1;
            } else if ch == open {
                if depth == 0 {
                    return Some(byte);
                }
                depth -= 1;
            }
        }
    }
    None
}

fn prev_char_boundary(text: &str, byte: usize) -> usize {
    if byte == 0 {
        return 0;
    }
    let mut b = byte - 1;
    while b > 0 && !text.is_char_boundary(b) {
        b -= 1;
    }
    b
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn no_match_in_plain_text() {
        assert_eq!(find_match("hello world", 5), None);
    }

    #[test]
    fn match_paren_forward() {
        let text = "fn foo(x: i32) {}";
        let r = find_match(text, 6);
        assert_eq!(r, Some((6, 13)));
    }

    #[test]
    fn match_paren_backward() {
        let text = "fn foo(x: i32) {}";
        let r = find_match(text, 13);
        assert_eq!(r, Some((13, 6)));
    }

    #[test]
    fn match_braces_nested() {
        let text = "{{}}";
        assert_eq!(find_match(text, 0), Some((0, 3)));
        assert_eq!(find_match(text, 1), Some((1, 2)));
        assert_eq!(find_match(text, 2), Some((2, 1)));
        assert_eq!(find_match(text, 3), Some((3, 0)));
    }

    #[test]
    fn unmatched_returns_none() {
        assert_eq!(find_match("(((", 0), None);
        assert_eq!(find_match(")))", 2), None);
    }

    #[test]
    fn matches_char_before_cursor() {
        let text = "{}";
        let r = find_match(text, 2);
        assert_eq!(r, Some((1, 0)));
    }
}
