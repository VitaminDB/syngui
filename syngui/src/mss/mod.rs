pub mod cascade;
pub mod code_editor;
pub mod fields;
pub mod inheritance;
pub mod matching;
mod parser;
mod style_engine;
mod stylesheet;
mod value;

pub use fields::{IconState, MssFields, TextShadow, TextTransform};
pub use inheritance::{
    extract_inherited, is_inherited, resolve_cascade_keyword, INHERITED_PROPERTIES,
};
pub use matching::{selector_matches, selector_pseudo, SelectorMatchContext};
pub use parser::{MssParser, ParseError, ParseWarning};
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
    let content = std::fs::read_to_string(path).map_err(|e| MssError::Io(e))?;

    let mut parser = MssParser::new(&content);
    let (stylesheet, warnings) = parser.parse().map_err(|e| MssError::Parse(e))?;
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
