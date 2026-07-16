//! Regression suite: descendant-combinator cascade for non-inherited
//! widget-specific properties.
//!
//! Symptom that prompted this: in volna_plus,
//! `.stat-card Icon { icon-size: 22; }` did not actually shrink the icon —
//! it stayed at the default 24px. Visual debugging suggested rules of the
//! shape `.ancestor Type { non-inherited-prop: value }` were silently
//! ignored, and the same was reported for `.events-tabs-pane Tab { font-size }`,
//! `.icon-rail ListView { row-padding }`, etc.
//!
//! These tests build minimal trees that mirror real usage (a `.stat-card`
//! card containing a `Column` containing an `Icon`) and assert the resolved
//! `MssFields` reflect the descendant rule. They exercise both cascade
//! entry points (`apply_styles_to_tree` — the deterministic full pass; and
//! `apply_styles_dirty` — the dirty-bypass main-loop path) so we catch
//! divergence between them.

use syngui::testing::*;
use syngui::prelude::*;

const STAT_CARD_TREE: &str = ".stat-card Icon";
const STAT_CARD_ICON_SIZE: f32 = 22.0;

/// Build the canonical `stat-card` tree: a DecoratedBox carrying the class,
/// a Column wrapper, and a leaf Icon. This mirrors `volna_plus::stat_card`.
fn build_stat_card_tree() -> Box<dyn Widget> {
    Box::new(
        DecoratedBox::new()
            .class("stat-card")
            .child(
                Column::new()
                    .child(Icon::new("MI_WAVES"))
                    .child(Text::new("42"))
            )
    )
}

fn icon_size_of(harness: &TestHarness) -> Option<f32> {
    let icons = harness.find_by_type_name("Icon");
    let id = *icons.first().expect("expected at least one Icon");
    harness.element_mss(id).and_then(|m| m.icon_size)
}

fn font_size_of_text_with_class(harness: &TestHarness, class: &str) -> Option<f32> {
    harness.find_by_class(class)
        .into_iter()
        .next()
        .and_then(|id| harness.element_mss(id))
        .and_then(|m| m.font_size)
}

// ─────────────────────────────────────────────────────────────────────────
// Group 1: descendant-cascade for widget-specific (non-inherited) properties
// ─────────────────────────────────────────────────────────────────────────

#[test]
fn descendant_icon_size_first_apply_full_cascade() {
    // Direct mirror of card.mss:36 `.stat-card Icon { icon-size: 22 }`.
    // First-time application via the deterministic full-cascade entry point.
    let mss = format!("{} {{ icon-size: {}; }}", STAT_CARD_TREE, STAT_CARD_ICON_SIZE);
    let mut h = TestHarness::new(build_stat_card_tree());
    h.apply_mss(&mss);
    h.layout(800.0, 600.0);

    assert_eq!(
        icon_size_of(&h),
        Some(STAT_CARD_ICON_SIZE),
        "Icon nested under .stat-card must receive icon-size: 22 from the descendant rule"
    );
}

#[test]
fn descendant_icon_size_first_apply_dirty_path() {
    // Same rule, but routed through the dirty-bypass main-loop cascade —
    // verifies the optimised path agrees with the full one.
    let mss = format!("{} {{ icon-size: {}; }}", STAT_CARD_TREE, STAT_CARD_ICON_SIZE);
    let mut h = TestHarness::new(build_stat_card_tree());
    h.apply_mss_dirty(&mss);
    h.layout(800.0, 600.0);

    assert_eq!(
        icon_size_of(&h),
        Some(STAT_CARD_ICON_SIZE),
        "Dirty-path cascade must also apply descendant rule on first pass"
    );
}

#[test]
fn child_combinator_is_not_descendant() {
    // `.stat-card > Icon` — strict child. Icon is grandchild via Column,
    // so this rule MUST NOT match (sanity-check matcher precision).
    let mss = format!(".stat-card > Icon {{ icon-size: {}; }}", STAT_CARD_ICON_SIZE);
    let mut h = TestHarness::new(build_stat_card_tree());
    h.apply_mss(&mss);
    h.layout(800.0, 600.0);

    assert_eq!(
        icon_size_of(&h),
        None,
        "Child combinator must NOT match a grandchild Icon"
    );
}

#[test]
fn descendant_padding_three_levels_deep() {
    // .root .panel Icon — three-segment chain through the wrapper Column.
    // (Column has no class; the chain still resolves via the .root → Icon
    // ancestor walk; the .panel segment matches DecoratedBox itself.)
    let mss = ".root .panel Icon { padding: 5; }";
    let widget: Box<dyn Widget> = Box::new(
        DecoratedBox::new().class("root").child(
            DecoratedBox::new().class("panel").child(
                Column::new().child(Icon::new("MI_WAVES"))
            )
        )
    );
    let mut h = TestHarness::new(widget);
    h.apply_mss(mss);
    h.layout(800.0, 600.0);

    let pad = harness_icon_padding(&h);
    assert!(pad > 4.5 && pad < 5.5, "expected padding≈5 from .root .panel Icon, got {}", pad);
}

#[test]
fn group_selector_descendant_branch_applies() {
    // `.foo, .bar Icon { icon-size: 22 }` — the second branch should match.
    let mss = format!(".foo, .stat-card Icon {{ icon-size: {}; }}", STAT_CARD_ICON_SIZE);
    let mut h = TestHarness::new(build_stat_card_tree());
    h.apply_mss(&mss);
    h.layout(800.0, 600.0);

    assert_eq!(
        icon_size_of(&h),
        Some(STAT_CARD_ICON_SIZE),
        "Group selector with a descendant branch must apply when that branch matches"
    );
}

// ─────────────────────────────────────────────────────────────────────────
// Group 2: inherited-property descendant rules (sanity — these should
// always have worked because inheritance covers them even without matching)
// ─────────────────────────────────────────────────────────────────────────

#[test]
fn descendant_inherited_property_reaches_descendant_box() {
    // .stat-card .label { font-size: 12 } — mirrors card.mss:31.
    // Use DecoratedBox.label (has MssFields) instead of Text (which keeps its
    // own private mss_* fields and isn't observable via element.mss()).
    let widget: Box<dyn Widget> = Box::new(
        DecoratedBox::new().class("stat-card").child(
            Column::new().child(DecoratedBox::new().class("label"))
        )
    );
    let mut h = TestHarness::new(widget);
    h.apply_mss(".stat-card .label { font-size: 12; }");
    h.layout(800.0, 600.0);

    assert_eq!(
        font_size_of_text_with_class(&h, "label"),
        Some(12.0),
        "Descendant rule .stat-card .label must apply font-size to .label DecoratedBox"
    );
}

// ─────────────────────────────────────────────────────────────────────────
// Group 3: invalidation correctness — the painful real-world cases
// ─────────────────────────────────────────────────────────────────────────

#[test]
fn dirty_path_reapplies_after_class_change_on_ancestor() {
    // Initially the ancestor has class .stat-card; after we change it to
    // .other-card, the descendant rule .stat-card Icon must STOP applying.
    // The dirty-path cascade uses a `subtree_dirty` bypass — if it doesn't
    // invalidate descendants when an ancestor's classes change, the icon
    // would keep its previous icon-size.
    use syngui::mss::cascade::{apply_styles_dirty, mark_subtree_styles_dirty};

    let widget = build_stat_card_tree();
    let mut h = TestHarness::new(widget);
    let engine = h.apply_mss_dirty(&format!(".stat-card Icon {{ icon-size: {}; }}", STAT_CARD_ICON_SIZE));
    h.layout(800.0, 600.0);
    assert_eq!(icon_size_of(&h), Some(STAT_CARD_ICON_SIZE), "precondition");

    // Mutate ancestor's class. We do this directly on the element layer, the
    // same way reconcile_children_ref does, then propagate the dirty flag
    // down — which is what we want callers to do.
    let card_id = *h.find_by_class("stat-card").first().expect("ancestor present");
    h.set_classes(card_id, vec!["other-card".to_string()]);
    mark_subtree_styles_dirty(&mut h.tree, card_id);

    apply_styles_dirty(&mut h.tree, &engine);
    h.layout(800.0, 600.0);

    assert_eq!(
        icon_size_of(&h),
        None,
        "After ancestor class changes, descendant icon-size rule must no longer apply"
    );
}

#[test]
fn dirty_path_picks_up_new_descendant_rule_after_stylesheet_swap() {
    // Apply stylesheet without the rule first, then swap to one with it.
    // Dirty-bypass must invalidate everything when the stylesheet changes.
    use syngui::mss::cascade::{apply_styles_dirty, mark_subtree_styles_dirty};

    let widget = build_stat_card_tree();
    let mut h = TestHarness::new(widget);

    let _ = h.apply_mss_dirty("Icon { color: red; }");
    h.layout(800.0, 600.0);
    assert_eq!(icon_size_of(&h), None, "no rule yet → no icon-size");

    let new_stylesheet = syngui::mss::parse_stylesheet_str(
        &format!(".stat-card Icon {{ icon-size: {}; }}", STAT_CARD_ICON_SIZE)
    ).expect("parse");
    let mut engine = syngui::mss::StyleEngine::new(new_stylesheet);
    // Caller must invalidate the entire tree on stylesheet swap.
    let root = h.root_id;
    mark_subtree_styles_dirty(&mut h.tree, root);
    apply_styles_dirty(&mut h.tree, &mut engine);
    h.layout(800.0, 600.0);

    assert_eq!(
        icon_size_of(&h),
        Some(STAT_CARD_ICON_SIZE),
        "After stylesheet swap, descendant rule from new sheet must apply"
    );
}

#[test]
fn descendant_reapplies_when_ancestor_class_changes_via_set_classes() {
    // Real-world scenario: an ancestor's classes change (e.g. via a reactive
    // signal flipping `.theme-light` → `.theme-dark`). The fix in
    // `tree::update_element` propagates `styles_dirty` down the subtree;
    // here we exercise the lower-level `set_classes` API which mimics the
    // same effect by also marking the subtree dirty (via TestHarness).
    use syngui::mss::cascade::apply_styles_dirty;

    let mss = format!(
        ".old-card Icon {{ icon-size: 10; }} .new-card Icon {{ icon-size: {}; }}",
        STAT_CARD_ICON_SIZE
    );
    let widget: Box<dyn Widget> = Box::new(
        DecoratedBox::new().class("old-card").child(
            Column::new().child(Icon::new("MI_WAVES"))
        )
    );
    let mut h = TestHarness::new(widget);
    let engine = h.apply_mss_dirty(&mss);
    h.layout(800.0, 600.0);
    assert_eq!(icon_size_of(&h), Some(10.0), "precondition: .old-card Icon=10");

    let card_id = *h.find_by_class("old-card").first().expect("ancestor");
    h.set_classes(card_id, vec!["new-card".to_string()]);
    // Note: TestHarness::set_classes only marks the node itself dirty (mirrors
    // what `element.set_classes` does in production). Without subtree
    // invalidation, descendant rules would NOT re-evaluate. Now they should
    // because `tree::update_element` calls mark_subtree_styles_dirty in
    // the real reconcile path; here we simulate the same.
    syngui::mss::cascade::mark_subtree_styles_dirty(&mut h.tree, card_id);

    apply_styles_dirty(&mut h.tree, &engine);
    h.layout(800.0, 600.0);

    assert_eq!(
        icon_size_of(&h),
        Some(STAT_CARD_ICON_SIZE),
        "After ancestor class .old-card → .new-card, descendant rule of new class must apply"
    );
}

#[test]
fn full_and_dirty_paths_agree_for_descendant_rule() {
    // Same input through both cascade paths should produce identical
    // resolved `icon-size`. Catches future divergence between the two.
    let mss = format!(".stat-card Icon {{ icon-size: {}; }}", STAT_CARD_ICON_SIZE);

    let mut h_full = TestHarness::new(build_stat_card_tree());
    h_full.apply_mss(&mss);
    h_full.layout(800.0, 600.0);

    let mut h_dirty = TestHarness::new(build_stat_card_tree());
    h_dirty.apply_mss_dirty(&mss);
    h_dirty.layout(800.0, 600.0);

    assert_eq!(icon_size_of(&h_full), icon_size_of(&h_dirty),
        "full and dirty cascade paths must produce identical icon-size");
}

// ─────────────────────────────────────────────────────────────────────────
// Helpers
// ─────────────────────────────────────────────────────────────────────────

fn harness_icon_padding(h: &TestHarness) -> f32 {
    let id = *h.find_by_type_name("Icon").first().expect("icon");
    h.element_mss(id).and_then(|m| m.padding_top).unwrap_or(0.0)
}
