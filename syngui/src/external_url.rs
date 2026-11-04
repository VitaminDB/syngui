/// Открыть внешнюю ссылку. На Android — всегда системным `ACTION_VIEW`
/// (см. [`crate::platform::open_url`]), фича `links` не нужна.
#[cfg(target_os = "android")]
pub fn open_url(url: &str) -> Result<(), String> {
    crate::platform::open_url(url)
}

#[cfg(all(feature = "links", not(target_os = "android")))]
pub fn open_url(url: &str) -> Result<(), String> {
    webbrowser::open(url).map_err(|e| e.to_string())
}

#[cfg(all(not(feature = "links"), not(target_os = "android")))]
pub fn open_url(_url: &str) -> Result<(), String> {
    Err("syngui feature `links` is disabled — enable it or provide a custom handler".into())
}
