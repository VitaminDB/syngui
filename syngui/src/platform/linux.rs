//! Linux: xdg-desktop-portal (фича `portal`) с fallback на системные утилиты.
//!
//! В synshell портал `FileChooser` реализует Проводник оболочки (synfiles
//! `--choose`), в KDE — xdg-desktop-portal-kde: приложению всё равно.

use std::path::{Path, PathBuf};
use std::process::{Command, Stdio};

use super::{MediaKind, PickCallback, PickedFile};

pub(super) fn pick_media(kind: MediaKind, cb: PickCallback) {
    let spawned = std::thread::Builder::new()
        .name("syngui-pick".into())
        .spawn(move || {
            let result = pick_blocking(kind);
            crate::async_runtime::run_on_main_thread(move || cb(result));
        });
    if let Err(e) = spawned {
        log::warn!("[syngui] pick_media: поток не запустился: {e}");
    }
}

fn pick_blocking(kind: MediaKind) -> Result<Option<PickedFile>, String> {
    #[cfg(feature = "portal")]
    match portal::open_file(kind) {
        Ok(path) => return Ok(path.map(picked)),
        Err(e) => log::info!("[syngui] портал FileChooser недоступен ({e}), пробую zenity/kdialog"),
    }
    match dialog_fallback(kind)? {
        Some(path) => Ok(Some(picked(path))),
        None => Ok(None),
    }
}

fn picked(path: PathBuf) -> PickedFile {
    let name = path
        .file_name()
        .map(|n| n.to_string_lossy().into_owned())
        .unwrap_or_default();
    let mime = super::mime_for_path(&path).to_string();
    PickedFile { path, name, mime }
}

pub(super) fn title(kind: MediaKind) -> &'static str {
    match kind {
        MediaKind::Image => "Выберите фото",
        MediaKind::Video => "Выберите видео",
        MediaKind::ImageOrVideo => "Выберите фото или видео",
    }
}

/// Маски для диалогов без MIME-фильтров.
const IMAGE_GLOBS: &[&str] = &[
    "*.jpg", "*.jpeg", "*.png", "*.webp", "*.gif", "*.bmp", "*.heic", "*.avif",
];
const VIDEO_GLOBS: &[&str] = &["*.mp4", "*.m4v", "*.webm", "*.mkv", "*.mov", "*.avi", "*.3gp"];

fn globs(kind: MediaKind) -> Vec<&'static str> {
    match kind {
        MediaKind::Image => IMAGE_GLOBS.to_vec(),
        MediaKind::Video => VIDEO_GLOBS.to_vec(),
        MediaKind::ImageOrVideo => IMAGE_GLOBS.iter().chain(VIDEO_GLOBS).copied().collect(),
    }
}

fn has_cmd(name: &str) -> bool {
    std::env::var_os("PATH").is_some_and(|paths| {
        std::env::split_paths(&paths).any(|dir| dir.join(name).is_file())
    })
}

/// `zenity --file-selection` / `kdialog --getopenfilename`. Код выхода 1 —
/// отмена.
fn dialog_fallback(kind: MediaKind) -> Result<Option<PathBuf>, String> {
    let globs = globs(kind);
    let kde = std::env::var("XDG_CURRENT_DESKTOP").is_ok_and(|d| d.contains("KDE"));
    let use_kdialog = has_cmd("kdialog") && (kde || !has_cmd("zenity"));
    let mut cmd = if use_kdialog {
        let mut c = Command::new("kdialog");
        let home = std::env::var("HOME").unwrap_or_else(|_| "/".into());
        c.arg("--title").arg(title(kind));
        c.arg("--getopenfilename").arg(home).arg(format!("{} ({})", globs.join(" "), title(kind)));
        c
    } else if has_cmd("zenity") {
        let mut c = Command::new("zenity");
        c.arg("--file-selection").arg(format!("--title={}", title(kind)));
        c.arg(format!("--file-filter={} | {}", title(kind), globs.join(" ")));
        c
    } else {
        return Err("нет диалога выбора файла: ни портала, ни zenity/kdialog".into());
    };
    let out = cmd
        .stdin(Stdio::null())
        .stderr(Stdio::null())
        .output()
        .map_err(|e| format!("диалог выбора файла: {e}"))?;
    if !out.status.success() {
        return Ok(None);
    }
    let path = String::from_utf8_lossy(&out.stdout).trim_end_matches('\n').to_string();
    Ok((!path.is_empty()).then(|| PathBuf::from(path)))
}

// ── Поделиться / открыть ────────────────────────────────────────────────

pub(super) fn share_text(_title: &str, text: &str) -> Result<(), String> {
    #[cfg(feature = "clipboard")]
    {
        crate::clipboard::copy(text);
        Ok(())
    }
    #[cfg(not(feature = "clipboard"))]
    {
        pipe_to_clipboard(text)
    }
}

#[cfg(not(feature = "clipboard"))]
fn pipe_to_clipboard(text: &str) -> Result<(), String> {
    use std::io::Write;
    let (prog, args): (&str, &[&str]) = if std::env::var_os("WAYLAND_DISPLAY").is_some() && has_cmd("wl-copy") {
        ("wl-copy", &[])
    } else if has_cmd("xclip") {
        ("xclip", &["-selection", "clipboard"])
    } else {
        return Err("буфер обмена недоступен (нет фичи `clipboard`, wl-copy, xclip)".into());
    };
    let mut child = Command::new(prog)
        .args(args)
        .stdin(Stdio::piped())
        .spawn()
        .map_err(|e| format!("{prog}: {e}"))?;
    child
        .stdin
        .take()
        .ok_or("нет stdin")?
        .write_all(text.as_bytes())
        .map_err(|e| format!("{prog}: {e}"))?;
    // wl-copy форкается и держит буфер сам — ждём только родителя.
    let _ = child.wait();
    Ok(())
}

/// Файл кладётся в буфер обмена (`text/uri-list` — вставляется в
/// мессенджер/файловый менеджер как файл) и показывается в файловом
/// менеджере.
pub(super) fn share_file(path: &Path, _mime: &str, _title: &str) -> Result<(), String> {
    let path = path
        .canonicalize()
        .map_err(|e| format!("{}: {e}", path.display()))?;
    let uri = crate::clipboard::file_uri(&path);
    #[cfg(feature = "clipboard")]
    crate::clipboard::copy_rich(&path.to_string_lossy(), &[("text/uri-list", uri.as_bytes())]);
    show_in_file_manager(&path, &uri)
}

fn show_in_file_manager(path: &Path, _uri: &str) -> Result<(), String> {
    #[cfg(feature = "portal")]
    match portal::show_items(_uri) {
        Ok(()) => return Ok(()),
        Err(e) => log::info!("[syngui] FileManager1.ShowItems: {e}"),
    }
    let dir = path.parent().unwrap_or(Path::new("/"));
    xdg_open(dir)
}

fn in_flatpak() -> bool {
    Path::new("/.flatpak-info").exists()
}

pub(super) fn open_path(path: &Path) -> Result<(), String> {
    // Вне песочницы xdg-open сразу запускает приложение по умолчанию; портал
    // OpenURI может сначала спросить, чем открыть. Во flatpak — только портал.
    #[cfg(feature = "portal")]
    if in_flatpak() || !has_cmd("xdg-open") {
        return portal::open_file_uri(path);
    }
    if in_flatpak() {
        return Err("во flatpak нужен портал: включите фичу syngui `portal`".into());
    }
    xdg_open(path)
}

fn xdg_open(path: &Path) -> Result<(), String> {
    let mut child = Command::new("xdg-open")
        .arg(path)
        .stdin(Stdio::null())
        .stdout(Stdio::null())
        .stderr(Stdio::null())
        .spawn()
        .map_err(|e| format!("xdg-open: {e}"))?;
    // Не держим зомби: дожидаемся в фоне.
    std::thread::spawn(move || {
        let _ = child.wait();
    });
    Ok(())
}

/// `file:///a%20b` → `/a b`. Не-file URI — `None`.
#[cfg_attr(not(feature = "portal"), allow(dead_code))]
pub(super) fn uri_to_path(uri: &str) -> Option<PathBuf> {
    let rest = uri.strip_prefix("file://")?;
    // Хост (`file://localhost/…`) отбрасываем.
    let rest = &rest[rest.find('/')?..];
    let bytes = rest.as_bytes();
    let mut out = Vec::with_capacity(bytes.len());
    let mut i = 0;
    while i < bytes.len() {
        if bytes[i] == b'%' && i + 2 < bytes.len() {
            let hex = std::str::from_utf8(&bytes[i + 1..i + 3]).ok()?;
            out.push(u8::from_str_radix(hex, 16).ok()?);
            i += 3;
        } else {
            out.push(bytes[i]);
            i += 1;
        }
    }
    use std::os::unix::ffi::OsStringExt;
    Some(PathBuf::from(std::ffi::OsString::from_vec(out)))
}

#[cfg(feature = "portal")]
mod portal {
    use std::collections::HashMap;
    use std::path::{Path, PathBuf};

    use zbus::blocking::{Connection, Proxy};
    use zbus::zvariant::{OwnedObjectPath, OwnedValue, Value};

    use super::super::MediaKind;

    const DEST: &str = "org.freedesktop.portal.Desktop";
    const PATH: &str = "/org/freedesktop/portal/desktop";

    fn token() -> String {
        static SEQ: std::sync::atomic::AtomicU32 = std::sync::atomic::AtomicU32::new(0);
        let n = SEQ.fetch_add(1, std::sync::atomic::Ordering::Relaxed);
        format!("syngui{}_{n}", std::process::id())
    }

    /// Вызвать метод портала, возвращающий Request, и дождаться `Response`.
    /// Подписка ставится ДО вызова на предсказуемый путь запроса
    /// (`…/request/<sender>/<token>`), иначе быстрый ответ можно пропустить.
    fn request(
        conn: &Connection,
        token: &str,
        call: impl FnOnce() -> Result<OwnedObjectPath, String>,
    ) -> Result<(u32, HashMap<String, OwnedValue>), String>
    {
        let sender = conn
            .unique_name()
            .ok_or("нет имени на шине")?
            .as_str()
            .trim_start_matches(':')
            .replace('.', "_");
        let expected = format!("{PATH}/request/{sender}/{token}");
        let req = Proxy::new(conn, DEST, expected.as_str(), "org.freedesktop.portal.Request")
            .map_err(|e| e.to_string())?;
        let mut signals = req.receive_signal("Response").map_err(|e| e.to_string())?;
        let handle = call()?;
        let message = if handle.as_str() == expected {
            signals.next()
        } else {
            // Старый портал (< 0.9) выдаёт свой путь.
            let req = Proxy::new(conn, DEST, handle.as_str(), "org.freedesktop.portal.Request")
                .map_err(|e| e.to_string())?;
            req.receive_signal("Response").map_err(|e| e.to_string())?.next()
        }
        .ok_or("портал закрыл соединение")?;
        message
            .body()
            .deserialize::<(u32, HashMap<String, OwnedValue>)>()
            .map_err(|e| e.to_string())
    }

    pub(super) fn open_file(kind: MediaKind) -> Result<Option<PathBuf>, String> {
        let conn = Connection::session().map_err(|e| e.to_string())?;
        let token = token();
        let images = ("Изображения".to_string(), vec![(1u32, "image/*".to_string())]);
        let videos = ("Видео".to_string(), vec![(1u32, "video/*".to_string())]);
        let both = (
            "Фото и видео".to_string(),
            vec![(1u32, "image/*".to_string()), (1u32, "video/*".to_string())],
        );
        let filters = match kind {
            MediaKind::Image => vec![images],
            MediaKind::Video => vec![videos],
            MediaKind::ImageOrVideo => vec![both, images, videos],
        };
        let mut options: HashMap<&str, Value<'_>> = HashMap::new();
        options.insert("handle_token", Value::from(token.as_str()));
        options.insert("modal", Value::from(true));
        options.insert("multiple", Value::from(false));
        options.insert("current_filter", Value::from(filters[0].clone()));
        options.insert("filters", Value::from(filters));
        let proxy = Proxy::new(&conn, DEST, PATH, "org.freedesktop.portal.FileChooser")
            .map_err(|e| e.to_string())?;
        let (code, results) = request(&conn, &token, || {
            proxy
                .call("OpenFile", &("", super::title(kind), options))
                .map_err(|e| e.to_string())
        })?;
        match code {
            0 => {}
            1 => return Ok(None),
            n => return Err(format!("портал FileChooser: ответ {n}")),
        }
        let uris: Vec<String> = results
            .get("uris")
            .and_then(|v| Vec::<String>::try_from(v.try_clone().ok()?).ok())
            .unwrap_or_default();
        let Some(uri) = uris.first() else { return Ok(None) };
        super::uri_to_path(uri)
            .map(Some)
            .ok_or_else(|| format!("портал вернул не file-URI: {uri}"))
    }

    pub(super) fn open_file_uri(path: &Path) -> Result<(), String> {
        let conn = Connection::session().map_err(|e| e.to_string())?;
        let file = std::fs::File::open(path).map_err(|e| format!("{}: {e}", path.display()))?;
        let proxy = Proxy::new(&conn, DEST, PATH, "org.freedesktop.portal.OpenURI")
            .map_err(|e| e.to_string())?;
        let mut options: HashMap<&str, Value<'_>> = HashMap::new();
        options.insert("handle_token", Value::from(token()));
        let fd = zbus::zvariant::Fd::from(&file);
        // Ответ (какое приложение выбрано) не ждём: файл уже передан порталу.
        let _: OwnedObjectPath = proxy
            .call("OpenFile", &("", fd, options))
            .map_err(|e| e.to_string())?;
        Ok(())
    }

    pub(super) fn show_items(uri: &str) -> Result<(), String> {
        let conn = Connection::session().map_err(|e| e.to_string())?;
        let proxy = Proxy::new(
            &conn,
            "org.freedesktop.FileManager1",
            "/org/freedesktop/FileManager1",
            "org.freedesktop.FileManager1",
        )
        .map_err(|e| e.to_string())?;
        proxy
            .call::<_, _, ()>("ShowItems", &(vec![uri], ""))
            .map_err(|e| e.to_string())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn decode_file_uri() {
        assert_eq!(
            uri_to_path("file:///home/u/%D0%A4%D0%BE%D1%82%D0%BE%201.jpg"),
            Some(PathBuf::from("/home/u/Фото 1.jpg"))
        );
        assert_eq!(uri_to_path("file://localhost/a/b"), Some(PathBuf::from("/a/b")));
        assert_eq!(uri_to_path("https://x/y"), None);
    }
}
