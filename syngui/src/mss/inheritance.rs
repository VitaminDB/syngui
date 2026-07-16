use super::style_engine::ComputedStyle;
use super::value::StyleValue;

pub const INHERITED_PROPERTIES: &[&str] = &[
    "color",
    "font-family",
    "font-size",
    "font-weight",
    "letter-spacing",
    "text-align",
    "text-vertical-align",
    "text-decoration",
    "text-transform",
    "text-shadow",
    "cursor",
    "line-height",
    "caret-color",
];

#[inline]
pub fn is_inherited(property: &str) -> bool {
    property.starts_with("--") || INHERITED_PROPERTIES.contains(&property)
}

pub fn resolve_cascade_keyword(
    value: &StyleValue,
    property: &str,
    parent_inherited: &ComputedStyle,
) -> StyleValue {
    match value {
        StyleValue::Inherit => parent_inherited
            .get(property)
            .cloned()
            .unwrap_or(StyleValue::None),
        StyleValue::Initial => StyleValue::None,
        StyleValue::Unset => {
            if is_inherited(property) {
                parent_inherited
                    .get(property)
                    .cloned()
                    .unwrap_or(StyleValue::None)
            } else {
                StyleValue::None
            }
        }
        other => other.clone(),
    }
}

pub fn extract_inherited(style: &ComputedStyle) -> ComputedStyle {
    let mut out = ComputedStyle::default();
    for (prop, val) in style.properties() {
        if is_inherited(prop) {
            out.set(prop, val.clone());
        }
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::mss::value::Color;

    #[test]
    fn test_is_inherited_whitelist() {
        assert!(is_inherited("color"));
        assert!(is_inherited("font-size"));
        assert!(is_inherited("font-family"));
        assert!(is_inherited("text-align"));
        assert!(is_inherited("cursor"));
        assert!(!is_inherited("padding"));
        assert!(!is_inherited("background"));
        assert!(!is_inherited("border-radius"));
        assert!(!is_inherited("width"));
    }

    #[test]
    fn test_resolve_inherit_picks_parent_value() {
        let mut parent = ComputedStyle::default();
        parent.set("color", StyleValue::Color(Color::rgb(255, 0, 0)));

        let resolved = resolve_cascade_keyword(&StyleValue::Inherit, "color", &parent);
        assert_eq!(resolved, StyleValue::Color(Color::rgb(255, 0, 0)));
    }

    #[test]
    fn test_resolve_inherit_missing_parent() {
        let parent = ComputedStyle::default();
        let resolved = resolve_cascade_keyword(&StyleValue::Inherit, "color", &parent);
        assert_eq!(resolved, StyleValue::None);
    }

    #[test]
    fn test_resolve_initial_always_none() {
        let mut parent = ComputedStyle::default();
        parent.set("color", StyleValue::Color(Color::rgb(255, 0, 0)));
        let resolved = resolve_cascade_keyword(&StyleValue::Initial, "color", &parent);
        assert_eq!(resolved, StyleValue::None);
    }

    #[test]
    fn test_resolve_unset_inherited_vs_not() {
        let mut parent = ComputedStyle::default();
        parent.set("color", StyleValue::Color(Color::rgb(1, 2, 3)));
        parent.set("padding", StyleValue::px(8.0));

        let resolved_color = resolve_cascade_keyword(&StyleValue::Unset, "color", &parent);
        assert_eq!(resolved_color, StyleValue::Color(Color::rgb(1, 2, 3)));

        let resolved_padding = resolve_cascade_keyword(&StyleValue::Unset, "padding", &parent);
        assert_eq!(resolved_padding, StyleValue::None);
    }

    #[test]
    fn test_resolve_non_keyword_passthrough() {
        let parent = ComputedStyle::default();
        let v = StyleValue::px(12.0);
        let resolved = resolve_cascade_keyword(&v, "font-size", &parent);
        assert_eq!(resolved, v);
    }

    #[test]
    fn test_custom_properties_are_inherited() {
        assert!(is_inherited("--md-text-color"));
        assert!(is_inherited("--anything"));
        assert!(is_inherited("--"));
        assert!(is_inherited("color"));
        assert!(!is_inherited("padding"));
    }

    #[test]
    fn test_extract_inherited_includes_custom_properties() {
        let mut s = ComputedStyle::default();
        s.set("--md-text-color", StyleValue::Color(Color::rgb(10, 20, 30)));
        s.set("--node-bg", StyleValue::String("transparent".to_string()));
        s.set("color", StyleValue::Color(Color::rgb(255, 255, 255)));
        s.set("padding", StyleValue::px(8.0));

        let inh = extract_inherited(&s);
        assert_eq!(
            inh.get("--md-text-color"),
            Some(&StyleValue::Color(Color::rgb(10, 20, 30)))
        );
        assert_eq!(
            inh.get("--node-bg"),
            Some(&StyleValue::String("transparent".to_string()))
        );
        assert_eq!(inh.get("color"), Some(&StyleValue::Color(Color::rgb(255, 255, 255))));
        assert_eq!(inh.get("padding"), None);
    }

    #[test]
    fn test_unset_on_custom_property_inherits_from_parent() {
        let mut parent = ComputedStyle::default();
        parent.set("--accent", StyleValue::Color(Color::rgb(100, 200, 50)));
        let resolved = resolve_cascade_keyword(&StyleValue::Unset, "--accent", &parent);
        assert_eq!(resolved, StyleValue::Color(Color::rgb(100, 200, 50)));
    }

    #[test]
    fn test_extract_inherited_filters_non_inherited() {
        let mut s = ComputedStyle::default();
        s.set("color", StyleValue::Color(Color::rgb(1, 2, 3)));
        s.set("font-size", StyleValue::px(14.0));
        s.set("padding", StyleValue::px(8.0));
        s.set("background", StyleValue::Color(Color::rgb(255, 255, 255)));

        let inh = extract_inherited(&s);
        assert_eq!(inh.get("color"), Some(&StyleValue::Color(Color::rgb(1, 2, 3))));
        assert_eq!(inh.get("font-size"), Some(&StyleValue::px(14.0)));
        assert_eq!(inh.get("padding"), None);
        assert_eq!(inh.get("background"), None);
    }
}
