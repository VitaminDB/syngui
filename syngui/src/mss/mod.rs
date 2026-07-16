mod parser;
mod stylesheet;
mod style_engine;
pub mod matching;
mod value;
pub mod fields;
pub mod inheritance;
pub mod cascade;
pub mod code_editor;

pub use parser::{MssParser, ParseError, ParseWarning};
pub use stylesheet::{
    StyleSheet, StyleRule, Selector, KeyframeStep, KeyframesDefinition,
    SelectorPart, Combinator, SelectorChain,
};
pub use matching::{SelectorMatchContext, selector_matches, selector_pseudo};
pub use style_engine::{StyleEngine, ComputedStyle, StyleContext, ElementState, TextAlign, TextDecoration, Overflow, window_flags};
pub use value::{StyleValue, Color as MssColor, Unit, Dimension};
pub use fields::{IconState, MssFields, TextTransform, TextShadow};
pub use inheritance::{
    INHERITED_PROPERTIES, is_inherited, resolve_cascade_keyword, extract_inherited,
};

use std::path::Path;

pub fn load_stylesheet<P: AsRef<Path>>(path: P) -> Result<StyleSheet, MssError> {
    let content = std::fs::read_to_string(path)
        .map_err(|e| MssError::Io(e))?;

    let mut parser = MssParser::new(&content);
    let (stylesheet, warnings) = parser.parse()
        .map_err(|e| MssError::Parse(e))?;
    for w in &warnings {
        eprintln!("[MSS warning] line {}: {}", w.line, w.message);
    }
    Ok(stylesheet)
}

pub fn parse_stylesheet_str(content: &str) -> Result<StyleSheet, ParseError> {
    let (stylesheet, _warnings) = MssParser::new(content).parse()?;
    Ok(stylesheet)
}

pub fn merge_stylesheet_str(base: &mut StyleSheet, content: &str) -> Result<(), ParseError> {
    let (additional, _warnings) = MssParser::new(content).parse()?;
    base.merge(&additional);
    Ok(())
}

#[derive(Debug)]
pub enum MssError {
    Io(std::io::Error),
    Parse(ParseError),
}

impl std::fmt::Display for MssError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            MssError::Io(e) => write!(f, "IO error: {}", e),
            MssError::Parse(e) => write!(f, "Parse error: {:?}", e),
        }
    }
}

impl std::error::Error for MssError {}
