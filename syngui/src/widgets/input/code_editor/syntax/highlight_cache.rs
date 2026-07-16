use super::capture_names::TokenClass;
use smallvec::SmallVec;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Span {
    pub byte_start: u32,
    pub byte_end: u32,
    pub class: TokenClass,
}

impl Span {
    pub fn new(byte_start: u32, byte_end: u32, class: TokenClass) -> Self {
        Self {
            byte_start,
            byte_end,
            class,
        }
    }

    pub fn len(&self) -> u32 {
        self.byte_end.saturating_sub(self.byte_start)
    }

    pub fn is_empty(&self) -> bool {
        self.byte_end <= self.byte_start
    }
}

#[derive(Debug, Clone, Default)]
pub struct LineSpans {
    pub spans: SmallVec<[Span; 4]>,
}

impl LineSpans {
    pub fn new() -> Self {
        Self::default()
    }

    pub fn push(&mut self, span: Span) {
        if !span.is_empty() {
            self.spans.push(span);
        }
    }

    pub fn is_empty(&self) -> bool {
        self.spans.is_empty()
    }

    pub fn iter(&self) -> impl Iterator<Item = &Span> {
        self.spans.iter()
    }

    pub fn sort_by_start(&mut self) {
        self.spans.sort_by_key(|s| s.byte_start);
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn empty_span_filtered_on_push() {
        let mut ls = LineSpans::new();
        ls.push(Span::new(5, 5, TokenClass::Keyword));
        assert!(ls.is_empty());
    }

    #[test]
    fn sort_by_start() {
        let mut ls = LineSpans::new();
        ls.push(Span::new(10, 12, TokenClass::Keyword));
        ls.push(Span::new(2, 5, TokenClass::String));
        ls.push(Span::new(7, 9, TokenClass::Number));
        ls.sort_by_start();
        let starts: Vec<u32> = ls.iter().map(|s| s.byte_start).collect();
        assert_eq!(starts, vec![2, 7, 10]);
    }
}
