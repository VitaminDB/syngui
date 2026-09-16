use crate::core::sync::Mutex;
use hashbrown::HashMap;
use std::sync::Arc;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub struct ImageHandle(pub u32);

#[derive(Clone)]
pub enum ImageSource {
    Path(String),
    Bytes {
        key: String,
        data: Arc<Vec<u8>>,
    },
    RawRgba {
        key: String,
        width: u32,
        height: u32,
        rgba: Arc<Vec<u8>>,
    },
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
    /// Готовые mip-уровни 1..n (считаются в потоке декодирования, а не в
    /// кадре). Пусто у потоковых кадров (`single_level`) и у данных, для
    /// которых цепочку достроит загрузчик.
    pub mips: Vec<MipLevel>,
    /// Только нулевой уровень: видеокадры и прочие потоковые обновления
    /// не минифицируются, а строить им мипы каждый кадр — десятки мс CPU.
    pub single_level: bool,
}

/// Один mip-уровень (RGBA8, premultiplied как и `ImageData::rgba`).
#[derive(Clone, Debug)]
pub struct MipLevel {
    pub width: u32,
    pub height: u32,
    pub rgba: Arc<[u8]>,
}

impl ImageData {
    /// Статичная картинка: цепочка мипов строится сразу (вызывать из
    /// фонового потока — для большого фото это десятки миллисекунд).
    pub fn with_mips(width: u32, height: u32, rgba: impl Into<Arc<[u8]>>) -> Self {
        let rgba: Arc<[u8]> = rgba.into();
        let mips = crate::gpu::image_cache::build_mips(width, height, &rgba);
        Self {
            width,
            height,
            rgba,
            mips,
            single_level: false,
        }
    }

    /// Потоковый кадр: без мипов, текстура с одним уровнем.
    pub fn single_level(width: u32, height: u32, rgba: impl Into<Arc<[u8]>>) -> Self {
        Self {
            width,
            height,
            rgba: rgba.into(),
            mips: Vec::new(),
            single_level: true,
        }
    }
}

/// Предел большей стороны декодированных растров (0 — без предела):
/// превышающие уменьшаются вдвое, пока не впишутся, ещё в потоке
/// декодирования. Для UI, показывающего постеры 200×300, декодировать
/// 1000×1500 и грузить 7 МБ мипов на кадр незачем. Задавать до первого
/// запроса картинки, например перед `start`.
static MAX_BITMAP_SIDE: std::sync::atomic::AtomicU32 = std::sync::atomic::AtomicU32::new(0);

pub fn set_max_bitmap_side(px: u32) {
    MAX_BITMAP_SIDE.store(px, std::sync::atomic::Ordering::Relaxed);
}

pub fn max_bitmap_side() -> u32 {
    MAX_BITMAP_SIDE.load(std::sync::atomic::Ordering::Relaxed)
}

/// Бюджет памяти (байт RGBA с мипами) на картинки, которые никто не
/// показывает (0 — без предела, ничего не выгружается). Картинка
/// становится «простаивающей», когда её отпустил последний `Image`
/// (размонтирован или сменил источник); сверх бюджета самые давние такие
/// удаляются из стора и из GPU, а при новом запросе грузятся заново. Нужен
/// экранам, листающим большие фото (фон-кадр под каждым тайтлом): без него
/// каждая показанная картинка остаётся в видеопамяти до выхода.
static IDLE_IMAGE_BUDGET: std::sync::atomic::AtomicUsize = std::sync::atomic::AtomicUsize::new(0);

pub fn set_idle_image_budget(bytes: usize) {
    IDLE_IMAGE_BUDGET.store(bytes, std::sync::atomic::Ordering::Relaxed);
}

pub fn idle_image_budget() -> usize {
    IDLE_IMAGE_BUDGET.load(std::sync::atomic::Ordering::Relaxed)
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
    /// Сколько держателей запросили картинку и ещё не отпустили
    /// ([`ImageStore::release`]). Держатели, которые не отпускают (видео,
    /// markdown), оставляют её в сторе навсегда, как и раньше.
    refs: u32,
}

impl ImageEntry {
    /// Занимаемая память: RGBA и цепочка мипов (≈ +1/3).
    fn bytes(&self) -> usize {
        self.width as usize * self.height as usize * 4 * 4 / 3
    }
}

#[allow(dead_code)]
enum LoadResult {
    Success {
        key: String,
        handle: ImageHandle,
        data: ImageData,
    },
    Failed {
        key: String,
    },
}

pub struct ImageStore {
    images: HashMap<String, ImageEntry>,
    handle_to_key: HashMap<u32, String>,
    next_handle: u32,
    pending_uploads: Vec<(ImageHandle, ImageData)>,
    bg_results: Arc<Mutex<Vec<LoadResult>>>,
    /// Готовые картинки без держателей, от давних к свежим.
    idle: std::collections::VecDeque<String>,
    idle_bytes: usize,
    /// Выгруженные handle — рендерер освобождает их текстуры.
    pending_frees: Vec<ImageHandle>,
}

impl ImageStore {
    pub fn new() -> Self {
        Self {
            images: HashMap::new(),
            handle_to_key: HashMap::new(),
            next_handle: 1,
            pending_uploads: Vec::new(),
            bg_results: Arc::new(Mutex::new(Vec::new())),
            idle: std::collections::VecDeque::new(),
            idle_bytes: 0,
            pending_frees: Vec::new(),
        }
    }

    /// Запросить картинку; каждый запрос — ссылка, которую держатель
    /// отпускает через [`Self::release`] (иначе картинка не выгружается).
    pub fn request(&mut self, source: &ImageSource) -> (ImageHandle, ImageLoadState) {
        let key = source.key().to_string();

        if let Some(entry) = self.images.get_mut(&key) {
            entry.refs = entry.refs.saturating_add(1);
            let (handle, state, bytes) = (entry.handle, entry.state, entry.bytes());
            if entry.refs == 1 && state == ImageLoadState::Ready {
                if let Some(i) = self.idle.iter().position(|k| *k == key) {
                    self.idle.remove(i);
                    self.idle_bytes = self.idle_bytes.saturating_sub(bytes);
                }
            }
            return (handle, state);
        }

        let handle = ImageHandle(self.next_handle);
        self.next_handle += 1;
        self.handle_to_key.insert(handle.0, key.clone());

        match source {
            ImageSource::RawRgba {
                width,
                height,
                rgba,
                ..
            } => {
                self.images.insert(
                    key,
                    ImageEntry {
                        handle,
                        state: ImageLoadState::Ready,
                        width: *width,
                        height: *height,
                        refs: 1,
                    },
                );
                self.pending_uploads.push((
                    handle,
                    ImageData::with_mips(*width, *height, Arc::<[u8]>::from(rgba.as_slice())),
                ));
                (handle, ImageLoadState::Ready)
            }
            ImageSource::Bytes { data, .. } => {
                self.images.insert(
                    key.clone(),
                    ImageEntry {
                        handle,
                        state: ImageLoadState::Loading,
                        width: 0,
                        height: 0,
                        refs: 1,
                    },
                );
                self.spawn_decode(key, handle, data.clone());
                (handle, ImageLoadState::Loading)
            }
            ImageSource::Path(path) => {
                self.images.insert(
                    key.clone(),
                    ImageEntry {
                        handle,
                        state: ImageLoadState::Loading,
                        width: 0,
                        height: 0,
                        refs: 1,
                    },
                );
                self.spawn_load(key, handle, path.clone());
                (handle, ImageLoadState::Loading)
            }
            ImageSource::Url(url) => {
                self.images.insert(
                    key.clone(),
                    ImageEntry {
                        handle,
                        state: ImageLoadState::Loading,
                        width: 0,
                        height: 0,
                        refs: 1,
                    },
                );
                self.spawn_url_load(key, handle, url.clone());
                (handle, ImageLoadState::Loading)
            }
        }
    }

    pub fn request_rgba(
        &mut self,
        key: &str,
        width: u32,
        height: u32,
        rgba: Vec<u8>,
    ) -> (ImageHandle, ImageLoadState) {
        let source = ImageSource::RawRgba {
            key: key.to_string(),
            width,
            height,
            rgba: Arc::new(rgba),
        };
        self.request(&source)
    }

    /// Отпустить ссылку, взятую [`Self::request`]. Готовая картинка без
    /// держателей становится простаивающей и может быть выгружена по
    /// бюджету [`set_idle_image_budget`].
    pub fn release(&mut self, handle: ImageHandle) {
        let Some(key) = self.handle_to_key.get(&handle.0) else {
            return;
        };
        let Some(entry) = self.images.get_mut(key) else {
            return;
        };
        if entry.refs == 0 {
            return;
        }
        entry.refs -= 1;
        if entry.refs == 0 && entry.state == ImageLoadState::Ready {
            self.idle_bytes += entry.bytes();
            self.idle.push_back(key.clone());
            self.evict_idle();
        }
    }

    /// Выгрузить самые давние простаивающие картинки сверх бюджета.
    fn evict_idle(&mut self) {
        let budget = idle_image_budget();
        if budget == 0 {
            return;
        }
        while self.idle_bytes > budget {
            let Some(key) = self.idle.pop_front() else {
                self.idle_bytes = 0;
                break;
            };
            let Some(entry) = self.images.remove(&key) else {
                continue;
            };
            self.idle_bytes = self.idle_bytes.saturating_sub(entry.bytes());
            self.handle_to_key.remove(&entry.handle.0);
            self.pending_uploads.retain(|(h, _)| *h != entry.handle);
            self.pending_frees.push(entry.handle);
        }
    }

    /// Handle выгруженных картинок, чьи текстуры пора освободить.
    pub fn take_pending_frees(&mut self) -> Vec<ImageHandle> {
        std::mem::take(&mut self.pending_frees)
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
        let data = ImageData::single_level(width, height, rgba);
        // Потоковые кадры (видео) идут в один и тот же handle: если
        // предыдущий ещё не залит, показывать его уже незачем — заменяем,
        // иначе картинка отстаёт от звука на длину очереди загрузок.
        match self
            .pending_uploads
            .iter_mut()
            .find(|(h, _)| *h == handle)
        {
            Some(slot) => slot.1 = data,
            None => self.pending_uploads.push((handle, data)),
        }
    }

    pub fn take_pending_uploads(&mut self) -> Vec<(ImageHandle, ImageData)> {
        std::mem::take(&mut self.pending_uploads)
    }

    /// До `limit` ожидающих загрузок (в порядке поступления); остальные
    /// остаются в очереди — см. [`Self::has_pending_uploads`].
    pub fn take_pending_uploads_limited(&mut self, limit: usize) -> Vec<(ImageHandle, ImageData)> {
        if self.pending_uploads.len() <= limit {
            return std::mem::take(&mut self.pending_uploads);
        }
        let n = limit.min(self.pending_uploads.len());
        self.pending_uploads.drain(..n).collect()
    }

    pub fn has_pending_uploads(&self) -> bool {
        !self.pending_uploads.is_empty()
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
        let mut became_idle = false;
        for result in results {
            match result {
                LoadResult::Success { key, handle, data } => {
                    let Some(entry) = self.images.get_mut(&key) else {
                        continue;
                    };
                    entry.state = ImageLoadState::Ready;
                    entry.width = data.width;
                    entry.height = data.height;
                    // Все держатели ушли, пока картинка грузилась.
                    if entry.refs == 0 {
                        self.idle_bytes += entry.bytes();
                        self.idle.push_back(key);
                        became_idle = true;
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
        if became_idle {
            self.evict_idle();
        }
    }

    pub fn has_loading(&self) -> bool {
        self.images
            .values()
            .any(|e| e.state == ImageLoadState::Loading)
    }

    #[cfg(feature = "image")]
    fn spawn_decode(&self, key: String, handle: ImageHandle, data: Arc<Vec<u8>>) {
        let results = self.bg_results.clone();
        std::thread::spawn(move || match decode_image_bytes(&data) {
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
        });
    }

    #[cfg(not(feature = "image"))]
    fn spawn_decode(&mut self, key: String, _handle: ImageHandle, _data: Arc<Vec<u8>>) {
        log::warn!(
            "Image decoding requires 'image' feature. Image '{}' will not load.",
            key
        );
        if let Some(entry) = self.images.get_mut(&key) {
            entry.state = ImageLoadState::Failed;
        }
    }

    #[cfg(feature = "image")]
    fn spawn_load(&self, key: String, handle: ImageHandle, path: String) {
        let results = self.bg_results.clone();
        std::thread::spawn(move || match std::fs::read(&path) {
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
        });
    }

    #[cfg(not(feature = "image"))]
    fn spawn_load(&mut self, key: String, _handle: ImageHandle, _path: String) {
        log::warn!(
            "Image loading requires 'image' feature. Image '{}' will not load.",
            key
        );
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
            let (mut w, mut h) = rgba.dimensions();
            let mut data: Arc<[u8]> = Arc::from(rgba.into_raw().into_boxed_slice());
            let limit = max_bitmap_side();
            while limit > 0 && w.max(h) > limit && w.max(h) > 1 {
                let (nw, nh, next) = crate::gpu::image_cache::downscale_half(w, h, &data);
                w = nw;
                h = nh;
                data = Arc::from(next.into_boxed_slice());
            }
            Ok(ImageData::with_mips(w, h, data))
        }
        #[cfg(feature = "svg")]
        Err(_) if looks_like_svg(bytes) => decode_svg(bytes),
        Err(e) => Err(e.to_string()),
    }
}

/// Распознаёт SVG по корневому тегу, пропуская XML-пролог, DOCTYPE и
/// комментарии перед ним.
///
/// Раньше здесь был поиск подстроки `<svg` в первых 1024 байтах, и файл с
/// длинной шапкой-комментарием (у иконки synthos она заняла 2 КБ) переставал
/// опознаваться — картинка молча превращалась в плейсхолдер ошибки. Разбор
/// пролога надёжнее любого окна: тег ищется там, где он обязан быть по
/// XML-грамматике, а не в произвольном префиксе.
#[cfg(feature = "svg")]
fn looks_like_svg(bytes: &[u8]) -> bool {
    // BOM + всё, что разрешено перед корневым элементом.
    let mut rest = bytes.strip_prefix(b"\xEF\xBB\xBF").unwrap_or(bytes);
    loop {
        rest = trim_ascii_start(rest);
        let close: &[u8] = if rest.starts_with(b"<?") {
            b"?>"
        } else if rest.starts_with(b"<!--") {
            b"-->"
        } else if rest.starts_with(b"<!") {
            // DOCTYPE без внутреннего подмножества — до первого '>'.
            b">"
        } else {
            return rest.len() >= 4 && rest[..4].eq_ignore_ascii_case(b"<svg");
        };
        match find(rest, close) {
            Some(i) => rest = &rest[i + close.len()..],
            None => return false,
        }
    }
}

#[cfg(feature = "svg")]
fn trim_ascii_start(bytes: &[u8]) -> &[u8] {
    let skip = bytes.iter().take_while(|b| b.is_ascii_whitespace()).count();
    &bytes[skip..]
}

#[cfg(feature = "svg")]
fn find(haystack: &[u8], needle: &[u8]) -> Option<usize> {
    haystack.windows(needle.len()).position(|w| w == needle)
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
    let mut pixmap =
        tiny_skia::Pixmap::new(w_px, h_px).ok_or_else(|| format!("pixmap alloc {w_px}x{h_px}"))?;
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
    Ok(ImageData::with_mips(w_px, h_px, Arc::<[u8]>::from(rgba.into_boxed_slice())))
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

    /// Бюджет — глобальный, поэтому все проверки выгрузки в одном тесте.
    #[test]
    fn idle_images_evicted_over_budget_oldest_first() {
        let mut store = ImageStore::new();
        // 8×8 RGBA с мипами = 341 байт; бюджет вмещает одну картинку.
        set_idle_image_budget(400);
        let (a, _) = store.request_rgba("a", 8, 8, solid(8, 8, 1));
        let (b, _) = store.request_rgba("b", 8, 8, solid(8, 8, 2));
        let (c, _) = store.request_rgba("c", 8, 8, solid(8, 8, 3));
        let _ = store.take_pending_uploads();

        // Держатель есть — не выгружается.
        assert!(store.take_pending_frees().is_empty());
        store.release(a);
        assert!(store.take_pending_frees().is_empty(), "одна простаивающая влезает");
        store.release(b);
        assert_eq!(store.take_pending_frees(), vec![a], "выгружается самая давняя");
        assert_eq!(store.state_of(a), None);

        // Повторный запрос простаивающей снимает её с очереди выгрузки.
        let (b2, state) = store.request_rgba("b", 8, 8, solid(8, 8, 2));
        assert_eq!((b2, state), (b, ImageLoadState::Ready));
        store.release(c);
        assert!(store.take_pending_frees().is_empty());

        // Выгруженная грузится заново под новым handle.
        let (a2, _) = store.request_rgba("a", 8, 8, solid(8, 8, 1));
        assert_ne!(a2, a);

        // Двойной release не уводит счётчик в минус и не дублирует очередь.
        store.release(b2);
        store.release(b2);
        assert_eq!(store.take_pending_frees(), vec![c]);
        set_idle_image_budget(0);
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
    fn update_rgba_keeps_only_latest_frame_per_handle() {
        let mut store = ImageStore::new();
        let (handle, _) = store.request_rgba("video", 2, 2, solid(2, 2, 0x10));
        let _ = store.take_pending_uploads();

        store.update_rgba(handle, 2, 2, solid(2, 2, 0x20));
        store.update_rgba(handle, 2, 2, solid(2, 2, 0x30));
        let uploads = store.take_pending_uploads();
        assert_eq!(uploads.len(), 1, "устаревший кадр не должен ждать в очереди");
        assert!(
            uploads[0].1.rgba.iter().all(|&b| b == 0x30),
            "залиться должен последний кадр"
        );
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
        assert!(looks_like_svg(b"\xEF\xBB\xBF<svg/>"));
        assert!(!looks_like_svg(b"<html><body/></html>"));
        assert!(!looks_like_svg(&[0x89, 0x50, 0x4E, 0x47]));
    }

    /// Шапка-комментарий длиннее любого фиксированного окна сниффинга: именно
    /// на ней ломалась иконка synthos.
    #[cfg(feature = "svg")]
    #[test]
    fn looks_like_svg_skips_long_leading_comment() {
        let mut bytes = b"<?xml version=\"1.0\" encoding=\"UTF-8\"?>\n<!--\n".to_vec();
        bytes.extend(std::iter::repeat(b'x').take(4096));
        bytes.extend_from_slice(b"\n-->\n<svg xmlns=\"http://www.w3.org/2000/svg\"/>");
        assert!(looks_like_svg(&bytes));
    }

    #[cfg(feature = "svg")]
    #[test]
    fn looks_like_svg_skips_doctype() {
        assert!(looks_like_svg(
            b"<!DOCTYPE svg PUBLIC \"-//W3C//DTD SVG 1.1//EN\" \"svg11.dtd\">\n<svg/>"
        ));
    }

    #[cfg(feature = "svg")]
    #[test]
    fn looks_like_svg_rejects_unterminated_comment() {
        assert!(!looks_like_svg(b"<!-- comment never closed <svg/>"));
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
