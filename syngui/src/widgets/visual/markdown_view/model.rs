#[derive(Clone, Debug)]
pub enum MdBlock {
    Heading {
        level: u8,
        inlines: Vec<MdInline>,
        id: Option<String>,
    },
    Paragraph {
        inlines: Vec<MdInline>,
    },
    CodeBlock {
        #[allow(dead_code)]
        language: Option<String>,
        code: String,
    },
    BlockQuote {
        blocks: Vec<MdBlock>,
    },
    UnorderedList {
        items: Vec<MdListItem>,
    },
    OrderedList {
        start: u64,
        items: Vec<MdListItem>,
    },
    TaskList {
        items: Vec<MdTaskItem>,
    },
    Table {
        headers: Vec<MdTableCell>,
        rows: Vec<Vec<MdTableCell>>,
        alignments: Vec<MdAlign>,
    },
    HorizontalRule,
    FootnoteDefinition {
        label: String,
        blocks: Vec<MdBlock>,
    },
}

#[derive(Clone, Debug)]
pub enum MdInline {
    Text(String),
    Bold(Vec<MdInline>),
    Italic(Vec<MdInline>),
    Strikethrough(Vec<MdInline>),
    Code(String),
    Link {
        children: Vec<MdInline>,
        #[allow(dead_code)]
        url: String,
    },
    Image {
        alt: String,
        #[allow(dead_code)]
        url: String,
    },
    SoftBreak,
    HardBreak,
    FootnoteRef(String),
}

#[derive(Clone, Debug)]
pub struct MdListItem {
    pub blocks: Vec<MdBlock>,
}

#[derive(Clone, Debug)]
pub struct MdTaskItem {
    pub checked: bool,
    pub inlines: Vec<MdInline>,
}

#[derive(Clone, Debug)]
pub struct MdTableCell {
    pub inlines: Vec<MdInline>,
}

#[derive(Clone, Copy, Debug, Default)]
pub enum MdAlign {
    #[default]
    Left,
    Center,
    Right,
}
