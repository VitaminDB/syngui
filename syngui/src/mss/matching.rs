use super::stylesheet::*;
use crate::widget::tree::ElementId;

pub trait SelectorMatchContext {
    fn element_classes(&self, id: ElementId) -> &[String];
    fn element_type_name(&self, id: ElementId) -> &str;
    fn parent_id(&self, id: ElementId) -> Option<ElementId>;
    fn previous_sibling(&self, id: ElementId) -> Option<ElementId>;
    fn previous_siblings(&self, id: ElementId) -> Vec<ElementId>;
}

/// `#id` хранится у элемента служебным классом `#id` ([`id_class`]) —
/// у парсера в имени класса `#` не бывает, коллизий нет.
pub fn id_class(id: &str) -> String {
    format!("#{id}")
}

fn has_id(id_str: &str, id: ElementId, ctx: &impl SelectorMatchContext) -> bool {
    ctx.element_classes(id)
        .iter()
        .any(|c| c.strip_prefix('#') == Some(id_str))
}

fn part_matches(part: &SelectorPart, id: ElementId, ctx: &impl SelectorMatchContext) -> bool {
    match part {
        SelectorPart::Class(class) => ctx.element_classes(id).contains(class),
        SelectorPart::Element(elem) => ctx.element_type_name(id) == elem.as_str(),
        SelectorPart::Universal => true,
        SelectorPart::Id(want) => has_id(want, id, ctx),
        SelectorPart::Compound {
            element,
            id: want_id,
            classes,
        } => {
            if let Some(elem) = element {
                if ctx.element_type_name(id) != elem.as_str() {
                    return false;
                }
            }
            if let Some(want) = want_id {
                if !has_id(want, id, ctx) {
                    return false;
                }
            }
            for class in classes {
                if !ctx.element_classes(id).contains(class) {
                    return false;
                }
            }
            true
        }
    }
}

fn chain_matches(chain: &SelectorChain, id: ElementId, ctx: &impl SelectorMatchContext) -> bool {
    if chain.segments.is_empty() {
        return false;
    }
    if !part_matches(chain.target(), id, ctx) {
        return false;
    }
    match_left(chain, chain.combinators.len(), id, ctx)
}

/// Сегменты `0..=upto-1` левее уже совпавшего `current`. С возвратом: для
/// `.x > .y .z` мало найти ближайшего предка `.y` — если у него родитель не
/// `.x`, надо пробовать следующего `.y` выше.
fn match_left(
    chain: &SelectorChain,
    upto: usize,
    current: ElementId,
    ctx: &impl SelectorMatchContext,
) -> bool {
    if upto == 0 {
        return true;
    }
    let i = upto - 1;
    let part = &chain.segments[i];
    match &chain.combinators[i] {
        Combinator::Descendant => {
            let mut ancestor = ctx.parent_id(current);
            while let Some(anc) = ancestor {
                if part_matches(part, anc, ctx) && match_left(chain, i, anc, ctx) {
                    return true;
                }
                ancestor = ctx.parent_id(anc);
            }
            false
        }
        Combinator::Child => match ctx.parent_id(current) {
            Some(parent) => part_matches(part, parent, ctx) && match_left(chain, i, parent, ctx),
            None => false,
        },
        Combinator::AdjacentSibling => match ctx.previous_sibling(current) {
            Some(prev) => part_matches(part, prev, ctx) && match_left(chain, i, prev, ctx),
            None => false,
        },
        Combinator::GeneralSibling => ctx
            .previous_siblings(current)
            .into_iter()
            .any(|sib| part_matches(part, sib, ctx) && match_left(chain, i, sib, ctx)),
    }
}

pub fn selector_matches(
    selector: &Selector,
    id: ElementId,
    ctx: &impl SelectorMatchContext,
) -> bool {
    match selector {
        Selector::Class(c) => ctx.element_classes(id).contains(c),
        Selector::ClassPseudo(c, _) => ctx.element_classes(id).contains(c),
        Selector::Element(e) => ctx.element_type_name(id) == e.as_str(),
        Selector::ElementPseudo(e, _) => ctx.element_type_name(id) == e.as_str(),
        Selector::Universal => true,
        Selector::Id(want) => has_id(want, id, ctx),
        Selector::Complex(chain) => chain_matches(chain, id, ctx),
        Selector::Group(chains) => chains.iter().any(|c| chain_matches(c, id, ctx)),
    }
}

pub fn selector_pseudo(selector: &Selector) -> Option<&str> {
    selector.pseudo()
}

#[cfg(test)]
mod tests {
    use super::*;

    struct MockTree {
        nodes: Vec<(ElementId, Vec<String>, String, Option<ElementId>)>,
        children: Vec<(ElementId, Vec<ElementId>)>,
    }

    impl MockTree {
        fn new() -> Self {
            Self {
                nodes: vec![],
                children: vec![],
            }
        }

        fn add(&mut self, id: u64, classes: &[&str], elem_type: &str, parent: Option<u64>) {
            let eid = ElementId(id);
            let parent_id = parent.map(ElementId);
            self.nodes.push((
                eid,
                classes.iter().map(|s| s.to_string()).collect(),
                elem_type.to_string(),
                parent_id,
            ));

            if let Some(pid) = parent_id {
                if let Some(entry) = self.children.iter_mut().find(|(p, _)| *p == pid) {
                    entry.1.push(eid);
                } else {
                    self.children.push((pid, vec![eid]));
                }
            }
        }

        fn find(
            &self,
            id: ElementId,
        ) -> Option<&(ElementId, Vec<String>, String, Option<ElementId>)> {
            self.nodes.iter().find(|(eid, _, _, _)| *eid == id)
        }
    }

    impl SelectorMatchContext for MockTree {
        fn element_classes(&self, id: ElementId) -> &[String] {
            self.find(id)
                .map(|(_, c, _, _)| c.as_slice())
                .unwrap_or(&[])
        }

        fn element_type_name(&self, id: ElementId) -> &str {
            self.find(id).map(|(_, _, t, _)| t.as_str()).unwrap_or("")
        }

        fn parent_id(&self, id: ElementId) -> Option<ElementId> {
            self.find(id).and_then(|(_, _, _, p)| *p)
        }

        fn previous_sibling(&self, id: ElementId) -> Option<ElementId> {
            let parent = self.parent_id(id)?;
            let children = self.children.iter().find(|(p, _)| *p == parent)?;
            let pos = children.1.iter().position(|&c| c == id)?;
            if pos > 0 {
                Some(children.1[pos - 1])
            } else {
                None
            }
        }

        fn previous_siblings(&self, id: ElementId) -> Vec<ElementId> {
            let parent = match self.parent_id(id) {
                Some(p) => p,
                None => return vec![],
            };
            let children = match self.children.iter().find(|(p, _)| *p == parent) {
                Some(c) => &c.1,
                None => return vec![],
            };
            let pos = match children.iter().position(|&c| c == id) {
                Some(p) => p,
                None => return vec![],
            };
            children[..pos].iter().rev().copied().collect()
        }
    }

    fn make_tree() -> MockTree {
        let mut t = MockTree::new();
        t.add(1, &["app"], "", None);
        t.add(2, &["card"], "", Some(1));
        t.add(3, &["title"], "Button", Some(2));
        t.add(4, &["subtitle"], "", Some(2));
        t.add(5, &["card"], "", Some(1));
        t.add(6, &["item"], "Button", Some(5));
        t
    }

    fn parsed(sel: &str) -> Selector {
        let css = format!("{sel} {{ color: red; }}");
        let (sheet, _) = crate::mss::MssParser::new(&css).parse().unwrap();
        sheet.rules()[0].selector.clone()
    }

    #[test]
    fn descendant_backtracks_to_farther_ancestor() {
        // x > y > (div) > y > z: ближайший `.y` к `.z` — у него родитель не `.x`,
        // совпадение даёт дальний `.y`.
        let mut t = MockTree::new();
        t.add(1, &["x"], "", None);
        t.add(2, &["y"], "", Some(1));
        t.add(3, &[], "", Some(2));
        t.add(4, &["y"], "", Some(3));
        t.add(5, &["z"], "", Some(4));
        assert!(selector_matches(&parsed(".x > .y .z"), ElementId(5), &t));
        assert!(!selector_matches(&parsed(".q > .y .z"), ElementId(5), &t));
    }

    #[test]
    fn id_selector_matches_id_class() {
        let mut t = MockTree::new();
        let idc = id_class("main");
        t.add(1, &["panel", idc.as_str()], "Column", None);
        t.add(2, &["label"], "Text", Some(1));
        assert!(selector_matches(&parsed("#main"), ElementId(1), &t));
        assert!(!selector_matches(&parsed("#other"), ElementId(1), &t));
        assert!(selector_matches(&parsed("#main .label"), ElementId(2), &t));
        assert!(selector_matches(&parsed("Column#main.panel"), ElementId(1), &t));
    }

    #[test]
    fn test_simple_class_match() {
        let tree = make_tree();
        let sel = Selector::Class("card".to_string());
        assert!(selector_matches(&sel, ElementId(2), &tree));
        assert!(selector_matches(&sel, ElementId(5), &tree));
        assert!(!selector_matches(&sel, ElementId(3), &tree));
    }

    #[test]
    fn test_simple_element_match() {
        let tree = make_tree();
        let sel = Selector::Element("Button".to_string());
        assert!(selector_matches(&sel, ElementId(3), &tree));
        assert!(selector_matches(&sel, ElementId(6), &tree));
        assert!(!selector_matches(&sel, ElementId(2), &tree));
    }

    #[test]
    fn test_descendant_combinator() {
        let tree = make_tree();
        let sel = Selector::Complex(SelectorChain {
            segments: vec![
                SelectorPart::Class("card".to_string()),
                SelectorPart::Class("title".to_string()),
            ],
            combinators: vec![Combinator::Descendant],
            pseudo: None,
            leading_combinator: None,
        });
        assert!(selector_matches(&sel, ElementId(3), &tree));
        assert!(!selector_matches(&sel, ElementId(4), &tree));
        assert!(!selector_matches(&sel, ElementId(2), &tree));
    }

    #[test]
    fn test_descendant_through_multiple_levels() {
        let tree = make_tree();
        let sel = Selector::Complex(SelectorChain {
            segments: vec![
                SelectorPart::Class("app".to_string()),
                SelectorPart::Class("title".to_string()),
            ],
            combinators: vec![Combinator::Descendant],
            pseudo: None,
            leading_combinator: None,
        });
        assert!(selector_matches(&sel, ElementId(3), &tree));
    }

    #[test]
    fn test_child_combinator() {
        let tree = make_tree();
        let sel = Selector::Complex(SelectorChain {
            segments: vec![
                SelectorPart::Class("card".to_string()),
                SelectorPart::Class("title".to_string()),
            ],
            combinators: vec![Combinator::Child],
            pseudo: None,
            leading_combinator: None,
        });
        assert!(selector_matches(&sel, ElementId(3), &tree));

        let sel2 = Selector::Complex(SelectorChain {
            segments: vec![
                SelectorPart::Class("app".to_string()),
                SelectorPart::Class("title".to_string()),
            ],
            combinators: vec![Combinator::Child],
            pseudo: None,
            leading_combinator: None,
        });
        assert!(!selector_matches(&sel2, ElementId(3), &tree));
    }

    #[test]
    fn test_adjacent_sibling() {
        let tree = make_tree();
        let sel = Selector::Complex(SelectorChain {
            segments: vec![
                SelectorPart::Class("title".to_string()),
                SelectorPart::Class("subtitle".to_string()),
            ],
            combinators: vec![Combinator::AdjacentSibling],
            pseudo: None,
            leading_combinator: None,
        });
        assert!(selector_matches(&sel, ElementId(4), &tree));
        assert!(!selector_matches(&sel, ElementId(3), &tree));
    }

    #[test]
    fn test_general_sibling() {
        let tree = make_tree();
        let sel = Selector::Complex(SelectorChain {
            segments: vec![
                SelectorPart::Class("card".to_string()),
                SelectorPart::Class("card".to_string()),
            ],
            combinators: vec![Combinator::GeneralSibling],
            pseudo: None,
            leading_combinator: None,
        });
        assert!(selector_matches(&sel, ElementId(5), &tree));
        assert!(!selector_matches(&sel, ElementId(2), &tree));
    }

    #[test]
    fn test_group_selector() {
        let tree = make_tree();
        let sel = Selector::Group(vec![
            SelectorChain::simple(SelectorPart::Class("title".to_string())),
            SelectorChain::simple(SelectorPart::Class("item".to_string())),
        ]);
        assert!(selector_matches(&sel, ElementId(3), &tree));
        assert!(selector_matches(&sel, ElementId(6), &tree));
        assert!(!selector_matches(&sel, ElementId(2), &tree));
    }

    #[test]
    fn test_three_level_chain() {
        let tree = make_tree();
        let sel = Selector::Complex(SelectorChain {
            segments: vec![
                SelectorPart::Class("app".to_string()),
                SelectorPart::Class("card".to_string()),
                SelectorPart::Class("title".to_string()),
            ],
            combinators: vec![Combinator::Child, Combinator::Descendant],
            pseudo: None,
            leading_combinator: None,
        });
        assert!(selector_matches(&sel, ElementId(3), &tree));
    }

    #[test]
    fn test_element_in_class() {
        let tree = make_tree();
        let sel = Selector::Complex(SelectorChain {
            segments: vec![
                SelectorPart::Class("card".to_string()),
                SelectorPart::Element("Button".to_string()),
            ],
            combinators: vec![Combinator::Descendant],
            pseudo: None,
            leading_combinator: None,
        });
        assert!(selector_matches(&sel, ElementId(3), &tree));
        assert!(selector_matches(&sel, ElementId(6), &tree));
    }

    #[test]
    fn test_universal_selector() {
        let tree = make_tree();
        let sel = Selector::Universal;
        assert!(selector_matches(&sel, ElementId(1), &tree));
        assert!(selector_matches(&sel, ElementId(3), &tree));
    }

    #[test]
    fn test_pseudo_preserved() {
        let sel = Selector::Complex(SelectorChain {
            segments: vec![
                SelectorPart::Class("card".to_string()),
                SelectorPart::Class("title".to_string()),
            ],
            combinators: vec![Combinator::Descendant],
            pseudo: Some("hover".to_string()),
            leading_combinator: None,
        });
        assert_eq!(selector_pseudo(&sel), Some("hover"));
    }
}
