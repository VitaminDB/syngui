pub(super) struct ParserCursor<'a> {
    pub input: &'a str,
    pub position: usize,
    pub line: usize,
}

impl<'a> ParserCursor<'a> {
    pub fn new(input: &'a str) -> Self {
        Self { input, position: 0, line: 1 }
    }

    pub fn skip_whitespace(&mut self) {
        while !self.is_eof() {
            match self.peek() {
                Some(' ') | Some('\t') | Some('\n') | Some('\r') => {
                    if self.peek() == Some('\n') {
                        self.line += 1;
                    }
                    self.advance();
                }
                _ => break,
            }
        }
    }

    pub fn skip_comment(&mut self) {
        if self.peek() == Some('/') && self.peek_next() == Some('*') {
            self.advance();
            self.advance();

            while !self.is_eof() {
                if self.peek() == Some('*') && self.peek_next() == Some('/') {
                    self.advance();
                    self.advance();
                    break;
                }
                if self.peek() == Some('\n') {
                    self.line += 1;
                }
                self.advance();
            }
        }
    }

    pub fn starts_with(&self, s: &str) -> bool {
        self.input[self.position..].starts_with(s)
    }

    pub fn peek(&self) -> Option<char> {
        self.input[self.position..].chars().next()
    }

    pub fn peek_next(&self) -> Option<char> {
        let mut chars = self.input[self.position..].chars();
        chars.next();
        chars.next()
    }

    pub fn advance(&mut self) {
        if let Some(c) = self.input[self.position..].chars().next() {
            self.position += c.len_utf8();
        }
    }

    pub fn is_eof(&self) -> bool {
        self.position >= self.input.len()
    }

    pub fn consume(&mut self, s: &str) {
        for c in s.chars() {
            if self.peek() == Some(c) {
                self.advance();
            }
        }
    }

    pub fn split_by_comma<'b>(&self, s: &'b str) -> Vec<&'b str> {
        let mut parts = Vec::new();
        let mut depth = 0;
        let mut start = 0;

        for (i, c) in s.char_indices() {
            match c {
                '(' | '[' => depth += 1,
                ')' | ']' => depth -= 1,
                ',' if depth == 0 => {
                    parts.push(&s[start..i]);
                    start = i + 1;
                }
                _ => {}
            }
        }
        parts.push(&s[start..]);
        parts
    }
}
