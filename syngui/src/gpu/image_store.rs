use hashbrown::HashMap;
use std::sync::Arc;
use crate::core::sync::Mutex;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub struct ImageHandle(pub u32);

#[derive(Clone)]
pub enum ImageSource {
    Path(String),
    Bytes { key: String, data: Arc<Vec<u8>> },
    RawRgba { key: String, width: u32, height: u32, rgba: Arc<Vec<u8>> },
    Url(String),
}

impl ImageSource {
    fn key(&self) -> &str {
        match self {
            ImageSource::Path(path) => path,
            ImageSource::Bytes { key, .. } => key,
            ImageSource::RawRgba { key, .. } => key,
            ImageSource::Url(url) => url,
        }
    }
}

pub struct ImageData {
    pub width: u32,
    pub height: u32,
    pub rgba: Arc<[u8]>,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum ImageLoadState {
    Loading,
    Ready,
    Failed,
}

struct ImageEntry {
    handle: ImageHandle,
    state: ImageLoadState,
    width: u32,
    height: u32,
}

#[allow(dead_code)]
enum LoadResult {
    Success { key: String, handle: ImageHandle, data: ImageData },
    Failed { key: String },
}

pub struct ImageStore {
    images: HashMap<String, ImageEntry>,
    handle_to_key: HashMap<u32, String>,
    next_handle: u32,
    pending_uploads: Vec<(ImageHandle, ImageData)>,
    bg_results: Arc<Mutex<Vec<LoadResult>>>,
}

impl ImageStore {
    pub fn new() -> Self {
        Self {
            images: HashMap::new(),
            handle_to_key: HashMap::new(),
            next_handle: 1,
            pending_uploads: Vec::new(),
            bg_results: Arc::new(Mutex::new(Vec::new())),
        }
    }

    pub fn request(&mut self, source: &ImageSource) -> (ImageHandle, ImageLoadState) {
        let key = source.key().to_string();

        if let Some(entry) = self.images.get(&key) {
            return (entry.handle, entry.state);
        }

        let handle = ImageHandle(self.next_handle);
        self.next_handle += 1;
        self.handle_to_key.insert(handle.0, key.clone());

        match source {
            ImageSource::RawRgba { width, height, rgba, .. } => {
                self.images.insert(key, ImageEntry {
                    handle,
                    state: ImageLoadState::Ready,
                    width: *width,
                    height: *height,
                });
                self.pending_uploads.push((handle, ImageData {
                    width: *width,
                    height: *height,
                    rgba: Arc::from(rgba.as_slice()),
                }));
                (handle, ImageLoadState::Ready)
            }
            ImageSource::Bytes { data, .. } => {
                self.images.insert(key.clone(), ImageEntry {
                    handle,
                    state: ImageLoadState::Loading,
                    width: 0,
                    height: 0,
                });
                self.spawn_decode(key, handle, data.clone());
                (handle, ImageLoadState::Loading)
            }
            ImageSource::Path(path) => {
                self.images.insert(key.clone(), ImageEntry {
                    handle,
                    state: ImageLoadState::Loading,
                    width: 0,
                    height: 0,
                });
                self.spawn_load(key, handle, path.clone());
                (handle, ImageLoadState::Loading)
            }
            ImageSource::Url(url) => {
                self.images.insert(key.clone(), ImageEntry {
                    handle,
                    state: ImageLoadState::Loading,
                    width: 0,
                    height: 0,
                });
                self.spawn_url_load(key, handle, url.clone());
                (handle, ImageLoadState::Loading)
            }
        }
    }

    pub fn request_rgba(&mut self, key: &str, width: u32, height: u32, rgba: Vec<u8>) -> (ImageHandle, ImageLoadState) {
        let source = ImageSource::RawRgba {
            key: key.to_string(),
            width,
            height,
            rgba: Arc::new(rgba),
        };
        self.request(&source)
    }

    pub fn update_rgba(
        &mut self,
        handle: ImageHandle,
        width: u32,
        height: u32,
        rgba: impl Into<Arc<[u8]>>,
    ) {
        let rgba = rgba.into();
        let Some(key) = self.handle_to_key.get(&handle.0) else {
            return;
        };
        if let Some(entry) = self.images.get_mut(key) {
            entry.state = ImageLoadState::Ready;
            entry.width = width;
            entry.height = height;
        }
        self.pending_uploads.push((handle, ImageData { width, height, rgba }));
    }

    pub fn take_pending_uploads(&mut self) -> Vec<(ImageHandle, ImageData)> {
        std::mem::take(&mut self.pending_uploads)
    }

    pub fn state_of(&self, handle: ImageHandle) -> Option<ImageLoadState> {
        let key = self.handle_to_key.get(&handle.0)?;
        self.images.get(key).map(|e| e.state)
    }

    pub fn dimensions(&self, handle: ImageHandle) -> Option<(u32, u32)> {
        let key = self.handle_to_key.get(&handle.0)?;
        self.images
            .get(key)
            .filter(|e| e.state == ImageLoadState::Ready)
            .map(|e| (e.width, e.height))
    }

    pub fn poll_bg(&mut self) {
        let results: Vec<LoadResult> = {
            let mut guard = self.bg_results.lock().unwrap();
            std::mem::take(&mut *guard)
        };
        for result in results {
            match result {
                LoadResult::Success { key, handle, data } => {
                    if let Some(entry) = self.images.get_mut(&key) {
                        entry.state = ImageLoadState::Ready;
                        entry.width = data.width;
                        entry.height = data.height;
                    }
                    self.pending_uploads.push((handle, data));
                }
                LoadResult::Failed { key } => {
                    if let Some(entry) = self.images.get_mut(&key) {
                        entry.state = ImageLoadState::Failed;
                    }
                }
            }
        }
    }

    pub fn has_loading(&self) -> bool {
        self.images.values().any(|e| e.state == ImageLoadState::Loading)
    }

    #[cfg(feature = "image")]
    fn spawn_decode(&self, key: String, handle: ImageHandle, data: Arc<Vec<u8>>) {
        let results = self.bg_results.clone();
        std::thread::spawn(move || {
            match decode_image_bytes(&data) {
                Ok(image_data) => {
                    results.lock().unwrap().push(LoadResult::Success {
                        key,
                        handle,
                        data: image_data,
                    });
                }
                Err(_e) => {
                    log::error!("Failed to decode image '{}': {}", key, _e);
                    results.lock().unwrap().push(LoadResult::Failed { key });
                }
            }
        });
    }

    #[cfg(not(feature = "image"))]
    fn spawn_decode(&mut self, key: String, _handle: ImageHandle, _data: Arc<Vec<u8>>) {
        log::warn!("Image decoding requires 'image' feature. Image '{}' will not load.", key);
        if let Some(entry) = self.images.get_mut(&key) {
            entry.state = ImageLoadState::Failed;
        }
    }

    #[cfg(feature = "image")]
    fn spawn_load(&self, key: String, handle: ImageHandle, path: String) {
        let results = self.bg_results.clone();
        std::thread::spawn(move || {
            match std::fs::read(&path) {
                Ok(bytes) => match decode_image_bytes(&bytes) {
                    Ok(image_data) => {
                        results.lock().unwrap().push(LoadResult::Success {
                            key,
                            handle,
                            data: image_data,
                        });
                    }
                    Err(_e) => {
                        log::error!("Failed to decode image '{}': {}", key, _e);
                        results.lock().unwrap().push(LoadResult::Failed { key });
                    }
                },
                Err(_e) => {
                    log::error!("Failed to read image file '{}': {}", path, _e);
                    results.lock().unwrap().push(LoadResult::Failed { key });
                }
            }
        });
    }

    #[cfg(not(feature = "image"))]
    fn spawn_load(&mut self, key: String, _handle: ImageHandle, _path: String) {
        log::warn!("Image loading requires 'image' feature. Image '{}' will not load.", key);
        if let Some(entry) = self.images.get_mut(&key) {
            entry.state = ImageLoadState::Failed;
        }
    }

    #[cfg(feature = "image-network")]
    fn spawn_url_load(&self, key: String, handle: ImageHandle, url: String) {
        let results = self.bg_results.clone();
        std::thread::spawn(move || {
            let bytes_result = fetch_url_bytes(&url);
            match bytes_result {
                Ok(bytes) => match decode_image_bytes(&bytes) {
                    Ok(image_data) => {
                        results.lock().unwrap().push(LoadResult::Success {
                            key,
                            handle,
                            data: image_data,
                        });
                    }
                    Err(e) => {
                        log::error!("Failed to decode image '{}': {}", key, e);
                        results.lock().unwrap().push(LoadResult::Failed { key });
                    }
                },
                Err(e) => {
                    log::error!("Failed to fetch image '{}': {}", key, e);
                    results.lock().unwrap().push(LoadResult::Failed { key });
                }
            }
        });
    }

    #[cfg(not(feature = "image-network"))]
    fn spawn_url_load(&mut self, key: String, _handle: ImageHandle, _url: String) {
        log::warn!(
            "Image URL loading requires 'image-network' feature. Image '{}' will not load.",
            key
        );
        if let Some(entry) = self.images.get_mut(&key) {
            entry.state = ImageLoadState::Failed;
        }
    }
}

#[cfg(feature = "image")]
fn decode_image_bytes(bytes: &[u8]) -> Result<ImageData, String> {
    match image::load_from_memory(bytes) {
        Ok(img) => {
            let rgba = img.to_rgba8();
            let (w, h) = rgba.dimensions();
            Ok(ImageData {
                width: w,
                height: h,
                rgba: Arc::from(rgba.into_raw().into_boxed_slice()),
            })
        }
        #[cfg(feature = "svg")]
        Err(_) if looks_like_svg(bytes) => decode_svg(bytes),
        Err(e) => Err(e.to_string()),
    }
}

#[cfg(feature = "svg")]
fn looks_like_svg(bytes: &[u8]) -> bool {
    let n = bytes.len().min(1024);
    bytes[..n].windows(4).any(|w| w.eq_ignore_ascii_case(b"<svg"))
}

#[cfg(feature = "svg")]
fn decode_svg(bytes: &[u8]) -> Result<ImageData, String> {
    use resvg::tiny_skia;
    use resvg::usvg;

    let opt = usvg::Options::default();
    let tree = usvg::Tree::from_data(bytes, &opt).map_err(|e| format!("svg parse: {e}"))?;
    let size = tree.size();
    let max_side = size.width().max(size.height());
    const MAX_PX: f32 = 2048.0;
    let scale = if max_side > MAX_PX {
        MAX_PX / max_side
    } else {
        1.0
    };
    let w_px = ((size.width() * scale).round() as u32).max(1);
    let h_px = ((size.height() * scale).round() as u32).max(1);
    let mut pixmap = tiny_skia::Pixmap::new(w_px, h_px)
        .ok_or_else(|| format!("pixmap alloc {w_px}x{h_px}"))?;
    let transform = tiny_skia::Transform::from_scale(scale, scale);
    resvg::render(&tree, transform, &mut pixmap.as_mut());

    let mut rgba = pixmap.data().to_vec();
    for px in rgba.chunks_exact_mut(4) {
        let a = px[3];
        if a > 0 && a < 255 {
            let inv = 255.0 / a as f32;
            px[0] = ((px[0] as f32 * inv).round() as u32).min(255) as u8;
            px[1] = ((px[1] as f32 * inv).round() as u32).min(255) as u8;
            px[2] = ((px[2] as f32 * inv).round() as u32).min(255) as u8;
        }
    }
    Ok(ImageData {
        width: w_px,
        height: h_px,
        rgba: Arc::from(rgba.into_boxed_slice()),
    })
}

#[cfg(feature = "image-network")]
fn fetch_url_bytes(url: &str) -> Result<Vec<u8>, String> {
    if let Some(rest) = url.strip_prefix("data:") {
        return decode_data_url(rest);
    }
    if let Some(path) = url.strip_prefix("file://") {
        let decoded = percent_decode_path(path);
        return std::fs::read(&decoded).map_err(|e| format!("file://{}: {e}", decoded));
    }
    if url.starts_with("http://") || url.starts_with("https://") {
        let mut response = ureq::get(url)
            .call()
            .map_err(|e| format!("HTTP {url}: {e}"))?;
        let mut buf = Vec::new();
        std::io::Read::read_to_end(&mut response.body_mut().as_reader(), &mut buf)
            .map_err(|e| format!("HTTP body {url}: {e}"))?;
        return Ok(buf);
    }
    Err(format!("unsupported url scheme: {url}"))
}

#[cfg(feature = "image-network")]
fn decode_data_url(rest: &str) -> Result<Vec<u8>, String> {
    use base64::Engine;
    let comma = rest
        .find(',')
        .ok_or_else(|| "data: URL без запятой".to_string())?;
    let header = &rest[..comma];
    let payload = &rest[comma + 1..];
    let is_base64 = header.split(';').any(|p| p.eq_ignore_ascii_case("base64"));
    if is_base64 {
        base64::engine::general_purpose::STANDARD
            .decode(payload.as_bytes())
            .map_err(|e| format!("base64 decode: {e}"))
    } else {
        Ok(percent_decode_bytes(payload))
    }
}

#[cfg(feature = "image-network")]
fn percent_decode_bytes(s: &str) -> Vec<u8> {
    let bytes = s.as_bytes();
    let mut out = Vec::with_capacity(bytes.len());
    let mut i = 0;
    while i < bytes.len() {
        if bytes[i] == b'%' && i + 2 < bytes.len() {
            let hi = (bytes[i + 1] as char).to_digit(16);
            let lo = (bytes[i + 2] as char).to_digit(16);
            if let (Some(hi), Some(lo)) = (hi, lo) {
                out.push(((hi << 4) | lo) as u8);
                i += 3;
                continue;
            }
        }
        out.push(bytes[i]);
        i += 1;
    }
    out
}

#[cfg(feature = "image-network")]
fn percent_decode_path(s: &str) -> String {
    String::from_utf8_lossy(&percent_decode_bytes(s)).into_owned()
}

#[cfg(test)]
mod tests {
    use super::*;

    fn solid(width: u32, height: u32, byte: u8) -> Vec<u8> {
        vec![byte; (width * height * 4) as usize]
    }

    #[test]
    fn request_rgba_returns_ready_handle() {
        let mut store = ImageStore::new();
        let (handle, state) = store.request_rgba("k1", 4, 4, solid(4, 4, 0));
        assert_eq!(state, ImageLoadState::Ready);
        assert_eq!(store.state_of(handle), Some(ImageLoadState::Ready));
        assert_eq!(store.dimensions(handle), Some((4, 4)));
        let uploads = store.take_pending_uploads();
        assert_eq!(uploads.len(), 1);
        assert_eq!(uploads[0].0, handle);
        assert_eq!(uploads[0].1.rgba.len(), 64);
    }

    #[test]
    fn update_rgba_overwrites_pending_upload_for_same_handle() {
        let mut store = ImageStore::new();
        let (handle, _) = store.request_rgba("video", 2, 2, solid(2, 2, 0x10));
        let _ = store.take_pending_uploads();

        store.update_rgba(handle, 2, 2, solid(2, 2, 0xAA));
        let uploads = store.take_pending_uploads();
        assert_eq!(uploads.len(), 1, "должен быть ровно один pending upload");
        assert_eq!(uploads[0].0, handle);
        assert!(uploads[0].1.rgba.iter().all(|&b| b == 0xAA));
        assert_eq!(store.state_of(handle), Some(ImageLoadState::Ready));
    }

    #[test]
    fn update_rgba_can_change_dimensions() {
        let mut store = ImageStore::new();
        let (handle, _) = store.request_rgba("img", 4, 4, solid(4, 4, 0));
        let _ = store.take_pending_uploads();

        store.update_rgba(handle, 8, 6, solid(8, 6, 0));
        assert_eq!(store.dimensions(handle), Some((8, 6)));
        let uploads = store.take_pending_uploads();
        assert_eq!(uploads[0].1.width, 8);
        assert_eq!(uploads[0].1.height, 6);
        assert_eq!(uploads[0].1.rgba.len(), 8 * 6 * 4);
    }

    #[test]
    fn update_rgba_unknown_handle_is_noop() {
        let mut store = ImageStore::new();
        store.update_rgba(ImageHandle(999), 1, 1, solid(1, 1, 0));
        assert!(store.take_pending_uploads().is_empty());
        assert_eq!(store.state_of(ImageHandle(999)), None);
    }

    #[test]
    fn handle_to_key_index_used_for_state_of() {
        let mut store = ImageStore::new();
        let (h1, _) = store.request_rgba("a", 2, 2, solid(2, 2, 0));
        let (h2, _) = store.request_rgba("b", 3, 3, solid(3, 3, 0));
        assert_ne!(h1, h2);
        assert_eq!(store.state_of(h1), Some(ImageLoadState::Ready));
        assert_eq!(store.state_of(h2), Some(ImageLoadState::Ready));
        assert_eq!(store.dimensions(h1), Some((2, 2)));
        assert_eq!(store.dimensions(h2), Some((3, 3)));
    }

    #[cfg(feature = "svg")]
    #[test]
    fn looks_like_svg_recognizes_xml_and_raw_tag() {
        assert!(looks_like_svg(b"<?xml version=\"1.0\"?><svg/>"));
        assert!(looks_like_svg(b"<svg width=\"10\"/>"));
        assert!(looks_like_svg(b"  \n<SVG xmlns=\"...\"/>"));
        assert!(!looks_like_svg(b"<html><body/></html>"));
        assert!(!looks_like_svg(&[0x89, 0x50, 0x4E, 0x47]));
    }

    #[cfg(feature = "svg")]
    #[test]
    fn decode_image_bytes_renders_svg_to_rgba() {
        let svg = br##"<?xml version="1.0"?><svg xmlns="http://www.w3.org/2000/svg" width="32" height="32"><rect width="32" height="32" fill="#ff0000"/></svg>"##;
        let data = decode_image_bytes(svg).expect("должен распарсить и отрендерить SVG");
        assert_eq!(data.width, 32);
        assert_eq!(data.height, 32);
        assert_eq!(data.rgba.len(), 32 * 32 * 4);
        let has_red = data
            .rgba
            .chunks_exact(4)
            .any(|p| p[0] > 200 && p[1] < 50 && p[2] < 50 && p[3] == 255);
        assert!(has_red, "ожидаем красный пиксель в результате рендера");
    }

    #[cfg(feature = "svg")]
    #[test]
    fn decode_image_bytes_caps_oversized_svg() {
        let svg = br##"<svg xmlns="http://www.w3.org/2000/svg" width="4096" height="4096"><rect width="100%" height="100%" fill="#000"/></svg>"##;
        let data = decode_image_bytes(svg).expect("oversized svg должен растеризоваться");
        assert_eq!(data.width, 2048);
        assert_eq!(data.height, 2048);
    }
}
