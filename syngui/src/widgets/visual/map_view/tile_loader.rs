use crate::gpu::tile_atlas::TileKey;
use super::tile_cache::TileCache;
use hashbrown::HashMap;
use std::sync::Arc;
use std::sync::atomic::Ordering::Relaxed;
use crate::core::sync::Mutex;

#[derive(Clone, Debug)]
pub enum TileState {
    Loading,
    Loaded(Vec<u8>),
    Failed,
}

struct TileEntry {
    state: TileState,
}

pub struct TileLoader {
    cache: Arc<Mutex<HashMap<TileKey, TileEntry>>>,
    pending_count: Arc<std::sync::atomic::AtomicUsize>,
    deferred: Arc<std::sync::atomic::AtomicBool>,
    tile_cache: Option<Arc<TileCache>>,
}

impl TileLoader {
    pub fn new() -> Self {
        Self {
            cache: Arc::new(Mutex::new(HashMap::new())),
            pending_count: Arc::new(std::sync::atomic::AtomicUsize::new(0)),
            deferred: Arc::new(std::sync::atomic::AtomicBool::new(false)),
            tile_cache: None,
        }
    }

    pub fn with_cache(tile_cache: Arc<TileCache>) -> Self {
        Self {
            cache: Arc::new(Mutex::new(HashMap::new())),
            pending_count: Arc::new(std::sync::atomic::AtomicUsize::new(0)),
            deferred: Arc::new(std::sync::atomic::AtomicBool::new(false)),
            tile_cache: Some(tile_cache),
        }
    }

    pub fn request_tile(&self, key: TileKey, url: String) -> TileState {
        let mut cache = self.cache.lock().unwrap();

        if let Some(entry) = cache.get(&key) {
            return entry.state.clone();
        }

        if self.pending_count.load(Relaxed) >= 8 {
            self.deferred.store(true, Relaxed);
            return TileState::Loading;
        }

        cache.insert(key, TileEntry { state: TileState::Loading });

        #[cfg(any(target_arch = "wasm32", feature = "map-native"))]
        {
            self.pending_count.fetch_add(1, Relaxed);
            drop(cache);

            let cache_ref = Arc::clone(&self.cache);
            let pending_ref = Arc::clone(&self.pending_count);
            let tile_cache = self.tile_cache.clone();

            spawn_tile_task(async move {
                if let Some(ref tc) = tile_cache {
                    if let Some(rgba) = tc.get(&key).await {
                        if let Ok(mut cache) = cache_ref.lock() {
                            cache.insert(key, TileEntry { state: TileState::Loaded(rgba) });
                        }
                        pending_ref.fetch_sub(1, Relaxed);
                        return;
                    }
                }

                let state = match fetch_png(&url).await {
                    Ok(png) => match decode_to_rgba(&png) {
                        Ok(rgba) => {
                            if let Some(ref tc) = tile_cache {
                                tc.put(&key, &png).await;
                            }
                            TileState::Loaded(rgba)
                        }
                        Err(e) => {
                            log::warn!("Tile decode failed for {:?}: {}", key, e);
                            TileState::Failed
                        }
                    },
                    Err(e) => {
                        log::warn!("Tile fetch failed for {:?}: {}", key, e);
                        TileState::Failed
                    }
                };

                if let Ok(mut cache) = cache_ref.lock() {
                    cache.insert(key, TileEntry { state });
                }
                pending_ref.fetch_sub(1, Relaxed);
            });
        }

        #[cfg(not(any(target_arch = "wasm32", feature = "map-native")))]
        {
            let _ = url;
            cache.insert(key, TileEntry { state: TileState::Failed });
        }

        TileState::Loading
    }

    pub fn get_tile(&self, key: &TileKey) -> Option<TileState> {
        let cache = self.cache.lock().unwrap();
        cache.get(key).map(|e| e.state.clone())
    }

    pub fn has_pending(&self) -> bool {
        self.pending_count.load(Relaxed) > 0
    }

    pub fn take_deferred(&self) -> bool {
        self.deferred.swap(false, Relaxed)
    }

    pub fn clear_provider(&self, provider_id: u8) {
        let mut cache = self.cache.lock().unwrap();
        cache.retain(|k, _| k.provider_id != provider_id);
    }
}

#[cfg(all(not(target_arch = "wasm32"), feature = "map-native"))]
fn spawn_tile_task<F>(fut: F)
where
    F: std::future::Future<Output = ()> + Send + 'static,
{
    crate::async_runtime::spawn(fut);
}

#[cfg(target_arch = "wasm32")]
fn spawn_tile_task<F>(fut: F)
where
    F: std::future::Future<Output = ()> + 'static,
{
    wasm_bindgen_futures::spawn_local(fut);
}

#[cfg(all(not(target_arch = "wasm32"), feature = "map-native"))]
async fn fetch_png(url: &str) -> Result<Vec<u8>, String> {
    let url = url.to_string();
    tokio::task::spawn_blocking(move || ureq_get_bytes(&url))
        .await
        .map_err(|e| format!("join error: {}", e))?
}

#[cfg(all(not(target_arch = "wasm32"), feature = "map-native"))]
fn ureq_get_bytes(url: &str) -> Result<Vec<u8>, String> {
    let response = ureq::get(url)
        .call()
        .map_err(|e| format!("HTTP error: {}", e))?;

    response
        .into_body()
        .read_to_vec()
        .map_err(|e| format!("Read error: {}", e))
}

#[cfg(target_arch = "wasm32")]
async fn fetch_png(url: &str) -> Result<Vec<u8>, String> {
    crate::app::input_mapping::fetch_bytes(url).await
}

pub(super) fn decode_to_rgba(bytes: &[u8]) -> Result<Vec<u8>, String> {
    let img = image::load_from_memory(bytes)
        .map_err(|e| format!("Decode error: {}", e))?;

    let rgba = img.to_rgba8();

    if rgba.width() != 256 || rgba.height() != 256 {
        let resized = image::imageops::resize(&rgba, 256, 256, image::imageops::FilterType::Lanczos3);
        Ok(resized.into_raw())
    } else {
        Ok(rgba.into_raw())
    }
}
