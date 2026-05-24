pub mod cache;

use crate::config::Config;
use crate::error::NxrResult;
use crate::wal::{Wal, WalLayer, WalOperation};
use cache::KvCache as CacheImpl;
use std::sync::Arc;

pub struct KvCache {
    inner: CacheImpl,
    wal: Option<Arc<Wal>>,
}

impl KvCache {
    pub fn new(config: &Config) -> NxrResult<Self> {
        let inner = CacheImpl::new(config)?;
        Ok(Self { inner, wal: None })
    }

    pub fn with_wal(mut self, wal: Arc<Wal>) -> Self {
        self.inner.set_wal(Some(wal.clone()));
        self.wal = Some(wal);
        self
    }

    pub fn set(&mut self, key: &str, value: &[u8], ttl: u32) -> NxrResult<()> {
        if key.len() > 512 {
            return Err(crate::error::NxrError::Kv("Key max 512 bytes".into()));
        }
        if value.len() > 10 * 1024 * 1024 {
            return Err(crate::error::NxrError::Kv("Value max 10 MB".into()));
        }

        if let Some(ref wal) = self.wal {
            let mut payload = Vec::with_capacity(key.len() + value.len() + 8);
            payload.extend_from_slice(&(key.len() as u32).to_le_bytes());
            payload.extend_from_slice(key.as_bytes());
            payload.extend_from_slice(&(value.len() as u32).to_le_bytes());
            payload.extend_from_slice(value);
            payload.extend_from_slice(&ttl.to_le_bytes());
            wal.append(WalOperation::Insert, WalLayer::Kv, &payload)?;
        }

        self.inner.set(key, value, ttl)
    }

    pub fn get(&self, key: &str) -> NxrResult<Option<Vec<u8>>> {
        self.inner.get(key)
    }

    pub fn delete(&mut self, key: &str) -> NxrResult<()> {
        if let Some(ref wal) = self.wal {
            wal.append(WalOperation::Delete, WalLayer::Kv, key.as_bytes())?;
        }
        self.inner.delete(key)
    }

    pub fn len(&self) -> usize {
        self.inner.len()
    }
}
