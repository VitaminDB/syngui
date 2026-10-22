pub mod cascade;
pub mod code_editor;
mod defaults;
pub mod fields;
pub mod inheritance;
pub mod matching;
mod parser;
pub(crate) use parser::expand_shorthand;
pub(crate) use parser::transform::normalize_angle;
mod style_engine;
mod stylesheet;
mod value;

/// Толерантный разбор без потерь со span'ами (для редактора тем, LSP).
pub use mss_lang::syntax;

pub use fields::{IconState, MssFields, TextShadow, TextTransform};
pub use inheritance::{
    extract_inherited, is_inherited, resolve_cascade_keyword, INHERITED_PROPERTIES,
};
pub use matching::{selector_matches, selector_pseudo, SelectorMatchContext};
pub use parser::{line_col, MssParser, ParseError, ParseErrorKind, ParseWarning};
pub use style_engine::{
    window_flags, ComputedStyle, ElementState, Overflow, StyleContext, StyleEngine, TextAlign,
    TextDecoration,
};
pub use stylesheet::{
    Combinator, KeyframeStep, KeyframesDefinition, Selector, SelectorChain, SelectorPart,
    StyleRule, StyleSheet,
};
pub use value::{Color as MssColor, Dimension, StyleValue, Unit};

use std::path::Path;

pub fn load_stylesheet<P: AsRef<Path>>(path: P) -> Result<StyleSheet, MssError> {
    let path = path.as_ref();
    let content = std::fs::read_to_string(path).map_err(MssError::Io)?;
    let name = path.display().to_string();
    parse_stylesheet_named(&content, &name).map_err(MssError::Parse)
}

/// Разбор таблицы стилей с восстановлением после ошибок. Ошибки и замечания
/// уходят в журнал (`log::warn`) с позицией; правило с ошибкой пропускается.
/// `Err` не бывает — тип оставлен ради совместимости.
pub fn parse_stylesheet_str(content: &str) -> Result<StyleSheet, ParseError> {
    parse_logged(content, None)
}

/// [`parse_stylesheet_str`] с именем источника (файл, тема) в журнале.
pub fn parse_stylesheet_named(content: &str, source: &str) -> Result<StyleSheet, ParseError> {
    parse_logged(content, Some(source))
}

pub fn merge_stylesheet_str(base: &mut StyleSheet, content: &str) -> Result<(), ParseError> {
    let additional = parse_logged(content, None)?;
    base.merge(&additional);
    Ok(())
}

/// [`merge_stylesheet_str`] с именем источника в журнале.
pub fn merge_stylesheet_named(
    base: &mut StyleSheet,
    content: &str,
    source: &str,
) -> Result<(), ParseError> {
    let additional = parse_logged(content, Some(source))?;
    base.merge(&additional);
    Ok(())
}

fn parse_logged(content: &str, source: Option<&str>) -> Result<StyleSheet, ParseError> {
    let (stylesheet, warnings) = MssParser::new(content).parse()?;
    log_warnings(&warnings, content, source);
    Ok(stylesheet)
}

/// Предупреждения разбора — в журнал, каждое один раз за процесс: тема
/// пересобирается на каждое изменение (редактор тем — на каждую клавишу),
/// и одно и то же замечание не должно сыпаться в журнал повторно.
fn log_warnings(warnings: &[ParseWarning], content: &str, source: Option<&str>) {
    use std::collections::HashSet;
    use std::hash::{Hash, Hasher};
    use std::sync::{Mutex, OnceLock};

    if warnings.is_empty() {
        return;
    }
    static SEEN: OnceLock<Mutex<HashSet<u64>>> = OnceLock::new();
    let seen = SEEN.get_or_init(|| Mutex::new(HashSet::new()));
    for w in warnings {
        // Ключ — источник, сообщение и строка текста с ошибкой: правка
        // соседних строк не повторяет то же замечание.
        let line_text = content.lines().nth(w.line).unwrap_or("");
        let mut h = std::collections::hash_map::DefaultHasher::new();
        (source, &w.message, line_text.trim()).hash(&mut h);
        let fresh = match seen.lock() {
            Ok(mut set) => {
                if set.len() > 4096 {
                    set.clear();
                }
                set.insert(h.finish())
            }
            Err(_) => true,
        };
        if !fresh {
            continue;
        }
        match source {
            Some(name) => log::warn!(
                "MSS {name}:{}:{}: {}",
                w.line + 1,
                w.column + 1,
                w.message
            ),
            None => log::warn!("MSS: строка {}, столбец {}: {}", w.line + 1, w.column + 1, w.message),
        }
    }
}

#[derive(Debug)]
pub enum MssError {
    Io(std::io::Error),
    Parse(ParseError),
}

impl std::fmt::Display for MssError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            MssError::Io(e) => write!(f, "ошибка чтения: {}", e),
            MssError::Parse(e) => write!(f, "ошибка разбора: {}", e),
        }
    }
}

impl std::error::Error for MssError {}
