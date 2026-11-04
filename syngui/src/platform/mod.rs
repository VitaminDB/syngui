//! Платформенные службы: выбор фото/видео, «Поделиться», открыть файл.
//!
//! | Функция        | Android                                   | Linux                                              |
//! |----------------|-------------------------------------------|----------------------------------------------------|
//! | [`pick_media`] | системный фото-пикер (`ACTION_PICK_IMAGES`, fallback `ACTION_GET_CONTENT`), копия в кэш приложения | портал `FileChooser.OpenFile` (фича `portal`), fallback `zenity` / `kdialog` |
//! | [`share_text`] | `ACTION_SEND` + chooser                   | текст в буфер обмена                               |
//! | [`share_file`] | `ACTION_SEND` + chooser (FileProvider или копия в Downloads) | файл в буфер (`text/uri-list`) и показ в файловом менеджере |
//! | [`open_path`]  | `ACTION_VIEW`                             | `xdg-open`, портал `OpenURI.OpenFile` (во flatpak — первым) |
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
}
