use crate::config::Config;
use crate::error::{NxrError, NxrResult};
use crate::index::IndexManager;
use crate::wal::{Wal, WalEntry, WalLayer, WalOperation};
use std::sync::Arc;

#[derive(Clone)]
pub struct GarbageCollector {
    config: Config,
    wal: Option<Arc<Wal>>,
}

impl GarbageCollector {
    pub fn new(config: &Config) -> Self {
        Self { config: config.clone(), wal: None }
    }

    pub fn with_wal(mut self, wal: Arc<Wal>) -> Self {
        self.wal = Some(wal);
        self
    }

    pub fn run(&self, index: Option<&mut IndexManager>) -> NxrResult<()> {
        log::info!("GC: Starting garbage collection cycle");

        self.cleanup_wal()?;
        self.merge_wal()?;
        self.compress_cold_vectors()?;
        self.cleanup_expired_kv()?;

        if let Some(idx) = index {
            let frag = idx.fragmentation();
            if frag > self.config.pipeline.index_fragmentation_threshold {
                log::info!("GC: Fragmentation {:.2} > threshold, rebuilding index", frag);
                idx.rebuild()?;
            }
        }

        log::info!("GC: Cycle complete");
        Ok(())
    }

    fn cleanup_wal(&self) -> NxrResult<()> {
        log::info!("GC: Cleaning up old WAL segments");
        let wal_dir = &self.config.wal_dir;
        if !wal_dir.exists() {
            return Ok(());
        }

        let mut entries: Vec<(u32, std::path::PathBuf)> = Vec::new();
        for entry in std::fs::read_dir(wal_dir)? {
            let entry = entry?;
            let name = entry.file_name().into_string().unwrap_or_default();
            if name.starts_with("wal_") && name.ends_with(".log") {
                let num: u32 = name[4..name.len() - 4].parse().unwrap_or(0);
                entries.push((num, entry.path()));
            }
        }

        entries.sort_by_key(|e| e.0);
        // Keep last 3 segments
        let to_remove = entries.len().saturating_sub(3);
        for (_, path) in entries.iter().take(to_remove) {
            log::info!("GC: Removing WAL segment {:?}", path);
            let _ = std::fs::remove_file(path);
        }

        Ok(())
    }

    /// Merge WAL: replay all operations into a single compact segment
    fn merge_wal(&self) -> NxrResult<()> {
        log::info!("GC: Merging WAL segments");
        let wal_dir = &self.config.wal_dir;
        if !wal_dir.exists() {
            return Ok(());
        }

        let mut entries: Vec<(u32, std::path::PathBuf)> = Vec::new();
        for entry in std::fs::read_dir(wal_dir)? {
            let entry = entry?;
            let name = entry.file_name().into_string().unwrap_or_default();
            if name.starts_with("wal_") && name.ends_with(".log") {
                let num: u32 = name[4..name.len() - 4].parse().unwrap_or(0);
                entries.push((num, entry.path()));
            }
        }
        entries.sort_by_key(|e| e.0);

        // Only merge if more than 2 segments exist
        if entries.len() <= 2 {
            return Ok(());
        }

        // Read all ops from all segments using proper WalEntry::decode
        let mut all_entries: Vec<WalEntry> = Vec::new();
        for (_, path) in &entries {
            let bytes = std::fs::read(path)
                .map_err(|e| NxrError::Io(e))?;
            let mut offset = 0;
            while offset + 22 <= bytes.len() {
                // Peek payload_len to know full entry size
                let payload_len = u32::from_le_bytes(
                    bytes[offset + 14..offset + 18].try_into().unwrap_or([0; 4]),
                ) as usize;
                let entry_size = 22 + payload_len;
                if offset + entry_size > bytes.len() {
                    break;
                }
                if let Ok(entry) = WalEntry::decode(&bytes[offset..offset + entry_size]) {
                    all_entries.push(entry);
                }
                offset += entry_size;
            }
        }

        if all_entries.is_empty() {
            return Ok(());
        }

        // Deduplicate: for each (layer, key), keep only the last operation
        // Vector key = first 8 bytes of payload (id)
        // Graph key = first 8 bytes of payload (node_id or edge_id)
        // Kv key = payload as-is
        let mut seen: std::collections::HashMap<(WalLayer, Vec<u8>), usize> = std::collections::HashMap::new();
        let mut deduped: Vec<WalEntry> = Vec::new();

        for entry in all_entries {
            let key = match entry.layer {
                WalLayer::Vector | WalLayer::Graph => {
                    if entry.payload.len() >= 8 {
                        entry.payload[..8].to_vec()
                    } else {
                        entry.payload.clone()
                    }
                }
                WalLayer::Kv => entry.payload.clone(),
            };

            // If deletion, remove any prior entry for same key and skip the delete itself
            if entry.operation == WalOperation::Delete {
                seen.retain(|k, _| *k != (entry.layer, key.clone()));
                deduped.retain(|e| {
                    let e_key = match e.layer {
                        WalLayer::Vector | WalLayer::Graph => {
                            if e.payload.len() >= 8 { e.payload[..8].to_vec() } else { e.payload.clone() }
                        }
                        WalLayer::Kv => e.payload.clone(),
                    };
                    !(e.layer == entry.layer && e_key == key)
                });
                continue;
            }

            let map_key = (entry.layer, key);
            if let Some(&old_idx) = seen.get(&map_key) {
                deduped[old_idx] = entry;
            } else {
                seen.insert(map_key.clone(), deduped.len());
                deduped.push(entry);
            }
        }

        // Write merged segment using WalEntry::encode for proper format
        let merged_path = wal_dir.join("wal_merged.log");
        let mut buf = Vec::new();
        for entry in &deduped {
            buf.extend_from_slice(&entry.encode());
        }

        std::fs::write(&merged_path, &buf)
            .map_err(|e| NxrError::Io(e))?;

        // Remove old segments
        for (_, path) in &entries {
            let _ = std::fs::remove_file(path);
        }

        log::info!("GC: Merged {} entries into wal_merged.log (deduped from {})",
            deduped.len(), seen.len());
        Ok(())
    }

    /// Compress cold vector segments: merge small segments into larger ones
    fn compress_cold_vectors(&self) -> NxrResult<()> {
        log::info!("GC: Compressing cold vector segments");
        let segments_path = &self.config.vector.segments_path;
        if !segments_path.exists() {
            return Ok(());
        }

        let mut segments: Vec<(u32, std::path::PathBuf)> = Vec::new();
        for entry in std::fs::read_dir(segments_path)? {
            let entry = entry?;
            let name = entry.file_name().into_string().unwrap_or_default();
            if name.starts_with("seg_") && name.ends_with(".bin") {
                let meta = entry.metadata()?;
                let num: u32 = name[4..name.len() - 4].parse().unwrap_or(0);
                segments.push((num, entry.path()));
                // Log size info
                log::info!("GC:   segment {} size: {} bytes", name, meta.len());
            }
        }

        // Merge segments < 1MB into larger ones
        let merge_threshold = 1024 * 1024; // 1MB
        let mut small_segs: Vec<(u32, std::path::PathBuf)> = segments
            .into_iter()
            .filter(|(_, path)| {
                std::fs::metadata(path).map(|m| m.len() < merge_threshold).unwrap_or(false)
            })
            .collect();

        if small_segs.len() < 2 {
            return Ok(());
        }

        small_segs.sort_by_key(|e| e.0);

        // Read and merge small segments
        let merged_path = segments_path.join("seg_merged.bin");
        let mut merged_data = Vec::new();
        for (_, path) in &small_segs {
            match std::fs::read(path) {
                Ok(data) => merged_data.extend_from_slice(&data),
                Err(_) => continue,
            }
        }

        std::fs::write(&merged_path, &merged_data)
            .map_err(|e| NxrError::Io(e))?;

        // Remove old small segments
        for (_, path) in &small_segs {
            let _ = std::fs::remove_file(path);
        }

        log::info!("GC: Merged {} small segments into seg_merged.bin ({} bytes)",
            small_segs.len(), merged_data.len());
        Ok(())
    }

    fn cleanup_expired_kv(&self) -> NxrResult<()> {
        log::info!("GC: Cleaning up expired KV entries");
        let cold_dir = &self.config.kv.cold_path;
        if !cold_dir.exists() {
            return Ok(());
        }

        let now = chrono::Utc::now().timestamp();
        for entry in std::fs::read_dir(cold_dir)? {
            let entry = entry?;
            let metadata = entry.metadata()?;
            let modified = metadata
                .modified()
                .map(|t| t.duration_since(std::time::UNIX_EPOCH).unwrap().as_secs() as i64)
                .unwrap_or(0);

            if now - modified > 7 * 86400 {
                log::info!("GC: Removing expired cold KV {:?}", entry.path());
                let _ = std::fs::remove_file(entry.path());
            }
        }

        Ok(())
    }

    pub fn rebuild_index(&self, index: &mut IndexManager) -> NxrResult<()> {
        let frag = index.fragmentation();
        let threshold = self.config.pipeline.index_fragmentation_threshold;
        log::info!("GC: Index fragmentation: {:.2} (threshold: {:.2})", frag, threshold);
        if frag > threshold {
            log::info!("GC: Rebuilding index");
            index.rebuild()?;
            log::info!("GC: Index rebuilt (new fragmentation: {:.2})", index.fragmentation());
        } else {
            log::info!("GC: Index fragmentation OK, no rebuild needed");
        }
        Ok(())
    }
}
