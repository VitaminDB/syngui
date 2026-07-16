use crate::gpu::tile_atlas::TileKey;
use hashbrown::HashMap;
use std::sync::Arc;
use std::sync::atomic::{AtomicU64, Ordering};
use crate::core::sync::Mutex;

use super::tile_loader::decode_to_rgba;

#[cfg(feature = "map-native")]
use std::path::PathBuf;
#[cfg(feature = "map-native")]
use std::sync::atomic::AtomicBool;

#[cfg(feature = "map-native")]
const TILE_KEY_SIZE: usize = 10;

pub struct TileCache {
    backend: CacheBackend,
    max_tiles: usize,
    max_bytes: Option<u64>,
    current_bytes: Arc<AtomicU64>,
}

enum CacheBackend {
    #[cfg(feature = "map-native")]
    Disk(Arc<DiskInner>),
    Memory(MemoryInner),
    #[cfg(all(target_arch = "wasm32", feature = "map"))]
    IndexedDb(super::tile_cache_idb::IdbBackend),
}

#[cfg(feature = "map-native")]
struct DiskInner {
    path: PathBuf,
    index: Mutex<Vec<TileKey>>,
    sizes: Mutex<HashMap<TileKey, u64>>,
    dirty: AtomicBool,
}

struct MemoryInner {
    tiles: Mutex<HashMap<TileKey, Vec<u8>>>,
    order: Mutex<Vec<TileKey>>,
}

impl TileCache {
    #[cfg(feature = "map-native")]
    pub fn new(path: impl Into<PathBuf>) -> Self {
        let path = path.into();
        let index = Self::load_index_from_disk(&path);
        let mut sizes: HashMap<TileKey, u64> = HashMap::with_capacity(index.len());
        let mut total: u64 = 0;
        for key in &index {
            let p = Self::tile_path_for(&path, key);
            if let Ok(md) = std::fs::metadata(&p) {
                let sz = md.len();
                sizes.insert(*key, sz);
                total = total.saturating_add(sz);
            }
        }
        Self {
            backend: CacheBackend::Disk(Arc::new(DiskInner {
                path,
                index: Mutex::new(index),
                sizes: Mutex::new(sizes),
                dirty: AtomicBool::new(false),
            })),
            max_tiles: 1000,
            max_bytes: None,
            current_bytes: Arc::new(AtomicU64::new(total)),
        }
    }

    pub fn memory(max_tiles: usize) -> Self {
        Self {
            backend: CacheBackend::Memory(MemoryInner {
                tiles: Mutex::new(HashMap::new()),
                order: Mutex::new(Vec::new()),
            }),
            max_tiles,
            max_bytes: None,
            current_bytes: Arc::new(AtomicU64::new(0)),
        }
    }

    #[cfg(all(target_arch = "wasm32", feature = "map"))]
    pub fn indexed_db(db_name: impl Into<String>) -> Self {
        Self {
            backend: CacheBackend::IndexedDb(super::tile_cache_idb::IdbBackend::new(db_name.into())),
            max_tiles: 1000,
            max_bytes: None,
            current_bytes: Arc::new(AtomicU64::new(0)),
        }
    }

    pub fn max_tiles(mut self, n: usize) -> Self {
        self.max_tiles = n;
        self
    }

    pub fn max_bytes(mut self, bytes: u64) -> Self {
        self.max_bytes = Some(bytes);
        self
    }

    pub fn current_bytes(&self) -> u64 {
        self.current_bytes.load(Ordering::Relaxed)
    }

    pub async fn get(&self, key: &TileKey) -> Option<Vec<u8>> {
        match &self.backend {
            #[cfg(feature = "map-native")]
            CacheBackend::Disk(inner) => {
                let inner = Arc::clone(inner);
                let key = *key;
                tokio::task::spawn_blocking(move || disk_get(&inner, &key))
                    .await
                    .ok()
                    .flatten()
            }
            CacheBackend::Memory(mem) => {
                let data = {
                    let tiles = mem.tiles.lock().ok()?;
                    tiles.get(key)?.clone()
                };
                if let Ok(mut ord) = mem.order.lock() {
                    if let Some(pos) = ord.iter().position(|k| k == key) {
                        let k = ord.remove(pos);
                        ord.push(k);
                    }
                }
                decode_to_rgba(&data).ok()
            }
            #[cfg(all(target_arch = "wasm32", feature = "map"))]
            CacheBackend::IndexedDb(idb) => idb.get(key).await,
        }
    }

    pub async fn put(&self, key: &TileKey, data: &[u8]) {
        match &self.backend {
            #[cfg(feature = "map-native")]
            CacheBackend::Disk(inner) => {
                let inner = Arc::clone(inner);
                let current_bytes = Arc::clone(&self.current_bytes);
                let max_tiles = self.max_tiles;
                let max_bytes = self.max_bytes;
                let key = *key;
                let data = data.to_vec();
                let _ = tokio::task::spawn_blocking(move || {
                    disk_put(&inner, &current_bytes, max_tiles, max_bytes, &key, &data)
                })
                .await;
            }
            CacheBackend::Memory(mem) => {
                let new_size = data.len() as u64;
                if let (Ok(mut t), Ok(mut ord)) = (mem.tiles.lock(), mem.order.lock()) {
                    if let Some(old) = t.insert(*key, data.to_vec()) {
                        self.current_bytes.fetch_sub(old.len() as u64, Ordering::Relaxed);
                    }
                    self.current_bytes.fetch_add(new_size, Ordering::Relaxed);
                    ord.retain(|k| k != key);
                    ord.push(*key);

                    loop {
                        let over_count = ord.len() > self.max_tiles;
                        let over_bytes = self
                            .max_bytes
                            .map(|m| self.current_bytes.load(Ordering::Relaxed) > m)
                            .unwrap_or(false);
                        if !over_count && !over_bytes {
                            break;
                        }
                        if ord.is_empty() {
                            break;
                        }
                        let evicted = ord.remove(0);
                        if let Some(old) = t.remove(&evicted) {
                            self.current_bytes.fetch_sub(old.len() as u64, Ordering::Relaxed);
                        }
                    }
                }
            }
            #[cfg(all(target_arch = "wasm32", feature = "map"))]
            CacheBackend::IndexedDb(idb) => {
                idb.put(key, data, self.max_tiles, self.max_bytes, &self.current_bytes)
                    .await;
            }
        }
    }

    pub fn flush(&self) {
        #[cfg(feature = "map-native")]
        if let CacheBackend::Disk(inner) = &self.backend {
            if inner.dirty.load(Ordering::Relaxed) {
                if let Ok(idx) = inner.index.lock() {
                    Self::save_index_to_disk(&inner.path, &idx);
                    inner.dirty.store(false, Ordering::Relaxed);
                }
            }
        }
    }

    pub fn clear_provider(&self, provider_id: u8) {
        match &self.backend {
            #[cfg(feature = "map-native")]
            CacheBackend::Disk(inner) => {
                if let (Ok(mut idx), Ok(mut szs)) = (inner.index.lock(), inner.sizes.lock()) {
                    let evicted: Vec<TileKey> = idx
                        .iter()
                        .filter(|k| k.provider_id == provider_id)
                        .copied()
                        .collect();

                    for key in &evicted {
                        let tile_path = Self::tile_path_for(&inner.path, key);
                        let _ = std::fs::remove_file(&tile_path);
                        if let Some(sz) = szs.remove(key) {
                            self.current_bytes.fetch_sub(sz, Ordering::Relaxed);
                        }
                    }

                    idx.retain(|k| k.provider_id != provider_id);
                    inner.dirty.store(true, Ordering::Relaxed);
                }
            }
            CacheBackend::Memory(mem) => {
                if let (Ok(mut t), Ok(mut ord)) = (mem.tiles.lock(), mem.order.lock()) {
                    ord.retain(|k| {
                        if k.provider_id == provider_id {
                            if let Some(v) = t.remove(k) {
                                self.current_bytes.fetch_sub(v.len() as u64, Ordering::Relaxed);
                            }
                            false
                        } else {
                            true
                        }
                    });
                }
            }
            #[cfg(all(target_arch = "wasm32", feature = "map"))]
            CacheBackend::IndexedDb(idb) => {
                idb.clear_provider(provider_id);
            }
        }
    }

    #[cfg(feature = "map-native")]
    fn tile_path_for(base: &PathBuf, key: &TileKey) -> PathBuf {
        base.join(key.provider_id.to_string())
            .join(key.z.to_string())
            .join(format!("{}_{}.png", key.x, key.y))
    }

    #[cfg(feature = "map-native")]
    fn load_index_from_disk(path: &PathBuf) -> Vec<TileKey> {
        let index_path = path.join("index.bin");
        let bytes = match std::fs::read(&index_path) {
            Ok(b) => b,
            Err(_) => return Vec::new(),
        };

        if bytes.len() % TILE_KEY_SIZE != 0 {
            return Vec::new();
        }

        let mut index = Vec::with_capacity(bytes.len() / TILE_KEY_SIZE);
        for chunk in bytes.chunks_exact(TILE_KEY_SIZE) {
            let x = u32::from_le_bytes([chunk[0], chunk[1], chunk[2], chunk[3]]);
            let y = u32::from_le_bytes([chunk[4], chunk[5], chunk[6], chunk[7]]);
            let z = chunk[8];
            let provider_id = chunk[9];
            index.push(TileKey { x, y, z, provider_id });
        }
        index
    }

    #[cfg(feature = "map-native")]
    fn save_index_to_disk(path: &PathBuf, index: &[TileKey]) {
        let _ = std::fs::create_dir_all(path);
        let index_path = path.join("index.bin");
        let mut bytes = Vec::with_capacity(index.len() * TILE_KEY_SIZE);
        for key in index {
            bytes.extend_from_slice(&key.x.to_le_bytes());
            bytes.extend_from_slice(&key.y.to_le_bytes());
            bytes.push(key.z);
            bytes.push(key.provider_id);
        }
        let _ = std::fs::write(&index_path, &bytes);
    }
}

#[cfg(feature = "map-native")]
fn disk_get(inner: &DiskInner, key: &TileKey) -> Option<Vec<u8>> {
    let tile_path = TileCache::tile_path_for(&inner.path, key);
    let png_bytes = std::fs::read(&tile_path).ok()?;
    let rgba = decode_to_rgba(&png_bytes).ok()?;

    if let Ok(mut idx) = inner.index.lock() {
        if let Some(pos) = idx.iter().position(|k| k == key) {
            let k = idx.remove(pos);
            idx.push(k);
            inner.dirty.store(true, Ordering::Relaxed);
        }
    }

    Some(rgba)
}

#[cfg(feature = "map-native")]
fn disk_put(
    inner: &DiskInner,
    current_bytes: &AtomicU64,
    max_tiles: usize,
    max_bytes: Option<u64>,
    key: &TileKey,
    data: &[u8],
) {
    let new_size = data.len() as u64;
    let tile_path = TileCache::tile_path_for(&inner.path, key);

    if let Some(parent) = tile_path.parent() {
        let _ = std::fs::create_dir_all(parent);
    }

    if std::fs::write(&tile_path, data).is_err() {
        return;
    }

    if let (Ok(mut idx), Ok(mut szs)) = (inner.index.lock(), inner.sizes.lock()) {
        if let Some(pos) = idx.iter().position(|k| k == key) {
            idx.remove(pos);
            if let Some(old_sz) = szs.remove(key) {
                current_bytes.fetch_sub(old_sz, Ordering::Relaxed);
            }
        }
        idx.push(*key);
        szs.insert(*key, new_size);
        current_bytes.fetch_add(new_size, Ordering::Relaxed);

        loop {
            let over_count = idx.len() > max_tiles;
            let over_bytes = max_bytes
                .map(|m| current_bytes.load(Ordering::Relaxed) > m)
                .unwrap_or(false);
            if !over_count && !over_bytes {
                break;
            }
            if idx.is_empty() {
                break;
            }
            let evicted = idx.remove(0);
            let evict_path = TileCache::tile_path_for(&inner.path, &evicted);
            let _ = std::fs::remove_file(&evict_path);
            if let Some(sz) = szs.remove(&evicted) {
                current_bytes.fetch_sub(sz, Ordering::Relaxed);
            }
        }

        inner.dirty.store(true, Ordering::Relaxed);
    }
}

impl Drop for TileCache {
    fn drop(&mut self) {
        self.flush();
    }
}

// Safety: all mutable state behind Mutex/Atomic; на wasm — single-thread модель
// (idb-хэндлы создаются локально в async, в структуре не хранятся).
unsafe impl Send for TileCache {}
unsafe impl Sync for TileCache {}
