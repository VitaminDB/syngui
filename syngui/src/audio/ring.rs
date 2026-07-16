pub(super) struct VisRing {
    buf: Vec<f32>,
    head: usize,
    len: usize,
}

impl VisRing {
    pub(super) fn new(cap: usize) -> Self {
        Self {
            buf: vec![0.0; cap.max(1)],
            head: 0,
            len: 0,
        }
    }

    pub(super) fn push(&mut self, samples: &[f32]) {
        let cap = self.buf.len();
        for &s in samples {
            self.buf[self.head] = s;
            self.head = (self.head + 1) % cap;
            if self.len < cap {
                self.len += 1;
            }
        }
    }

    pub(super) fn snapshot(&self, out: &mut Vec<f32>) {
        out.clear();
        if self.len == 0 {
            return;
        }
        let cap = self.buf.len();
        let start = (self.head + cap - self.len) % cap;
        out.reserve(self.len);
        for i in 0..self.len {
            out.push(self.buf[(start + i) % cap]);
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn ring_keeps_last_n_after_overflow() {
        let mut r = VisRing::new(4);
        r.push(&[1.0, 2.0, 3.0, 4.0, 5.0, 6.0]);
        let mut out = Vec::new();
        r.snapshot(&mut out);
        assert_eq!(out, vec![3.0, 4.0, 5.0, 6.0]);
    }

    #[test]
    fn ring_partial_fill() {
        let mut r = VisRing::new(8);
        r.push(&[0.5, -0.5]);
        let mut out = Vec::new();
        r.snapshot(&mut out);
        assert_eq!(out, vec![0.5, -0.5]);
    }
}
