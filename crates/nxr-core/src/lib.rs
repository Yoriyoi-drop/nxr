pub mod config;
pub mod wal;
pub mod vector;
pub mod graph;
pub mod kv;
pub mod query;
pub mod pipeline;
pub mod index;
pub mod snapshot;
pub mod error;

use std::path::PathBuf;

pub struct NxrDb {
    pub config: config::Config,
    pub wal: wal::Wal,
    pub vector: vector::VectorEngine,
    pub graph: graph::GraphStore,
    pub kv: kv::KvCache,
    pub index: index::IndexManager,
    pub pipeline: pipeline::QueryPipeline,
    pub gc: pipeline::gc::GarbageCollector,
}

impl NxrDb {
    pub fn open(path: &str) -> Result<Self, error::NxrError> {
        let config_path = PathBuf::from(path).join("config.toml");
        let config = if config_path.exists() {
            config::Config::load(&config_path)?
        } else {
            config::Config::default().with_db_path(path)
        };

        let wal = wal::Wal::open(&config.wal_dir)?;
        let vector = vector::VectorEngine::new(&config)?;
        let graph = graph::GraphStore::new(&config)?;
        let kv = kv::KvCache::new(&config)?;
        let index = index::IndexManager::new(&config)?;
        let pipeline = pipeline::QueryPipeline::new(&config);
        let gc = pipeline::gc::GarbageCollector::new(&config);

        Ok(Self { config, wal, vector, graph, kv, index, pipeline, gc })
    }

    pub fn start_gc_loop(&self) {
        let gc = self.gc.clone();
        std::thread::spawn(move || loop {
            std::thread::sleep(std::time::Duration::from_secs(86400));
            if let Err(e) = gc.run(None) {
                log::error!("GC error: {}", e);
            }
        });
    }
}
