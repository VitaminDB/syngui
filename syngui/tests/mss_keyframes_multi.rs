//! Регрессия краха 2026-09-01: фреймы `@keyframes` со списком позиций
//! (`0%, 100% { … }`) не парсились, хвост утекал в общий список правил
//! пустым селектором, а RuleIndex каскада паниковал в target().

use syngui::mss::{parse_stylesheet_str, Selector};
use syngui::testing::TestHarness;
use syngui::widgets::Text;

const SRC: &str = r#"
.pulse {
    opacity: 0.85;
}
@keyframes breathe {
    0%, 100% { opacity: 0.85; }
    50%      { opacity: 1.0; }
}
.after {
    color: #ff0000;
}
"#;

#[test]
fn multi_position_frames_parse_into_keyframes() {
    let sheet = parse_stylesheet_str(SRC).expect("должен парситься");
    // Ни одно правило не должно иметь пустой селектор.
    for rule in sheet.rules() {
        let empty = match &rule.selector {
            Selector::Complex(c) => c.segments.is_empty(),
            Selector::Group(chains) => chains.iter().any(|c| c.segments.is_empty()),
            _ => false,
        };
        assert!(!empty, "утёкшее правило с пустым селектором: {:?}", rule.selector);
    }
    // Оба обычных правила на месте (хвост не съеден восстановлением).
    assert_eq!(sheet.rules().len(), 2, "{:?}", sheet.rules());
    // Кейфреймы: три шага (0, 0.5, 1.0).
    let kf = sheet.get_keyframes("breathe").expect("keyframes breathe");
    let mut positions: Vec<f32> = kf.steps.iter().map(|s| s.position).collect();
    positions.sort_by(|a, b| a.partial_cmp(b).unwrap());
    assert_eq!(positions, vec![0.0, 0.5, 1.0]);
}

#[test]
fn cascade_survives_empty_selector_chains() {
    // Битый селектор, дающий пустую цепочку («, .x» → "" + ".x").
    let src = ".a { color: #fff; }\n, .broken { opacity: 0.5; }\n";
    let mut h = TestHarness::new(Box::new(Text::new("x")));
    // Не должно паниковать в RuleIndex::build.
    h.apply_mss(src);
}
