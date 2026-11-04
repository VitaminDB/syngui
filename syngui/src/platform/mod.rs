//! Платформенные службы: выбор фото/видео, «Поделиться», открыть файл.
//!
//! | Функция        | Android                                   | Linux                                              |
//! |----------------|-------------------------------------------|----------------------------------------------------|
//! | [`pick_media`] | системный фото-пикер (`ACTION_PICK_IMAGES`, fallback `ACTION_GET_CONTENT`), копия в кэш приложения | портал `FileChooser.OpenFile` (фича `portal`), fallback `zenity` / `kdialog` |
//! | [`share_text`] | `ACTION_SEND` + chooser                   | текст в буфер обмена                               |
//! | [`share_file`] | `ACTION_SEND` + chooser (FileProvider или копия в Downloads) | файл в буфер (`text/uri-list`) и показ в файловом менеджере |
//! | [`open_path`]  | `ACTION_VIEW`                             | `xdg-open`, портал `OpenURI.OpenFile` (во flatpak — первым) |
//! | [`open_url`]   | `ACTION_VIEW` (WhatsApp, браузер, почта…) | [`crate::open_url`] (фича `links`)                 |
//! | [`set_system_bars`] | цвет под статус-баром и навигационной панелью, светлые/тёмные значки | no-op |
//!
//! Остальные платформы возвращают `Err("не поддерживается")`.
//!
//! Колбэк [`pick_media`] всегда вызывается в UI-потоке (через
//! [`crate::async_runtime::run_on_main_thread`]), так что в нём можно трогать
//! сигналы. Подробно — `docs/18-platform.md`.

use std::path::{Path, PathBuf};

#[cfg(target_os = "android")]
pub(crate) mod android;
#[cfg(all(target_os = "linux", not(target_arch = "wasm32")))]
mod linux;

/// Что выбирать в [`pick_media`].
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum MediaKind {
    Image,
    Video,
    ImageOrVideo,
}

/// Выбранный файл. `path` — локальная копия/файл, его можно читать
/// обычным `std::fs`; `name` — имя, под которым файл показывала система;
/// `mime` — тип (`image/jpeg`, `video/mp4`, …; `application/octet-stream`,
/// если неизвестен).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct PickedFile {
    pub path: PathBuf,
    pub name: String,
    pub mime: String,
}

type PickCallback = Box<dyn FnOnce(Result<Option<PickedFile>, String>) + Send + 'static>;

/// Выбрать фото и/или видео системным диалогом. Не блокирует: колбэк
/// вызывается в UI-потоке, когда пользователь выбрал файл (`Ok(Some)`),
/// отменил (`Ok(None)`) или случилась ошибка (`Err`).
///
/// На Android файл копируется в `cacheDir/syngui-picked/` (content:// URI
/// после закрытия пикера читать нельзя); копию стоит перенести к себе или
/// удалить после использования.
pub fn pick_media(
    kind: MediaKind,
    on_done: impl FnOnce(Result<Option<PickedFile>, String>) + Send + 'static,
) {
    let cb: PickCallback = Box::new(on_done);
    imp_pick(kind, cb);
}

/// Поделиться текстом (на Linux — копия в буфер обмена, `Ok` значит
/// «скопировано»: приложение может так и сообщить пользователю).
pub fn share_text(title: &str, text: &str) -> Result<(), String> {
    imp_share_text(title, text)
}

/// Поделиться файлом. `mime` пустой — определить по расширению.
pub fn share_file(path: &Path, mime: &str, title: &str) -> Result<(), String> {
    let mime = if mime.is_empty() { mime_for_path(path) } else { mime };
    imp_share_file(path, mime, title)
}

/// Открыть файл в системном просмотрщике (например, чтобы распечатать
/// PNG/PDF).
pub fn open_path(path: &Path) -> Result<(), String> {
    if !path.exists() {
        return Err(format!("нет файла: {}", path.display()));
    }
    imp_open_path(path)
}

/// Открыть ссылку (`https:`, `mailto:`, `tel:`, …) в подходящем
/// приложении: на Android — `ACTION_VIEW` (ссылку `https://wa.me/…` откроет
/// WhatsApp, если он стоит, иначе браузер); на остальных — [`crate::open_url`].
pub fn open_url(url: &str) -> Result<(), String> {
    #[cfg(target_os = "android")]
    return android::open_url(url);
    #[cfg(not(target_os = "android"))]
    crate::open_url(url)
}

/// Цвет системных панелей и их значков.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct SystemBars {
    /// Фон под статус-баром и навигационной (жестовой) панелью.
    pub color: crate::core::Color,
    /// Светлые значки (для тёмного фона); `false` — тёмные.
    pub light_icons: bool,
}

static SYSTEM_BARS: std::sync::Mutex<Option<SystemBars>> = std::sync::Mutex::new(None);
static SYSTEM_BARS_DIRTY: std::sync::atomic::AtomicBool = std::sync::atomic::AtomicBool::new(false);

/// Покрасить системные панели (статус-бар сверху, навигационная панель
/// снизу) и выбрать светлые или тёмные значки на них — например, при смене
/// темы приложения. Можно вызывать в любой момент и из любого потока;
/// выбор держится до следующего вызова и главнее автоматического
/// переключения значков по теме (`with_theme_styles`).
///
/// На Android полосы под панелями заливает сам syngui (окно рисуется под
/// ними, edge-to-edge с Android 15), на Android 11–14 дополнительно
/// задаются `setStatusBarColor`/`setNavigationBarColor`. На прочих
/// платформах — `Ok(())` без действий.
pub fn set_system_bars(color: crate::core::Color, light_icons: bool) -> Result<(), String> {
    let bars = SystemBars { color, light_icons };
    *SYSTEM_BARS.lock().unwrap_or_else(|e| e.into_inner()) = Some(bars);
    SYSTEM_BARS_DIRTY.store(true, std::sync::atomic::Ordering::SeqCst);
    crate::signal::request_redraw();
    #[cfg(target_os = "android")]
    return android::set_system_bars(&bars);
    #[cfg(not(target_os = "android"))]
    Ok(())
}

/// Заданные приложением панели ([`set_system_bars`]), если задавались.
pub fn system_bars() -> Option<SystemBars> {
    *SYSTEM_BARS.lock().unwrap_or_else(|e| e.into_inner())
}

/// Забрать флаг «панели сменились» (кадр применяет цвет один раз).
#[allow(dead_code)]
pub(crate) fn take_system_bars_dirty() -> Option<SystemBars> {
    if SYSTEM_BARS_DIRTY.swap(false, std::sync::atomic::Ordering::SeqCst) {
        system_bars()
    } else {
        None
    }
}

/// Будет ли [`share_text`] настоящим «Поделиться» (Android), а не копией в
/// буфер обмена — чтобы приложение выбрало подпись кнопки.
pub fn has_native_share() -> bool {
    cfg!(target_os = "android")
}

/// MIME по расширению (наиболее частые типы медиа и документов).
pub fn mime_for_path(path: &Path) -> &'static str {
    let ext = path
        .extension()
        .and_then(|e| e.to_str())
        .map(|e| e.to_ascii_lowercase())
        .unwrap_or_default();
    match ext.as_str() {
        "jpg" | "jpeg" | "jfif" => "image/jpeg",
        "png" => "image/png",
        "gif" => "image/gif",
        "webp" => "image/webp",
        "bmp" => "image/bmp",
        "heic" => "image/heic",
        "heif" => "image/heif",
        "avif" => "image/avif",
        "svg" => "image/svg+xml",
        "tif" | "tiff" => "image/tiff",
        "mp4" | "m4v" => "video/mp4",
        "webm" => "video/webm",
        "mkv" => "video/x-matroska",
        "mov" => "video/quicktime",
        "avi" => "video/x-msvideo",
        "3gp" => "video/3gpp",
        "pdf" => "application/pdf",
        "txt" => "text/plain",
        "json" => "application/json",
        "html" | "htm" => "text/html",
        "zip" => "application/zip",
        _ => "application/octet-stream",
    }
}

// ── Android ─────────────────────────────────────────────────────────────

#[cfg(target_os = "android")]
fn imp_pick(kind: MediaKind, cb: PickCallback) {
    android::pick_media(kind, cb)
}
#[cfg(target_os = "android")]
fn imp_share_text(title: &str, text: &str) -> Result<(), String> {
    android::share_text(title, text)
}
#[cfg(target_os = "android")]
fn imp_share_file(path: &Path, mime: &str, title: &str) -> Result<(), String> {
    android::share_file(path, mime, title)
}
#[cfg(target_os = "android")]
fn imp_open_path(path: &Path) -> Result<(), String> {
    android::open_path(path)
}

// ── Linux ───────────────────────────────────────────────────────────────

#[cfg(all(target_os = "linux", not(target_arch = "wasm32")))]
fn imp_pick(kind: MediaKind, cb: PickCallback) {
    linux::pick_media(kind, cb)
}
#[cfg(all(target_os = "linux", not(target_arch = "wasm32")))]
fn imp_share_text(title: &str, text: &str) -> Result<(), String> {
    linux::share_text(title, text)
}
#[cfg(all(target_os = "linux", not(target_arch = "wasm32")))]
fn imp_share_file(path: &Path, mime: &str, title: &str) -> Result<(), String> {
    linux::share_file(path, mime, title)
}
#[cfg(all(target_os = "linux", not(target_arch = "wasm32")))]
fn imp_open_path(path: &Path) -> Result<(), String> {
    linux::open_path(path)
}

// ── Прочие платформы ────────────────────────────────────────────────────

#[cfg(not(any(target_os = "android", all(target_os = "linux", not(target_arch = "wasm32")))))]
const UNSUPPORTED: &str = "не поддерживается";

#[cfg(not(any(target_os = "android", all(target_os = "linux", not(target_arch = "wasm32")))))]
fn imp_pick(_kind: MediaKind, cb: PickCallback) {
    crate::async_runtime::run_on_main_thread(move || cb(Err(UNSUPPORTED.into())));
}
#[cfg(not(any(target_os = "android", all(target_os = "linux", not(target_arch = "wasm32")))))]
fn imp_share_text(_title: &str, _text: &str) -> Result<(), String> {
    Err(UNSUPPORTED.into())
}
#[cfg(not(any(target_os = "android", all(target_os = "linux", not(target_arch = "wasm32")))))]
fn imp_share_file(_path: &Path, _mime: &str, _title: &str) -> Result<(), String> {
    Err(UNSUPPORTED.into())
}
#[cfg(not(any(target_os = "android", all(target_os = "linux", not(target_arch = "wasm32")))))]
fn imp_open_path(_path: &Path) -> Result<(), String> {
    Err(UNSUPPORTED.into())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn mime_by_extension() {
        assert_eq!(mime_for_path(Path::new("/a/b.JPG")), "image/jpeg");
        assert_eq!(mime_for_path(Path::new("v.mp4")), "video/mp4");
        assert_eq!(mime_for_path(Path::new("noext")), "application/octet-stream");
    }

    /// Вне Android панели — без действий, но выбор запоминается и один раз
    /// помечается для кадра.
    #[test]
    fn system_bars_remembered() {
        let c = crate::core::Color::from_srgb(0x1E, 0x1A, 0x16, 1.0);
        assert_eq!(set_system_bars(c, true), Ok(()));
        assert_eq!(system_bars(), Some(SystemBars { color: c, light_icons: true }));
        assert!(take_system_bars_dirty().is_some());
        assert!(take_system_bars_dirty().is_none());
    }
}
