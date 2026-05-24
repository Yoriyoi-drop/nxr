use crate::config::Config;
use crate::error::{NxrError, NxrResult};
use crate::wal::Wal;
use std::collections::{HashMap, VecDeque};
use std::fs;
use std::sync::Arc;
use std::time::{SystemTime, UNIX_EPOCH};

#[derive(Debug, Clone)]
struct KvEntry {
    value: Vec<u8>,
    ttl: u32,
    created: u64,
    flags: u8,
    access_count: u64,
    last_access: u64,
}

pub struct KvCache {
    hot: HashMap<String, KvEntry>,
    warm: HashMap<String, KvEntry>,
    cold_dir: String,
    max_hot: usize,
    max_warm: usize,
    access_log: VecDeque<String>,
    wal: Option<Arc<Wal>>,
}

impl KvCache {
    pub fn new(config: &Config) -> NxrResult<Self> {
        fs::create_dir_all(&config.kv.cold_path)?;
        let max_hot = (config.kv.hot_zone_mb as usize) * 1024 * 1024 / 4096;
        let max_warm = (config.kv.warm_zone_mb as usize) * 1024 * 1024 / 4096;
        Ok(Self {
            hot: HashMap::new(),
            warm: HashMap::new(),
            cold_dir: config.kv.cold_path.to_string_lossy().to_string(),
            max_hot,
            max_warm,
            access_log: VecDeque::new(),
            wal: None,
        })
    }

    pub fn set_wal(&mut self, wal: Option<Arc<Wal>>) {
        self.wal = wal;
    }

    fn now() -> u64 {
        SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .unwrap()
            .as_secs()
    }

    fn is_expired(entry: &KvEntry) -> bool {
        if entry.ttl == 0 {
            return false;
        }
        Self::now() - entry.created >= entry.ttl as u64
    }

    pub fn set(&mut self, key: &str, value: &[u8], ttl: u32) -> NxrResult<()> {
        let entry = KvEntry {
            value: value.to_vec(),
            ttl,
            created: Self::now(),
            flags: 0,
            access_count: 0,
            last_access: Self::now(),
        };

        // Try hot zone first
        if self.hot.len() < self.max_hot {
            self.hot.insert(key.to_string(), entry);
            self.access_log.push_back(key.to_string());
            return Ok(());
        }

        // Evict from warm if needed, then promote to hot
        if self.warm.len() >= self.max_warm {
            self.evict_warm()?;
        }

        self.warm.insert(key.to_string(), entry);
        Ok(())
    }

    pub fn get(&self, key: &str) -> NxrResult<Option<Vec<u8>>> {
        // Check hot
        if let Some(entry) = self.hot.get(key) {
            if Self::is_expired(entry) {
                return Ok(None);
            }
            return Ok(Some(entry.value.clone()));
        }

        // Check warm
        if let Some(entry) = self.warm.get(key) {
            if Self::is_expired(entry) {
                return Ok(None);
            }
            return Ok(Some(entry.value.clone()));
        }

        // Check cold (disk)
        self.get_cold(key)
    }

    fn get_cold(&self, key: &str) -> NxrResult<Option<Vec<u8>>> {
        let path = format!("{}/{}", self.cold_dir, Self::sanitize_key(key));
        match fs::read(&path) {
            Ok(data) => Ok(Some(data)),
            Err(e) if e.kind() == std::io::ErrorKind::NotFound => Ok(None),
            Err(e) => Err(NxrError::Kv(format!("Cold read error: {}", e))),
        }
    }

    pub fn delete(&mut self, key: &str) -> NxrResult<()> {
        self.hot.remove(key);
        self.warm.remove(key);
        let path = format!("{}/{}", self.cold_dir, Self::sanitize_key(key));
        let _ = fs::remove_file(&path);
        Ok(())
    }

    fn evict_warm(&mut self) -> NxrResult<()> {
        // Find least recently used in warm
        let lru_key = self.warm.iter()
            .min_by_key(|(_, e)| e.last_access)
            .map(|(k, _)| k.clone());

        if let Some(key) = lru_key {
            if let Some(entry) = self.warm.remove(&key) {
                // Move to cold (disk)
                let path = format!("{}/{}", self.cold_dir, Self::sanitize_key(&key));
                fs::write(&path, &entry.value)?;
            }
        }

        Ok(())
    }

    fn sanitize_key(key: &str) -> String {
        key.replace('/', "_").replace('\\', "_").replace('\0', "")
    }

    pub fn len(&self) -> usize {
        self.hot.len() + self.warm.len()
    }

    pub fn hot_keys(&self) -> Vec<String> {
        self.hot.keys().cloned().collect()
    }

    pub fn warm_keys(&self) -> Vec<String> {
        self.warm.keys().cloned().collect()
    }

    pub fn promote_to_hot(&mut self, key: &str) -> NxrResult<()> {
        if let Some(entry) = self.warm.remove(key) {
            if self.hot.len() >= self.max_hot {
                let lru_key = self.hot.iter()
                    .min_by_key(|(_, e)| e.last_access)
                    .map(|(k, _)| k.clone());
                if let Some(k) = lru_key {
                    if let Some(e) = self.hot.remove(&k) {
                        self.warm.insert(k, e);
                    }
                }
            }
            self.hot.insert(key.to_string(), entry);
        }
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::config::Config;

    fn test_cache(name: &str) -> KvCache {
        let tmp = std::env::temp_dir().join(format!("nxr_kv_{}", name));
        let _ = std::fs::remove_dir_all(&tmp);
        let mut config = Config::default();
        config.kv.hot_zone_mb = 1;
        config.kv.warm_zone_mb = 2;
        config.kv.cold_path = tmp.join("cold");
        KvCache::new(&config).unwrap()
    }

    #[test]
    fn test_set_and_get() {
        let mut cache = test_cache("set_get");
        cache.set("key1", b"value1", 0).unwrap();
        let val = cache.get("key1").unwrap().unwrap();
        assert_eq!(val, b"value1");
    }

    #[test]
    fn test_get_missing() {
        let cache = test_cache("missing");
        let val = cache.get("nonexistent").unwrap();
        assert!(val.is_none());
    }

    #[test]
    fn test_delete() {
        let mut cache = test_cache("delete");
        cache.set("key1", b"value1", 0).unwrap();
        cache.delete("key1").unwrap();
        let val = cache.get("key1").unwrap();
        assert!(val.is_none());
    }

    #[test]
    fn test_ttl_expiry() {
        let mut cache = test_cache("ttl");
        cache.set("key1", b"value1", 1).unwrap();
        std::thread::sleep(std::time::Duration::from_millis(1100));
        let val = cache.get("key1").unwrap();
        assert!(val.is_none());
    }

    #[test]
    fn test_key_long() {
        let mut cache = test_cache("long_key");
        let long_key = "x".repeat(600);
        let result = cache.set(&long_key, b"val", 0);
        assert!(result.is_ok());
    }

    #[test]
    fn test_hot_warm_promotion() {
        let mut cache = test_cache("promotion");
        cache.set("k1", b"v1", 0).unwrap();
        assert!(cache.hot.contains_key("k1"));
        cache.promote_to_hot("nonexistent").unwrap();
    }
}
