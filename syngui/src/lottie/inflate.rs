//! Декодер DEFLATE (RFC 1951) с нуля — для dotLottie (.lottie = ZIP).
//! Канонические коды Хаффмана декодируются по алгоритму puff (Mark Adler):
//! побитно, по счётчикам длин — без таблиц быстрых переходов, зато просто
//! и надёжно; скорости для анимаций в сотни килобайт достаточно.

pub type InflateError = &'static str;

struct BitReader<'a> {
    data: &'a [u8],
    pos: usize,
    bit: u32,
}

impl<'a> BitReader<'a> {
    fn new(data: &'a [u8]) -> Self {
        Self { data, pos: 0, bit: 0 }
    }

    #[inline]
    fn bit(&mut self) -> Result<u32, InflateError> {
        let byte = *self.data.get(self.pos).ok_or("обрыв потока deflate")?;
        let v = (byte >> self.bit) & 1;
        self.bit += 1;
        if self.bit == 8 {
            self.bit = 0;
            self.pos += 1;
        }
        Ok(v as u32)
    }

    #[inline]
    fn bits(&mut self, n: u32) -> Result<u32, InflateError> {
        let mut v = 0u32;
        for i in 0..n {
            v |= self.bit()? << i;
        }
        Ok(v)
    }

    /// К началу байта (stored-блоки).
    fn align(&mut self) {
        if self.bit != 0 {
            self.bit = 0;
            self.pos += 1;
        }
    }
}

/// Канонический код Хаффмана: счётчики кодов каждой длины и символы
/// в каноническом порядке.
struct Huffman {
    counts: [u16; 16],
    symbols: Vec<u16>,
}

impl Huffman {
    /// Из длин кодов символов (0 — символ не используется).
    fn new(lengths: &[u8]) -> Result<Huffman, InflateError> {
        let mut counts = [0u16; 16];
        for &l in lengths {
            counts[l as usize] += 1;
        }
        counts[0] = 0;
        // Проверка переполнения кодов.
        let mut left = 1i32;
        for &c in counts.iter().skip(1) {
            left <<= 1;
            left -= c as i32;
            if left < 0 {
                return Err("некорректный код Хаффмана");
            }
        }
        let mut offs = [0u16; 16];
        for l in 1..15 {
            offs[l + 1] = offs[l] + counts[l];
        }
        let mut symbols = vec![0u16; lengths.len()];
        for (sym, &l) in lengths.iter().enumerate() {
            if l != 0 {
                symbols[offs[l as usize] as usize] = sym as u16;
                offs[l as usize] += 1;
            }
        }
        Ok(Huffman { counts, symbols })
    }

    fn decode(&self, r: &mut BitReader) -> Result<u16, InflateError> {
        let mut code = 0i32;
        let mut first = 0i32;
        let mut index = 0i32;
        for len in 1..16 {
            code |= r.bit()? as i32;
            let count = self.counts[len] as i32;
            if code - first < count {
                return Ok(self.symbols[(index + (code - first)) as usize]);
            }
            index += count;
            first = (first + count) << 1;
            code <<= 1;
        }
        Err("код вне алфавита")
    }
}

const LEN_BASE: [u16; 29] = [
    3, 4, 5, 6, 7, 8, 9, 10, 11, 13, 15, 17, 19, 23, 27, 31, 35, 43, 51, 59, 67, 83, 99, 115, 131, 163, 195, 227, 258,
];
const LEN_EXTRA: [u8; 29] = [
    0, 0, 0, 0, 0, 0, 0, 0, 1, 1, 1, 1, 2, 2, 2, 2, 3, 3, 3, 3, 4, 4, 4, 4, 5, 5, 5, 5, 0,
];
const DIST_BASE: [u16; 30] = [
    1, 2, 3, 4, 5, 7, 9, 13, 17, 25, 33, 49, 65, 97, 129, 193, 257, 385, 513, 769, 1025, 1537, 2049, 3073, 4097, 6145,
    8193, 12289, 16385, 24577,
];
const DIST_EXTRA: [u8; 30] = [
    0, 0, 0, 0, 1, 1, 2, 2, 3, 3, 4, 4, 5, 5, 6, 6, 7, 7, 8, 8, 9, 9, 10, 10, 11, 11, 12, 12, 13, 13,
];

fn fixed_tables() -> (Huffman, Huffman) {
    let mut lit = [0u8; 288];
    for (i, l) in lit.iter_mut().enumerate() {
        *l = match i {
            0..=143 => 8,
            144..=255 => 9,
            256..=279 => 7,
            _ => 8,
        };
    }
    let dist = [5u8; 30];
    (Huffman::new(&lit).expect("fixed lit"), Huffman::new(&dist).expect("fixed dist"))
}

fn inflate_block(r: &mut BitReader, out: &mut Vec<u8>, lit: &Huffman, dist: &Huffman, limit: usize) -> Result<(), InflateError> {
    loop {
        let sym = lit.decode(r)?;
        match sym {
            0..=255 => {
                if out.len() >= limit {
                    return Err("превышен лимит распаковки");
                }
                out.push(sym as u8);
            }
            256 => return Ok(()),
            257..=285 => {
                let i = (sym - 257) as usize;
                let len = LEN_BASE[i] as usize + r.bits(LEN_EXTRA[i] as u32)? as usize;
                let dsym = dist.decode(r)? as usize;
                if dsym >= 30 {
                    return Err("некорректная дистанция");
                }
                let d = DIST_BASE[dsym] as usize + r.bits(DIST_EXTRA[dsym] as u32)? as usize;
                if d > out.len() {
                    return Err("дистанция за началом буфера");
                }
                if out.len() + len > limit {
                    return Err("превышен лимит распаковки");
                }
                let start = out.len() - d;
                for k in 0..len {
                    let b = out[start + k];
                    out.push(b);
                }
            }
            _ => return Err("некорректный литерал"),
        }
    }
}

/// Распаковать сырой DEFLATE-поток. `limit` — максимум выходных байт
/// (защита от zip-бомб).
pub fn inflate(data: &[u8], limit: usize) -> Result<Vec<u8>, InflateError> {
    let mut r = BitReader::new(data);
    let mut out = Vec::with_capacity(data.len() * 3);
    loop {
        let last = r.bit()? == 1;
        match r.bits(2)? {
            0 => {
                r.align();
                if r.pos + 4 > r.data.len() {
                    return Err("обрыв stored-блока");
                }
                let len = u16::from_le_bytes([r.data[r.pos], r.data[r.pos + 1]]) as usize;
                let nlen = u16::from_le_bytes([r.data[r.pos + 2], r.data[r.pos + 3]]) as usize;
                if len != (!nlen & 0xFFFF) {
                    return Err("stored: len != !nlen");
                }
                r.pos += 4;
                if r.pos + len > r.data.len() {
                    return Err("обрыв stored-данных");
                }
                if out.len() + len > limit {
                    return Err("превышен лимит распаковки");
                }
                out.extend_from_slice(&r.data[r.pos..r.pos + len]);
                r.pos += len;
            }
            1 => {
                let (lit, dist) = fixed_tables();
                inflate_block(&mut r, &mut out, &lit, &dist, limit)?;
            }
            2 => {
                let hlit = r.bits(5)? as usize + 257;
                let hdist = r.bits(5)? as usize + 1;
                let hclen = r.bits(4)? as usize + 4;
                const ORDER: [usize; 19] = [16, 17, 18, 0, 8, 7, 9, 6, 10, 5, 11, 4, 12, 3, 13, 2, 14, 1, 15];
                let mut cl = [0u8; 19];
                for &o in ORDER.iter().take(hclen) {
                    cl[o] = r.bits(3)? as u8;
                }
                let clh = Huffman::new(&cl)?;
                let mut lengths = vec![0u8; hlit + hdist];
                let mut i = 0;
                while i < hlit + hdist {
                    let sym = clh.decode(&mut r)?;
                    match sym {
                        0..=15 => {
                            lengths[i] = sym as u8;
                            i += 1;
                        }
                        16 => {
                            if i == 0 {
                                return Err("повтор без предыдущей длины");
                            }
                            let prev = lengths[i - 1];
                            for _ in 0..(3 + r.bits(2)?) {
                                if i >= lengths.len() {
                                    return Err("переполнение длин");
                                }
                                lengths[i] = prev;
                                i += 1;
                            }
                        }
                        17 => {
                            for _ in 0..(3 + r.bits(3)?) {
                                if i >= lengths.len() {
                                    return Err("переполнение длин");
                                }
                                i += 1;
                            }
                        }
                        18 => {
                            for _ in 0..(11 + r.bits(7)?) {
                                if i >= lengths.len() {
                                    return Err("переполнение длин");
                                }
                                i += 1;
                            }
                        }
                        _ => return Err("некорректный код длин"),
                    }
                }
                if lengths[256] == 0 {
                    return Err("нет кода конца блока");
                }
                let lit = Huffman::new(&lengths[..hlit])?;
                let dist = Huffman::new(&lengths[hlit..])?;
                inflate_block(&mut r, &mut out, &lit, &dist, limit)?;
            }
            _ => return Err("некорректный тип блока"),
        }
        if last {
            return Ok(out);
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn stored_block() {
        // python: zlib.compress(b"hello", 0)[2:-4] → stored deflate
        let data = [0x01, 0x05, 0x00, 0xfa, 0xff, b'h', b'e', b'l', b'l', b'o'];
        assert_eq!(inflate(&data, 1 << 20).unwrap(), b"hello");
    }

    #[test]
    fn fixed_huffman() {
        // python: zlib.compressobj(9, wbits=-15) на b"abcabcabcabc"
        let data = [0x4b, 0x4c, 0x4a, 0x4e, 0x84, 0x21, 0x00];
        assert_eq!(inflate(&data, 1 << 20).unwrap(), b"abcabcabcabc");
    }

    #[test]
    fn rejects_overrun() {
        let data = [0x01, 0xff, 0xff, 0x00, 0x00];
        assert!(inflate(&data, 1 << 20).is_err());
    }
}
