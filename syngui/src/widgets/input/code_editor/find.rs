use std::ops::Range;

#[derive(Debug, Clone, Default)]
pub struct FindState {
    pub visible: bool,
    pub query: String,
    pub matches: Vec<Range<usize>>,
    pub current: Option<usize>,
}

impl FindState {
    pub fn new() -> Self {
        Self::default()
    }

    pub fn open(&mut self) {
        self.visible = true;
    }

    pub fn close(&mut self) {
        self.visible = false;
        self.query.clear();
        self.matches.clear();
        self.current = None;
    }

    pub fn search(&mut self, text: &str) {
        self.matches.clear();
        if self.query.is_empty() {
            self.current = None;
            return;
        }
        let qlen = self.query.len();
        for (start, _) in text.match_indices(&self.query) {
            self.matches.push(start..start + qlen);
        }
        self.current = if self.matches.is_empty() {
            None
        } else {
            Some(0)
        };
    }

    pub fn update_query(&mut self, text: &str, new_query: String) {
        self.query = new_query;
        self.search(text);
    }

    pub fn next_match(&mut self) -> Option<Range<usize>> {
        if self.matches.is_empty() {
            return None;
        }
        let cur = self.current.unwrap_or(0);
        let next = (cur + 1) % self.matches.len();
        self.current = Some(next);
        Some(self.matches[next].clone())
    }

    pub fn prev_match(&mut self) -> Option<Range<usize>> {
        if self.matches.is_empty() {
            return None;
        }
        let cur = self.current.unwrap_or(0);
        let prev = if cur == 0 {
            self.matches.len() - 1
        } else {
            cur - 1
        };
        self.current = Some(prev);
        Some(self.matches[prev].clone())
    }

    pub fn current_match(&self) -> Option<Range<usize>> {
        self.current.map(|i| self.matches[i].clone())
    }

    pub fn count(&self) -> usize {
        self.matches.len()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn empty_query_no_matches() {
        let mut f = FindState::new();
        f.search("hello world");
        assert_eq!(f.count(), 0);
        assert!(f.current.is_none());
    }

    #[test]
    fn finds_all_matches() {
        let mut f = FindState::new();
        f.update_query("ab ab ab", "ab".to_string());
        assert_eq!(f.count(), 3);
        assert_eq!(f.matches[0], 0..2);
        assert_eq!(f.matches[1], 3..5);
        assert_eq!(f.matches[2], 6..8);
        assert_eq!(f.current, Some(0));
    }

    #[test]
    fn next_match_cycles() {
        let mut f = FindState::new();
        f.update_query("ab ab ab", "ab".to_string());
        assert_eq!(f.next_match(), Some(3..5));
        assert_eq!(f.next_match(), Some(6..8));
        assert_eq!(f.next_match(), Some(0..2));
    }

    #[test]
    fn prev_match_cycles() {
        let mut f = FindState::new();
        f.update_query("ab ab ab", "ab".to_string());
        assert_eq!(f.prev_match(), Some(6..8));
        assert_eq!(f.prev_match(), Some(3..5));
    }

    #[test]
    fn close_clears_state() {
        let mut f = FindState::new();
        f.open();
        f.update_query("foo bar foo", "foo".to_string());
        assert_eq!(f.count(), 2);
        f.close();
        assert!(!f.visible);
        assert!(f.query.is_empty());
        assert!(f.matches.is_empty());
        assert!(f.current.is_none());
    }

    #[test]
    fn search_5mb_under_50ms() {
        let mut text = String::with_capacity(100 * 1024);
        for i in 0..50 {
            text.push_str("xxxxxxxxxx");
            if i % 10 == 0 {
                text.push_str("needle");
            }
            text.push_str("yyyyyyyyyy");
        }
        while text.len() < 100 * 1024 {
            text.push('z');
        }
        let mut f = FindState::new();
        let start = std::time::Instant::now();
        f.update_query(&text, "needle".to_string());
        let elapsed = start.elapsed();
        assert_eq!(f.count(), 5, "expected 5 needle matches");
        assert!(
            elapsed.as_millis() < 50,
            "search took {}ms (>50ms target)",
            elapsed.as_millis()
        );
    }
}
