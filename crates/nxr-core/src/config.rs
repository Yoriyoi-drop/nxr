use serde::{Deserialize, Serialize};
use std::path::{Path, PathBuf};

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Config {
    #[serde(default = "default_db_path")]
    pub db_path: PathBuf,
    #[serde(default = "default_wal_dir")]
    pub wal_dir: PathBuf,
    #[serde(default)]
    pub vector: VectorConfig,
    #[serde(default)]
    pub graph: GraphConfig,
    #[serde(default)]
    pub kv: KvConfig,
    #[serde(default)]
    pub index: IndexConfig,
    #[serde(default)]
    pub pipeline: PipelineConfig,

    #[serde(default)]
    pub snapshot: SnapshotConfig,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SnapshotConfig {
    #[serde(default = "default_snapshots_path")]
    pub snapshots_path: PathBuf,
    #[serde(default = "default_max_snapshots")]
    pub max_snapshots: u32,
    #[serde(default = "default_snapshot_interval")]
    pub interval_hours: u32,
}

impl Default for SnapshotConfig {
    fn default() -> Self {
        Self {
            snapshots_path: default_snapshots_path(),
            max_snapshots: default_max_snapshots(),
            interval_hours: default_snapshot_interval(),
        }
    }
}

impl Default for Config {
    fn default() -> Self {
        Self {
            db_path: PathBuf::from("/var/nxr-db"),
            wal_dir: PathBuf::from("/var/nxr-db/wal"),
            vector: VectorConfig {
                dimension: 1536,
                ef_construction: 200,
                m_max: 16,
                space: "cosine".into(),
                segment_size_mb: 64,
                hnsw_path: PathBuf::from("/var/nxr-db/vectors/index.hnsw"),
                segments_path: PathBuf::from("/var/nxr-db/vectors/segments"),
            },
            graph: GraphConfig {
                nodes_path: PathBuf::from("/var/nxr-db/graph/nodes.dat"),
                edges_path: PathBuf::from("/var/nxr-db/graph/edges.dat"),
                adj_list_path: PathBuf::from("/var/nxr-db/graph/adj_list.idx"),
                labels_path: PathBuf::from("/var/nxr-db/graph/labels.idx"),
            },
            kv: KvConfig {
                hot_zone_mb: 2048,
                warm_zone_mb: 51200,
                hot_path: PathBuf::from("/var/nxr-db/kv/hot.mem"),
                cold_path: PathBuf::from("/var/nxr-db/kv/cold"),
            },
            index: IndexConfig {
                btree_order: 128,
            },
            pipeline: PipelineConfig {
                max_context_tokens: 128000,
                memory_similarity_weight: 0.6,
                memory_recency_weight: 0.25,
                memory_importance_weight: 0.15,
                gc_interval_hours: 24,
                index_fragmentation_threshold: 0.3,
                merge_strategy: "union".into(),
            },
            snapshot: SnapshotConfig::default(),
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct VectorConfig {
    #[serde(default = "default_dimension")]
    pub dimension: u32,
    #[serde(default = "default_ef")]
    pub ef_construction: u32,
    #[serde(default = "default_mmax")]
    pub m_max: u32,
    #[serde(default = "default_space")]
    pub space: String,
    #[serde(default = "default_seg_size")]
    pub segment_size_mb: u32,
    #[serde(default = "default_hnsw_path")]
    pub hnsw_path: PathBuf,
    #[serde(default = "default_segments_path")]
    pub segments_path: PathBuf,
}

impl Default for VectorConfig {
    fn default() -> Self {
        Self {
            dimension: default_dimension(),
            ef_construction: default_ef(),
            m_max: default_mmax(),
            space: default_space(),
            segment_size_mb: default_seg_size(),
            hnsw_path: default_hnsw_path(),
            segments_path: default_segments_path(),
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct GraphConfig {
    #[serde(default = "default_nodes_path")]
    pub nodes_path: PathBuf,
    #[serde(default = "default_edges_path")]
    pub edges_path: PathBuf,
    #[serde(default = "default_adj_path")]
    pub adj_list_path: PathBuf,
    #[serde(default = "default_labels_path")]
    pub labels_path: PathBuf,
}

impl Default for GraphConfig {
    fn default() -> Self {
        Self {
            nodes_path: default_nodes_path(),
            edges_path: default_edges_path(),
            adj_list_path: default_adj_path(),
            labels_path: default_labels_path(),
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct KvConfig {
    #[serde(default = "default_hot_mb")]
    pub hot_zone_mb: u32,
    #[serde(default = "default_warm_mb")]
    pub warm_zone_mb: u32,
    #[serde(default = "default_hot_path")]
    pub hot_path: PathBuf,
    #[serde(default = "default_cold_path")]
    pub cold_path: PathBuf,
}

impl Default for KvConfig {
    fn default() -> Self {
        Self {
            hot_zone_mb: default_hot_mb(),
            warm_zone_mb: default_warm_mb(),
            hot_path: default_hot_path(),
            cold_path: default_cold_path(),
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct IndexConfig {
    #[serde(default = "default_btree_order")]
    pub btree_order: u32,
}

impl Default for IndexConfig {
    fn default() -> Self {
        Self { btree_order: default_btree_order() }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PipelineConfig {
    #[serde(default = "default_max_tokens")]
    pub max_context_tokens: u32,
    #[serde(default = "default_sim_weight")]
    pub memory_similarity_weight: f32,
    #[serde(default = "default_recency_weight")]
    pub memory_recency_weight: f32,
    #[serde(default = "default_importance_weight")]
    pub memory_importance_weight: f32,
    #[serde(default = "default_gc_interval")]
    pub gc_interval_hours: u32,
    #[serde(default = "default_frag_threshold")]
    pub index_fragmentation_threshold: f32,
    #[serde(default = "default_merge_strategy")]
    pub merge_strategy: String,
}

impl Default for PipelineConfig {
    fn default() -> Self {
        Self {
            max_context_tokens: default_max_tokens(),
            memory_similarity_weight: default_sim_weight(),
            memory_recency_weight: default_recency_weight(),
            memory_importance_weight: default_importance_weight(),
            gc_interval_hours: default_gc_interval(),
            index_fragmentation_threshold: default_frag_threshold(),
            merge_strategy: default_merge_strategy(),
        }
    }
}

impl Config {
    pub fn load<P: AsRef<Path>>(path: P) -> Result<Self, crate::error::NxrError> {
        let content = std::fs::read_to_string(path.as_ref())
            .map_err(|e| crate::error::NxrError::Config(format!("Failed to read config: {}", e)))?;
        toml::from_str(&content).map_err(|e| {
            crate::error::NxrError::Config(format!("Failed to parse config: {}", e))
        })
    }

    pub fn with_db_path(mut self, path: &str) -> Self {
        let base = PathBuf::from(path);
        self.db_path = base.clone();
        self.wal_dir = base.join("wal");
        self.vector.hnsw_path = base.join("vectors/index.hnsw");
        self.vector.segments_path = base.join("vectors/segments");
        self.graph.nodes_path = base.join("graph/nodes.dat");
        self.graph.edges_path = base.join("graph/edges.dat");
        self.graph.adj_list_path = base.join("graph/adj_list.idx");
        self.graph.labels_path = base.join("graph/labels.idx");
        self.kv.hot_path = base.join("kv/hot.mem");
        self.kv.cold_path = base.join("kv/cold");
        self.snapshot.snapshots_path = base.join("snapshots");
        self
    }
}

fn default_db_path() -> PathBuf { PathBuf::from("/var/nxr-db") }
fn default_wal_dir() -> PathBuf { PathBuf::from("/var/nxr-db/wal") }
fn default_dimension() -> u32 { 1536 }
fn default_ef() -> u32 { 200 }
fn default_mmax() -> u32 { 16 }
fn default_space() -> String { "cosine".into() }
fn default_seg_size() -> u32 { 64 }
fn default_hnsw_path() -> PathBuf { PathBuf::from("/var/nxr-db/vectors/index.hnsw") }
fn default_segments_path() -> PathBuf { PathBuf::from("/var/nxr-db/vectors/segments") }
fn default_nodes_path() -> PathBuf { PathBuf::from("/var/nxr-db/graph/nodes.dat") }
fn default_edges_path() -> PathBuf { PathBuf::from("/var/nxr-db/graph/edges.dat") }
fn default_adj_path() -> PathBuf { PathBuf::from("/var/nxr-db/graph/adj_list.idx") }
fn default_labels_path() -> PathBuf { PathBuf::from("/var/nxr-db/graph/labels.idx") }
fn default_hot_mb() -> u32 { 2048 }
fn default_warm_mb() -> u32 { 51200 }
fn default_hot_path() -> PathBuf { PathBuf::from("/var/nxr-db/kv/hot.mem") }
fn default_cold_path() -> PathBuf { PathBuf::from("/var/nxr-db/kv/cold") }
fn default_btree_order() -> u32 { 128 }
fn default_max_tokens() -> u32 { 128000 }
fn default_sim_weight() -> f32 { 0.6 }
fn default_recency_weight() -> f32 { 0.25 }
fn default_importance_weight() -> f32 { 0.15 }
fn default_gc_interval() -> u32 { 24 }
fn default_frag_threshold() -> f32 { 0.3 }
fn default_merge_strategy() -> String { "union".into() }
fn default_snapshots_path() -> PathBuf { PathBuf::from("/var/nxr-db/snapshots") }
fn default_max_snapshots() -> u32 { 7 }
fn default_snapshot_interval() -> u32 { 6 }
