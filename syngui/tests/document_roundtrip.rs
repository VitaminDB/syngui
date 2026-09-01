//! Round-trip корпус DocumentEditor: markdown → модель → markdown.
//!
//! Гарантия сериализатора — идемпотентность со второго прохода:
//! `serialize(parse(s1)) == s1`, где `s1 = serialize(parse(исходник))`.
//! Первый проход имеет право нормализовать (маркеры списков, нумерацию,
//! atx-заголовки, «картинка — всегда блок» и т.п.).

use syngui::widgets::input::document_editor::*;

fn roundtrip(src: &str) -> String {
    let m1 = parse_document(src);
    let s1 = serialize_document(&m1);
    let m2 = parse_document(&s1);
    let s2 = serialize_document(&m2);
    assert_eq!(
        s1, s2,
        "сериализация не идемпотентна\n--- исходник ---\n{src}\n--- s1 ---\n{s1}\n--- s2 ---\n{s2}"
    );
    s1
}

#[test]
fn fixtures_roundtrip() {
    let dir = concat!(env!("CARGO_MANIFEST_DIR"), "/tests/fixtures/doc");
    let mut count = 0;
    for entry in std::fs::read_dir(dir).expect("нет каталога фикстур") {
        let path = entry.unwrap().path();
        if path.extension().and_then(|e| e.to_str()) != Some("md") {
            continue;
        }
        let src = std::fs::read_to_string(&path).unwrap();
        roundtrip(&src);
        count += 1;
    }
    assert!(count >= 10, "фикстуры не найдены (обнаружено {count})");
}

// ─── Структурные проверки парсера ───────────────────────────────────────────

fn first_kind(src: &str) -> BlockKind {
    parse_document(src).blocks.into_iter().next().expect("пустой документ").kind
}

#[test]
fn heading_attrs() {
    let m = parse_document("## Проект X {icon=🚀}\n");
    let b = &m.blocks[0];
    match &b.kind {
        BlockKind::Heading { level, text } => {
            assert_eq!(*level, 2);
            assert_eq!(text.text(), "Проект X");
        }
        other => panic!("не заголовок: {other:?}"),
    }
    assert_eq!(b.attrs.get("icon"), Some("🚀"));
}

#[test]
fn callout_and_toggle() {
    let m = parse_document(
        "> [!warning]{color=#e0a030} Важно\n>\n> Тело.\n\n> [!toggle]{open} Секция\n>\n> Внутри.\n\n> [!toggle] Свёрнута\n",
    );
    match &m.blocks[0].kind {
        BlockKind::Callout { kind, title, children } => {
            assert_eq!(kind, "warning");
            assert_eq!(title.text(), "Важно");
            assert_eq!(children.len(), 1);
        }
        other => panic!("не callout: {other:?}"),
    }
    assert_eq!(m.blocks[0].attrs.get("color"), Some("#e0a030"));
    match &m.blocks[1].kind {
        BlockKind::Toggle { summary, collapsed, children } => {
            assert_eq!(summary.text(), "Секция");
            assert!(!collapsed, "флаг open должен раскрывать");
            assert_eq!(children.len(), 1);
        }
        other => panic!("не toggle: {other:?}"),
    }
    match &m.blocks[2].kind {
        BlockKind::Toggle { collapsed, .. } => assert!(collapsed),
        other => panic!("не toggle: {other:?}"),
    }
}

#[test]
fn embed_paragraph() {
    match first_kind("![[Доска проекта]]\n") {
        BlockKind::Embed { target } => assert_eq!(target, "Доска проекта"),
        other => panic!("не embed: {other:?}"),
    }
    // С атрибутами.
    let m = parse_document("![[Канвас]]{height=400}\n");
    assert!(matches!(&m.blocks[0].kind, BlockKind::Embed { target } if target == "Канвас"));
    assert_eq!(m.blocks[0].attrs.get("height"), Some("400"));
}

#[test]
fn wiki_links() {
    let m = parse_document("Ссылка на [[Проект X]] и [[Проект X|проект]].\n");
    let BlockKind::Paragraph(text) = &m.blocks[0].kind else { panic!() };
    let wikis: Vec<_> = text
        .0
        .iter()
        .filter_map(|r| match &r.style.link {
            Some(LinkTarget::Wiki { target }) => Some((target.clone(), r.text.clone())),
            _ => None,
        })
        .collect();
    assert_eq!(
        wikis,
        vec![
            ("Проект X".to_string(), "Проект X".to_string()),
            ("Проект X".to_string(), "проект".to_string()),
        ]
    );
}

#[test]
fn media_splits_paragraph() {
    let m = parse_document("до ![врезка](pic.jpg) после\n");
    let kinds: Vec<_> = m
        .blocks
        .iter()
        .map(|b| match &b.kind {
            BlockKind::Paragraph(t) => format!("p:{}", t.text().trim().to_string()),
            BlockKind::Media { media, .. } => format!("media:{media:?}"),
            other => format!("{other:?}"),
        })
        .collect();
    assert_eq!(kinds, vec!["p:до", "media:Image", "p:после"]);
}

#[test]
fn media_attrs_and_kind() {
    let m = parse_document("![демо](blob:0c8f34aa11.mp4){loop width=70%}\n");
    match &m.blocks[0].kind {
        BlockKind::Media { media, url, alt } => {
            assert_eq!(*media, MediaKind::Video);
            assert_eq!(url, "blob:0c8f34aa11.mp4");
            assert_eq!(alt, "демо");
        }
        other => panic!("не media: {other:?}"),
    }
    assert!(m.blocks[0].attrs.flag("loop"));
    assert_eq!(m.blocks[0].attrs.get("width"), Some("70%"));
    // Переопределение типа атрибутом.
    let m = parse_document("![отчёт](report.pdf){kind=file}\n");
    assert!(matches!(&m.blocks[0].kind, BlockKind::Media { media: MediaKind::File, .. }));
}

#[test]
fn lists_shapes() {
    let m = parse_document("- a\n- b\n  - c\n\n3. x\n4. y\n");
    match &m.blocks[1].kind {
        BlockKind::Bullet { text, children } => {
            assert_eq!(text.text(), "b");
            assert_eq!(children.len(), 1);
        }
        other => panic!("не bullet: {other:?}"),
    }
    match (&m.blocks[2].kind, &m.blocks[3].kind) {
        (
            BlockKind::Numbered { number: n1, .. },
            BlockKind::Numbered { number: n2, .. },
        ) => {
            assert_eq!((*n1, *n2), (3, 4));
        }
        other => panic!("не нумерация: {other:?}"),
    }
}

#[test]
fn todos() {
    let m = parse_document("- [ ] раз\n- [x] два\n");
    assert!(matches!(&m.blocks[0].kind, BlockKind::Todo { checked: false, .. }));
    assert!(matches!(&m.blocks[1].kind, BlockKind::Todo { checked: true, .. }));
}

#[test]
fn hard_break_survives() {
    let s1 = roundtrip("строка с жёстким\\\nпереносом\n");
    assert!(s1.contains("\\\n"), "hard break потерян: {s1:?}");
}

#[test]
fn literal_specials_survive() {
    // Литеральные символы разметки не должны превращаться в разметку.
    let m1 = parse_document("Литеральные \\*звёздочки\\* и \\[скобки\\].\n");
    let BlockKind::Paragraph(t) = &m1.blocks[0].kind else { panic!() };
    assert_eq!(t.text(), "Литеральные *звёздочки* и [скобки].");
    assert!(t.0.iter().all(|r| r.style.plain()));
    let s1 = serialize_document(&m1);
    let m2 = parse_document(&s1);
    let BlockKind::Paragraph(t2) = &m2.blocks[0].kind else { panic!() };
    assert_eq!(t2.text(), t.text());
}

#[test]
fn empty_document() {
    let m = parse_document("");
    assert!(m.blocks.is_empty());
    assert_eq!(serialize_document(&m), "");
}

#[test]
fn code_fence_with_backticks() {
    let src = "````md\nВнутри ```тройные``` кавычки.\n````\n";
    let s1 = roundtrip(src);
    match first_kind(src) {
        BlockKind::CodeBlock { language, code } => {
            assert_eq!(language.as_deref(), Some("md"));
            assert!(code.contains("```тройные```"));
        }
        other => panic!("не код: {other:?}"),
    }
    assert!(s1.starts_with("````md\n"), "нужен более длинный fence: {s1:?}");
}

#[test]
fn table_alignment() {
    let m = parse_document("| a | b | c |\n| --- | :-: | --: |\n| 1 | 2 | 3 |\n");
    match &m.blocks[0].kind {
        BlockKind::Table { aligns, headers, rows } => {
            assert_eq!(aligns, &[DocAlign::Left, DocAlign::Center, DocAlign::Right]);
            assert_eq!(headers.len(), 3);
            assert_eq!(rows.len(), 1);
        }
        other => panic!("не таблица: {other:?}"),
    }
}

#[test]
fn html_preserved_as_text() {
    // Сырой HTML не интерпретируется, но и не теряется.
    let m = parse_document("<div class=\"x\">содержимое</div>\n");
    let all: String = m
        .blocks
        .iter()
        .filter_map(|b| b.kind.text().map(|t| t.text()))
        .collect();
    assert!(all.contains("содержимое"), "html потерян: {m:?}");
}

#[test]
fn ids_unique() {
    let src = "# a\n\nтекст ![i](p.png) хвост\n\n- x\n  - y\n";
    let m = parse_document(src);
    let mut seen = std::collections::HashSet::new();
    m.for_each(&mut |b| {
        assert!(seen.insert(b.id), "дубль id {:?}", b.id);
    });
}
