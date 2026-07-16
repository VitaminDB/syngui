#[cfg(feature = "links")]
pub fn open_url(url: &str) -> Result<(), String> {
    webbrowser::open(url).map_err(|e| e.to_string())
}

#[cfg(not(feature = "links"))]
pub fn open_url(_url: &str) -> Result<(), String> {
    Err("syngui feature `links` is disabled — enable it or provide a custom handler".into())
}
