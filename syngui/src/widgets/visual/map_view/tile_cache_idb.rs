use crate::core::sync::Mutex;
use crate::gpu::tile_atlas::TileKey;
use std::sync::atomic::{AtomicBool, AtomicU64, Ordering};
use wasm_bindgen::{JsCast, JsValue};

use super::tile_loader::decode_to_rgba;

const STORE: &str = "tiles";
const META_KEY: &str = "__meta__";

/// IndexedDB-бэкенд кэша тайлов (wasm). PNG-байты хранятся в object store,
/// LRU-порядок и суммарный размер — в памяти, персистятся отдельной записью
/// `__meta__`. Соединение открывается на каждую операцию (idempotent,
/// дешёвое для уже существующей БД) — хэндлы `!Send` не хранятся в структуре.
pub struct IdbBackend {
    db_name: String,
    order: Mutex<Vec<(String, u64)>>,
    bytes: AtomicU64,
    loaded: AtomicBool,
}

impl IdbBackend {
    pub fn new(db_name: String) -> Self {
        Self {
            db_name,
            order: Mutex::new(Vec::new()),
            bytes: AtomicU64::new(0),
            loaded: AtomicBool::new(false),
        }
    }

    fn key_str(key: &TileKey) -> String {
        format!("{}/{}/{}/{}", key.provider_id, key.z, key.x, key.y)
    }

    async fn open(&self) -> Result<idb::Database, String> {
        use idb::{DatabaseEvent, Factory, ObjectStoreParams};
        let factory = Factory::new().map_err(|e| format!("idb factory: {:?}", e))?;
        let mut req = factory
            .open(&self.db_name, Some(1))
            .map_err(|e| format!("idb open: {:?}", e))?;
        req.on_upgrade_needed(|event| {
            if let Ok(db) = event.database() {
                let _ = db.create_object_store(STORE, ObjectStoreParams::new());
            }
        });
        req.await.map_err(|e| format!("idb open await: {:?}", e))
    }

    async fn ensure_loaded(&self) {
        if self.loaded.swap(true, Ordering::Relaxed) {
            return;
        }
        let db = match self.open().await {
            Ok(db) => db,
            Err(_) => return,
        };
        if let Some(val) = idb_get(&db, META_KEY).await {
            if let Some(s) = val.as_string() {
                let mut total = 0u64;
                let mut ord = Vec::new();
                for entry in s.split(';') {
                    if entry.is_empty() {
                        continue;
                    }
                    if let Some((k, sz)) = entry.rsplit_once(':') {
                        if let Ok(sz) = sz.parse::<u64>() {
                            total = total.saturating_add(sz);
                            ord.push((k.to_string(), sz));
                        }
                    }
                }
                self.bytes.store(total, Ordering::Relaxed);
                if let Ok(mut o) = self.order.lock() {
                    *o = ord;
                }
            }
        }
    }

    pub async fn get(&self, key: &TileKey) -> Option<Vec<u8>> {
        self.ensure_loaded().await;
        let ks = Self::key_str(key);
        let db = self.open().await.ok()?;
        let val = idb_get(&db, &ks).await?;
        let bytes = val.dyn_into::<js_sys::Uint8Array>().ok()?.to_vec();

        if let Ok(mut ord) = self.order.lock() {
            if let Some(pos) = ord.iter().position(|(k, _)| k == &ks) {
                let item = ord.remove(pos);
                ord.push(item);
            }
        }

        decode_to_rgba(&bytes).ok()
    }

    pub async fn put(
        &self,
        key: &TileKey,
        data: &[u8],
        max_tiles: usize,
        max_bytes: Option<u64>,
        current_bytes: &AtomicU64,
    ) {
        self.ensure_loaded().await;
        let ks = Self::key_str(key);
        let new_size = data.len() as u64;

        let db = match self.open().await {
            Ok(db) => db,
            Err(_) => return,
        };

        if idb_put(&db, &ks, data).await.is_err() {
            self.evict_oldest(&db, current_bytes, 16).await;
            if idb_put(&db, &ks, data).await.is_err() {
                return;
            }
        }

        let evicted: Vec<String> = {
            let mut ord = match self.order.lock() {
                Ok(o) => o,
                Err(_) => return,
            };
            if let Some(pos) = ord.iter().position(|(k, _)| k == &ks) {
                let (_, old) = ord.remove(pos);
                self.bytes.fetch_sub(old, Ordering::Relaxed);
                current_bytes.fetch_sub(old, Ordering::Relaxed);
            }
            ord.push((ks.clone(), new_size));
            self.bytes.fetch_add(new_size, Ordering::Relaxed);
            current_bytes.fetch_add(new_size, Ordering::Relaxed);

            let mut evicted = Vec::new();
            loop {
                let over_count = ord.len() > max_tiles;
                let over_bytes = max_bytes
                    .map(|m| self.bytes.load(Ordering::Relaxed) > m)
                    .unwrap_or(false);
                if !over_count && !over_bytes {
                    break;
                }
                if ord.is_empty() {
                    break;
                }
                let (k, sz) = ord.remove(0);
                self.bytes.fetch_sub(sz, Ordering::Relaxed);
                current_bytes.fetch_sub(sz, Ordering::Relaxed);
                evicted.push(k);
            }
            evicted
        };

        for k in &evicted {
            let _ = idb_delete(&db, k).await;
        }

        let meta = self.serialize_meta();
        let _ = idb_put_str(&db, META_KEY, &meta).await;
    }

    async fn evict_oldest(&self, db: &idb::Database, current_bytes: &AtomicU64, n: usize) {
        let evicted: Vec<String> = {
            let mut ord = match self.order.lock() {
                Ok(o) => o,
                Err(_) => return,
            };
            let mut ev = Vec::new();
            for _ in 0..n {
                if ord.is_empty() {
                    break;
                }
                let (k, sz) = ord.remove(0);
                self.bytes.fetch_sub(sz, Ordering::Relaxed);
                current_bytes.fetch_sub(sz, Ordering::Relaxed);
                ev.push(k);
            }
            ev
        };
        for k in &evicted {
            let _ = idb_delete(db, k).await;
        }
    }

    fn serialize_meta(&self) -> String {
        let ord = match self.order.lock() {
            Ok(o) => o,
            Err(_) => return String::new(),
        };
        let mut s = String::new();
        for (k, sz) in ord.iter() {
            s.push_str(k);
            s.push(':');
            s.push_str(&sz.to_string());
            s.push(';');
        }
        s
    }

    pub fn clear_provider(&self, provider_id: u8) {
        let prefix = format!("{}/", provider_id);
        if let Ok(mut ord) = self.order.lock() {
            ord.retain(|(k, _)| !k.starts_with(&prefix));
        }
    }
}

async fn idb_get(db: &idb::Database, key: &str) -> Option<JsValue> {
    use idb::TransactionMode;
    let tx = db.transaction(&[STORE], TransactionMode::ReadOnly).ok()?;
    let store = tx.object_store(STORE).ok()?;
    let val = store
        .get(JsValue::from_str(key))
        .ok()?
        .await
        .ok()?;
    val
}

async fn idb_put(db: &idb::Database, key: &str, data: &[u8]) -> Result<(), String> {
    use idb::TransactionMode;
    let tx = db
        .transaction(&[STORE], TransactionMode::ReadWrite)
        .map_err(|e| format!("tx: {:?}", e))?;
    let store = tx.object_store(STORE).map_err(|e| format!("store: {:?}", e))?;
    let arr = js_sys::Uint8Array::from(data);
    let key_js = JsValue::from_str(key);
    store
        .put(&arr.into(), Some(&key_js))
        .map_err(|e| format!("put: {:?}", e))?
        .await
        .map_err(|e| format!("put await: {:?}", e))?;
    tx.commit()
        .map_err(|e| format!("commit: {:?}", e))?
        .await
        .map_err(|e| format!("commit await: {:?}", e))?;
    Ok(())
}

async fn idb_put_str(db: &idb::Database, key: &str, value: &str) -> Result<(), String> {
    use idb::TransactionMode;
    let tx = db
        .transaction(&[STORE], TransactionMode::ReadWrite)
        .map_err(|e| format!("tx: {:?}", e))?;
    let store = tx.object_store(STORE).map_err(|e| format!("store: {:?}", e))?;
    let key_js = JsValue::from_str(key);
    store
        .put(&JsValue::from_str(value), Some(&key_js))
        .map_err(|e| format!("put: {:?}", e))?
        .await
        .map_err(|e| format!("put await: {:?}", e))?;
    tx.commit()
        .map_err(|e| format!("commit: {:?}", e))?
        .await
        .map_err(|e| format!("commit await: {:?}", e))?;
    Ok(())
}

async fn idb_delete(db: &idb::Database, key: &str) -> Result<(), String> {
    use idb::TransactionMode;
    let tx = db
        .transaction(&[STORE], TransactionMode::ReadWrite)
        .map_err(|e| format!("tx: {:?}", e))?;
    let store = tx.object_store(STORE).map_err(|e| format!("store: {:?}", e))?;
    store
        .delete(JsValue::from_str(key))
        .map_err(|e| format!("delete: {:?}", e))?
        .await
        .map_err(|e| format!("delete await: {:?}", e))?;
    tx.commit()
        .map_err(|e| format!("commit: {:?}", e))?
        .await
        .map_err(|e| format!("commit await: {:?}", e))?;
    Ok(())
}
