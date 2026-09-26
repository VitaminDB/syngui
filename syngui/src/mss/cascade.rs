use super::inheritance::{extract_inherited, resolve_cascade_keyword};
use super::matching::{selector_matches, selector_pseudo};
use super::style_engine::{ComputedStyle, StyleEngine};
use super::stylesheet::StyleRule;
use super::value::StyleValue;
use crate::widget::{ElementId, ElementTree};

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
        "window-maximized" => wf::MAXIMIZED,
        "window-fullscreen" => wf::FULLSCREEN,
        "window-focused" => wf::FOCUSED,
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
/// Индекс правил по классу и типу элемента. Ключи — собственные строки, а
/// не заимствованные у stylesheet: индекс переживает вызов и лежит в кэше
/// [`rule_index`] до смены стилей.
struct RuleIndex {
    by_class: std::collections::HashMap<String, Vec<u32>>,
    by_type: std::collections::HashMap<String, Vec<u32>>,
    catch_all: Vec<u32>,
    /// Классы из не-целевых сегментов селекторов перед `>`/пробелом:
    /// их смена у элемента меняет совпадения у потомков.
    ancestor_classes: std::collections::HashSet<String>,
    /// То же для `+`/`~`: смена меняет совпадения у последующих сиблингов.
    sibling_classes: std::collections::HashSet<String>,
}

/// Результат последнего каскада для элемента — чтобы (а) не применять
/// стиль заново, если он не изменился, и (б) не пересчитывать потомков,
/// если унаследованный набор тот же. Именно так грязность «затухает»:
/// смена inline `translate-x` у контейнера полок не заставляет пересчитывать
/// тысячу карточек под ним.
pub(crate) struct CascadeCache {
    /// Унаследованный стиль от родителя, при котором стиль был вычислен.
    parent_inh: std::sync::Arc<ComputedStyle>,
    /// Унаследованный стиль, переданный потомкам.
    out_inh: std::sync::Arc<ComputedStyle>,
    base: ComputedStyle,
    hover: Option<ComputedStyle>,
    active: Option<ComputedStyle>,
    focus: Option<ComputedStyle>,
    selected: Option<ComputedStyle>,
    checked: Option<ComputedStyle>,
    disabled: Option<ComputedStyle>,
}

impl std::fmt::Debug for CascadeCache {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("CascadeCache").finish_non_exhaustive()
    }
}

thread_local! {
    /// Объединение контекстных классов всех построенных индексов (никогда
    /// не сужается): `update_element` спрашивает его без доступа к движку
    /// стилей. Пока ни один индекс не построен — ответ консервативный.
    static CONTEXT_CLASSES: std::cell::RefCell<Option<(
        std::collections::HashSet<String>,
        std::collections::HashSet<String>,
    )>> = const { std::cell::RefCell::new(None) };
}

/// Как смена классов элемента влияет на каскад вокруг него.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum ClassChangeScope {
    /// Только сам элемент.
    SelfOnly,
    /// Элемент и его потомки (класс участвует в `A B` / `A > B`).
    Subtree,
    /// Поддерево родителя (класс участвует в `A + B` / `A ~ B`).
    ParentSubtree,
}

/// Кого нужно пометить `styles_dirty` при смене классов элемента.
/// `changed` — классы, которые появились или пропали.
pub(crate) fn class_change_scope<'a>(changed: impl Iterator<Item = &'a str>) -> ClassChangeScope {
    CONTEXT_CLASSES.with(|cell| {
        let cell = cell.borrow();
        let Some((ancestors, siblings)) = cell.as_ref() else {
            return ClassChangeScope::ParentSubtree;
        };
        let mut scope = ClassChangeScope::SelfOnly;
        for c in changed {
            if siblings.contains(c) {
                return ClassChangeScope::ParentSubtree;
            }
            if ancestors.contains(c) {
                scope = ClassChangeScope::Subtree;
            }
        }
        scope
    })
}

impl RuleIndex {
    fn build(rules: &[StyleRule]) -> Self {
        use super::stylesheet::{Selector, SelectorChain, SelectorPart};

        crate::perf::counters::incr(crate::perf::counters::Tally::StyleIndexBuilds);

        let mut by_class: std::collections::HashMap<String, Vec<u32>> =
            std::collections::HashMap::new();
        let mut by_type: std::collections::HashMap<String, Vec<u32>> =
            std::collections::HashMap::new();
        let mut catch_all: Vec<u32> = Vec::new();
        let mut ancestor_classes: std::collections::HashSet<String> =
            std::collections::HashSet::new();
        let mut sibling_classes: std::collections::HashSet<String> =
            std::collections::HashSet::new();

        fn context_classes(
            chain: &SelectorChain,
            ancestors: &mut std::collections::HashSet<String>,
            siblings: &mut std::collections::HashSet<String>,
        ) {
            use super::stylesheet::Combinator;
            // segments[i] связан с segments[i+1] через combinators[i];
            // последний сегмент — цель, контекстом не является.
            for (i, comb) in chain.combinators.iter().enumerate() {
                let Some(part) = chain.segments.get(i) else {
                    break;
                };
                let set: &mut std::collections::HashSet<String> = match comb {
                    Combinator::Descendant | Combinator::Child => &mut *ancestors,
                    Combinator::AdjacentSibling | Combinator::GeneralSibling => &mut *siblings,
                };
                match part {
                    SelectorPart::Class(c) => {
                        set.insert(c.clone());
                    }
                    SelectorPart::Compound { classes, .. } => {
                        for c in classes {
                            set.insert(c.clone());
                        }
                    }
                    _ => {}
                }
            }
        }

        fn slot_chain(
            chain: &SelectorChain,
            i: u32,
            by_class: &mut std::collections::HashMap<String, Vec<u32>>,
            by_type: &mut std::collections::HashMap<String, Vec<u32>>,
            catch_all: &mut Vec<u32>,
        ) {
            // Пустая цепочка (битый селектор, напр. утёкший keyframe-фрейм)
            // ничего не матчит — не кладём её ни в одно ведро вместо паники
            // в target(); chain_matches для неё и так возвращает false.
            if chain.segments.is_empty() {
                return;
            }
            match chain.target() {
                SelectorPart::Class(c) => by_class.entry(c.clone()).or_default().push(i),
                SelectorPart::Element(e) => by_type.entry(e.clone()).or_default().push(i),
                SelectorPart::Compound {
                    classes, element, ..
                } => {
                    // Compound требует ВСЕ свои классы, поэтому ведро любого
                    // из них корректно сужает кандидатов; берём первый.
                    if let Some(c) = classes.first() {
                        by_class.entry(c.clone()).or_default().push(i);
                    } else if let Some(e) = element {
                        by_type.entry(e.clone()).or_default().push(i);
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
                    by_class.entry(c.clone()).or_default().push(i)
                }
                Selector::Element(e) | Selector::ElementPseudo(e, _) => {
                    by_type.entry(e.clone()).or_default().push(i)
                }
                Selector::Universal | Selector::Id(_) => catch_all.push(i),
                Selector::Complex(chain) => {
                    context_classes(chain, &mut ancestor_classes, &mut sibling_classes);
                    slot_chain(chain, i, &mut by_class, &mut by_type, &mut catch_all)
                }
                Selector::Group(chains) => {
                    // Правило попадает в ведро каждой цепочки; дубли снимает
                    // sort+dedup в candidates().
                    for chain in chains {
                        context_classes(chain, &mut ancestor_classes, &mut sibling_classes);
                        slot_chain(chain, i, &mut by_class, &mut by_type, &mut catch_all);
                    }
                }
            }
        }

        CONTEXT_CLASSES.with(|cell| {
            let mut cell = cell.borrow_mut();
            let (a, s) = cell.get_or_insert_with(Default::default);
            a.extend(ancestor_classes.iter().cloned());
            s.extend(sibling_classes.iter().cloned());
        });

        Self {
            by_class,
            by_type,
            catch_all,
            ancestor_classes,
            sibling_classes,
        }
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

thread_local! {
    /// Индекс строится по всему stylesheet (в synthos это около 1900 блоков
    /// правил), а `apply_styles` вызывается после каждого прохода пересборки
    /// — до девяти раз за кадр. Держим его до смены стилей: ключ — версия
    /// таблицы, глобально уникальная, так что движки разных окон не путают
    /// кэш.
    static RULE_INDEX: std::cell::RefCell<Option<(u64, std::rc::Rc<RuleIndex>)>> =
        const { std::cell::RefCell::new(None) };
}

fn rule_index(style_engine: &StyleEngine) -> std::rc::Rc<RuleIndex> {
    let version = style_engine.stylesheet_version();
    RULE_INDEX.with(|cell| {
        if let Some((v, index)) = cell.borrow().as_ref() {
            if *v == version {
                return index.clone();
            }
        }
        let index = std::rc::Rc::new(RuleIndex::build(style_engine.stylesheet().rules()));
        *cell.borrow_mut() = Some((version, index.clone()));
        index
    })
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
    let index = rule_index(style_engine);
    let mut cand: Vec<u32> = Vec::new();
    let window_flags = tree.window_flags;

    let mut inherited_for: std::collections::HashMap<ElementId, ComputedStyle> =
        std::collections::HashMap::with_capacity(order.len());

    for id in order {
        let parent_inh = tree
            .elements
            .get(&id)
            .and_then(|n| n.parent)
            .and_then(|p| inherited_for.get(&p).cloned())
            .unwrap_or_default();

        let (has_identity, has_inline, type_name, classes) =
            if let Some(node) = tree.elements.get(&id) {
                (
                    !node.element.get_classes().is_empty()
                        || !node.element.element_type_name().is_empty(),
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
        let mut disabled = ComputedStyle::default();
        let mut has_hover = false;
        let mut has_active = false;
        let mut has_focus = false;
        let mut has_selected = false;
        let mut has_checked = false;
        let mut has_disabled = false;
        let mut has_base = base.properties().next().is_some();

        if has_identity || has_inline {
            index.candidates(&classes, &type_name, &mut cand);
            let mut matching: Vec<(usize, (u32, u32, u32), &StyleRule)> = cand
                .iter()
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
                            base.set(
                                prop,
                                resolve_for_cascade(style_engine, val, prop, &parent_inh),
                            );
                        }
                    }
                    Some("hover") => {
                        has_hover = true;
                        for (prop, val) in &rule.declarations {
                            hover.set(
                                prop,
                                resolve_for_cascade(style_engine, val, prop, &parent_inh),
                            );
                        }
                    }
                    Some("checked") => {
                        has_checked = true;
                        for (prop, val) in &rule.declarations {
                            checked.set(
                                prop,
                                resolve_for_cascade(style_engine, val, prop, &parent_inh),
                            );
                        }
                    }
                    Some("active") | Some("pressed") => {
                        has_active = true;
                        for (prop, val) in &rule.declarations {
                            active.set(
                                prop,
                                resolve_for_cascade(style_engine, val, prop, &parent_inh),
                            );
                        }
                    }
                    Some("focus") => {
                        has_focus = true;
                        for (prop, val) in &rule.declarations {
                            focus.set(
                                prop,
                                resolve_for_cascade(style_engine, val, prop, &parent_inh),
                            );
                        }
                    }
                    Some("selected") => {
                        has_selected = true;
                        for (prop, val) in &rule.declarations {
                            selected.set(
                                prop,
                                resolve_for_cascade(style_engine, val, prop, &parent_inh),
                            );
                        }
                    }
                    Some("disabled") => {
                        has_disabled = true;
                        for (prop, val) in &rule.declarations {
                            disabled.set(
                                prop,
                                resolve_for_cascade(style_engine, val, prop, &parent_inh),
                            );
                        }
                    }
                    Some(p) => match window_pseudo_matches(p, window_flags) {
                        Some(true) => {
                            has_base = true;
                            for (prop, val) in &rule.declarations {
                                base.set(
                                    prop,
                                    resolve_for_cascade(style_engine, val, prop, &parent_inh),
                                );
                            }
                        }
                        // Неизвестный псевдокласс — правило не действует
                        // (парсер такие и не пропускает, см. is_known_pseudo).
                        Some(false) | None => {}
                    },
                }
            }

            if has_inline {
                if let Some(node) = tree.elements.get(&id) {
                    let inline = node.inline_styles.clone();
                    has_base = true;
                    for (prop, val) in &inline {
                        base.set(
                            prop,
                            resolve_for_cascade(style_engine, val, prop, &parent_inh),
                        );
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

        if has_base
            || has_hover
            || has_active
            || has_focus
            || has_selected
            || has_checked
            || has_disabled
        {
            if let Some(node) = tree.elements.get_mut(&id) {
                node.element.reset_mss_styles();
                node.element.apply_computed_style(&base);
                let hover_full = if has_hover {
                    Some(merge_layer(&base, &hover))
                } else {
                    None
                };
                let active_full = if has_active {
                    Some(merge_layer(&base, &active))
                } else {
                    None
                };
                let focus_full = if has_focus {
                    Some(merge_layer(&base, &focus))
                } else {
                    None
                };
                let selected_full = if has_selected {
                    Some(merge_layer(&base, &selected))
                } else {
                    None
                };
                let checked_full = if has_checked {
                    Some(merge_layer(&base, &checked))
                } else {
                    None
                };
                node.element.apply_transition_styles(
                    &base,
                    hover_full.as_ref(),
                    active_full.as_ref(),
                    focus_full.as_ref(),
                    selected_full.as_ref(),
                    checked_full.as_ref(),
                );
                let disabled_full = has_disabled.then(|| merge_layer(&base, &disabled));
                node.element.apply_disabled_style(disabled_full.as_ref());
                node.element
                    .setup_keyframe_animation(&base, style_engine.stylesheet());
                node.mss_margin_set = base.has_margin();
                node.mss_margin = base.margin();
                node.mss_flex_grow = base.flex_grow().unwrap_or(0.0);
                node.mss_flex_shrink = base.flex_shrink().unwrap_or(0.0);
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
    let index = rule_index(style_engine);
    let _ = (&index.ancestor_classes, &index.sibling_classes);
    let mut cand: Vec<u32> = Vec::new();
    let window_flags = tree.window_flags;

    // Наследуемые стили — за Rc: чистые элементы (обычно почти всё дерево)
    // передают их дальше бампом счётчика вместо клона HashMap на элемент.
    let empty_inh: std::sync::Arc<ComputedStyle> = std::sync::Arc::new(ComputedStyle::default());
    let mut inherited_for: std::collections::HashMap<ElementId, std::sync::Arc<ComputedStyle>> =
        std::collections::HashMap::with_capacity(order.len());

    for id in order {
        let parent_id = tree.elements.get(&id).and_then(|n| n.parent);
        let parent_inh = parent_id
            .and_then(|p| inherited_for.get(&p).cloned())
            .unwrap_or_else(|| empty_inh.clone());

        // Чистый элемент с тем же унаследованным входом — стиль не
        // изменился, потомкам уходит прежний выход.
        let (has_identity, has_inline, is_dirty) = match tree.elements.get(&id) {
            Some(node) => {
                if !node.styles_dirty {
                    if let Some(cache) = node.cascade_cache.as_ref() {
                        if std::sync::Arc::ptr_eq(&cache.parent_inh, &parent_inh)
                            || *cache.parent_inh == *parent_inh
                        {
                            inherited_for.insert(id, cache.out_inh.clone());
                            continue;
                        }
                    }
                }
                let has_identity = !node.element.get_classes().is_empty()
                    || !node.element.element_type_name().is_empty();
                let has_inline = !node.inline_styles.is_empty();
                if has_identity {
                    index.candidates(
                        node.element.get_classes(),
                        node.element.element_type_name(),
                        &mut cand,
                    );
                } else {
                    cand.clear();
                }
                (has_identity, has_inline, node.styles_dirty)
            }
            None => {
                inherited_for.insert(id, parent_inh);
                continue;
            }
        };
        crate::perf::incr(crate::perf::Counter::ApplyStylesIter);

        if !has_identity && !has_inline {
            if let Some(node) = tree.elements.get_mut(&id) {
                let unchanged = node
                    .cascade_cache
                    .as_ref()
                    .map(|c| c.base == *parent_inh)
                    .unwrap_or(false);
                if !unchanged && (node.had_mss_rules || parent_inh.properties().next().is_some()) {
                    node.element.reset_mss_styles();
                    node.element.apply_computed_style(&parent_inh);
                    node.had_mss_rules = parent_inh.properties().next().is_some();
                    node.refresh_hint_cache();
                }
                node.styles_dirty = false;
                node.cascade_cache = Some(Box::new(CascadeCache {
                    parent_inh: parent_inh.clone(),
                    out_inh: parent_inh.clone(),
                    base: (*parent_inh).clone(),
                    hover: None,
                    active: None,
                    focus: None,
                    selected: None,
                    checked: None,
                    disabled: None,
                }));
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
        let mut disabled = ComputedStyle::default();
        let mut has_hover = false;
        let mut has_active = false;
        let mut has_focus = false;
        let mut has_selected = false;
        let mut has_checked = false;
        let mut has_disabled = false;
        let mut has_base = base.properties().next().is_some();

        crate::perf::add(crate::perf::Counter::ApplyStylesRuleTest, cand.len() as u64);
        let mut matching: Vec<(usize, (u32, u32, u32), &StyleRule)> = cand
            .iter()
            .map(|&i| (i as usize, &rules[i as usize]))
            .filter(|(_, rule)| selector_matches(&rule.selector, id, tree))
            .map(|(i, rule)| (i, rule.selector.specificity(), rule))
            .collect();
        matching.sort_by(|a, b| a.1.cmp(&b.1).then(a.0.cmp(&b.0)));

        for (_, _spec, rule) in &matching {
            let pseudo = selector_pseudo(&rule.selector);
            let (layer, flag): (&mut ComputedStyle, &mut bool) = match pseudo {
                None => (&mut base, &mut has_base),
                Some("hover") => (&mut hover, &mut has_hover),
                Some("checked") => (&mut checked, &mut has_checked),
                Some("active") | Some("pressed") => (&mut active, &mut has_active),
                Some("focus") => (&mut focus, &mut has_focus),
                Some("selected") => (&mut selected, &mut has_selected),
                Some("disabled") => (&mut disabled, &mut has_disabled),
                Some(p) => match window_pseudo_matches(p, window_flags) {
                    Some(false) | None => continue,
                    Some(true) => (&mut base, &mut has_base),
                },
            };
            *flag = true;
            for (prop, val) in &rule.declarations {
                layer.set(
                    prop,
                    resolve_for_cascade(style_engine, val, prop, &parent_inh),
                );
            }
        }

        log::trace!(
            target: "syngui::mss::cascade",
            "apply id={:?} matched={} dirty={}",
            id, matching.len(), is_dirty,
        );

        let has_any_rules = has_base
            || has_hover
            || has_active
            || has_focus
            || has_selected
            || has_checked
            || has_disabled
            || has_inline;

        if !has_any_rules {
            if let Some(node) = tree.elements.get_mut(&id) {
                if node.had_mss_rules {
                    node.element.reset_mss_styles();
                    let empty = ComputedStyle::default();
                    node.element.apply_computed_style(&empty);
                    node.element
                        .apply_transition_styles(&empty, None, None, None, None, None);
                    node.element.apply_disabled_style(None);
                    node.had_mss_rules = false;
                    node.refresh_hint_cache();
                }
                node.styles_dirty = false;
                node.cascade_cache = Some(Box::new(CascadeCache {
                    parent_inh: parent_inh.clone(),
                    out_inh: empty_inh.clone(),
                    base: ComputedStyle::default(),
                    hover: None,
                    active: None,
                    focus: None,
                    selected: None,
                    checked: None,
                    disabled: None,
                }));
            }
            inherited_for.insert(id, empty_inh.clone());
            continue;
        }

        let Some(node) = tree.elements.get_mut(&id) else {
            inherited_for.insert(id, parent_inh);
            continue;
        };
        if !node.inline_styles.is_empty() {
            for (prop, value) in &node.inline_styles {
                let resolved = resolve_for_cascade(style_engine, value, prop, &parent_inh);
                base.set(prop, resolved);
            }
        }
        let hover_full = has_hover.then(|| merge_layer(&base, &hover));
        let active_full = has_active.then(|| merge_layer(&base, &active));
        let focus_full = has_focus.then(|| merge_layer(&base, &focus));
        let selected_full = has_selected.then(|| merge_layer(&base, &selected));
        let checked_full = has_checked.then(|| merge_layer(&base, &checked));
        let disabled_full = has_disabled.then(|| merge_layer(&base, &disabled));

        // Дифф с прошлым результатом: тот же стиль — ничего не применяем
        // (и не перезапускаем transition/keyframe-анимации).
        let unchanged = node
            .cascade_cache
            .as_ref()
            .map(|c| {
                c.base == base
                    && c.hover == hover_full
                    && c.active == active_full
                    && c.focus == focus_full
                    && c.selected == selected_full
                    && c.checked == checked_full
                    && c.disabled == disabled_full
            })
            .unwrap_or(false);
        if unchanged {
            node.styles_dirty = false;
            let out_inh = node
                .cascade_cache
                .as_ref()
                .map(|c| c.out_inh.clone())
                .unwrap_or_else(|| empty_inh.clone());
            if let Some(c) = node.cascade_cache.as_mut() {
                c.parent_inh = parent_inh.clone();
            }
            inherited_for.insert(id, out_inh);
            continue;
        }

        node.element.reset_mss_styles();
        node.element.apply_computed_style(&base);
        node.element.apply_transition_styles(
            &base,
            hover_full.as_ref(),
            active_full.as_ref(),
            focus_full.as_ref(),
            selected_full.as_ref(),
            checked_full.as_ref(),
        );
        node.element.apply_disabled_style(disabled_full.as_ref());
        node.element
            .setup_keyframe_animation(&base, style_engine.stylesheet());
        node.mss_margin_set = base.has_margin();
        node.mss_margin = base.margin();
        node.mss_flex_grow = base.flex_grow().unwrap_or(0.0);
        node.mss_flex_shrink = base.flex_shrink().unwrap_or(0.0);
        node.had_mss_rules = true;
        node.styles_dirty = false;
        node.refresh_hint_cache();

        let out_inh = std::sync::Arc::new(extract_inherited(&base));
        node.cascade_cache = Some(Box::new(CascadeCache {
            parent_inh: parent_inh.clone(),
            out_inh: out_inh.clone(),
            base,
            hover: hover_full,
            active: active_full,
            focus: focus_full,
            selected: selected_full,
            checked: checked_full,
            disabled: disabled_full,
        }));
        // Стили могли запустить keyframe-анимацию или transition.
        tree.note_animation_started(id);

        inherited_for.insert(id, out_inh);
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

#[cfg(all(test, feature = "testing"))]
mod index_cache_tests {
    use crate::perf::counters::snapshot;
    use crate::prelude::*;
    use crate::testing::TestHarness;

    /// Индекс правил строится один раз на таблицу стилей: `apply_styles`
    /// зовётся после каждого прохода пересборки, и раньше каждый вызов
    /// перебирал весь stylesheet.
    #[test]
    fn rule_index_is_built_once_per_stylesheet() {
        let mut h = TestHarness::new(Box::new(
            DecoratedBox::new()
                .class("card")
                .child(DecoratedBox::new().class("label")),
        ));
        h.layout(800.0, 600.0);
        let engine = h.apply_mss(".card { padding: 8; } .label { font-size: 14; }");
        h.layout(800.0, 600.0);
        let id = *h
            .find_by_class("label")
            .first()
            .expect("в дереве есть .label");
        assert_eq!(h.element_mss(id).and_then(|m| m.font_size), Some(14.0));

        let start = snapshot();
        for _ in 0..5 {
            h.apply_styles(&engine);
        }
        assert_eq!(
            snapshot().since(&start).style_index_builds,
            0,
            "индекс собрался заново на неизменных стилях"
        );

        // Новая таблица — новый индекс, и значения из неё доезжают.
        let start = snapshot();
        h.apply_mss(".label { font-size: 22; }");
        assert_eq!(snapshot().since(&start).style_index_builds, 1);
        h.layout(800.0, 600.0);
        assert_eq!(h.element_mss(id).and_then(|m| m.font_size), Some(22.0));
    }
}
