use crate::config::Config;
use crate::error::{NxrError, NxrResult};
use crate::graph::GraphStore;
use crate::wal::{Wal, WalLayer, WalOperation};
use chrono::Utc;
use std::fs;
use std::path::{Path, PathBuf};
use std::sync::Arc;

pub struct SnapshotManager {
    snapshots_dir: PathBuf,
    wal: Option<Arc<Wal>>,
    max_snapshots: u32,
}

impl SnapshotManager {
    pub fn new(config: &Config) -> Self {
        let snapshots_dir = config.snapshot.snapshots_path.clone();
        let _ = fs::create_dir_all(&snapshots_dir);
        Self {
            snapshots_dir,
            wal: None,
            max_snapshots: config.snapshot.max_snapshots,
        }
    }

    pub fn with_wal(mut self, wal: Arc<Wal>) -> Self {
        self.wal = Some(wal);
        self
    }

    pub fn create(
        &self,
        graph: &GraphStore,
        kv_state: &[u8],
        vector_meta: &[u8],
    ) -> NxrResult<Snapshot> {
        let timestamp = Utc::now().format("%Y%m%d_%H%M%S_%3f").to_string();
        let snapshot_id = format!("snap_{}", timestamp);
        let snap_dir = self.snapshots_dir.join(&snapshot_id);
        fs::create_dir_all(&snap_dir)?;

        let lsn = if let Some(ref wal) = self.wal {
            let payload = snapshot_id.as_bytes().to_vec();
            wal.append(WalOperation::Insert, WalLayer::Kv, &payload)?;
            // track snapshot in WAL for recovery
            payload[..8].try_into().map(u64::from_le_bytes).ok().unwrap_or(0)
        } else {
            0
        };

        let meta = SnapshotMeta {
            id: snapshot_id.clone(),
            timestamp: Utc::now().timestamp(),
            lsn,
            vector_count: 0,
            graph_nodes: graph.node_count() as u64,
            graph_edges: graph.edge_count() as u64,
            kv_size: kv_state.len() as u64,
        };

        graph.export_binary(&snap_dir.join("graph.bin"))?;
        fs::write(&snap_dir.join("kv_state.bin"), kv_state)?;
        fs::write(&snap_dir.join("vector_meta.bin"), vector_meta)?;

        let meta_path = snap_dir.join("meta.json");
        let meta_json = serde_json::to_string_pretty(&meta)
            .map_err(|e| NxrError::Snapshot(format!("Serialize meta: {}", e)))?;
        fs::write(&meta_path, meta_json)?;

        self.prune_old()?;

        log::info!("Snapshot created: {} ({} nodes, {} edges)",
            snapshot_id, meta.graph_nodes, meta.graph_edges);
        Ok(Snapshot { meta, path: snap_dir })
    }

    pub fn list(&self) -> NxrResult<Vec<SnapshotMeta>> {
        let mut snapshots = Vec::new();
        if !self.snapshots_dir.exists() {
            return Ok(snapshots);
        }
        for entry in fs::read_dir(&self.snapshots_dir)? {
            let entry = entry?;
            let meta_path = entry.path().join("meta.json");
            if meta_path.exists() {
                let content = fs::read_to_string(&meta_path)?;
                if let Ok(meta) = serde_json::from_str::<SnapshotMeta>(&content) {
                    snapshots.push(meta);
                }
            }
        }
        snapshots.sort_by(|a, b| b.timestamp.cmp(&a.timestamp));
        Ok(snapshots)
    }

    pub fn latest(&self) -> NxrResult<Option<SnapshotMeta>> {
        let snapshots = self.list()?;
        Ok(snapshots.into_iter().next())
    }

    pub fn restore(
        &self,
        snapshot_id: &str,
        graph: &mut GraphStore,
    ) -> NxrResult<()> {
        let snap_dir = self.snapshots_dir.join(snapshot_id);
        if !snap_dir.exists() {
            return Err(NxrError::Snapshot(format!(
                "Snapshot '{}' not found", snapshot_id
            )));
        }
        graph.import_binary(&snap_dir.join("graph.bin"))?;
        log::info!("Snapshot restored: {}", snapshot_id);
        Ok(())
    }

    fn prune_old(&self) -> NxrResult<()> {
        let mut snapshots = self.list()?;
        if snapshots.len() <= self.max_snapshots as usize {
            return Ok(());
        }
        let to_remove = snapshots.split_off(self.max_snapshots as usize);
        for meta in to_remove {
            let path = self.snapshots_dir.join(&meta.id);
            if path.exists() {
                fs::remove_dir_all(&path)?;
                log::info!("Pruned old snapshot: {}", meta.id);
            }
        }
        Ok(())
    }
}

#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
pub struct SnapshotMeta {
    pub id: String,
    pub timestamp: i64,
    pub lsn: u64,
    pub vector_count: u64,
    pub graph_nodes: u64,
    pub graph_edges: u64,
    pub kv_size: u64,
}

pub struct Snapshot {
    pub meta: SnapshotMeta,
    pub path: PathBuf,
}

pub trait BinaryExport {
    fn export_binary(&self, path: &Path) -> NxrResult<()>;
    fn import_binary(&mut self, path: &Path) -> NxrResult<()>;
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::config::Config;
    use crate::graph::GraphStore;

    fn setup(name: &str) -> (SnapshotManager, GraphStore) {
        let tmp = std::env::temp_dir().join(format!("nxr_snap_{}", name));
        let _ = std::fs::remove_dir_all(&tmp);
        let mut config = Config::default();
        config = config.with_db_path(tmp.to_str().unwrap());
        config.snapshot.max_snapshots = 3;
        config.snapshot.snapshots_path = tmp.join("snapshots");
        std::fs::create_dir_all(&config.snapshot.snapshots_path).unwrap();
        std::fs::create_dir_all(config.vector.segments_path.parent().unwrap()).unwrap();
        std::fs::create_dir_all(config.graph.nodes_path.parent().unwrap()).unwrap();
        std::fs::create_dir_all(config.kv.cold_path.parent().unwrap()).unwrap();
        std::fs::create_dir_all(&config.wal_dir).unwrap();

        let mut graph = GraphStore::new(&config).unwrap();
        graph.add_node("User", Vec::new()).unwrap();
        graph.add_node("Topic", Vec::new()).unwrap();

        let mgr = SnapshotManager::new(&config);
        (mgr, graph)
    }

    #[test]
    fn test_create_snapshot() {
        let (mgr, graph) = setup("create");
        let snap = mgr.create(&graph, b"kv_data", b"vector_data").unwrap();
        assert!(!snap.meta.id.is_empty());
        assert!(snap.path.exists());
        assert!(snap.path.join("meta.json").exists());
        assert!(snap.path.join("graph.bin").exists());
        assert!(snap.path.join("kv_state.bin").exists());
    }

    #[test]
    fn test_list_snapshots() {
        let (mgr, graph) = setup("list");
        mgr.create(&graph, b"k1", b"v1").unwrap();
        std::thread::sleep(std::time::Duration::from_millis(5));
        mgr.create(&graph, b"k2", b"v2").unwrap();

        let snapshots = mgr.list().unwrap();
        assert_eq!(snapshots.len(), 2);
    }

    #[test]
    fn test_latest_snapshot() {
        let (mgr, graph) = setup("latest");
        mgr.create(&graph, b"k1", b"v1").unwrap();
        let latest = mgr.latest().unwrap();
        assert!(latest.is_some());
    }

    #[test]
    fn test_prune_old() {
        let (mgr, graph) = setup("prune");
        for i in 0..5 {
            mgr.create(&graph, &[i], &[i]).unwrap();
        }
        let snapshots = mgr.list().unwrap();
        assert!(snapshots.len() <= 3);
    }

    #[test]
    fn test_restore_nonexistent() {
        let (mgr, mut graph) = setup("restore");
        let result = mgr.restore("nonexistent", &mut graph);
        assert!(result.is_err());
    }
}
