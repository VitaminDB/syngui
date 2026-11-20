//! Размеры бумаги: ключи как у `PageSize` в PPD (`A4`, `Letter`, `4x6`…),
//! размеры в мм, подписи. Размер, которого нет в таблице, берётся из PPD
//! принтера ([`register`]).

use std::collections::HashMap;
use std::sync::{Mutex, OnceLock};

#[derive(Debug, Clone, PartialEq)]
pub struct Paper {
    pub key: String,
    /// Подпись («A4», «Letter», «10 × 15 см»).
    pub label: String,
    pub width_mm: f32,
    pub height_mm: f32,
}

impl Paper {
    /// «A4 · 210 × 297 мм».
    pub fn full_label(&self) -> String {
        let mm = crate::i18n::builtin("print.mm", "mm");
        let dims = format!("{} × {} {mm}", fmt_mm(self.width_mm), fmt_mm(self.height_mm));
        format!("{} · {dims}", self.label)
    }
}

fn fmt_mm(v: f32) -> String {
    let r = (v * 10.0).round() / 10.0;
    if (r - r.round()).abs() < 0.05 {
        format!("{}", r.round() as i32)
    } else {
        format!("{r:.1}")
    }
}

/// (ключ, подпись, ширина, высота мм) — книжная ориентация.
const KNOWN: &[(&str, &str, f32, f32)] = &[
    ("A3", "A3", 297.0, 420.0),
    ("A4", "A4", 210.0, 297.0),
    ("A5", "A5", 148.0, 210.0),
    ("A6", "A6", 105.0, 148.0),
    ("B4", "B4 (JIS)", 257.0, 364.0),
    ("B5", "B5 (JIS)", 182.0, 257.0),
    ("JB5", "B5 (JIS)", 182.0, 257.0),
    ("ISOB5", "B5", 176.0, 250.0),
    ("B5-ISO", "B5", 176.0, 250.0),
    ("B6", "B6 (JIS)", 128.0, 182.0),
    ("Letter", "Letter", 215.9, 279.4),
    ("Legal", "Legal", 215.9, 355.6),
    ("Executive", "Executive", 184.15, 266.7),
    ("Tabloid", "Tabloid", 279.4, 431.8),
    ("Folio", "Folio", 215.9, 330.2),
    ("4x6", "10 × 15 см", 101.6, 152.4),
    ("100x150mm", "10 × 15 см", 100.0, 150.0),
    ("5x7", "13 × 18 см", 127.0, 177.8),
    ("3.5x5", "9 × 13 см", 88.9, 127.0),
    ("Postcard", "Postcard", 100.0, 148.0),
    ("EnvDL", "DL", 110.0, 220.0),
    ("DL", "DL", 110.0, 220.0),
    ("EnvC5", "C5", 162.0, 229.0),
    ("C5", "C5", 162.0, 229.0),
    ("EnvC6", "C6", 114.0, 162.0),
    ("Env10", "No. 10", 104.8, 241.3),
    ("COM10", "No. 10", 104.8, 241.3),
];

/// Размеры для «Сохранить в PDF» (порядок списка).
pub const PDF_SIZES: &[&str] = &["A4", "A3", "A5", "A6", "Letter", "Legal", "100x150mm", "5x7"];

fn extra() -> &'static Mutex<HashMap<String, (f32, f32, String)>> {
    static E: OnceLock<Mutex<HashMap<String, (f32, f32, String)>>> = OnceLock::new();
    E.get_or_init(Default::default)
}

/// Запомнить размер из PPD (ключ, пункты, подпись драйвера).
pub fn register(key: &str, width_pt: f32, height_pt: f32, label: &str) {
    let mm = |pt: f32| pt * 25.4 / 72.0;
    extra().lock().unwrap().insert(key.to_string(), (mm(width_pt), mm(height_pt), label.to_string()));
}

/// Размер бумаги по ключу, мм.
pub fn size_mm(key: &str) -> Option<(f32, f32)> {
    if let Some((w, h, _)) = extra().lock().unwrap().get(key) {
        return Some((*w, *h));
    }
    if let Some(&(_, _, w, h)) = KNOWN.iter().find(|(k, ..)| k.eq_ignore_ascii_case(key)) {
        return Some((w, h));
    }
    parse_dims(key)
}

/// `Custom.210x297mm`, `100x150mm`, `4x6` (дюймы), `w288h432` (пункты).
fn parse_dims(key: &str) -> Option<(f32, f32)> {
    let k = key.strip_prefix("Custom.").unwrap_or(key);
    if let Some(rest) = k.strip_prefix('w') {
        let (w, h) = rest.split_once('h')?;
        let (w, h): (f32, f32) = (w.parse().ok()?, h.trim_end_matches(|c: char| !c.is_ascii_digit()).parse().ok()?);
        return Some((w * 25.4 / 72.0, h * 25.4 / 72.0));
    }
    let (unit, body) = if let Some(b) = k.strip_suffix("mm") {
        (1.0, b)
    } else if let Some(b) = k.strip_suffix("cm") {
        (10.0, b)
    } else if let Some(b) = k.strip_suffix("in") {
        (25.4, b)
    } else {
        (25.4, k)
    };
    let (w, h) = body.split_once('x')?;
    Some((w.parse::<f32>().ok()? * unit, h.parse::<f32>().ok()? * unit))
}

/// Бумага по ключу; `driver_label` — подпись из PPD, если в таблице нет.
pub fn paper(key: &str, driver_label: Option<&str>) -> Option<Paper> {
    let (w, h) = size_mm(key)?;
    let label = KNOWN
        .iter()
        .find(|(k, ..)| k.eq_ignore_ascii_case(key))
        .map(|(_, l, ..)| l.replace("см", &crate::i18n::builtin("print.cm", "cm")))
        .or_else(|| driver_label.map(str::to_string))
        .or_else(|| extra().lock().unwrap().get(key).map(|(_, _, l)| l.clone()).filter(|l| !l.is_empty()))
        .unwrap_or_else(|| key.to_string());
    Some(Paper { key: key.to_string(), label, width_mm: w, height_mm: h })
}

/// Размер по умолчанию по стране: Letter в США и Канаде, иначе A4.
pub fn default_key() -> &'static str {
    let loc = ["LC_PAPER", "LC_ALL", "LANG"].iter().filter_map(|v| std::env::var(v).ok()).find(|v| !v.is_empty()).unwrap_or_default();
    if ["_US", "_CA", "_PH", "_MX", "_CL", "_CO", "_VE"].iter().any(|c| loc.contains(c)) {
        "Letter"
    } else {
        "A4"
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn sizes() {
        assert_eq!(size_mm("A4"), Some((210.0, 297.0)));
        assert_eq!(size_mm("Custom.100x150mm"), Some((100.0, 150.0)));
        let (w, h) = size_mm("w288h432").unwrap();
        assert!((w - 101.6).abs() < 0.01 && (h - 152.4).abs() < 0.01);
        assert_eq!(size_mm("bogus"), None);
        register("roc16k", 552.0, 765.0, "16K");
        assert!(size_mm("roc16k").is_some());
        assert_eq!(paper("roc16k", None).unwrap().label, "16K");
    }

    #[test]
    fn labels() {
        assert!(paper("A4", None).unwrap().full_label().starts_with("A4 · 210 × 297"));
    }
}
