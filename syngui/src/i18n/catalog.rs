use super::lang::Lang;
use super::plural::PluralRule;
use std::collections::HashMap;
use std::fmt;

/// Каталог одного языка: метаданные из `@`-строк и пары `key = "value"`.
#[derive(Clone, Debug)]
pub struct Catalog {
    pub tag: Lang,
    pub name: String,
    pub english: Option<String>,
    pub plural: PluralRule,
    entries: HashMap<String, String>,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct CatalogError {
    pub line: usize,
    pub message: String,
}

impl fmt::Display for CatalogError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "line {}: {}", self.line, self.message)
    }
}

impl std::error::Error for CatalogError {}

fn err(line: usize, message: impl Into<String>) -> CatalogError {
    CatalogError { line, message: message.into() }
}

fn valid_key(key: &str) -> bool {
    !key.is_empty() && key.bytes().all(|b| b.is_ascii_alphanumeric() || matches!(b, b'_' | b'.' | b'-'))
}

fn parse_quoted(rest: &str, line: usize) -> Result<String, CatalogError> {
    let rest = rest.trim_start();
    let mut chars = rest.chars();
    if chars.next() != Some('"') {
        return Err(err(line, "value must be a double-quoted string"));
    }
    let mut value = String::new();
    loop {
        match chars.next() {
            None => return Err(err(line, "unterminated string")),
            Some('\\') => match chars.next() {
                Some('n') => value.push('\n'),
                Some('t') => value.push('\t'),
                Some('"') => value.push('"'),
                Some('\\') => value.push('\\'),
                Some(other) => return Err(err(line, format!("unknown escape \\{other}"))),
                None => return Err(err(line, "unterminated escape")),
            },
            Some('"') => break,
            Some(ch) => value.push(ch),
        }
    }
    let tail = chars.as_str().trim_start();
    if !tail.is_empty() && !tail.starts_with('#') {
        return Err(err(line, "unexpected text after closing quote"));
    }
    Ok(value)
}

impl Catalog {
    pub fn parse(text: &str) -> Result<Catalog, CatalogError> {
        let mut tag: Option<Lang> = None;
        let mut name: Option<String> = None;
        let mut english: Option<String> = None;
        let mut plural: Option<PluralRule> = None;
        let mut entries: HashMap<String, String> = HashMap::new();

        for (idx, raw) in text.lines().enumerate() {
            let line = idx + 1;
            let trimmed = raw.trim();
            if trimmed.is_empty() || trimmed.starts_with('#') {
                continue;
            }
            let Some((key, rest)) = trimmed.split_once('=') else {
                return Err(err(line, "expected `key = \"value\"`"));
            };
            let key = key.trim();
            let value = parse_quoted(rest, line)?;
            if let Some(meta) = key.strip_prefix('@') {
                match meta {
                    "tag" => {
                        tag = Some(Lang::parse(&value).ok_or_else(|| err(line, "invalid @tag"))?);
                    }
                    "name" => name = Some(value),
                    "english" => english = Some(value),
                    "plural" => {
                        plural = Some(PluralRule::parse(&value).ok_or_else(|| err(line, "unknown @plural rule"))?);
                    }
                    other => log::debug!("i18n: unknown metadata @{other} at line {line}"),
                }
                continue;
            }
            if !valid_key(key) {
                return Err(err(line, format!("invalid key `{key}`")));
            }
            if entries.insert(key.to_string(), value).is_some() {
                log::warn!("i18n: duplicate key `{key}` at line {line}, last one wins");
            }
        }

        let tag = tag.ok_or_else(|| err(0, "missing @tag"))?;
        let name = name.ok_or_else(|| err(0, "missing @name"))?;
        let plural = plural.unwrap_or_else(|| tag.plural_rule());
        Ok(Catalog { tag, name, english, plural, entries })
    }

    pub fn get(&self, key: &str) -> Option<&str> {
        self.entries.get(key).map(String::as_str)
    }

    pub fn keys(&self) -> impl Iterator<Item = &str> {
        self.entries.keys().map(String::as_str)
    }

    pub fn len(&self) -> usize {
        self.entries.len()
    }

    pub fn is_empty(&self) -> bool {
        self.entries.is_empty()
    }

    /// Вливает `other` поверх: совпадающие ключи и метаданные берутся из `other`.
    pub fn merge_from(&mut self, other: Catalog) {
        self.name = other.name;
        if other.english.is_some() {
            self.english = other.english;
        }
        self.plural = other.plural;
        self.entries.extend(other.entries);
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    const SAMPLE: &str = r#"
# comment line
@tag = "ru"
@name = "Русский"   # trailing comment
@english = "Russian"

nav.chat = "Чат"
chat.error = "Не удалось \"{path}\":\n{error}"
files.one = "{n} файл"
files.few = "{n} файла"
files.many = "{n} файлов"
"#;

    #[test]
    fn parses_metadata_and_entries() {
        let c = Catalog::parse(SAMPLE).unwrap();
        assert_eq!(c.tag.tag(), "ru");
        assert_eq!(c.name, "Русский");
        assert_eq!(c.english.as_deref(), Some("Russian"));
        assert_eq!(c.plural, PluralRule::EastSlavic);
        assert_eq!(c.get("nav.chat"), Some("Чат"));
        assert_eq!(c.get("chat.error"), Some("Не удалось \"{path}\":\n{error}"));
        assert_eq!(c.len(), 5);
    }

    #[test]
    fn plural_override_and_defaults() {
        let c = Catalog::parse("@tag = \"en\"\n@name = \"English\"\n@plural = \"polish\"\n").unwrap();
        assert_eq!(c.plural, PluralRule::Polish);
        let c = Catalog::parse("@tag = \"zh-CN\"\n@name = \"中文\"\n").unwrap();
        assert_eq!(c.plural, PluralRule::OtherOnly);
    }

    #[test]
    fn duplicate_key_last_wins() {
        let c = Catalog::parse("@tag = \"en\"\n@name = \"English\"\na = \"1\"\na = \"2\"\n").unwrap();
        assert_eq!(c.get("a"), Some("2"));
    }

    #[test]
    fn errors_carry_line_numbers() {
        let e = Catalog::parse("@tag = \"en\"\n@name = \"English\"\nbad line\n").unwrap_err();
        assert_eq!(e.line, 3);
        let e = Catalog::parse("@tag = \"en\"\n@name = \"English\"\nk = unquoted\n").unwrap_err();
        assert_eq!(e.line, 3);
        let e = Catalog::parse("@tag = \"en\"\n@name = \"English\"\nk = \"open\n").unwrap_err();
        assert_eq!(e.line, 3);
        let e = Catalog::parse("@tag = \"en\"\n@name = \"English\"\nk = \"a\" junk\n").unwrap_err();
        assert_eq!(e.line, 3);
        let e = Catalog::parse("@tag = \"en\"\n@name = \"English\"\nk = \"\\q\"\n").unwrap_err();
        assert_eq!(e.line, 3);
        let e = Catalog::parse("@tag = \"en\"\n@name = \"English\"\nbad key! = \"x\"\n").unwrap_err();
        assert_eq!(e.line, 3);
        assert_eq!(Catalog::parse("@name = \"English\"\n").unwrap_err().message, "missing @tag");
        assert_eq!(Catalog::parse("@tag = \"en\"\n").unwrap_err().message, "missing @name");
        assert_eq!(Catalog::parse("@tag = \"C\"\n@name = \"x\"\n").unwrap_err().line, 1);
    }

    #[test]
    fn merge_overrides_entries() {
        let mut base = Catalog::parse("@tag = \"en\"\n@name = \"English\"\na = \"1\"\nb = \"2\"\n").unwrap();
        let over = Catalog::parse("@tag = \"en\"\n@name = \"English (app)\"\nb = \"3\"\nc = \"4\"\n").unwrap();
        base.merge_from(over);
        assert_eq!(base.get("a"), Some("1"));
        assert_eq!(base.get("b"), Some("3"));
        assert_eq!(base.get("c"), Some("4"));
        assert_eq!(base.name, "English (app)");
    }
}
