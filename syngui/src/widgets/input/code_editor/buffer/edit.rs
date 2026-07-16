use std::ops::Range;

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Edit {
    pub range: Range<usize>,
    pub replacement: String,
    pub kind: EditKind,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum EditKind {
    Insert,
    Delete,
    Replace,
}

impl Edit {
    pub fn insert(at_byte: usize, text: impl Into<String>) -> Self {
        Self {
            range: at_byte..at_byte,
            replacement: text.into(),
            kind: EditKind::Insert,
        }
    }

    pub fn delete(range: Range<usize>) -> Self {
        Self {
            range,
            replacement: String::new(),
            kind: EditKind::Delete,
        }
    }

    pub fn replace(range: Range<usize>, replacement: impl Into<String>) -> Self {
        Self {
            range,
            replacement: replacement.into(),
            kind: EditKind::Replace,
        }
    }
}

/// Поля идентичны [`Edit`]; отдельный тип нужен для type-safety в API
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct InverseEdit {
    pub range: Range<usize>,
    pub replacement: String,
    pub kind: EditKind,
}

impl From<InverseEdit> for Edit {
    fn from(value: InverseEdit) -> Self {
        Edit {
            range: value.range,
            replacement: value.replacement,
            kind: value.kind,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn insert_constructs_correctly() {
        let e = Edit::insert(5, "abc");
        assert_eq!(e.range, 5..5);
        assert_eq!(e.replacement, "abc");
        assert_eq!(e.kind, EditKind::Insert);
    }

    #[test]
    fn delete_constructs_correctly() {
        let e = Edit::delete(2..7);
        assert_eq!(e.range, 2..7);
        assert!(e.replacement.is_empty());
        assert_eq!(e.kind, EditKind::Delete);
    }

    #[test]
    fn replace_constructs_correctly() {
        let e = Edit::replace(0..3, "xyz");
        assert_eq!(e.range, 0..3);
        assert_eq!(e.replacement, "xyz");
        assert_eq!(e.kind, EditKind::Replace);
    }

    #[test]
    fn inverse_to_edit() {
        let inv = InverseEdit {
            range: 3..5,
            replacement: "ab".into(),
            kind: EditKind::Insert,
        };
        let edit: Edit = inv.into();
        assert_eq!(edit.range, 3..5);
        assert_eq!(edit.replacement, "ab");
        assert_eq!(edit.kind, EditKind::Insert);
    }
}
