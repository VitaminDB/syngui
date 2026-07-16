use super::value::StyleValue;
use std::collections::HashMap;

#[derive(Debug, Clone)]
pub struct KeyframeStep {
    pub position: f32,
    pub declarations: HashMap<String, StyleValue>,
}

#[derive(Debug, Clone)]
pub struct KeyframesDefinition {
    pub name: String,
    pub steps: Vec<KeyframeStep>,
}

#[derive(Debug, Clone, PartialEq)]
pub enum SelectorPart {
    Class(String),
    Element(String),
    Universal,
    Id(String),
    Compound {
        element: Option<String>,
        id: Option<String>,
        classes: Vec<String>,
    },
}

impl SelectorPart {
    pub fn specificity(&self) -> (u32, u32, u32) {
        match self {
            SelectorPart::Id(_) => (1, 0, 0),
            SelectorPart::Class(_) => (0, 1, 0),
            SelectorPart::Element(_) => (0, 0, 1),
            SelectorPart::Universal => (0, 0, 0),
            SelectorPart::Compound { element, id, classes } => {
                let ids = if id.is_some() { 1 } else { 0 };
                let cls = classes.len() as u32;
                let elems = if element.is_some() { 1 } else { 0 };
                (ids, cls, elems)
            }
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Combinator {
    Descendant,
    Child,
    AdjacentSibling,
    GeneralSibling,
}

#[derive(Debug, Clone, PartialEq)]
pub struct SelectorChain {
    pub segments: Vec<SelectorPart>,
    pub combinators: Vec<Combinator>,
    pub pseudo: Option<String>,
    pub leading_combinator: Option<Combinator>,
}

impl SelectorChain {
    pub fn simple(part: SelectorPart) -> Self {
        Self {
            segments: vec![part],
            combinators: vec![],
            pseudo: None,
            leading_combinator: None,
        }
    }

    pub fn simple_pseudo(part: SelectorPart, pseudo: String) -> Self {
        Self {
            segments: vec![part],
            combinators: vec![],
            pseudo: Some(pseudo),
            leading_combinator: None,
        }
    }

    pub fn specificity(&self) -> (u32, u32, u32) {
        let mut s = (0u32, 0u32, 0u32);
        for seg in &self.segments {
            let (a, b, c) = seg.specificity();
            s.0 += a;
            s.1 += b;
            s.2 += c;
        }
        if self.pseudo.is_some() {
            s.1 += 1;
        }
        s
    }

    pub fn target(&self) -> &SelectorPart {
        self.segments.last().expect("SelectorChain must have at least one segment")
    }

    pub fn is_simple(&self) -> bool {
        self.segments.len() == 1
    }

    pub fn pseudo(&self) -> Option<&str> {
        self.pseudo.as_deref()
    }
}

#[derive(Debug, Clone, Default)]
pub struct StyleSheet {
    variables: HashMap<String, StyleValue>,
    rules: Vec<StyleRule>,
    keyframes: HashMap<String, KeyframesDefinition>,
}

impl StyleSheet {
    pub fn new() -> Self {
        Self {
            variables: HashMap::new(),
            rules: Vec::new(),
            keyframes: HashMap::new(),
        }
    }

    pub fn set_variable(&mut self, name: String, value: StyleValue) {
        self.variables.insert(name, value);
    }

    pub fn get_variable(&self, name: &str) -> Option<&StyleValue> {
        self.variables.get(name)
    }

    pub fn add_rule(&mut self, rule: StyleRule) {
        self.rules.push(rule);
    }

    pub fn rules(&self) -> &[StyleRule] {
        &self.rules
    }

    pub fn find_class_styles(&self, class: &str) -> Option<&StyleRule> {
        self.rules.iter()
            .find(|r| matches!(&r.selector, Selector::Class(c) if c == class))
    }

    pub fn find_class_pseudo_styles(&self, class: &str, pseudo: &str) -> Option<&StyleRule> {
        self.rules.iter()
            .find(|r| matches!(&r.selector, Selector::ClassPseudo(c, p) if c == class && p == pseudo))
    }

    pub fn find_element_styles(&self, element_type: &str) -> Option<&StyleRule> {
        self.rules.iter()
            .find(|r| matches!(&r.selector, Selector::Element(e) if e == element_type))
    }

    pub fn find_element_pseudo_styles(&self, element_type: &str, pseudo: &str) -> Option<&StyleRule> {
        self.rules.iter()
            .find(|r| matches!(&r.selector, Selector::ElementPseudo(e, p) if e == element_type && p == pseudo))
    }

    pub fn add_keyframes(&mut self, keyframes: KeyframesDefinition) {
        self.keyframes.insert(keyframes.name.clone(), keyframes);
    }

    pub fn get_keyframes(&self, name: &str) -> Option<&KeyframesDefinition> {
        self.keyframes.get(name)
    }

    pub fn all_keyframes(&self) -> &HashMap<String, KeyframesDefinition> {
        &self.keyframes
    }

    pub fn merge(&mut self, other: &StyleSheet) {
        for (name, value) in &other.variables {
            self.variables.insert(name.clone(), value.clone());
        }
        self.rules.extend(other.rules.iter().cloned());
        for (name, kf) in &other.keyframes {
            self.keyframes.insert(name.clone(), kf.clone());
        }
    }
}

#[derive(Debug, Clone)]
pub struct StyleRule {
    pub selector: Selector,
    pub selector_str: String,
    pub declarations: HashMap<String, StyleValue>,
}

impl StyleRule {
    pub fn get(&self, property: &str) -> Option<&StyleValue> {
        self.declarations.get(property)
    }
}

#[derive(Debug, Clone, PartialEq)]
pub enum Selector {
    Class(String),
    ClassPseudo(String, String),
    Element(String),
    ElementPseudo(String, String),
    Universal,
    Id(String),
    Complex(SelectorChain),
    Group(Vec<SelectorChain>),
}

impl Selector {
    pub fn specificity(&self) -> (u32, u32, u32) {
        match self {
            Selector::Id(_) => (1, 0, 0),
            Selector::Class(_) => (0, 1, 0),
            Selector::ClassPseudo(_, _) => (0, 2, 0),
            Selector::Element(_) => (0, 0, 1),
            Selector::ElementPseudo(_, _) => (0, 1, 1),
            Selector::Universal => (0, 0, 0),
            Selector::Complex(chain) => chain.specificity(),
            Selector::Group(chains) => {
                chains.iter()
                    .map(|c| c.specificity())
                    .max()
                    .unwrap_or((0, 0, 0))
            }
        }
    }

    pub fn matches_class(&self, class: &str) -> bool {
        match self {
            Selector::Class(c) => c == class,
            Selector::ClassPseudo(c, _) => c == class,
            _ => false,
        }
    }

    pub fn pseudo(&self) -> Option<&str> {
        match self {
            Selector::ClassPseudo(_, p) | Selector::ElementPseudo(_, p) => Some(p.as_str()),
            Selector::Complex(chain) => chain.pseudo(),
            _ => None,
        }
    }

    pub fn to_chain(&self) -> Option<SelectorChain> {
        match self {
            Selector::Class(c) => Some(SelectorChain::simple(SelectorPart::Class(c.clone()))),
            Selector::ClassPseudo(c, p) => Some(SelectorChain::simple_pseudo(
                SelectorPart::Class(c.clone()), p.clone(),
            )),
            Selector::Element(e) => Some(SelectorChain::simple(SelectorPart::Element(e.clone()))),
            Selector::ElementPseudo(e, p) => Some(SelectorChain::simple_pseudo(
                SelectorPart::Element(e.clone()), p.clone(),
            )),
            Selector::Universal => Some(SelectorChain::simple(SelectorPart::Universal)),
            Selector::Id(id) => Some(SelectorChain::simple(SelectorPart::Id(id.clone()))),
            Selector::Complex(chain) => Some(chain.clone()),
            Selector::Group(_) => None,
        }
    }

    pub fn chains(&self) -> Vec<SelectorChain> {
        match self {
            Selector::Group(chains) => chains.clone(),
            other => other.to_chain().into_iter().collect(),
        }
    }
}

impl PartialOrd for Selector {
    fn partial_cmp(&self, other: &Self) -> Option<std::cmp::Ordering> {
        self.specificity().partial_cmp(&other.specificity())
    }
}
