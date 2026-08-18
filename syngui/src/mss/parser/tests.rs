mod tests {
    use super::super::MssParser;
    use super::super::super::stylesheet::{Selector, SelectorPart, Combinator};
    use super::super::super::value::{StyleValue, Unit, Dimension};

    #[test]
    fn test_parse_simple_class() {
        let mut parser = MssParser::new(".card { background: #fff; }");
        let (sheet, _) = parser.parse().unwrap();
        assert_eq!(sheet.rules().len(), 1);
        assert_eq!(sheet.rules()[0].selector, Selector::Class("card".to_string()));
    }

    #[test]
    fn test_parse_simple_element() {
        let mut parser = MssParser::new("Button { border-radius: 8px; }");
        let (sheet, _) = parser.parse().unwrap();
        assert_eq!(sheet.rules().len(), 1);
        assert_eq!(sheet.rules()[0].selector, Selector::Element("Button".to_string()));
    }

    #[test]
    fn test_parse_class_pseudo() {
        let mut parser = MssParser::new(".card:hover { background: #eee; }");
        let (sheet, _) = parser.parse().unwrap();
        assert_eq!(sheet.rules().len(), 1);
        assert_eq!(
            sheet.rules()[0].selector,
            Selector::ClassPseudo("card".to_string(), "hover".to_string())
        );
    }

    #[test]
    fn test_parse_element_pseudo() {
        let mut parser = MssParser::new("Button:hover { background: #eee; }");
        let (sheet, _) = parser.parse().unwrap();
        assert_eq!(sheet.rules().len(), 1);
        assert_eq!(
            sheet.rules()[0].selector,
            Selector::ElementPseudo("Button".to_string(), "hover".to_string())
        );
    }

    #[test]
    fn test_parse_comma_group() {
        let mut parser = MssParser::new(".input, .textarea { border: 1px; }");
        let (sheet, _) = parser.parse().unwrap();
        assert_eq!(sheet.rules().len(), 1);
        match &sheet.rules()[0].selector {
            Selector::Group(chains) => {
                assert_eq!(chains.len(), 2);
                assert_eq!(chains[0].target(), &SelectorPart::Class("input".to_string()));
                assert_eq!(chains[1].target(), &SelectorPart::Class("textarea".to_string()));
            }
            other => panic!("Expected Group, got {:?}", other),
        }
    }

    #[test]
    fn test_parse_descendant_combinator() {
        let mut parser = MssParser::new(".card .title { font-size: 20px; }");
        let (sheet, _) = parser.parse().unwrap();
        assert_eq!(sheet.rules().len(), 1);
        match &sheet.rules()[0].selector {
            Selector::Complex(chain) => {
                assert_eq!(chain.segments.len(), 2);
                assert_eq!(chain.segments[0], SelectorPart::Class("card".to_string()));
                assert_eq!(chain.segments[1], SelectorPart::Class("title".to_string()));
                assert_eq!(chain.combinators, vec![Combinator::Descendant]);
            }
            other => panic!("Expected Complex, got {:?}", other),
        }
    }

    #[test]
    fn test_parse_child_combinator() {
        let mut parser = MssParser::new(".sidebar > .item { padding: 12px; }");
        let (sheet, _) = parser.parse().unwrap();
        assert_eq!(sheet.rules().len(), 1);
        match &sheet.rules()[0].selector {
            Selector::Complex(chain) => {
                assert_eq!(chain.segments.len(), 2);
                assert_eq!(chain.segments[0], SelectorPart::Class("sidebar".to_string()));
                assert_eq!(chain.segments[1], SelectorPart::Class("item".to_string()));
                assert_eq!(chain.combinators, vec![Combinator::Child]);
            }
            other => panic!("Expected Complex, got {:?}", other),
        }
    }

    #[test]
    fn test_parse_adjacent_sibling() {
        let mut parser = MssParser::new(".button + .button { margin-left: 8px; }");
        let (sheet, _) = parser.parse().unwrap();
        assert_eq!(sheet.rules().len(), 1);
        match &sheet.rules()[0].selector {
            Selector::Complex(chain) => {
                assert_eq!(chain.segments.len(), 2);
                assert_eq!(chain.combinators, vec![Combinator::AdjacentSibling]);
            }
            other => panic!("Expected Complex, got {:?}", other),
        }
    }

    #[test]
    fn test_parse_nested_rule() {
        let input = r#"
            .card {
                background: #fff;
                .title {
                    font-size: 20px;
                }
            }
        "#;
        let mut parser = MssParser::new(input);
        let (sheet, _) = parser.parse().unwrap();
        assert_eq!(sheet.rules().len(), 2);
        assert_eq!(sheet.rules()[0].selector, Selector::Class("card".to_string()));
        match &sheet.rules()[1].selector {
            Selector::Complex(chain) => {
                assert_eq!(chain.segments.len(), 2);
                assert_eq!(chain.segments[0], SelectorPart::Class("card".to_string()));
                assert_eq!(chain.segments[1], SelectorPart::Class("title".to_string()));
                assert_eq!(chain.combinators, vec![Combinator::Descendant]);
            }
            other => panic!("Expected Complex, got {:?}", other),
        }
    }

    #[test]
    fn test_parse_nested_pseudo() {
        let input = r#"
            Button {
                background: #333;
                &:hover {
                    background: #555;
                }
            }
        "#;
        let mut parser = MssParser::new(input);
        let (sheet, _) = parser.parse().unwrap();
        assert_eq!(sheet.rules().len(), 2);
        assert_eq!(sheet.rules()[0].selector, Selector::Element("Button".to_string()));
        assert_eq!(
            sheet.rules()[1].selector,
            Selector::ElementPseudo("Button".to_string(), "hover".to_string())
        );
    }

    #[test]
    fn test_parse_nested_child_combinator() {
        let input = r#"
            .sidebar {
                background: #eee;
                > .item {
                    padding: 12px;
                }
            }
        "#;
        let mut parser = MssParser::new(input);
        let (sheet, _) = parser.parse().unwrap();
        assert_eq!(sheet.rules().len(), 2);
        match &sheet.rules()[1].selector {
            Selector::Complex(chain) => {
                assert_eq!(chain.segments.len(), 2);
                assert_eq!(chain.segments[0], SelectorPart::Class("sidebar".to_string()));
                assert_eq!(chain.segments[1], SelectorPart::Class("item".to_string()));
                assert_eq!(chain.combinators, vec![Combinator::Child]);
            }
            other => panic!("Expected Complex, got {:?}", other),
        }
    }

    #[test]
    fn test_parse_comma_elements() {
        let mut parser = MssParser::new("TextField, SpinBox, DatePicker { border-radius: 8px; }");
        let (sheet, _) = parser.parse().unwrap();
        assert_eq!(sheet.rules().len(), 1);
        match &sheet.rules()[0].selector {
            Selector::Group(chains) => {
                assert_eq!(chains.len(), 3);
                assert_eq!(chains[0].target(), &SelectorPart::Element("TextField".to_string()));
                assert_eq!(chains[1].target(), &SelectorPart::Element("SpinBox".to_string()));
                assert_eq!(chains[2].target(), &SelectorPart::Element("DatePicker".to_string()));
            }
            other => panic!("Expected Group, got {:?}", other),
        }
    }

    #[test]
    fn test_parse_complex_with_pseudo() {
        let mut parser = MssParser::new(".card .title:hover { color: red; }");
        let (sheet, _) = parser.parse().unwrap();
        match &sheet.rules()[0].selector {
            Selector::Complex(chain) => {
                assert_eq!(chain.segments.len(), 2);
                assert_eq!(chain.pseudo, Some("hover".to_string()));
                assert_eq!(chain.combinators, vec![Combinator::Descendant]);
            }
            other => panic!("Expected Complex, got {:?}", other),
        }
    }

    #[test]
    fn test_parse_three_level_chain() {
        let mut parser = MssParser::new(".a > .b .c { color: red; }");
        let (sheet, _) = parser.parse().unwrap();
        match &sheet.rules()[0].selector {
            Selector::Complex(chain) => {
                assert_eq!(chain.segments.len(), 3);
                assert_eq!(chain.combinators, vec![Combinator::Child, Combinator::Descendant]);
            }
            other => panic!("Expected Complex, got {:?}", other),
        }
    }

    #[test]
    fn test_variables_still_work() {
        let input = r#"
            :root { --bg: #fff; }
            .card { background: var(--bg); }
        "#;
        let mut parser = MssParser::new(input);
        let (sheet, _) = parser.parse().unwrap();
        assert!(sheet.get_variable("--bg").is_some());
        assert_eq!(sheet.rules().len(), 1);
    }

    #[test]
    fn test_keyframes_still_work() {
        let input = r#"
            @keyframes fade {
                from { opacity: 0; }
                to { opacity: 1; }
            }
        "#;
        let mut parser = MssParser::new(input);
        let (sheet, _) = parser.parse().unwrap();
        assert!(sheet.get_keyframes("fade").is_some());
    }

    #[test]
    fn test_parse_gallery_component_mss() {
        let content = concat!(
            ":root { --bg-base: #f8fafc; --bg-surface: #ffffff; --bg-overlay: #f1f5f9; --bg-elevated: #e2e8f0; --text: #1e293b; --text-subtle: #64748b; --text-muted: #94a3b8; --accent: #3b82f6; --accent-hover: #2563eb; --success: #22c55e; --warning: #f59e0b; --error: #ef4444; --info: #06b6d4; --border: #e2e8f0; --shadow-color: rgba(0,0,0,0.08); --header-bg: #1e293b; --header-text: #ffffff; --sidebar-bg: #ffffff; --input-bg: #ffffff; --section-hover: #f8fafc; --button-bg: #ffffff; --button-hover-text: #ffffff; --button-pressed: #2563eb; --focus-ring: rgba(59,130,246,0.15); --purple: #8b5cf6; --pink: #ec4899; --orange: #f97316; --teal: #14b8a6; --indigo: #6366f1; --blue-muted: #dbeafe; --green-muted: #d1fae5; --amber-muted: #fef3c7; --purple-muted: #ede9fe; --red-muted: #fee2e2; --chart-grid: #e2e8f0; --tooltip-bg: #1e293b; --tooltip-border: #334155; --chart-shadow: rgba(0,0,0,0.06); --glass-bg: rgba(255,255,255,0.15); --glass-border: rgba(255,255,255,0.25); --glass-dark-bg: rgba(0,0,0,0.2); --glass-dark-border: rgba(255,255,255,0.1); --fx-shadow: rgba(0,0,0,0.12); --fx-shadow-md: rgba(0,0,0,0.18); --fx-shadow-lg: rgba(0,0,0,0.22); }\n",
            include_str!("../../../../app/widget_gallery_mss/styles/components/layout.mss"), "\n",
            include_str!("../../../../app/widget_gallery_mss/styles/components/widgets.mss"), "\n",
            include_str!("../../../../app/widget_gallery_mss/styles/components/keyframes.mss"), "\n",
            include_str!("../../../../app/widget_gallery_mss/styles/pages/gradients.mss"),
        );
        let mut parser = MssParser::new(content);
        let (sheet, _) = parser.parse().unwrap();
        assert!(sheet.get_variable("--bg-base").is_some());
        assert!(sheet.get_variable("--accent").is_some());
        assert!(sheet.rules().len() > 30);
        assert!(sheet.get_keyframes("slide-right").is_some());
        assert!(sheet.get_keyframes("combined").is_some());
    }

    #[test]
    fn test_nested_nesting_preserves_semantics() {
        let nested = r#"
            Button {
                background: #333;
                &:hover { background: #555; }
                &:pressed { background: #222; }
            }
        "#;
        let mut parser = MssParser::new(nested);
        let (sheet, _) = parser.parse().unwrap();

        assert_eq!(sheet.rules().len(), 3);
        assert_eq!(sheet.rules()[0].selector, Selector::Element("Button".to_string()));
        assert_eq!(sheet.rules()[1].selector, Selector::ElementPseudo("Button".to_string(), "hover".to_string()));
        assert_eq!(sheet.rules()[2].selector, Selector::ElementPseudo("Button".to_string(), "pressed".to_string()));
    }

    #[test]
    fn test_general_sibling_combinator() {
        let mut parser = MssParser::new(".a ~ .b { color: red; }");
        let (sheet, _) = parser.parse().unwrap();
        match &sheet.rules()[0].selector {
            Selector::Complex(chain) => {
                assert_eq!(chain.combinators, vec![Combinator::GeneralSibling]);
            }
            other => panic!("Expected Complex, got {:?}", other),
        }
    }

    // test_parse_volna_full_styles удалён: include_str! ссылался на
    // app/volna_plus, которого больше нет в репозитории — тестовый таргет
    // библиотеки не собирался целиком.

    #[test]
    fn test_parse_volna_configurator_style1() {
        let input = r#"
:root {
    --cfg-card-bg: #1E1E2E;
    --cfg-card-border: #2A2A3E;
    --cfg-input-bg: #16162A;
    --cfg-input-border: #2A2A3E;
    --cfg-text-primary: #E0E0F0;
    --cfg-text-secondary: #A0A0B8;
    --cfg-text-muted: #6C6C8A;
    --cfg-accent: #6C63FF;
    --cfg-active: #4CAF50;
    --cfg-inactive: #F44336;
    --cfg-hot: #FF6B8A;
    --cfg-hot-bg: #FF6B8A26;
    --cfg-cold: #4DD0E1;
    --cfg-cold-bg: #4DD0E126;
}

.cfg1-card {
    background: var(--cfg-card-bg);
    border-radius: 10px;
    border: 1px solid var(--cfg-card-border);
    width: 700px;
    padding: 20px;
}

.cfg1-status {
    font-size: 10px;
    font-weight: 700;
    border-radius: 4px;
    padding: 3px 8px;
}

.cfg1-status-active {
    color: var(--cfg-active);
    background: rgba(76, 175, 80, 0.15);
}

.cfg1-slider-cold {
    background: var(--cfg-cold);
    height: 6px;
    width: 30px;
    border-radius: 3px 0 0 3px;
}

.cfg1-slider-knob {
    background: var(--cfg-text-primary);
    border: 2px solid var(--cfg-text-secondary);
    border-radius: 7px;
    width: 14px;
    height: 14px;
    margin-left: 24px;
}

.cfg1-card:hover {
    border-color: var(--cfg-accent);
}

.cfg1-delete {
    transition: opacity 150ms ease;
    opacity: 0.7;
}

.cfg1-delete:hover {
    opacity: 1.0;
}
"#;
        let mut parser = MssParser::new(input);
        match parser.parse() {
            Ok((sheet, _warnings)) => {
                assert!(sheet.rules().len() >= 6, "Expected at least 6 rules, got {}", sheet.rules().len());
            }
            Err(e) => panic!("Failed to parse configurator style1: {:?}", e),
        }
    }

    #[test]
    fn test_parse_linear_gradient() {
        let input = r#".box { background: linear-gradient(90deg, #ff0000, #0000ff); }"#;
        let mut parser = MssParser::new(input);
        let (sheet, _) = parser.parse().unwrap();
        let rule = &sheet.rules()[0];
        let val = rule.declarations.get("background").unwrap();
        match val {
            StyleValue::Gradient(crate::core::Gradient::Linear { angle_deg, stops }) => {
                assert_eq!(*angle_deg, 90.0);
                assert_eq!(stops.len(), 2);
            }
            other => panic!("Expected Gradient::Linear, got {:?}", other),
        }
    }

    #[test]
    fn test_parse_linear_gradient_direction() {
        let input = r#".box { background: linear-gradient(to right, red, blue); }"#;
        let mut parser = MssParser::new(input);
        let (sheet, _) = parser.parse().unwrap();
        let val = sheet.rules()[0].declarations.get("background").unwrap();
        match val {
            StyleValue::Gradient(crate::core::Gradient::Linear { angle_deg, .. }) => {
                assert_eq!(*angle_deg, 90.0);
            }
            other => panic!("Expected Gradient::Linear, got {:?}", other),
        }
    }

    #[test]
    fn test_parse_linear_gradient_multi_stop() {
        let input = r#".box { background: linear-gradient(135deg, #ef4444, #f97316, #eab308, #22c55e); }"#;
        let mut parser = MssParser::new(input);
        let (sheet, _) = parser.parse().unwrap();
        let val = sheet.rules()[0].declarations.get("background").unwrap();
        match val {
            StyleValue::Gradient(crate::core::Gradient::Linear { stops, .. }) => {
                assert_eq!(stops.len(), 4);
            }
            other => panic!("Expected Gradient::Linear, got {:?}", other),
        }
    }

    #[test]
    fn test_parse_linear_gradient_with_positions() {
        let input = r#".box { background: linear-gradient(180deg, #000000 0%, #ffffff 100%); }"#;
        let mut parser = MssParser::new(input);
        let (sheet, _) = parser.parse().unwrap();
        let val = sheet.rules()[0].declarations.get("background").unwrap();
        match val {
            StyleValue::Gradient(crate::core::Gradient::Linear { stops, .. }) => {
                assert_eq!(stops.len(), 2);
                assert_eq!(stops[0].position, Some(0.0));
                assert_eq!(stops[1].position, Some(1.0));
            }
            other => panic!("Expected Gradient::Linear, got {:?}", other),
        }
    }

    #[test]
    fn test_parse_radial_gradient() {
        let input = r#".box { background: radial-gradient(circle at center, #ffffff, #3b82f6); }"#;
        let mut parser = MssParser::new(input);
        let (sheet, _) = parser.parse().unwrap();
        let val = sheet.rules()[0].declarations.get("background").unwrap();
        match val {
            StyleValue::Gradient(crate::core::Gradient::Radial { shape, center, stops, .. }) => {
                assert_eq!(*shape, crate::core::GradientShape::Circle);
                assert_eq!(*center, (0.5, 0.5));
                assert_eq!(stops.len(), 2);
            }
            other => panic!("Expected Gradient::Radial, got {:?}", other),
        }
    }

    #[test]
    fn test_parse_radial_gradient_ellipse() {
        let input = r#".box { background: radial-gradient(ellipse at 30% 70%, #fde68a, #f59e0b); }"#;
        let mut parser = MssParser::new(input);
        let (sheet, _) = parser.parse().unwrap();
        let val = sheet.rules()[0].declarations.get("background").unwrap();
        match val {
            StyleValue::Gradient(crate::core::Gradient::Radial { shape, center, stops, .. }) => {
                assert_eq!(*shape, crate::core::GradientShape::Ellipse);
                assert!((center.0 - 0.3).abs() < 0.01);
                assert!((center.1 - 0.7).abs() < 0.01);
                assert_eq!(stops.len(), 2);
            }
            other => panic!("Expected Gradient::Radial, got {:?}", other),
        }
    }

    #[test]
    fn test_parse_gallery_gradient_styles() {
        let input = include_str!("../../../../app/widget_gallery_mss/styles/pages/gradients.mss");
        let mut parser = MssParser::new(input);
        let (sheet, _warnings) = parser.parse().unwrap();
        let gradient_rules: Vec<_> = sheet.rules().iter()
            .filter(|r| r.declarations.values().any(|v| matches!(v, StyleValue::Gradient(_))))
            .collect();
        assert!(gradient_rules.len() >= 10, "Expected at least 10 gradient rules, got {}", gradient_rules.len());
    }

    #[test]
    fn test_compound_element_class() {
        let mut parser = MssParser::new("Button.btn-number { background: #2a2a4a; }");
        let (sheet, _) = parser.parse().unwrap();
        assert_eq!(sheet.rules().len(), 1);
        match &sheet.rules()[0].selector {
            Selector::Complex(chain) => {
                assert_eq!(chain.segments.len(), 1);
                assert_eq!(chain.segments[0], SelectorPart::Compound {
                    element: Some("Button".to_string()),
                    id: None,
                    classes: vec!["btn-number".to_string()],
                });
                assert_eq!(chain.pseudo, None);
            }
            other => panic!("Expected Complex with compound, got {:?}", other),
        }
    }

    #[test]
    fn test_compound_element_class_pseudo() {
        let mut parser = MssParser::new("Button.btn-number:hover { background: #3a3a5a; }");
        let (sheet, _) = parser.parse().unwrap();
        match &sheet.rules()[0].selector {
            Selector::Complex(chain) => {
                assert_eq!(chain.segments.len(), 1);
                assert_eq!(chain.segments[0], SelectorPart::Compound {
                    element: Some("Button".to_string()),
                    id: None,
                    classes: vec!["btn-number".to_string()],
                });
                assert_eq!(chain.pseudo, Some("hover".to_string()));
            }
            other => panic!("Expected Complex with compound+pseudo, got {:?}", other),
        }
    }

    #[test]
    fn test_compound_two_classes() {
        let mut parser = MssParser::new(".foo.bar { color: red; }");
        let (sheet, _) = parser.parse().unwrap();
        match &sheet.rules()[0].selector {
            Selector::Complex(chain) => {
                assert_eq!(chain.segments.len(), 1);
                assert_eq!(chain.segments[0], SelectorPart::Compound {
                    element: None,
                    id: None,
                    classes: vec!["foo".to_string(), "bar".to_string()],
                });
            }
            other => panic!("Expected Complex with compound, got {:?}", other),
        }
    }

    #[test]
    fn test_compound_in_descendant_chain() {
        let mut parser = MssParser::new(".card Button.active { color: red; }");
        let (sheet, _) = parser.parse().unwrap();
        match &sheet.rules()[0].selector {
            Selector::Complex(chain) => {
                assert_eq!(chain.segments.len(), 2);
                assert_eq!(chain.segments[0], SelectorPart::Class("card".to_string()));
                assert_eq!(chain.segments[1], SelectorPart::Compound {
                    element: Some("Button".to_string()),
                    id: None,
                    classes: vec!["active".to_string()],
                });
                assert_eq!(chain.combinators, vec![Combinator::Descendant]);
            }
            other => panic!("Expected Complex, got {:?}", other),
        }
    }

    #[test]
    fn test_descendant_still_works_with_space() {
        let mut parser = MssParser::new("Button .inner { color: red; }");
        let (sheet, _) = parser.parse().unwrap();
        match &sheet.rules()[0].selector {
            Selector::Complex(chain) => {
                assert_eq!(chain.segments.len(), 2);
                assert_eq!(chain.segments[0], SelectorPart::Element("Button".to_string()));
                assert_eq!(chain.segments[1], SelectorPart::Class("inner".to_string()));
                assert_eq!(chain.combinators, vec![Combinator::Descendant]);
            }
            other => panic!("Expected Complex descendant, got {:?}", other),
        }
    }

    #[test]
    fn test_parse_calculator_mss() {
        let input = include_str!("../../../../app/calculator/styles/calculator.mss");
        let mut parser = MssParser::new(input);
        let (sheet, warnings) = parser.parse().unwrap();
        assert!(warnings.is_empty(), "Unexpected warnings: {:?}", warnings);
        assert!(sheet.rules().len() >= 10, "Expected at least 10 rules, got {}", sheet.rules().len());
    }

    #[test]
    fn test_border_shorthand_expands_to_width_and_color() {
        let mut parser = MssParser::new("Button { border: 2px solid #ff0000; }");
        let (sheet, warnings) = parser.parse().unwrap();
        assert!(warnings.is_empty(), "Unexpected warnings: {:?}", warnings);
        let rule = &sheet.rules()[0];
        assert!(!rule.declarations.contains_key("border"), "raw `border` should be gone");
        assert!(
            !rule.declarations.contains_key("border-width"),
            "`border-width` must be expanded into per-side longhand"
        );
        for side in ["border-top-width", "border-right-width", "border-bottom-width", "border-left-width"] {
            match rule.declarations.get(side) {
                Some(StyleValue::Length(v, Unit::Px)) => assert_eq!(*v, 2.0, "{side}"),
                other => panic!("{side} missing/not px: {:?}", other),
            }
        }
        match rule.declarations.get("border-color").unwrap() {
            StyleValue::Color(c) => assert_eq!((c.r, c.g, c.b), (0xff, 0x00, 0x00)),
            other => panic!("border-color not a color: {:?}", other),
        }
    }

    fn decl(props: &str) -> std::collections::HashMap<String, StyleValue> {
        let src = format!(".c {{ {props} }}");
        let mut parser = MssParser::new(&src);
        let (sheet, warnings) = parser.parse().unwrap();
        assert!(warnings.is_empty(), "Unexpected warnings: {:?}", warnings);
        sheet.rules()[0].declarations.clone()
    }

    fn px(d: &std::collections::HashMap<String, StyleValue>, key: &str) -> f32 {
        match d.get(key).unwrap_or_else(|| panic!("missing {key}")) {
            StyleValue::Length(v, Unit::Px) => *v,
            StyleValue::Number(v) => *v,
            other => panic!("{key} not px: {:?}", other),
        }
    }

    #[test]
    fn padding_shorthand_1_value_expands_to_four_longhand() {
        let d = decl("padding: 10px;");
        assert!(!d.contains_key("padding"), "shorthand should be gone");
        assert_eq!(px(&d, "padding-top"), 10.0);
        assert_eq!(px(&d, "padding-right"), 10.0);
        assert_eq!(px(&d, "padding-bottom"), 10.0);
        assert_eq!(px(&d, "padding-left"), 10.0);
    }

    #[test]
    fn padding_shorthand_2_values_vh() {
        let d = decl("padding: 10px 20px;");
        assert_eq!(px(&d, "padding-top"), 10.0);
        assert_eq!(px(&d, "padding-right"), 20.0);
        assert_eq!(px(&d, "padding-bottom"), 10.0);
        assert_eq!(px(&d, "padding-left"), 20.0);
    }

    #[test]
    fn padding_shorthand_3_values_thb() {
        let d = decl("padding: 10px 20px 30px;");
        assert_eq!(px(&d, "padding-top"), 10.0);
        assert_eq!(px(&d, "padding-right"), 20.0);
        assert_eq!(px(&d, "padding-bottom"), 30.0);
        assert_eq!(px(&d, "padding-left"), 20.0);
    }

    #[test]
    fn padding_shorthand_4_values_trbl() {
        let d = decl("padding: 10px 20px 30px 40px;");
        assert_eq!(px(&d, "padding-top"), 10.0);
        assert_eq!(px(&d, "padding-right"), 20.0);
        assert_eq!(px(&d, "padding-bottom"), 30.0);
        assert_eq!(px(&d, "padding-left"), 40.0);
    }

    #[test]
    fn padding_shorthand_zero_without_unit() {
        let d = decl("padding: 0 12px;");
        assert_eq!(px(&d, "padding-top"), 0.0);
        assert_eq!(px(&d, "padding-right"), 12.0);
        assert_eq!(px(&d, "padding-bottom"), 0.0);
        assert_eq!(px(&d, "padding-left"), 12.0);
    }

    #[test]
    fn margin_shorthand_2_values() {
        let d = decl("margin: 10px 20px;");
        assert!(!d.contains_key("margin"));
        assert_eq!(px(&d, "margin-top"), 10.0);
        assert_eq!(px(&d, "margin-right"), 20.0);
        assert_eq!(px(&d, "margin-bottom"), 10.0);
        assert_eq!(px(&d, "margin-left"), 20.0);
    }

    #[test]
    fn margin_shorthand_4_values() {
        let d = decl("margin: 1px 2px 3px 4px;");
        assert_eq!(px(&d, "margin-top"), 1.0);
        assert_eq!(px(&d, "margin-right"), 2.0);
        assert_eq!(px(&d, "margin-bottom"), 3.0);
        assert_eq!(px(&d, "margin-left"), 4.0);
    }

    #[test]
    fn border_width_shorthand_2_values() {
        let d = decl("border-width: 2px 4px;");
        assert!(!d.contains_key("border-width"));
        assert_eq!(px(&d, "border-top-width"), 2.0);
        assert_eq!(px(&d, "border-right-width"), 4.0);
        assert_eq!(px(&d, "border-bottom-width"), 2.0);
        assert_eq!(px(&d, "border-left-width"), 4.0);
    }

    #[test]
    fn border_width_single_value_also_expands() {
        let d = decl("border-width: 1px;");
        assert!(!d.contains_key("border-width"));
        assert_eq!(px(&d, "border-top-width"), 1.0);
        assert_eq!(px(&d, "border-left-width"), 1.0);
    }

    #[test]
    fn border_radius_shorthand_2_values_to_four_corners() {
        let d = decl("border-radius: 10px 5px;");
        assert!(!d.contains_key("border-radius"));
        assert_eq!(px(&d, "border-top-left-radius"), 10.0);
        assert_eq!(px(&d, "border-top-right-radius"), 5.0);
        assert_eq!(px(&d, "border-bottom-right-radius"), 10.0);
        assert_eq!(px(&d, "border-bottom-left-radius"), 5.0);
    }

    #[test]
    fn border_radius_shorthand_4_values() {
        let d = decl("border-radius: 1px 2px 3px 4px;");
        assert_eq!(px(&d, "border-top-left-radius"), 1.0);
        assert_eq!(px(&d, "border-top-right-radius"), 2.0);
        assert_eq!(px(&d, "border-bottom-right-radius"), 3.0);
        assert_eq!(px(&d, "border-bottom-left-radius"), 4.0);
    }

    #[test]
    fn padding_shorthand_with_var_expands_to_four_vars() {
        let d = decl("padding: var(--p);");
        for side in ["padding-top", "padding-right", "padding-bottom", "padding-left"] {
            match d.get(side) {
                Some(StyleValue::Var(n)) => assert_eq!(n, "--p"),
                other => panic!("{side} should be Var(--p), got {:?}", other),
            }
        }
    }

    #[test]
    fn per_side_longhand_overrides_earlier_shorthand_in_same_rule() {
        let d = decl("padding: 10px; padding-left: 0;");
        assert_eq!(px(&d, "padding-top"), 10.0);
        assert_eq!(px(&d, "padding-right"), 10.0);
        assert_eq!(px(&d, "padding-bottom"), 10.0);
        assert_eq!(px(&d, "padding-left"), 0.0);
    }

    #[test]
    fn earlier_longhand_is_overwritten_by_later_shorthand() {
        let d = decl("padding-left: 0; padding: 10px;");
        assert_eq!(px(&d, "padding-left"), 10.0);
    }

    #[test]
    fn test_border_shorthand_with_var_keeps_var() {
        let mut parser = MssParser::new(":root { --c: #00ff00; } Button { border: 1px solid var(--c); }");
        let (sheet, warnings) = parser.parse().unwrap();
        assert!(warnings.is_empty(), "Unexpected warnings: {:?}", warnings);
        let rule = sheet.rules().iter().find(|r| matches!(&r.selector, Selector::Element(n) if n == "Button")).unwrap();
        assert!(rule.declarations.contains_key("border-color"));
        match rule.declarations.get("border-color").unwrap() {
            StyleValue::Var(name) => assert_eq!(name, "--c"),
            other => panic!("border-color should be Var, got: {:?}", other),
        }
    }

    #[test]
    fn test_var_with_color_fallback_parses_standalone() {
        let mut parser = MssParser::new("Button { background-color: var(--missing, #f44336); }");
        let (sheet, warnings) = parser.parse().unwrap();
        assert!(warnings.is_empty(), "Unexpected warnings: {:?}", warnings);
        let rule = sheet.rules().iter().find(|r| matches!(&r.selector, Selector::Element(n) if n == "Button")).unwrap();
        match rule.declarations.get("background-color").unwrap() {
            StyleValue::VarWithFallback(name, fallback) => {
                assert_eq!(name, "--missing");
                match fallback.as_ref() {
                    StyleValue::Color(c) => {
                        assert_eq!((c.r, c.g, c.b), (0xf4, 0x43, 0x36));
                    }
                    other => panic!("fallback must be Color, got {:?}", other),
                }
            }
            other => panic!("expected VarWithFallback, got {:?}", other),
        }
    }

    #[test]
    fn test_var_with_rgba_fallback_handles_nested_parens() {
        let mut parser = MssParser::new("Button { color: var(--c, rgba(255, 0, 0, 0.5)); }");
        let (sheet, warnings) = parser.parse().unwrap();
        assert!(warnings.is_empty(), "Unexpected warnings: {:?}", warnings);
        let rule = sheet.rules().iter().find(|r| matches!(&r.selector, Selector::Element(n) if n == "Button")).unwrap();
        match rule.declarations.get("color").unwrap() {
            StyleValue::VarWithFallback(_, fallback) => match fallback.as_ref() {
                StyleValue::Color(c) => {
                    assert_eq!((c.r, c.g, c.b), (255, 0, 0));
                    assert_eq!(c.a, 127);
                }
                other => panic!("fallback должен быть Color, got {:?}", other),
            },
            other => panic!("expected VarWithFallback, got {:?}", other),
        }
    }

    #[test]
    fn test_var_with_fallback_resolves_to_fallback_when_var_missing() {
        use crate::mss::style_engine::{StyleEngine, StyleContext};
        let mut parser = MssParser::new(".btn { background-color: var(--missing, #abcdef); }");
        let (sheet, _w) = parser.parse().unwrap();
        let mut engine = StyleEngine::new(sheet);
        let ctx = StyleContext { classes: vec!["btn".into()], ..Default::default() };
        let computed = engine.compute_style(&ctx);
        match computed.get("background-color").unwrap() {
            StyleValue::Color(c) => assert_eq!((c.r, c.g, c.b), (0xab, 0xcd, 0xef)),
            other => panic!("Expected resolved color, got {:?}", other),
        }
    }

    #[test]
    fn test_var_with_fallback_prefers_root_when_var_present() {
        use crate::mss::style_engine::{StyleEngine, StyleContext};
        let mut parser = MssParser::new(
            ":root { --bg: #112233; } .btn { background-color: var(--bg, #abcdef); }"
        );
        let (sheet, _w) = parser.parse().unwrap();
        let mut engine = StyleEngine::new(sheet);
        let ctx = StyleContext { classes: vec!["btn".into()], ..Default::default() };
        let computed = engine.compute_style(&ctx);
        match computed.get("background-color").unwrap() {
            StyleValue::Color(c) => assert_eq!((c.r, c.g, c.b), (0x11, 0x22, 0x33)),
            other => panic!("Expected :root color, got {:?}", other),
        }
    }

    #[test]
    fn test_border_style_parses_without_warnings() {
        let mut parser = MssParser::new(
            "Button { border-style: dashed; border-top-style: solid; border-left-style: none; }"
        );
        let (sheet, warnings) = parser.parse().unwrap();
        assert!(warnings.is_empty(), "Unexpected parser warnings: {:?}", warnings);
        let rule = sheet.rules().iter().find(|r| matches!(&r.selector, Selector::Element(n) if n == "Button")).unwrap();
        assert!(rule.declarations.contains_key("border-style"));
        assert!(rule.declarations.contains_key("border-top-style"));
        assert!(rule.declarations.contains_key("border-left-style"));

        let known = crate::mss::fields::KNOWN_PROPERTIES_FOR_TESTS;
        assert!(known.contains(&"border-style"));
        assert!(known.contains(&"border-top-style"));
        assert!(known.contains(&"border-right-style"));
        assert!(known.contains(&"border-bottom-style"));
        assert!(known.contains(&"border-left-style"));
    }

    #[test]
    fn test_checked_pseudo_parses_separately_from_hover() {
        let mut parser = MssParser::new(
            "Toggle:hover { background: #ff0000; } \
             Toggle:checked { background: #00ff00; }"
        );
        let (sheet, warnings) = parser.parse().unwrap();
        assert!(warnings.is_empty(), "Unexpected warnings: {:?}", warnings);
        let hover_rule = sheet.rules().iter().find(|r| {
            matches!(&r.selector, Selector::ElementPseudo(n, p) if n == "Toggle" && p == "hover")
        }).expect("Toggle:hover must be parsed as ElementPseudo(Toggle, hover)");
        let checked_rule = sheet.rules().iter().find(|r| {
            matches!(&r.selector, Selector::ElementPseudo(n, p) if n == "Toggle" && p == "checked")
        }).expect("Toggle:checked must be parsed as ElementPseudo(Toggle, checked)");
        match hover_rule.declarations.get("background").unwrap() {
            StyleValue::Color(c) => assert_eq!((c.r, c.g, c.b), (0xff, 0x00, 0x00)),
            other => panic!("hover bg not a colour: {:?}", other),
        }
        match checked_rule.declarations.get("background").unwrap() {
            StyleValue::Color(c) => assert_eq!((c.r, c.g, c.b), (0x00, 0xff, 0x00)),
            other => panic!("checked bg not a colour: {:?}", other),
        }
    }

    fn parse_width_dim(input: &str) -> Dimension {
        let mut parser = MssParser::new(input);
        let (sheet, _) = parser.parse().unwrap();
        sheet.rules()[0]
            .declarations
            .get("width")
            .unwrap()
            .as_dimension()
            .expect("width must resolve to a Dimension")
    }

    #[test]
    fn parse_width_auto() {
        let d = parse_width_dim(".x { width: auto; }");
        assert_eq!(d, Dimension::Auto);
        assert!(d.is_auto());
        assert!(!d.is_intrinsic());
    }

    #[test]
    fn parse_width_fit_content() {
        let d = parse_width_dim(".x { width: fit-content; }");
        assert_eq!(d, Dimension::FitContent);
        assert!(d.is_intrinsic());
        assert!(!d.is_auto());
    }

    #[test]
    fn parse_width_max_content() {
        let d = parse_width_dim(".x { width: max-content; }");
        assert_eq!(d, Dimension::MaxContent);
        assert!(d.is_intrinsic());
    }

    #[test]
    fn parse_width_min_content() {
        let d = parse_width_dim(".x { width: min-content; }");
        assert_eq!(d, Dimension::MinContent);
        assert!(d.is_intrinsic());
    }

    #[test]
    fn parse_height_fit_content() {
        let mut parser = MssParser::new(".x { height: fit-content; }");
        let (sheet, _) = parser.parse().unwrap();
        let d = sheet.rules()[0]
            .declarations
            .get("height")
            .unwrap()
            .as_dimension()
            .unwrap();
        assert_eq!(d, Dimension::FitContent);
    }

    #[test]
    fn parse_px_still_works_after_keyword_branch() {
        let d = parse_width_dim(".x { width: 120px; }");
        assert_eq!(d, Dimension::Px(120.0));
    }

    #[test]
    fn parse_percent_still_works_after_keyword_branch() {
        let d = parse_width_dim(".x { width: 50%; }");
        assert_eq!(d, Dimension::Percent(50.0));
    }
}
