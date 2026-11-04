//! Шрифты приложения из байтов, под именем семейства: `font-family` в MSS
//! выбирает их на любой платформе, в том числе на Android, где системных
//! шрифтов кроме Roboto нет. Реестр общий для процесса — его видят атласы
//! всех окон и снимки [`crate::shot`].
//!
//! ```ignore
//! syngui::text::register_font("Caveat", include_bytes!("../assets/Caveat-VF.ttf"));
//! // MSS: .title { font-family: "Caveat"; font-weight: 700; }
//! ```
//!
//! Под одним именем можно зарегистрировать несколько файлов (обычный,
//! жирный…): начертание выбирается по насыщенности из таблицы OS/2.
//! Переменный шрифт (ось `wght`) даёт любую насыщенность из одного файла.

use std::collections::HashMap;
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::{Arc, Mutex};

static FONTS: Mutex<Option<HashMap<String, Vec<Arc<[u8]>>>>> = Mutex::new(None);
static GENERATION: AtomicU64 = AtomicU64::new(0);

/// Зарегистрировать шрифт (TTF/OTF, в том числе переменный) под именем
/// семейства. Повторный вызов с тем же файлом ничего не меняет.
pub fn register_font(family: &str, data: impl Into<Arc<[u8]>>) {
    let data: Arc<[u8]> = data.into();
    if swash::FontRef::from_index(&data, 0).is_none() {
        log::warn!("register_font({family:?}): не шрифт ({} байт)", data.len());
        return;
    }
    let mut map = FONTS.lock().unwrap_or_else(|e| e.into_inner());
    let list = map.get_or_insert_with(HashMap::new).entry(key(family)).or_default();
    if list.iter().any(|d| d[..] == data[..]) {
        return;
    }
    list.push(data);
    GENERATION.fetch_add(1, Ordering::SeqCst);
}

/// Прочитать шрифт из файла и зарегистрировать.
pub fn register_font_file(family: &str, path: impl AsRef<std::path::Path>) -> std::io::Result<()> {
    let data = std::fs::read(path)?;
    register_font(family, data);
    Ok(())
}

/// Файлы семейства (пусто — не регистрировалось).
pub(crate) fn family_fonts(family: &str) -> Vec<Arc<[u8]>> {
    FONTS
        .lock()
        .unwrap_or_else(|e| e.into_inner())
        .as_ref()
        .and_then(|m| m.get(&key(family)).cloned())
        .unwrap_or_default()
}

/// Растёт при каждой регистрации: атлас сбрасывает «не найдено» по семействам.
pub(crate) fn generation() -> u64 {
    GENERATION.load(Ordering::SeqCst)
}

fn key(family: &str) -> String {
    family.trim().trim_matches(['"', '\'']).trim().to_lowercase()
}

/// Насыщенность начертания по OS/2 и есть ли ось `wght`.
pub(crate) fn face_weight(data: &[u8]) -> (u16, bool) {
    match swash::FontRef::from_index(data, 0) {
        Some(f) => {
            let variable = f.variations().any(|v| v.tag() == swash::tag_from_bytes(b"wght"));
            (f.attributes().weight().0, variable)
        }
        None => (400, false),
    }
}
