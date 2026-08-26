use std::fmt::Display;

fn placeholder_at(template: &str, open: usize) -> Option<&str> {
    let rest = &template[open + 1..];
    let close = rest.find('}')?;
    let name = &rest[..close];
    if name.is_empty() || !name.bytes().all(|b| b.is_ascii_alphanumeric() || b == b'_') {
        return None;
    }
    Some(name)
}

/// Подстановка `{name}` из `args`; неизвестные плейсхолдеры и одиночные `{` остаются как есть.
pub fn substitute(template: &str, args: &[(&str, &dyn Display)]) -> String {
    let mut out = String::with_capacity(template.len() + 16);
    let mut rest = template;
    while let Some(open) = rest.find('{') {
        out.push_str(&rest[..open]);
        match placeholder_at(rest, open) {
            Some(name) => {
                match args.iter().find(|(n, _)| *n == name) {
                    Some((_, value)) => out.push_str(&value.to_string()),
                    None => {
                        out.push('{');
                        out.push_str(name);
                        out.push('}');
                    }
                }
                rest = &rest[open + name.len() + 2..];
            }
            None => {
                out.push('{');
                rest = &rest[open + 1..];
            }
        }
    }
    out.push_str(rest);
    out
}

/// Имена плейсхолдеров в шаблоне, в порядке появления, без повторов.
pub fn placeholders(template: &str) -> Vec<&str> {
    let mut names: Vec<&str> = Vec::new();
    let mut rest = template;
    while let Some(open) = rest.find('{') {
        match placeholder_at(rest, open) {
            Some(name) => {
                if !names.contains(&name) {
                    names.push(name);
                }
                rest = &rest[open + name.len() + 2..];
            }
            None => rest = &rest[open + 1..],
        }
    }
    names
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn substitutes_named_args() {
        let s = substitute("Hello, {name}! {n} of {n}", &[("name", &"Ann"), ("n", &3)]);
        assert_eq!(s, "Hello, Ann! 3 of 3");
    }

    #[test]
    fn leaves_unknown_and_literal_braces() {
        assert_eq!(substitute("{a} {b}", &[("a", &1)]), "1 {b}");
        assert_eq!(substitute("x { y } {1,2} {", &[]), "x { y } {1,2} {");
        assert_eq!(substitute("{}", &[]), "{}");
    }

    #[test]
    fn lists_placeholders_once() {
        assert_eq!(placeholders("{n} of {total} ({n})"), vec!["n", "total"]);
        assert!(placeholders("no braces { here }").is_empty());
    }
}
