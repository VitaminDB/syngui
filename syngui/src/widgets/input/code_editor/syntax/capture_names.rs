use std::str::FromStr;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum TokenClass {
    Keyword,
    KeywordControl,
    Type,
    TypeBuiltin,
    Function,
    FunctionMacro,
    Constant,
    ConstantBuiltin,
    String,
    StringSpecial,
    Number,
    Comment,
    Operator,
    Punctuation,
    Variable,
    Property,
    Attribute,
    Tag,
    Namespace,
}

impl TokenClass {
    pub fn dotted(&self) -> &'static str {
        match self {
            TokenClass::Keyword => "keyword",
            TokenClass::KeywordControl => "keyword.control",
            TokenClass::Type => "type",
            TokenClass::TypeBuiltin => "type.builtin",
            TokenClass::Function => "function",
            TokenClass::FunctionMacro => "function.macro",
            TokenClass::Constant => "constant",
            TokenClass::ConstantBuiltin => "constant.builtin",
            TokenClass::String => "string",
            TokenClass::StringSpecial => "string.special",
            TokenClass::Number => "number",
            TokenClass::Comment => "comment",
            TokenClass::Operator => "operator",
            TokenClass::Punctuation => "punctuation",
            TokenClass::Variable => "variable",
            TokenClass::Property => "property",
            TokenClass::Attribute => "attribute",
            TokenClass::Tag => "tag",
            TokenClass::Namespace => "namespace",
        }
    }

    pub fn from_synoptic_kind(kind: &str) -> Option<TokenClass> {
        Some(match kind {
            "comment" => TokenClass::Comment,
            "string" | "character" => TokenClass::String,
            "operator" => TokenClass::Operator,
            "digit" | "number" => TokenClass::Number,
            "boolean" => TokenClass::ConstantBuiltin,
            "function" => TokenClass::Function,
            "macro" | "macros" => TokenClass::FunctionMacro,
            "namespace" => TokenClass::Namespace,
            "attribute" => TokenClass::Attribute,
            "type" | "struct" => TokenClass::Type,
            "keyword" => TokenClass::Keyword,
            "header" | "headers" => TokenClass::Keyword,
            "link" => TokenClass::StringSpecial,
            "tag" => TokenClass::Tag,
            "bold" | "italic" => TokenClass::Keyword,
            "blockquote" | "block_quote" => TokenClass::Comment,
            "key" => TokenClass::Property,
            "table" | "section" => TokenClass::Namespace,
            "reference" => TokenClass::Operator,
            "decorator" => TokenClass::Attribute,
            "self" | "this" => TokenClass::Variable,
            "selector" => TokenClass::Tag,
            "property" => TokenClass::Property,
            _ => return None,
        })
    }
}

impl FromStr for TokenClass {
    type Err = ();
    fn from_str(s: &str) -> Result<Self, Self::Err> {
        TokenClass::from_synoptic_kind(s).ok_or(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn dotted_roundtrip() {
        assert_eq!(TokenClass::Keyword.dotted(), "keyword");
        assert_eq!(TokenClass::KeywordControl.dotted(), "keyword.control");
        assert_eq!(TokenClass::FunctionMacro.dotted(), "function.macro");
    }

    #[test]
    fn from_synoptic_basic() {
        assert_eq!(
            TokenClass::from_synoptic_kind("keyword"),
            Some(TokenClass::Keyword)
        );
        assert_eq!(
            TokenClass::from_synoptic_kind("string"),
            Some(TokenClass::String)
        );
        assert_eq!(
            TokenClass::from_synoptic_kind("digit"),
            Some(TokenClass::Number)
        );
        assert_eq!(
            TokenClass::from_synoptic_kind("macro"),
            Some(TokenClass::FunctionMacro)
        );
    }

    #[test]
    fn from_synoptic_unknown_returns_none() {
        assert_eq!(TokenClass::from_synoptic_kind("nope"), None);
        assert_eq!(TokenClass::from_synoptic_kind(""), None);
    }
}
