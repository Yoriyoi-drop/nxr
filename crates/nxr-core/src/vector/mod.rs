pub mod hnsw;
pub mod segment;

use crate::config::Config;
use crate::error::{NxrError, NxrResult};
use crate::wal::{Wal, WalLayer, WalOperation};
use std::path::Path;
use std::sync::Arc;

pub struct VectorEngine {
    pub hnsw: hnsw::HnswIndex,
    segments: segment::SegmentManager,
    wal: Option<Arc<Wal>>,
    dimension: u32,
    hnsw_path: String,
}

impl VectorEngine {
    pub fn new(config: &Config) -> NxrResult<Self> {
        let hnsw_path = config.vector.hnsw_path.to_string_lossy().to_string();
        let hnsw = if Path::new(&hnsw_path).exists() {
            hnsw::HnswIndex::load(&hnsw_path)?
        } else {
            hnsw::HnswIndex::new(
                config.vector.dimension as usize,
                config.vector.ef_construction as usize,
                config.vector.m_max as usize,
                &config.vector.space,
            )
        };
        let segments = segment::SegmentManager::new(&config.vector.segments_path)?;
        Ok(Self { hnsw, segments, wal: None, dimension: config.vector.dimension, hnsw_path })
    }

    pub fn with_wal(mut self, wal: Arc<Wal>) -> Self {
        self.wal = Some(wal);
        self
    }

    fn save_hnsw(&self) -> NxrResult<()> {
        self.hnsw.save(&self.hnsw_path)
    }

    pub fn insert(&mut self, id: u64, vector: &[f32], metadata: &[u8]) -> NxrResult<()> {
        if vector.len() != self.dimension as usize {
            return Err(NxrError::Vector(format!(
                "Expected {} dimensions, got {}",
                self.dimension,
                vector.len()
            )));
        }
        if let Some(ref wal) = self.wal {
            let payload = Self::encode_payload(id, vector, metadata);
            wal.append(WalOperation::Insert, WalLayer::Vector, &payload)?;
        }
        self.hnsw.insert(id, vector)?;
        self.segments.write_segment(id, vector, metadata)?;
        self.save_hnsw()?;
        Ok(())
    }

    pub fn search(&self, query: &[f32], k: usize) -> NxrResult<Vec<(u64, f32)>> {
        if query.len() != self.dimension as usize {
            return Err(NxrError::Vector(format!(
                "Expected {} dimensions, got {}",
                self.dimension,
                query.len()
            )));
        }
        self.hnsw.search(query, k)
    }

    pub fn delete(&mut self, id: u64) -> NxrResult<()> {
        if let Some(ref wal) = self.wal {
            let payload = id.to_le_bytes().to_vec();
            wal.append(WalOperation::Delete, WalLayer::Vector, &payload)?;
        }
        self.hnsw.delete(id)?;
        self.segments.delete_segment(id)?;
        self.save_hnsw()?;
        Ok(())
    }

    fn encode_payload(id: u64, vector: &[f32], metadata: &[u8]) -> Vec<u8> {
        let vec_bytes: Vec<u8> = vector
            .iter()
            .flat_map(|f| f.to_le_bytes())
            .collect();
        let mut buf = Vec::with_capacity(8 + 4 + vec_bytes.len() + 4 + metadata.len());
        buf.extend_from_slice(&id.to_le_bytes());
        buf.extend_from_slice(&(vector.len() as u32).to_le_bytes());
        buf.extend_from_slice(&vec_bytes);
        buf.extend_from_slice(&(metadata.len() as u32).to_le_bytes());
        buf.extend_from_slice(metadata);
        buf
    }

    pub fn dimension(&self) -> u32 {
        self.dimension
    }

    pub fn len(&self) -> usize {
        self.hnsw.len()
    }
}
