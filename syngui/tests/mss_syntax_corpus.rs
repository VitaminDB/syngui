//! Толерантный разбор (`syngui::mss::syntax`) на всех `.mss` syngui и
//! synshell (если он лежит рядом): без паники и с теми же селекторами,
//! свойствами и переменными, что у основного парсера.

use std::collections::BTreeSet;
use std::path::{Path, PathBuf};

use syngui::mss::syntax::{self, Item, SelectorPartKind};
use syngui::mss::{MssParser, ParseErrorKind, SelectorPart, StyleSheet};

const KNOWN_PSEUDO: &[&str] = &[
    "hover",
    "active",
    "pressed",
    "focus",
    "selected",
    "checked",
    "disabled",
    "window-maximized",
    "window-fullscreen",
    "window-focused",
];

fn collect(dir: &Path, out: &mut Vec<PathBuf>) {
    let Ok(rd) = std::fs::read_dir(dir) else {
        return;
    };
    for e in rd.flatten() {
        let p = e.path();
        let name = e.file_name();
        let name = name.to_string_lossy();
        if p.is_dir() {
            if name != "target" && !name.starts_with('.') {
                collect(&p, out);
            }
        } else if name.ends_with(".mss") {
            out.push(p);
        }
    }
}

#[derive(Default, Debug, PartialEq)]
struct Names {
    selectors: BTreeSet<String>,
    properties: BTreeSet<String>,
}

fn main_names(sheet: &StyleSheet) -> Names {
    let mut n = Names::default();
    for r in sheet.rules() {
        for chain in r.selector.chains() {
            for seg in &chain.segments {
                add_segment(&mut n.selectors, seg);
            }
            if let Some(p) = &chain.pseudo {
                n.selectors.insert(format!(":{p}"));
            }
        }
        n.properties.extend(r.declarations.keys().cloned());
    }
    for k in sheet.all_keyframes().values() {
        for s in &k.steps {
            n.properties.extend(s.declarations.keys().cloned());
        }
    }
    n
}

fn add_segment(out: &mut BTreeSet<String>, seg: &SelectorPart) {
    match seg {
        SelectorPart::Class(c) => {
            out.insert(format!(".{c}"));
        }
        SelectorPart::Element(e) => {
            out.insert(e.clone());
        }
        SelectorPart::Id(i) => {
            out.insert(format!("#{i}"));
        }
        SelectorPart::Universal => {}
        SelectorPart::Compound {
            element,
            id,
            classes,
        } => {
            if let Some(e) = element {
                out.insert(e.clone());
            }
            if let Some(i) = id {
                out.insert(format!("#{i}"));
            }
            for c in classes {
                out.insert(format!(".{c}"));
            }
        }
    }
}

fn add_parts(out: &mut BTreeSet<String>, sel: &syntax::Selector, with_pseudo: bool) {
    for p in &sel.parts {
        match p.kind {
            SelectorPartKind::Class => {
                out.insert(format!(".{}", p.name));
            }
            SelectorPartKind::Id => {
                out.insert(format!("#{}", p.name));
            }
            SelectorPartKind::Element => {
                out.insert(p.name.clone());
            }
            SelectorPartKind::Pseudo if with_pseudo => {
                out.insert(format!(":{}", p.name));
            }
            _ => {}
        }
    }
}

/// Имена после развёртки сокращений — как их кладёт основной парсер.
fn expanded(name: &str, value: &str) -> Vec<String> {
    let (sheet, _) = MssParser::new(&format!(".x{{{name}:{value}}}"))
        .parse()
        .unwrap();
    sheet
        .rules()
        .iter()
        .flat_map(|r| r.declarations.keys().cloned())
        .collect()
}

fn syntax_names(doc: &syntax::Stylesheet) -> Names {
    let mut n = Names::default();
    for (rule, parents) in doc.walk_rules() {
        if rule.is_root {
            continue;
        }
        let Some(block) = &rule.block else { continue };
        let decls: Vec<_> = block.declarations().collect();
        if decls.is_empty() {
            continue;
        }
        // Как в основном парсере: селектор с неизвестным псевдоклассом
        // пропускается; у родителей берётся первый селектор без псевдокласса.
        let pseudo_ok = |s: &syntax::Selector| {
            s.parts
                .iter()
                .filter(|p| p.kind == SelectorPartKind::Pseudo)
                .all(|p| KNOWN_PSEUDO.contains(&p.name.as_str()))
        };
        let live: Vec<_> = rule.selectors.iter().filter(|s| pseudo_ok(s)).collect();
        if live.is_empty() {
            continue;
        }
        for s in &live {
            add_parts(&mut n.selectors, s, true);
        }
        for p in &parents {
            if let Some(first) = p.selectors.first() {
                add_parts(&mut n.selectors, first, false);
            }
        }
        for d in decls {
            n.properties.extend(expanded(&d.name, &d.value));
        }
    }
    for item in &doc.items {
        if let Item::Keyframes(k) = item {
            for s in &k.steps {
                if let Some(b) = &s.block {
                    n.properties
                        .extend(b.declarations().map(|d| d.name.clone()));
                }
            }
        }
    }
    n
}

#[test]
fn syntax_matches_main_parser_on_all_mss() {
    let root = Path::new(env!("CARGO_MANIFEST_DIR")).join("..");
    let mut files = Vec::new();
    collect(&root, &mut files);
    collect(&root.join("../synshell"), &mut files);
    // Дополнительные каталоги через `MSS_CORPUS_DIRS=путь1:путь2`.
    if let Ok(extra) = std::env::var("MSS_CORPUS_DIRS") {
        for d in extra.split(':').filter(|d| !d.is_empty()) {
            collect(Path::new(d), &mut files);
        }
    }
    assert!(files.len() > 10, "мало .mss: {}", files.len());

    let mut checked = 0;
    for path in &files {
        let src = std::fs::read_to_string(path).unwrap();
        let doc = syntax::parse(&src);
        let joined: String = doc.tokens.iter().map(|t| t.text(&src)).collect();
        assert_eq!(joined, src, "{}: токены с потерями", path.display());

        let (sheet, warnings) = MssParser::new(&src).parse().unwrap();
        let hard: Vec<_> = warnings
            .iter()
            .filter(|w| !matches!(w.kind, ParseErrorKind::UnknownPseudoClass { .. }))
            .collect();
        // Ошибки находят оба разборщика или ни один.
        assert_eq!(
            hard.is_empty(),
            doc.errors.is_empty(),
            "{}: основной {:?}, syntax {:?}",
            path.display(),
            hard,
            doc.errors
        );
        if !hard.is_empty() {
            continue;
        }

        let want = main_names(&sheet);
        let got = syntax_names(&doc);
        assert_eq!(
            got.selectors,
            want.selectors,
            "{}: селекторы",
            path.display()
        );
        assert_eq!(
            got.properties,
            want.properties,
            "{}: свойства",
            path.display()
        );

        for v in doc.root_variables() {
            assert!(
                sheet.get_variable(&v.name).is_some(),
                "{}: переменная {}",
                path.display(),
                v.name
            );
        }
        let main_kf: BTreeSet<_> = sheet.all_keyframes().keys().cloned().collect();
        let syn_kf: BTreeSet<_> = doc
            .keyframes_names()
            .iter()
            .map(|k| k.0.to_string())
            .collect();
        assert_eq!(syn_kf, main_kf, "{}: @keyframes", path.display());
        checked += 1;
    }
    eprintln!("проверено {checked} из {}", files.len());
    assert!(checked > 10);
}
