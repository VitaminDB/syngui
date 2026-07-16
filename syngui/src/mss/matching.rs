use super::stylesheet::*;
use crate::widget::tree::ElementId;

pub trait SelectorMatchContext {
    fn element_classes(&self, id: ElementId) -> &[String];
    fn element_type_name(&self, id: ElementId) -> &str;
    fn parent_id(&self, id: ElementId) -> Option<ElementId>;
    fn previous_sibling(&self, id: ElementId) -> Option<ElementId>;
    fn previous_siblings(&self, id: ElementId) -> Vec<ElementId>;
}

fn part_matches(part: &SelectorPart, id: ElementId, ctx: &impl SelectorMatchContext) -> bool {
    match part {
        SelectorPart::Class(class) => ctx.element_classes(id).contains(class),
        SelectorPart::Element(elem) => ctx.element_type_name(id) == elem.as_str(),
        SelectorPart::Universal => true,
        SelectorPart::Id(_) => false,
        SelectorPart::Compound { element, id: _id, classes } => {
            if let Some(elem) = element {
                if ctx.element_type_name(id) != elem.as_str() {
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

    let target = chain.target();
    if !part_matches(target, id, ctx) {
        return false;
    }

    if chain.segments.len() == 1 {
        return true;
    }

    let mut current_id = id;
    for i in (0..chain.combinators.len()).rev() {
        let combinator = &chain.combinators[i];
        let required_part = &chain.segments[i];

        match combinator {
            Combinator::Descendant => {
                let mut found = false;
                let mut ancestor = ctx.parent_id(current_id);
                while let Some(anc_id) = ancestor {
                    if part_matches(required_part, anc_id, ctx) {
                        current_id = anc_id;
                        found = true;
                        break;
                    }
                    ancestor = ctx.parent_id(anc_id);
                }
                if !found { return false; }
            }
            Combinator::Child => {
                match ctx.parent_id(current_id) {
                    Some(parent) if part_matches(required_part, parent, ctx) => {
                        current_id = parent;
                    }
                    _ => return false,
                }
            }
            Combinator::AdjacentSibling => {
                match ctx.previous_sibling(current_id) {
                    Some(prev) if part_matches(required_part, prev, ctx) => {
                        current_id = prev;
                    }
                    _ => return false,
                }
            }
            Combinator::GeneralSibling => {
                let siblings = ctx.previous_siblings(current_id);
                let mut found = false;
                for sib_id in siblings {
                    if part_matches(required_part, sib_id, ctx) {
                        current_id = sib_id;
                        found = true;
                        break;
                    }
                }
                if !found { return false; }
            }
        }
    }

    true
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
        Selector::Id(_) => false,
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
            Self { nodes: vec![], children: vec![] }
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

        fn find(&self, id: ElementId) -> Option<&(ElementId, Vec<String>, String, Option<ElementId>)> {
            self.nodes.iter().find(|(eid, _, _, _)| *eid == id)
        }
    }

    impl SelectorMatchContext for MockTree {
        fn element_classes(&self, id: ElementId) -> &[String] {
            self.find(id).map(|(_, c, _, _)| c.as_slice()).unwrap_or(&[])
        }

        fn element_type_name(&self, id: ElementId) -> &str {
            self.find(id).map(|(_, _, t, _)| t.as_str()).unwrap_or("")
        }

        fn parent_id(&self, id: ElementId) -> Option<ElementId> {
            self.find(id).and_then(|(_, _, _, p)| *p)
        }

        fn previous_sibling(&self, id: ElementId) -> Option<ElementId> {
            let parent = self.parent_id(id)?;
            let children = self.children.iter()
                .find(|(p, _)| *p == parent)?;
            let pos = children.1.iter().position(|&c| c == id)?;
            if pos > 0 { Some(children.1[pos - 1]) } else { None }
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
