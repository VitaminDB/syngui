use super::plural::PluralRule;
use std::fmt;

/// Нормализованный языковой тег: `ru`, `pt-BR`, `zh-CN`.
#[derive(Clone, Debug, PartialEq, Eq, Hash, PartialOrd, Ord)]
pub struct Lang(String);

impl Lang {
    /// Разбирает `ru_RU.UTF-8`, `pt_br`, `zh-Hans-CN`, `en`; `C`/`POSIX`/пусто → `None`.
    pub fn parse(raw: &str) -> Option<Lang> {
        let raw = raw.trim();
        let raw = raw.split(['.', '@']).next().unwrap_or("");
        let mut parts = raw.split(['-', '_']).filter(|p| !p.is_empty());
        let language = parts.next()?;
        if !(2..=3).contains(&language.len()) || !language.chars().all(|c| c.is_ascii_alphabetic()) {
            return None;
        }
        let mut tag = language.to_ascii_lowercase();
        for part in parts {
            let is_script = part.len() == 4 && part.chars().all(|c| c.is_ascii_alphabetic());
            if is_script {
                continue;
            }
            let is_region = (part.len() == 2 && part.chars().all(|c| c.is_ascii_alphabetic()))
                || (part.len() == 3 && part.chars().all(|c| c.is_ascii_digit()));
            if is_region {
                tag.push('-');
                tag.push_str(&part.to_ascii_uppercase());
            }
            break;
        }
        Some(Lang(tag))
    }

    /// `parse`, а при неудаче — английский.
    pub fn new(raw: &str) -> Lang {
        Lang::parse(raw).unwrap_or_else(Lang::en)
    }

    pub fn en() -> Lang {
        Lang("en".to_string())
    }

    pub fn tag(&self) -> &str {
        &self.0
    }

    /// Язык без региона: `pt-BR` → `pt`.
    pub fn base(&self) -> &str {
        self.0.split('-').next().unwrap_or(&self.0)
    }

    pub fn region(&self) -> Option<&str> {
        self.0.split_once('-').map(|(_, r)| r)
    }

    pub fn plural_rule(&self) -> PluralRule {
        PluralRule::for_language(self.base())
    }
}

impl fmt::Display for Lang {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(&self.0)
    }
}

impl From<&str> for Lang {
    fn from(raw: &str) -> Self {
        Lang::new(raw)
    }
}

impl From<String> for Lang {
    fn from(raw: String) -> Self {
        Lang::new(&raw)
    }
}

/// Лучшее совпадение среди доступных: точный тег → тот же базовый язык → ничего.
pub fn resolve(requested: &Lang, available: &[Lang]) -> Option<Lang> {
    if available.contains(requested) {
        return Some(requested.clone());
    }
    available.iter().find(|l| l.base() == requested.base()).cloned()
}

#[cfg(test)]
mod tests {
    use super::*;

    fn langs(tags: &[&str]) -> Vec<Lang> {
        tags.iter().map(|t| Lang::new(t)).collect()
    }

    #[test]
    fn parses_posix_and_bcp47_forms() {
        assert_eq!(Lang::parse("ru_RU.UTF-8").unwrap().tag(), "ru-RU");
        assert_eq!(Lang::parse("pt_br").unwrap().tag(), "pt-BR");
        assert_eq!(Lang::parse("zh-Hans-CN").unwrap().tag(), "zh-CN");
        assert_eq!(Lang::parse("zh-Hans").unwrap().tag(), "zh");
        assert_eq!(Lang::parse("EN").unwrap().tag(), "en");
        assert_eq!(Lang::parse("kk_KZ@cyrillic").unwrap().tag(), "kk-KZ");
        assert_eq!(Lang::parse("es-419").unwrap().tag(), "es-419");
    }

    #[test]
    fn rejects_c_posix_and_garbage() {
        assert!(Lang::parse("C").is_none());
        assert!(Lang::parse("POSIX").is_none());
        assert!(Lang::parse("").is_none());
        assert!(Lang::parse("C.UTF-8").is_none());
        assert!(Lang::parse("12").is_none());
        assert_eq!(Lang::new("C").tag(), "en");
    }

    #[test]
    fn base_and_region() {
        let l = Lang::new("pt-BR");
        assert_eq!(l.base(), "pt");
        assert_eq!(l.region(), Some("BR"));
        assert_eq!(Lang::new("ru").region(), None);
    }

    #[test]
    fn resolves_exact_then_base() {
        let available = langs(&["en", "ru", "pt-BR", "zh-CN"]);
        assert_eq!(resolve(&Lang::new("ru"), &available).unwrap().tag(), "ru");
        assert_eq!(resolve(&Lang::new("ru_RU.UTF-8"), &available).unwrap().tag(), "ru");
        assert_eq!(resolve(&Lang::new("pt-PT"), &available).unwrap().tag(), "pt-BR");
        assert_eq!(resolve(&Lang::new("zh-TW"), &available).unwrap().tag(), "zh-CN");
        assert_eq!(resolve(&Lang::new("zh"), &available).unwrap().tag(), "zh-CN");
        assert!(resolve(&Lang::new("de"), &available).is_none());
    }
}
