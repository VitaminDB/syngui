/// Категория множественного числа по CLDR (только целые числа).
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum PluralCategory {
    Zero,
    One,
    Two,
    Few,
    Many,
    Other,
}

impl PluralCategory {
    /// Суффикс ключа в каталоге: `key.one`, `key.few`, …
    pub fn suffix(self) -> &'static str {
        match self {
            PluralCategory::Zero => "zero",
            PluralCategory::One => "one",
            PluralCategory::Two => "two",
            PluralCategory::Few => "few",
            PluralCategory::Many => "many",
            PluralCategory::Other => "other",
        }
    }
}

/// Семейство правил; каждое покрывает несколько языков.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum PluralRule {
    OneOther,
    ZeroOneOther,
    EastSlavic,
    Polish,
    OtherOnly,
}

const RULES: &[(&str, PluralRule)] = &[
    ("en", PluralRule::OneOther),
    ("de", PluralRule::OneOther),
    ("es", PluralRule::OneOther),
    ("it", PluralRule::OneOther),
    ("tr", PluralRule::OneOther),
    ("kk", PluralRule::OneOther),
    ("fr", PluralRule::ZeroOneOther),
    ("pt", PluralRule::ZeroOneOther),
    ("ru", PluralRule::EastSlavic),
    ("uk", PluralRule::EastSlavic),
    ("pl", PluralRule::Polish),
    ("zh", PluralRule::OtherOnly),
    ("ja", PluralRule::OtherOnly),
    ("ko", PluralRule::OtherOnly),
];

const NAMES: &[(&str, PluralRule)] = &[
    ("one-other", PluralRule::OneOther),
    ("zero-one-other", PluralRule::ZeroOneOther),
    ("east-slavic", PluralRule::EastSlavic),
    ("polish", PluralRule::Polish),
    ("other-only", PluralRule::OtherOnly),
];

impl PluralRule {
    /// Правило для базового языка (`ru`, `pt`); неизвестный язык → `OneOther`.
    pub fn for_language(base: &str) -> PluralRule {
        RULES
            .iter()
            .find(|(tag, _)| *tag == base)
            .map(|(_, rule)| *rule)
            .unwrap_or(PluralRule::OneOther)
    }

    /// Разбор имени из `@plural = "…"`.
    pub fn parse(name: &str) -> Option<PluralRule> {
        NAMES.iter().find(|(n, _)| *n == name).map(|(_, rule)| *rule)
    }

    pub fn name(self) -> &'static str {
        NAMES.iter().find(|(_, r)| *r == self).map(|(n, _)| *n).unwrap_or("one-other")
    }

    pub fn category(self, n: u64) -> PluralCategory {
        let mod10 = n % 10;
        let mod100 = n % 100;
        match self {
            PluralRule::OneOther => {
                if n == 1 { PluralCategory::One } else { PluralCategory::Other }
            }
            PluralRule::ZeroOneOther => {
                if n <= 1 { PluralCategory::One } else { PluralCategory::Other }
            }
            PluralRule::EastSlavic => {
                if mod10 == 1 && mod100 != 11 {
                    PluralCategory::One
                } else if (2..=4).contains(&mod10) && !(12..=14).contains(&mod100) {
                    PluralCategory::Few
                } else {
                    PluralCategory::Many
                }
            }
            PluralRule::Polish => {
                if n == 1 {
                    PluralCategory::One
                } else if (2..=4).contains(&mod10) && !(12..=14).contains(&mod100) {
                    PluralCategory::Few
                } else {
                    PluralCategory::Many
                }
            }
            PluralRule::OtherOnly => PluralCategory::Other,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use PluralCategory::*;

    fn cats(rule: PluralRule, ns: &[u64]) -> Vec<PluralCategory> {
        ns.iter().map(|n| rule.category(*n)).collect()
    }

    #[test]
    fn east_slavic() {
        assert_eq!(
            cats(PluralRule::EastSlavic, &[0, 1, 2, 5, 11, 21, 22, 25, 101, 111, 112, 114]),
            vec![Many, One, Few, Many, Many, One, Few, Many, One, Many, Many, Many]
        );
    }

    #[test]
    fn polish() {
        assert_eq!(
            cats(PluralRule::Polish, &[0, 1, 2, 5, 12, 21, 22, 25, 101]),
            vec![Many, One, Few, Many, Many, Many, Few, Many, Many]
        );
    }

    #[test]
    fn one_other_and_zero_one_other() {
        assert_eq!(cats(PluralRule::OneOther, &[0, 1, 2]), vec![Other, One, Other]);
        assert_eq!(cats(PluralRule::ZeroOneOther, &[0, 1, 2]), vec![One, One, Other]);
        assert_eq!(cats(PluralRule::OtherOnly, &[0, 1, 2]), vec![Other, Other, Other]);
    }

    #[test]
    fn rule_table_covers_all_shipped_languages() {
        let expected = [
            ("en", PluralRule::OneOther), ("ru", PluralRule::EastSlavic), ("de", PluralRule::OneOther),
            ("fr", PluralRule::ZeroOneOther), ("es", PluralRule::OneOther), ("it", PluralRule::OneOther),
            ("pt", PluralRule::ZeroOneOther), ("pl", PluralRule::Polish), ("uk", PluralRule::EastSlavic),
            ("kk", PluralRule::OneOther), ("tr", PluralRule::OneOther), ("zh", PluralRule::OtherOnly),
            ("ja", PluralRule::OtherOnly), ("ko", PluralRule::OtherOnly),
        ];
        for (tag, rule) in expected {
            assert_eq!(PluralRule::for_language(tag), rule, "{tag}");
        }
        assert_eq!(PluralRule::for_language("xx"), PluralRule::OneOther);
    }

    #[test]
    fn names_round_trip() {
        for (name, rule) in NAMES {
            assert_eq!(PluralRule::parse(name), Some(*rule));
            assert_eq!(rule.name(), *name);
        }
        assert_eq!(PluralRule::parse("nope"), None);
    }
}
