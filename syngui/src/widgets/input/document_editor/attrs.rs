//! Инлайн-атрибуты элементов: `{width=70% align=center loop}`.
//!
//! Грамматика (Pandoc-подобная, но проще):
//! - блок = `{` элементы через пробелы `}`; переводы строк внутри запрещены;
//! - элемент = `key` (флаг) или `key=value`;
//! - key — ASCII: буквы/цифры/`_`/`-`/`.`/`:`; кириллический или иной ключ
//!   делает блок невалидным, и он остаётся обычным текстом;
//! - value — либо `"в кавычках"` (с экранированием `\"` и `\\`), либо голый
//!   токен до пробела/`}` (юникод разрешён: `icon=🚀`, `width=70%`).

use super::model::Attrs;

/// Парсит строку вида `{...}` (включая фигурные скобки). `None` — если это
/// не валидный блок атрибутов (тогда текст остаётся текстом).
pub fn parse_attr_block(s: &str) -> Option<Attrs> {
    let inner = s.strip_prefix('{')?.strip_suffix('}')?;
    if inner.contains('\n') || inner.contains('{') || inner.contains('}') {
        return None;
    }
    let mut attrs = Attrs::default();
    let mut chars = inner.char_indices().peekable();
    loop {
        // Пропускаем пробелы между элементами.
        while matches!(chars.peek(), Some((_, c)) if c.is_whitespace()) {
            chars.next();
        }
        let Some(&(key_start, _)) = chars.peek() else { break };
        // Ключ.
        let mut key_end = key_start;
        while let Some(&(i, c)) = chars.peek() {
            if is_key_char(c) {
                key_end = i + c.len_utf8();
                chars.next();
            } else {
                break;
            }
        }
        if key_end == key_start {
            return None; // Не-ASCII или мусор на месте ключа.
        }
        let key = &inner[key_start..key_end];
        // Значение (опционально).
        let mut value = String::new();
        if matches!(chars.peek(), Some((_, '='))) {
            chars.next();
            match chars.peek() {
                Some((_, '"')) => {
                    chars.next();
                    let mut closed = false;
                    while let Some((_, c)) = chars.next() {
                        match c {
                            '\\' => match chars.next() {
                                Some((_, esc @ ('"' | '\\'))) => value.push(esc),
                                Some((_, other)) => {
                                    value.push('\\');
                                    value.push(other);
                                }
                                None => return None,
                            },
                            '"' => {
                                closed = true;
                                break;
                            }
                            _ => value.push(c),
                        }
                    }
                    if !closed {
                        return None;
                    }
                }
                Some(_) => {
                    while let Some(&(_, c)) = chars.peek() {
                        if c.is_whitespace() {
                            break;
                        }
                        value.push(c);
                        chars.next();
                    }
                    if value.is_empty() {
                        return None; // `key=` без значения.
                    }
                }
                None => return None,
            }
        }
        attrs.set(key, value);
        // После элемента — либо пробел, либо конец.
        if let Some(&(_, c)) = chars.peek() {
            if !c.is_whitespace() {
                return None;
            }
        }
    }
    if attrs.is_empty() {
        return None; // `{}` — не считаем атрибутами.
    }
    Some(attrs)
}

fn is_key_char(c: char) -> bool {
    c.is_ascii_alphanumeric() || matches!(c, '_' | '-' | '.' | ':')
}

/// Отделяет от конца строки блок атрибутов: `"Заголовок {icon=🚀}"` →
/// `("Заголовок", Some(attrs))`. Если хвост не парсится — атрибутов нет.
pub fn split_trailing_attrs(text: &str) -> (String, Option<Attrs>) {
    let trimmed = text.trim_end();
    if trimmed.ends_with('}') {
        if let Some(open) = trimmed.rfind('{') {
            if let Some(attrs) = parse_attr_block(&trimmed[open..]) {
                return (trimmed[..open].trim_end().to_string(), Some(attrs));
            }
        }
    }
    (text.to_string(), None)
}

/// Отделяет блок атрибутов от начала строки: `"{width=70%} хвост"` →
/// `(Some(attrs), " хвост")`. Используется для attrs сразу после `![...](...)`.
pub fn split_leading_attrs(text: &str) -> (Option<Attrs>, String) {
    if text.starts_with('{') {
        if let Some(close) = text.find('}') {
            if let Some(attrs) = parse_attr_block(&text[..=close]) {
                return (Some(attrs), text[close + 1..].to_string());
            }
        }
    }
    (None, text.to_string())
}

/// Сериализует атрибуты в `{k=v flag k2="два слова"}`. Порядок ключей
/// детерминирован (BTreeMap). Пустой набор → пустая строка.
pub fn serialize_attrs(attrs: &Attrs) -> String {
    if attrs.is_empty() {
        return String::new();
    }
    let mut out = String::from("{");
    for (i, (key, value)) in attrs.0.iter().enumerate() {
        if i > 0 {
            out.push(' ');
        }
        out.push_str(key);
        if value.is_empty() {
            continue; // Флаг.
        }
        out.push('=');
        let needs_quotes =
            value.chars().any(|c| c.is_whitespace() || matches!(c, '"' | '{' | '}' | '\\'));
        if needs_quotes {
            out.push('"');
            for c in value.chars() {
                if matches!(c, '"' | '\\') {
                    out.push('\\');
                }
                out.push(c);
            }
            out.push('"');
        } else {
            out.push_str(value);
        }
    }
    out.push('}');
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parse_flags_and_values() {
        let a = parse_attr_block("{width=70% loop icon=🚀}").unwrap();
        assert_eq!(a.get("width"), Some("70%"));
        assert_eq!(a.get("icon"), Some("🚀"));
        assert!(a.flag("loop"));
        assert!(!a.flag("autoplay"));
    }

    #[test]
    fn parse_quoted_value() {
        let a = parse_attr_block(r#"{title="два слова" color=#e0a030}"#).unwrap();
        assert_eq!(a.get("title"), Some("два слова"));
        assert_eq!(a.get("color"), Some("#e0a030"));
    }

    #[test]
    fn reject_invalid() {
        assert!(parse_attr_block("{}").is_none());
        assert!(parse_attr_block("{не атрибуты}").is_none());
        assert!(parse_attr_block("{a=}").is_none());
        assert!(parse_attr_block("{a=\"незакрыто}").is_none());
        assert!(parse_attr_block("не блок").is_none());
    }

    #[test]
    fn trailing_and_leading() {
        let (rest, attrs) = split_trailing_attrs("Заголовок {icon=🚀}");
        assert_eq!(rest, "Заголовок");
        assert_eq!(attrs.unwrap().get("icon"), Some("🚀"));

        let (rest, attrs) = split_trailing_attrs("просто текст {со скобками не туда");
        assert_eq!(rest, "просто текст {со скобками не туда");
        assert!(attrs.is_none());

        let (attrs, rest) = split_leading_attrs("{width=50%} хвост");
        assert_eq!(attrs.unwrap().get("width"), Some("50%"));
        assert_eq!(rest, " хвост");
    }

    #[test]
    fn roundtrip() {
        for src in ["{a=1 b=два c}", "{icon=🚀 width=70%}", r#"{t="a \"b\" c"}"#] {
            let a = parse_attr_block(src).unwrap();
            let ser = serialize_attrs(&a);
            let b = parse_attr_block(&ser).unwrap();
            assert_eq!(a, b, "round-trip для {src}");
        }
    }
}
