//! Минимальный читатель ZIP (для dotLottie): центральный каталог,
//! методы store и deflate. Без шифрования, zip64 и потоковых дескрипторов.

use super::inflate::inflate;

/// Максимум распакованного файла — защита от zip-бомб.
const MAX_FILE: usize = 256 << 20;

pub struct ZipEntry {
    pub name: String,
    method: u16,
    local_offset: usize,
    comp_size: usize,
    uncomp_size: usize,
}

pub struct Zip<'a> {
    data: &'a [u8],
    pub entries: Vec<ZipEntry>,
}

fn u16le(d: &[u8], o: usize) -> usize {
    u16::from_le_bytes([d[o], d[o + 1]]) as usize
}

fn u32le(d: &[u8], o: usize) -> usize {
    u32::from_le_bytes([d[o], d[o + 1], d[o + 2], d[o + 3]]) as usize
}

impl<'a> Zip<'a> {
    pub fn is_zip(data: &[u8]) -> bool {
        data.starts_with(b"PK\x03\x04") || data.starts_with(b"PK\x05\x06")
    }

    pub fn parse(data: &'a [u8]) -> Result<Zip<'a>, &'static str> {
        // End of central directory: сигнатура PK\x05\x06 в последних 64К+22.
        let tail_start = data.len().saturating_sub(65_557);
        let eocd = (tail_start..data.len().saturating_sub(21))
            .rev()
            .find(|&i| data[i..].starts_with(b"PK\x05\x06"))
            .ok_or("ZIP: нет end of central directory")?;
        if eocd + 22 > data.len() {
            return Err("ZIP: обрезанный EOCD");
        }
        let count = u16le(data, eocd + 10);
        let cd_size = u32le(data, eocd + 12);
        let cd_off = u32le(data, eocd + 16);
        if cd_off + cd_size > data.len() {
            return Err("ZIP: каталог за концом файла");
        }
        let mut entries = Vec::with_capacity(count);
        let mut p = cd_off;
        for _ in 0..count {
            if p + 46 > data.len() || !data[p..].starts_with(b"PK\x01\x02") {
                return Err("ZIP: повреждён центральный каталог");
            }
            let method = u16le(data, p + 10) as u16;
            let comp_size = u32le(data, p + 20);
            let uncomp_size = u32le(data, p + 24);
            let name_len = u16le(data, p + 28);
            let extra_len = u16le(data, p + 30);
            let comment_len = u16le(data, p + 32);
            let local_offset = u32le(data, p + 42);
            let name = String::from_utf8_lossy(&data[p + 46..p + 46 + name_len]).into_owned();
            entries.push(ZipEntry {
                name,
                method,
                local_offset,
                comp_size,
                uncomp_size,
            });
            p += 46 + name_len + extra_len + comment_len;
        }
        Ok(Zip { data, entries })
    }

    /// Распаковать файл по имени (точное совпадение).
    pub fn read(&self, name: &str) -> Option<Vec<u8>> {
        let e = self.entries.iter().find(|e| e.name == name)?;
        self.read_entry(e)
    }

    pub fn read_entry(&self, e: &ZipEntry) -> Option<Vec<u8>> {
        let d = self.data;
        let p = e.local_offset;
        if p + 30 > d.len() || !d[p..].starts_with(b"PK\x03\x04") {
            return None;
        }
        let name_len = u16le(d, p + 26);
        let extra_len = u16le(d, p + 28);
        let start = p + 30 + name_len + extra_len;
        let end = start.checked_add(e.comp_size)?;
        if end > d.len() || e.uncomp_size > MAX_FILE {
            return None;
        }
        let raw = &d[start..end];
        match e.method {
            0 => Some(raw.to_vec()),
            8 => inflate(raw, e.uncomp_size.clamp(1, MAX_FILE))
                .map_err(|err| log::warn!("ZIP: {}: {err}", e.name))
                .ok(),
            m => {
                log::warn!("ZIP: {}: неподдерживаемый метод сжатия {m}", e.name);
                None
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// ZIP с одним файлом a.txt = b"hi" (stored), собран вручную.
    fn tiny_zip() -> Vec<u8> {
        let mut z = Vec::new();
        // local header
        z.extend(b"PK\x03\x04");
        z.extend([20, 0, 0, 0, 0, 0, 0, 0, 0, 0]); // ver, flags, method 0, time, date
        z.extend(0x9e5e7f2cu32.to_le_bytes()); // crc (не проверяется)
        z.extend(2u32.to_le_bytes()); // comp size
        z.extend(2u32.to_le_bytes()); // uncomp size
        z.extend(5u16.to_le_bytes()); // name len
        z.extend(0u16.to_le_bytes()); // extra len
        z.extend(b"a.txt");
        z.extend(b"hi");
        let cd_off = z.len();
        z.extend(b"PK\x01\x02");
        z.extend([20, 0, 20, 0, 0, 0, 0, 0, 0, 0, 0, 0]); // made by, ver, flags, method, time, date
        z.extend(0x9e5e7f2cu32.to_le_bytes());
        z.extend(2u32.to_le_bytes());
        z.extend(2u32.to_le_bytes());
        z.extend(5u16.to_le_bytes());
        z.extend([0u8; 12]); // extra len, comment len, disk, int attr, ext attr
        z.extend(0u32.to_le_bytes()); // local offset
        z.extend(b"a.txt");
        let cd_size = z.len() - cd_off;
        z.extend(b"PK\x05\x06");
        z.extend([0u8; 4]);
        z.extend(1u16.to_le_bytes());
        z.extend(1u16.to_le_bytes());
        z.extend((cd_size as u32).to_le_bytes());
        z.extend((cd_off as u32).to_le_bytes());
        z.extend(0u16.to_le_bytes());
        z
    }

    #[test]
    fn reads_stored_file() {
        let data = tiny_zip();
        let zip = Zip::parse(&data).unwrap();
        assert_eq!(zip.entries.len(), 1);
        assert_eq!(zip.read("a.txt").unwrap(), b"hi");
        assert!(zip.read("b.txt").is_none());
    }
}
