//! Локализация интерфейса: каталоги `key = "value"`, текущий язык как сигнал,
//! `tr!`/`trn!` внутри билдеров виджетов перестраивают поддерево при смене языка.

pub mod catalog;
pub mod format;
pub mod lang;
pub mod plural;
pub mod system;

pub use catalog::{Catalog, CatalogError};
pub use lang::Lang;
pub use plural::{PluralCategory, PluralRule};
pub use system::system_language;

use crate::signal::{self, use_signal, RwSignal};
use std::collections::HashSet;
use std::fmt::Display;
use std::sync::atomic::{AtomicU64, Ordering};
use std::cell::OnceCell;
use std::sync::{Mutex, OnceLock};

const BUILTIN: &[&str] = &[
    include_str!("../../i18n/en.lang"),
    include_str!("../../i18n/ru.lang"),
    include_str!("../../i18n/de.lang"),
    include_str!("../../i18n/fr.lang"),
    include_str!("../../i18n/es.lang"),
    include_str!("../../i18n/it.lang"),
    include_str!("../../i18n/pt-BR.lang"),
    include_str!("../../i18n/pl.lang"),
    include_str!("../../i18n/uk.lang"),
    include_str!("../../i18n/kk.lang"),
    include_str!("../../i18n/tr.lang"),
    include_str!("../../i18n/zh-CN.lang"),
    include_str!("../../i18n/ja.lang"),
    include_str!("../../i18n/ko.lang"),
];

/// Язык, доступный для выбора: тег и название на самом языке.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct LangInfo {
    pub tag: Lang,
    pub name: String,
    pub english: Option<String>,
}

struct Registry {
    app: Vec<Catalog>,
    builtin: Vec<Catalog>,
    requested: Lang,
    chain: Vec<Lang>,
    missing: HashSet<String>,
}

impl Registry {
    fn new() -> Self {
        let builtin = BUILTIN
            .iter()
            .filter_map(|text| match Catalog::parse(text) {
                Ok(c) => Some(c),
                Err(e) => {
                    log::error!("i18n: builtin catalog failed to parse: {e}");
                    None
                }
            })
            .collect();
        let mut reg = Registry {
            app: Vec::new(),
            builtin,
            requested: Lang::en(),
            chain: Vec::new(),
            missing: HashSet::new(),
        };
        reg.recompute_chain();
        reg
    }

    fn available(&self) -> Vec<Lang> {
        let mut tags: Vec<Lang> = self.app.iter().map(|c| c.tag.clone()).collect();
        for c in &self.builtin {
            if !tags.contains(&c.tag) {
                tags.push(c.tag.clone());
            }
        }
        tags
    }

    fn recompute_chain(&mut self) {
        let available = self.available();
        let mut chain: Vec<Lang> = Vec::new();
        if let Some(resolved) = lang::resolve(&self.requested, &available) {
            chain.push(resolved);
        }
        let base = Lang::new(self.requested.base());
        if available.contains(&base) && !chain.contains(&base) {
            chain.push(base);
        }
        if !chain.contains(&Lang::en()) {
            chain.push(Lang::en());
        }
        self.chain = chain;
    }

    fn catalogs_for<'a>(&'a self, lang: &'a Lang) -> impl Iterator<Item = &'a Catalog> + 'a {
        self.app.iter().chain(self.builtin.iter()).filter(move |c| &c.tag == lang)
    }

    fn lookup(&self, key: &str) -> Option<String> {
        self.chain
            .iter()
            .find_map(|lang| self.catalogs_for(lang).find_map(|c| c.get(key)).map(str::to_string))
    }

    fn lookup_plural(&self, key: &str, n: u64) -> Option<String> {
        for lang in &self.chain {
            for cat in self.catalogs_for(lang) {
                let form = format!("{key}.{}", cat.plural.category(n).suffix());
                let other = format!("{key}.other");
                if let Some(v) = cat.get(&form).or_else(|| cat.get(&other)).or_else(|| cat.get(key)) {
                    return Some(v.to_string());
                }
            }
        }
        None
    }

    fn effective(&self) -> Lang {
        self.chain.first().cloned().unwrap_or_else(Lang::en)
    }
}

static REGISTRY: OnceLock<Mutex<Registry>> = OnceLock::new();
static REVISION_COUNTER: AtomicU64 = AtomicU64::new(0);

thread_local! {
    static REVISION: OnceCell<RwSignal<u64>> = const { OnceCell::new() };
}

fn registry() -> &'static Mutex<Registry> {
    REGISTRY.get_or_init(|| Mutex::new(Registry::new()))
}

fn with_registry<R>(f: impl FnOnce(&mut Registry) -> R) -> R {
    let mut guard = match registry().lock() {
        Ok(g) => g,
        Err(poisoned) => poisoned.into_inner(),
    };
    f(&mut guard)
}

fn set_revision_on_this_thread(next: u64) {
    REVISION.with(|cell| {
        if let Some(sig) = cell.get() {
            sig.set(next);
        }
    });
}

fn bump_revision() {
    let next = REVISION_COUNTER.fetch_add(1, Ordering::SeqCst) + 1;
    if signal::is_main_thread() {
        set_revision_on_this_thread(next);
    } else {
        crate::async_runtime::run_on_main_thread(move || set_revision_on_this_thread(next));
    }
}

/// Подписывает текущий `Reactive`-элемент на смену языка. Вызывать из билдера виджетов.
pub fn subscribe() {
    if signal::is_main_thread() {
        REVISION.with(|cell| {
            let sig = cell.get_or_init(|| use_signal(REVISION_COUNTER.load(Ordering::SeqCst)));
            let _ = sig.get();
        });
    }
}

/// Регистрирует каталог приложения; каталог с тем же тегом дополняется (поздний побеждает).
pub fn register_catalog(text: &'static str) -> Result<Lang, CatalogError> {
    let catalog = Catalog::parse(text)?;
    let tag = catalog.tag.clone();
    with_registry(|reg| {
        match reg.app.iter_mut().find(|c| c.tag == tag) {
            Some(existing) => existing.merge_from(catalog),
            None => reg.app.push(catalog),
        }
        reg.recompute_chain();
    });
    bump_revision();
    Ok(tag)
}

/// Регистрирует несколько каталогов; ошибки разбора логируются, файл пропускается.
pub fn register_catalogs(texts: &[&'static str]) {
    for text in texts {
        if let Err(e) = register_catalog(text) {
            log::error!("i18n: catalog skipped: {e}");
        }
    }
}

/// Меняет язык интерфейса; все подписанные поддеревья перестраиваются.
pub fn set_language(lang: impl Into<Lang>) {
    let lang = lang.into();
    let (changed, effective) = with_registry(|reg| {
        let changed = reg.requested != lang;
        reg.requested = lang;
        reg.recompute_chain();
        (changed, reg.effective())
    });
    if !changed {
        return;
    }
    sync_calendar_locale(&effective);
    bump_revision();
}

#[cfg(not(test))]
fn sync_calendar_locale(effective: &Lang) {
    let locale = crate::widgets::visual::calendar::CalendarLocale::from_id(effective.tag())
        .unwrap_or_else(crate::widgets::visual::calendar::CalendarLocale::english);
    crate::widgets::visual::calendar::set_default_locale(locale);
}

#[cfg(test)]
fn sync_calendar_locale(_effective: &Lang) {}

/// Эффективный язык (после разрешения по доступным каталогам).
pub fn language() -> Lang {
    subscribe();
    with_registry(|reg| reg.effective())
}

/// Язык, запрошенный через `set_language`, до разрешения.
pub fn requested_language() -> Lang {
    with_registry(|reg| reg.requested.clone())
}

/// Языки для переключателя: каталоги приложения, а если их нет — встроенные.
pub fn languages() -> Vec<LangInfo> {
    let mut list = with_registry(|reg| {
        let source = if reg.app.is_empty() { &reg.builtin } else { &reg.app };
        source
            .iter()
            .map(|c| LangInfo { tag: c.tag.clone(), name: c.name.clone(), english: c.english.clone() })
            .collect::<Vec<_>>()
    });
    list.sort_by(|a, b| a.tag.cmp(&b.tag));
    list
}

/// Перевод ключа; при отсутствии возвращает сам ключ (и один раз пишет в лог).
pub fn tr(key: &str) -> String {
    subscribe();
    with_registry(|reg| {
        reg.lookup(key).unwrap_or_else(|| {
            if reg.missing.insert(key.to_string()) {
                log::debug!("i18n: missing key `{key}`");
            }
            key.to_string()
        })
    })
}

/// Перевод без фолбэка на ключ — для динамических ключей.
pub fn try_tr(key: &str) -> Option<String> {
    subscribe();
    with_registry(|reg| reg.lookup(key))
}

pub fn tr_args(key: &str, args: &[(&str, &dyn Display)]) -> String {
    format::substitute(&tr(key), args)
}

/// Форма по числу `n`; `{n}` подставляется автоматически.
pub fn trn(key: &str, n: u64) -> String {
    trn_args(key, n, &[])
}

pub fn trn_args(key: &str, n: u64, args: &[(&str, &dyn Display)]) -> String {
    subscribe();
    let template = with_registry(|reg| {
        reg.lookup_plural(key, n).unwrap_or_else(|| {
            if reg.missing.insert(key.to_string()) {
                log::debug!("i18n: missing plural key `{key}`");
            }
            key.to_string()
        })
    });
    let mut all: Vec<(&str, &dyn Display)> = Vec::with_capacity(args.len() + 1);
    all.push(("n", &n));
    all.extend_from_slice(args);
    format::substitute(&template, &all)
}

/// Строка встроенного виджета: каталог (приложение поверх встроенного) или литерал.
pub(crate) fn builtin(key: &str, fallback: &str) -> String {
    subscribe();
    with_registry(|reg| reg.lookup(key)).unwrap_or_else(|| fallback.to_string())
}

pub(crate) fn builtin_args(key: &str, fallback: &str, args: &[(&str, &dyn Display)]) -> String {
    format::substitute(&builtin(key, fallback), args)
}

#[macro_export]
macro_rules! tr {
    ($key:expr $(,)?) => {
        $crate::i18n::tr($key)
    };
    ($key:expr, $($name:ident = $value:expr),+ $(,)?) => {
        $crate::i18n::tr_args($key, &[$((stringify!($name), &$value as &dyn ::std::fmt::Display)),+])
    };
}

#[macro_export]
macro_rules! trn {
    ($key:expr, $n:expr $(,)?) => {
        $crate::i18n::trn($key, $n as u64)
    };
    ($key:expr, $n:expr, $($name:ident = $value:expr),+ $(,)?) => {
        $crate::i18n::trn_args($key, $n as u64, &[$((stringify!($name), &$value as &dyn ::std::fmt::Display)),+])
    };
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::testing::TestHarness;
    use crate::widget::Text;
    use crate::widgets::Reactive;
    use std::sync::atomic::AtomicUsize;
    use std::sync::Arc;

    const EN: &str = "@tag = \"en\"\n@name = \"English\"\ntest.hello = \"Hello, {name}\"\ntest.only_en = \"only en\"\ntest.files.one = \"{n} file\"\ntest.files.other = \"{n} files\"\n";
    const RU: &str = "@tag = \"ru\"\n@name = \"Русский\"\ntest.hello = \"Привет, {name}\"\ntest.files.one = \"{n} файл\"\ntest.files.few = \"{n} файла\"\ntest.files.many = \"{n} файлов\"\n";

    static SERIAL: Mutex<()> = Mutex::new(());

    fn setup() -> std::sync::MutexGuard<'static, ()> {
        let guard = SERIAL.lock().unwrap_or_else(|p| p.into_inner());
        register_catalogs(&[EN, RU]);
        set_language("en");
        guard
    }

    #[test]
    fn falls_back_through_chain_to_key() {
        let _serial = setup();
        set_language("ru");
        assert_eq!(tr!("test.hello", name = "Аня"), "Привет, Аня");
        assert_eq!(tr("test.only_en"), "only en");
        assert_eq!(tr("test.nope"), "test.nope");
        assert_eq!(try_tr("test.nope"), None);
        set_language("xx");
        assert_eq!(language().tag(), "en");
        assert_eq!(requested_language().tag(), "xx");
        set_language("en");
    }

    #[test]
    fn plural_forms_follow_catalog_rule() {
        let _serial = setup();
        set_language("ru");
        assert_eq!(trn!("test.files", 1), "1 файл");
        assert_eq!(trn!("test.files", 3), "3 файла");
        assert_eq!(trn!("test.files", 11), "11 файлов");
        set_language("en");
        assert_eq!(trn!("test.files", 1), "1 file");
        assert_eq!(trn!("test.files", 2), "2 files");
    }

    #[test]
    fn builtin_strings_and_language_list() {
        let _serial = setup();
        set_language("en");
        assert_eq!(builtin("dialog.ok", "x"), "OK");
        assert_eq!(builtin("nope.nope", "literal"), "literal");
        let tags: Vec<String> = languages().iter().map(|l| l.tag.to_string()).collect();
        assert_eq!(tags, vec!["en", "ru"]);
        assert_eq!(languages()[1].name, "Русский");
    }

    #[test]
    fn worker_thread_can_translate() {
        let _serial = setup();
        let s = std::thread::spawn(|| tr("test.only_en")).join().unwrap();
        assert_eq!(s, "only en");
    }

    #[test]
    fn reactive_subtree_rebuilds_on_language_change() {
        let _serial = setup();
        set_language("en");
        let builds = Arc::new(AtomicUsize::new(0));
        let counter = builds.clone();
        let widget = Reactive::new(move || {
            counter.fetch_add(1, Ordering::SeqCst);
            vec![Box::new(Text::new(tr("test.only_en"))) as Box<dyn crate::widget::Widget>]
        });
        let mut harness = TestHarness::new(Box::new(widget));
        harness.rebuild();
        let before = builds.load(Ordering::SeqCst);
        assert!(before >= 1);
        harness.rebuild();
        assert_eq!(builds.load(Ordering::SeqCst), before);
        set_language("ru");
        harness.rebuild();
        assert_eq!(builds.load(Ordering::SeqCst), before + 1);
        set_language("en");
    }
}
