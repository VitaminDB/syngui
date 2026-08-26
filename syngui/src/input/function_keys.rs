//! Функциональные клавиши F1–F12 в веб-сборке: что перехватывает
//! приложение, а что остаётся браузеру.
//!
//! winit на web вызывает `preventDefault()` для всех клавиш, пришедших на
//! canvas, — иначе Tab, Backspace и пробел уводили бы фокус и прокручивали
//! страницу. Побочный эффект: F5 перестаёт перезагружать страницу, F11 — не
//! разворачивает браузер во весь экран, F12 — не открывает инструменты
//! разработчика.
//!
//! [`FunctionKeys`] описывает набор клавиш, которые приложение забирает себе.
//! Остальные F1–F12 фреймворк останавливает на стадии capture у `window`,
//! до canvas, и браузер выполняет своё действие. Набор задаётся при запуске
//! ([`AppBuilder::capture_function_keys`](crate::app::AppBuilder::capture_function_keys))
//! и может меняться на лету — [`set_captured_function_keys`].
//!
//! На native и Android приложение получает все клавиши, настройка ни на что
//! не влияет.

use std::cell::Cell;
use std::fmt;
use std::ops::{BitOr, BitOrAssign};

use super::keyboard::Key;

/// Порядок клавиш соответствует битам 0..=11.
const ORDER: [Key; 12] = [
    Key::F1,
    Key::F2,
    Key::F3,
    Key::F4,
    Key::F5,
    Key::F6,
    Key::F7,
    Key::F8,
    Key::F9,
    Key::F10,
    Key::F11,
    Key::F12,
];

/// Набор функциональных клавиш F1–F12 (битовая маска).
#[derive(Clone, Copy, PartialEq, Eq, Hash, Default)]
pub struct FunctionKeys(u16);

impl FunctionKeys {
    /// Ни одной клавиши: все F1–F12 остаются браузеру.
    pub const NONE: Self = Self(0);
    /// Все двенадцать клавиш перехватывает приложение.
    pub const ALL: Self = Self(0x0FFF);

    /// Набор из перечисленных клавиш; не-функциональные клавиши игнорируются.
    pub fn of(keys: &[Key]) -> Self {
        keys.iter().fold(Self::NONE, |acc, &k| acc.with(k))
    }

    /// Добавить клавишу (не-функциональная клавиша ничего не меняет).
    pub fn with(self, key: Key) -> Self {
        match Self::bit(key) {
            Some(bit) => Self(self.0 | bit),
            None => self,
        }
    }

    /// Убрать клавишу.
    pub fn without(self, key: Key) -> Self {
        match Self::bit(key) {
            Some(bit) => Self(self.0 & !bit),
            None => self,
        }
    }

    pub fn contains(self, key: Key) -> bool {
        Self::bit(key).is_some_and(|bit| self.0 & bit != 0)
    }

    pub fn is_empty(self) -> bool {
        self.0 == 0
    }

    /// Клавиши набора в порядке F1..F12.
    pub fn iter(self) -> impl Iterator<Item = Key> {
        ORDER
            .iter()
            .enumerate()
            .filter(move |(i, _)| self.0 & (1 << i) != 0)
            .map(|(_, k)| *k)
    }

    /// Клавиша по значению `KeyboardEvent.code` браузера (`"F1"`..`"F12"`).
    /// Для остальных кодов — `None`.
    pub fn key_from_code(code: &str) -> Option<Key> {
        let n: usize = code.strip_prefix('F')?.parse().ok()?;
        (1..=12).contains(&n).then(|| ORDER[n - 1])
    }

    fn bit(key: Key) -> Option<u16> {
        ORDER.iter().position(|&k| k == key).map(|i| 1 << i)
    }
}

impl BitOr for FunctionKeys {
    type Output = Self;
    fn bitor(self, rhs: Self) -> Self {
        Self(self.0 | rhs.0)
    }
}

impl BitOrAssign for FunctionKeys {
    fn bitor_assign(&mut self, rhs: Self) {
        self.0 |= rhs.0;
    }
}

impl fmt::Debug for FunctionKeys {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_list().entries(self.iter()).finish()
    }
}

thread_local! {
    /// Текущая политика: клавиши, которые получает приложение.
    static CAPTURED: Cell<u16> = const { Cell::new(0) };
}

/// Задать набор клавиш F1–F12, которые перехватывает приложение (web).
/// Действует немедленно: слушатель читает политику при каждом нажатии.
pub fn set_captured_function_keys(keys: FunctionKeys) {
    CAPTURED.with(|c| c.set(keys.0));
}

/// Текущий набор клавиш F1–F12, которые перехватывает приложение (web).
pub fn captured_function_keys() -> FunctionKeys {
    FunctionKeys(CAPTURED.with(|c| c.get()))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn of_contains_without() {
        let keys = FunctionKeys::of(&[Key::F5, Key::F11, Key::A]);
        assert!(keys.contains(Key::F5));
        assert!(keys.contains(Key::F11));
        assert!(!keys.contains(Key::F12));
        assert!(!keys.contains(Key::A));
        let keys = keys.without(Key::F5);
        assert!(!keys.contains(Key::F5));
        assert!(keys.contains(Key::F11));
        assert!(keys.without(Key::F11).is_empty());
    }

    #[test]
    fn all_and_none() {
        assert!(FunctionKeys::NONE.is_empty());
        assert_eq!(FunctionKeys::ALL.iter().count(), 12);
        for key in ORDER {
            assert!(FunctionKeys::ALL.contains(key));
            assert!(!FunctionKeys::NONE.contains(key));
        }
        assert_eq!(FunctionKeys::default(), FunctionKeys::NONE);
    }

    #[test]
    fn bitor_merges() {
        let a = FunctionKeys::of(&[Key::F1]);
        let b = FunctionKeys::of(&[Key::F12]);
        let mut merged = a | b;
        assert_eq!(merged.iter().collect::<Vec<_>>(), vec![Key::F1, Key::F12]);
        merged |= FunctionKeys::of(&[Key::F3]);
        assert_eq!(
            merged.iter().collect::<Vec<_>>(),
            vec![Key::F1, Key::F3, Key::F12]
        );
    }

    #[test]
    fn key_from_code_parses_only_function_keys() {
        assert_eq!(FunctionKeys::key_from_code("F1"), Some(Key::F1));
        assert_eq!(FunctionKeys::key_from_code("F12"), Some(Key::F12));
        assert_eq!(FunctionKeys::key_from_code("F13"), None);
        assert_eq!(FunctionKeys::key_from_code("F0"), None);
        assert_eq!(FunctionKeys::key_from_code("KeyF"), None);
        assert_eq!(FunctionKeys::key_from_code("Fx"), None);
        assert_eq!(FunctionKeys::key_from_code(""), None);
    }

    #[test]
    fn debug_lists_keys() {
        let keys = FunctionKeys::of(&[Key::F11, Key::F2]);
        assert_eq!(format!("{keys:?}"), "[F2, F11]");
    }

    #[test]
    fn runtime_policy_roundtrip() {
        set_captured_function_keys(FunctionKeys::of(&[Key::F7]));
        assert!(captured_function_keys().contains(Key::F7));
        assert!(!captured_function_keys().contains(Key::F5));
        set_captured_function_keys(FunctionKeys::NONE);
        assert!(captured_function_keys().is_empty());
    }
}
