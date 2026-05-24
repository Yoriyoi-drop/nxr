pub mod embedding;
pub mod context;
pub mod ranker;
pub mod gc;

use crate::config::Config;
use crate::graph::store::GraphStore;
use crate::kv::KvCache;
use crate::vector::VectorEngine;
use context::ContextManager;
use ranker::MemoryRanker;

pub use gc::GarbageCollector;

pub struct QueryPipeline {
    pub context_manager: ContextManager,
    pub memory_ranker: MemoryRanker,
    pub gc: GarbageCollector,
}

impl QueryPipeline {
    pub fn new(config: &Config) -> Self {
        Self {
            context_manager: ContextManager::new(config.pipeline.max_context_tokens),
            memory_ranker: MemoryRanker::new(
                config.pipeline.memory_similarity_weight,
                config.pipeline.memory_recency_weight,
                config.pipeline.memory_importance_weight,
            ),
            gc: GarbageCollector::new(config),
        }
    }

    pub fn process_query(
        &self,
        query: &str,
        vector: &VectorEngine,
        _graph: &GraphStore,
        _kv: &KvCache,
    ) -> String {
        let context = self.context_manager.get_context();
        let results = self.memory_ranker.rank(query, vector, context);
        format!("Pipeline: {} results processed", results.len())
    }
}
