use crate::widget::{ElementId, ElementTree};
use super::stylesheet::StyleRule;
use super::style_engine::{StyleEngine, ComputedStyle};
use super::value::StyleValue;
use super::matching::{selector_matches, selector_pseudo};
use super::inheritance::{extract_inherited, resolve_cascade_keyword};

#[inline]
fn resolve_for_cascade(
    engine: &StyleEngine,
    value: &StyleValue,
    property: &str,
    parent_inherited: &ComputedStyle,
) -> StyleValue {
    match value {
        StyleValue::Inherit | StyleValue::Initial | StyleValue::Unset => {
            resolve_cascade_keyword(value, property, parent_inherited)
        }
        _ => engine.resolve_variable(value),
    }
}

#[inline]
fn window_pseudo_matches(pseudo: &str, window_flags: u8) -> Option<bool> {
    use super::style_engine::window_flags as wf;
    let flag = match pseudo {
        "window-maximized"  => wf::MAXIMIZED,
        "window-fullscreen" => wf::FULLSCREEN,
        "window-focused"    => wf::FOCUSED,
        _ => return None,
    };
    Some(window_flags & flag != 0)
}

/// Индекс правил по правому сегменту селектора.
///
/// Раньше каскад проверял КАЖДОЕ правило на КАЖДОМ элементе — при ~1000
/// правил и нескольких тысячах элементов это миллионы вызовов
/// selector_matches на один проход стилей (главное узкое место рантайма).
/// Индекс раскладывает правила по вёдрам: класс/тип правого сегмента —
/// кандидаты для элемента берутся только из вёдер его классов, его типа и
/// catch-all. Полная проверка совпадения остаётся за selector_matches.
struct RuleIndex<'a> {
    by_class: std::collections::HashMap<&'a str, Vec<u32>>,
    by_type: std::collections::HashMap<&'a str, Vec<u32>>,
    catch_all: Vec<u32>,
}

impl<'a> RuleIndex<'a> {
    fn build(rules: &'a [StyleRule]) -> Self {
        use super::stylesheet::{Selector, SelectorChain, SelectorPart};

        let mut by_class: std::collections::HashMap<&str, Vec<u32>> =
            std::collections::HashMap::new();
        let mut by_type: std::collections::HashMap<&str, Vec<u32>> =
            std::collections::HashMap::new();
        let mut catch_all: Vec<u32> = Vec::new();

        fn slot_chain<'a>(
            chain: &'a SelectorChain,
            i: u32,
            by_class: &mut std::collections::HashMap<&'a str, Vec<u32>>,
            by_type: &mut std::collections::HashMap<&'a str, Vec<u32>>,
            catch_all: &mut Vec<u32>,
        ) {
            match chain.target() {
                SelectorPart::Class(c) => by_class.entry(c.as_str()).or_default().push(i),
                SelectorPart::Element(e) => by_type.entry(e.as_str()).or_default().push(i),
                SelectorPart::Compound { classes, element, .. } => {
                    // Compound требует ВСЕ свои классы, поэтому ведро любого
                    // из них корректно сужает кандидатов; берём первый.
                    if let Some(c) = classes.first() {
                        by_class.entry(c.as_str()).or_default().push(i);
                    } else if let Some(e) = element {
                        by_type.entry(e.as_str()).or_default().push(i);
                    } else {
                        catch_all.push(i);
                    }
                }
                SelectorPart::Universal | SelectorPart::Id(_) => catch_all.push(i),
            }
        }

        for (i, rule) in rules.iter().enumerate() {
            let i = i as u32;
            match &rule.selector {
                Selector::Class(c) | Selector::ClassPseudo(c, _) => {
                    by_class.entry(c.as_str()).or_default().push(i)
                }
                Selector::Element(e) | Selector::ElementPseudo(e, _) => {
                    by_type.entry(e.as_str()).or_default().push(i)
                }
                Selector::Universal | Selector::Id(_) => catch_all.push(i),
                Selector::Complex(chain) => {
                    slot_chain(chain, i, &mut by_class, &mut by_type, &mut catch_all)
                }
                Selector::Group(chains) => {
                    // Правило попадает в ведро каждой цепочки; дубли снимает
                    // sort+dedup в candidates().
                    for chain in chains {
                        slot_chain(chain, i, &mut by_class, &mut by_type, &mut catch_all);
                    }
                }
            }
        }

        Self { by_class, by_type, catch_all }
    }

    /// Индексы правил-кандидатов для элемента, отсортированные и без дублей.
    fn candidates(&self, classes: &[String], type_name: &str, out: &mut Vec<u32>) {
        out.clear();
        out.extend_from_slice(&self.catch_all);
        if !type_name.is_empty() {
            if let Some(v) = self.by_type.get(type_name) {
                out.extend_from_slice(v);
            }
        }
        for cls in classes {
            // Часть builder-ов хранит классы одной строкой с пробелами.
            for token in cls.split_whitespace() {
                if let Some(v) = self.by_class.get(token) {
                    out.extend_from_slice(v);
                }
            }
        }
        out.sort_unstable();
        out.dedup();
    }
}

fn dfs_order(tree: &ElementTree, root: ElementId) -> Vec<ElementId> {
    let mut order = Vec::with_capacity(tree.elements.len());
    let mut stack: Vec<ElementId> = vec![root];
    while let Some(id) = stack.pop() {
        order.push(id);
        if let Some(node) = tree.elements.get(&id) {
            for &c in node.children.iter().rev() {
                stack.push(c);
            }
        }
    }
    order
}

pub fn apply_styles_to_tree(tree: &mut ElementTree, style_engine: &StyleEngine) {
    let root_id = match tree.root_id {
        Some(r) => r,
        None => return,
    };
    let order = dfs_order(tree, root_id);
    let rules: &[StyleRule] = style_engine.stylesheet().rules();
    let index = RuleIndex::build(rules);
    let mut cand: Vec<u32> = Vec::new();
    let window_flags = tree.window_flags;

    let mut inherited_for: std::collections::HashMap<ElementId, ComputedStyle> =
        std::collections::HashMap::with_capacity(order.len());

    for id in order {
        let parent_inh = tree.elements.get(&id)
            .and_then(|n| n.parent)
            .and_then(|p| inherited_for.get(&p).cloned())
            .unwrap_or_default();

        let (has_identity, has_inline, type_name, classes) = if let Some(node) = tree.elements.get(&id) {
            (
                !node.element.get_classes().is_empty() || !node.element.element_type_name().is_empty(),
                !node.inline_styles.is_empty(),
                node.element.element_type_name().to_string(),
                node.element.get_classes().to_vec(),
            )
        } else {
            inherited_for.insert(id, parent_inh);
            continue;
        };

        let mut base = parent_inh.clone();
        let mut hover = ComputedStyle::default();
        let mut active = ComputedStyle::default();
        let mut focus = ComputedStyle::default();
        let mut selected = ComputedStyle::default();
        let mut checked = ComputedStyle::default();
        let mut has_hover = false;
        let mut has_active = false;
        let mut has_focus = false;
        let mut has_selected = false;
        let mut has_checked = false;
        let mut has_base = base.properties().next().is_some();

        if has_identity || has_inline {
            index.candidates(&classes, &type_name, &mut cand);
            let mut matching: Vec<(usize, (u32, u32, u32), &StyleRule)> = cand.iter()
                .map(|&i| (i as usize, &rules[i as usize]))
                .filter(|(_, rule)| selector_matches(&rule.selector, id, tree))
                .map(|(i, rule)| (i, rule.selector.specificity(), rule))
                .collect();
            matching.sort_by(|a, b| a.1.cmp(&b.1).then(a.0.cmp(&b.0)));

            for (_, _spec, rule) in &matching {
                let pseudo = selector_pseudo(&rule.selector);
                match pseudo {
                    None => {
                        has_base = true;
                        for (prop, val) in &rule.declarations {
                            base.set(prop, resolve_for_cascade(style_engine, val, prop, &parent_inh));
                        }
                    }
                    Some("hover") => {
                        has_hover = true;
                        for (prop, val) in &rule.declarations {
                            hover.set(prop, resolve_for_cascade(style_engine, val, prop, &parent_inh));
                        }
                    }
                    Some("checked") => {
                        has_checked = true;
                        for (prop, val) in &rule.declarations {
                            checked.set(prop, resolve_for_cascade(style_engine, val, prop, &parent_inh));
                        }
                    }
                    Some("active") | Some("pressed") => {
                        has_active = true;
                        for (prop, val) in &rule.declarations {
                            active.set(prop, resolve_for_cascade(style_engine, val, prop, &parent_inh));
                        }
                    }
                    Some("focus") => {
                        has_focus = true;
                        for (prop, val) in &rule.declarations {
                            focus.set(prop, resolve_for_cascade(style_engine, val, prop, &parent_inh));
                        }
                    }
                    Some("selected") => {
                        has_selected = true;
                        for (prop, val) in &rule.declarations {
                            selected.set(prop, resolve_for_cascade(style_engine, val, prop, &parent_inh));
                        }
                    }
                    Some(p) => {
                        match window_pseudo_matches(p, window_flags) {
                            Some(true) => {
                                has_base = true;
                                for (prop, val) in &rule.declarations {
                                    base.set(prop, resolve_for_cascade(style_engine, val, prop, &parent_inh));
                                }
                            }
                            Some(false) => {  }
                            None => {
                                has_base = true;
                                for (prop, val) in &rule.declarations {
                                    base.set(prop, resolve_for_cascade(style_engine, val, prop, &parent_inh));
                                }
                            }
                        }
                    }
                }
            }

            if has_inline {
                if let Some(node) = tree.elements.get(&id) {
                    let inline = node.inline_styles.clone();
                    has_base = true;
                    for (prop, val) in &inline {
                        base.set(prop, resolve_for_cascade(style_engine, val, prop, &parent_inh));
                    }
                }
            }

            log::trace!(
                target: "syngui::mss::cascade",
                "id={:?} type={} matched={} base_props={}",
                id, type_name, matching.len(),
                base.properties().count(),
            );
        }

        let _ = (has_active, has_focus, has_selected, has_checked);

        if has_base || has_hover || has_active || has_focus || has_selected || has_checked {
            if let Some(node) = tree.elements.get_mut(&id) {
                node.element.reset_mss_styles();
                node.element.apply_computed_style(&base);
                let hover_full = if has_hover { Some(merge_layer(&base, &hover)) } else { None };
                let active_full = if has_active { Some(merge_layer(&base, &active)) } else { None };
                let focus_full = if has_focus { Some(merge_layer(&base, &focus)) } else { None };
                let selected_full = if has_selected { Some(merge_layer(&base, &selected)) } else { None };
                let checked_full = if has_checked { Some(merge_layer(&base, &checked)) } else { None };
                node.element.apply_transition_styles(
                    &base,
                    hover_full.as_ref(),
                    active_full.as_ref(),
                    focus_full.as_ref(),
                    selected_full.as_ref(),
                    checked_full.as_ref(),
                );
                node.element.setup_keyframe_animation(&base, style_engine.stylesheet());
                node.mss_margin_set = base.has_margin();
                node.mss_margin = base.margin();
                node.mss_flex_grow = base.flex_grow().unwrap_or(0.0);
                node.had_mss_rules = true;
                node.styles_dirty = false;
                node.refresh_hint_cache();
            }
            // Стили могли запустить keyframe-анимацию или transition.
            tree.note_animation_started(id);
        } else {
            if let Some(node) = tree.elements.get_mut(&id) {
                if node.had_mss_rules || base.properties().next().is_some() {
                    node.element.reset_mss_styles();
                    node.element.apply_computed_style(&base);
                    node.had_mss_rules = base.properties().next().is_some();
                    node.styles_dirty = false;
                    node.refresh_hint_cache();
                }
            }
        }

        inherited_for.insert(id, extract_inherited(&base));
    }
}

#[inline]
fn merge_layer(base: &ComputedStyle, layer: &ComputedStyle) -> ComputedStyle {
    let mut out = base.clone();
    for (prop, val) in layer.properties() {
        out.set(prop, val.clone());
    }
    out
}

pub fn apply_styles_dirty(tree: &mut ElementTree, style_engine: &StyleEngine) -> bool {
    let root_id = match tree.root_id {
        Some(r) => r,
        None => return false,
    };

    let any_dirty = tree.elements.iter().any(|(_, n)| n.styles_dirty);
    if !any_dirty {
        return false;
    }

    let order = dfs_order(tree, root_id);
    let rules: &[StyleRule] = style_engine.stylesheet().rules();
    let index = RuleIndex::build(rules);
    let mut cand: Vec<u32> = Vec::new();
    let window_flags = tree.window_flags;

    // Наследуемые стили — за Rc: чистые элементы (обычно почти всё дерево)
    // передают их дальше бампом счётчика вместо клона HashMap на элемент.
    let empty_inh: std::rc::Rc<ComputedStyle> = std::rc::Rc::new(ComputedStyle::default());
    let mut inherited_for: std::collections::HashMap<ElementId, std::rc::Rc<ComputedStyle>> =
        std::collections::HashMap::with_capacity(order.len());
    let mut ancestor_dirty_for: std::collections::HashMap<ElementId, bool> =
        std::collections::HashMap::with_capacity(order.len());

    for id in order {
        let parent_id = tree.elements.get(&id).and_then(|n| n.parent);
        let parent_inh = parent_id
            .and_then(|p| inherited_for.get(&p).cloned())
            .unwrap_or_else(|| empty_inh.clone());
        let ancestor_dirty = parent_id
            .and_then(|p| ancestor_dirty_for.get(&p).copied())
            .unwrap_or(false);

        let (has_identity, has_inline, is_dirty, type_name, classes) = if let Some(node) = tree.elements.get(&id) {
            (
                !node.element.get_classes().is_empty() || !node.element.element_type_name().is_empty(),
                !node.inline_styles.is_empty(),
                node.styles_dirty,
                node.element.element_type_name().to_string(),
                node.element.get_classes().to_vec(),
            )
        } else {
            inherited_for.insert(id, parent_inh);
            ancestor_dirty_for.insert(id, ancestor_dirty);
            continue;
        };

        let subtree_dirty = ancestor_dirty || is_dirty;
        ancestor_dirty_for.insert(id, subtree_dirty);

        if !subtree_dirty {
            log::trace!(
                target: "syngui::mss::cascade",
                "skip clean id={:?} type={}", id, type_name,
            );
            inherited_for.insert(id, parent_inh);
            continue;
        }

        if !has_identity && !has_inline {
            if let Some(node) = tree.elements.get_mut(&id) {
                if node.had_mss_rules || parent_inh.properties().next().is_some() {
                    node.element.reset_mss_styles();
                    node.element.apply_computed_style(&parent_inh);
                    node.had_mss_rules = parent_inh.properties().next().is_some();
                    node.refresh_hint_cache();
                }
                node.styles_dirty = false;
            }
            // parent_inh уже отфильтрован extract_inherited у предка —
            // передаём дальше без пересборки.
            inherited_for.insert(id, parent_inh);
            continue;
        }

        let mut base = (*parent_inh).clone();
        let mut hover = ComputedStyle::default();
        let mut active = ComputedStyle::default();
        let mut focus = ComputedStyle::default();
        let mut selected = ComputedStyle::default();
        let mut checked = ComputedStyle::default();
        let mut has_hover = false;
        let mut has_active = false;
        let mut has_focus = false;
        let mut has_selected = false;
        let mut has_checked = false;
        let mut has_base = base.properties().next().is_some();

        index.candidates(&classes, &type_name, &mut cand);
        let mut matching: Vec<(usize, (u32, u32, u32), &StyleRule)> = cand.iter()
            .map(|&i| (i as usize, &rules[i as usize]))
            .filter(|(_, rule)| selector_matches(&rule.selector, id, tree))
            .map(|(i, rule)| (i, rule.selector.specificity(), rule))
            .collect();
        matching.sort_by(|a, b| a.1.cmp(&b.1).then(a.0.cmp(&b.0)));

        for (_, _spec, rule) in &matching {
            let pseudo = selector_pseudo(&rule.selector);
            match pseudo {
                None => {
                    has_base = true;
                    for (prop, val) in &rule.declarations {
                        base.set(prop, resolve_for_cascade(style_engine, val, prop, &parent_inh));
                    }
                }
                Some("hover") => {
                    has_hover = true;
                    for (prop, val) in &rule.declarations {
                        hover.set(prop, resolve_for_cascade(style_engine, val, prop, &parent_inh));
                    }
                }
                Some("checked") => {
                    has_checked = true;
                    for (prop, val) in &rule.declarations {
                        checked.set(prop, resolve_for_cascade(style_engine, val, prop, &parent_inh));
                    }
                }
                Some("active") | Some("pressed") => {
                    has_active = true;
                    for (prop, val) in &rule.declarations {
                        active.set(prop, resolve_for_cascade(style_engine, val, prop, &parent_inh));
                    }
                }
                Some("focus") => {
                    has_focus = true;
                    for (prop, val) in &rule.declarations {
                        focus.set(prop, resolve_for_cascade(style_engine, val, prop, &parent_inh));
                    }
                }
                Some("selected") => {
                    has_selected = true;
                    for (prop, val) in &rule.declarations {
                        selected.set(prop, resolve_for_cascade(style_engine, val, prop, &parent_inh));
                    }
                }
                Some(p) => match window_pseudo_matches(p, window_flags) {
                    Some(true) => {
                        has_base = true;
                        for (prop, val) in &rule.declarations {
                            base.set(prop, resolve_for_cascade(style_engine, val, prop, &parent_inh));
                        }
                    }
                    Some(false) => {}
                    None => {
                        has_base = true;
                        for (prop, val) in &rule.declarations {
                            base.set(prop, resolve_for_cascade(style_engine, val, prop, &parent_inh));
                        }
                    }
                },
            }
        }

        log::trace!(
            target: "syngui::mss::cascade",
            "apply id={:?} type={} matched={} dirty={}",
            id, type_name, matching.len(), is_dirty,
        );

        let has_any_rules = has_base || has_hover || has_active || has_focus || has_selected || has_checked || has_inline;

        if !has_any_rules {
            let had_rules = tree.elements.get(&id).map(|n| n.had_mss_rules).unwrap_or(false);
            if had_rules {
                if let Some(node) = tree.elements.get_mut(&id) {
                    node.element.reset_mss_styles();
                    let empty = ComputedStyle::default();
                    node.element.apply_computed_style(&empty);
                    node.element.apply_transition_styles(&empty, None, None, None, None, None);
                    node.had_mss_rules = false;
                    node.styles_dirty = false;
                    node.refresh_hint_cache();
                }
            }
            inherited_for.insert(id, empty_inh.clone());
            continue;
        }

        if let Some(node) = tree.elements.get_mut(&id) {
            if node.styles_dirty {
                node.element.reset_mss_styles();
            }
            if !node.inline_styles.is_empty() {
                let inline_owned: Vec<_> = node.inline_styles.clone();
                for (prop, value) in &inline_owned {
                    let resolved = resolve_for_cascade(style_engine, value, prop, &parent_inh);
                    base.set(prop, resolved);
                }
            }
            let hover_full = if has_hover { Some(merge_layer(&base, &hover)) } else { None };
            let active_full = if has_active { Some(merge_layer(&base, &active)) } else { None };
            let focus_full = if has_focus { Some(merge_layer(&base, &focus)) } else { None };
            let selected_full = if has_selected { Some(merge_layer(&base, &selected)) } else { None };
            let checked_full = if has_checked { Some(merge_layer(&base, &checked)) } else { None };
            node.element.apply_computed_style(&base);
            node.element.apply_transition_styles(
                &base,
                hover_full.as_ref(),
                active_full.as_ref(),
                focus_full.as_ref(),
                selected_full.as_ref(),
                checked_full.as_ref(),
            );
            node.element.setup_keyframe_animation(&base, style_engine.stylesheet());
            node.mss_margin_set = base.has_margin();
            node.mss_margin = base.margin();
            node.mss_flex_grow = base.flex_grow().unwrap_or(0.0);
            node.had_mss_rules = true;
            node.styles_dirty = false;
            node.refresh_hint_cache();
        }
        // Стили могли запустить keyframe-анимацию или transition.
        tree.note_animation_started(id);

        inherited_for.insert(id, std::rc::Rc::new(extract_inherited(&base)));
    }

    true
}

pub fn mark_subtree_styles_dirty(tree: &mut ElementTree, id: ElementId) {
    let mut stack = vec![id];
    while let Some(node_id) = stack.pop() {
        if let Some(node) = tree.elements.get_mut(&node_id) {
            node.styles_dirty = true;
            for &c in &node.children {
                stack.push(c);
            }
        }
    }
}
