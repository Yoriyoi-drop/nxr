pub mod parser;
pub mod planner;
pub mod executor;

use crate::error::NxrResult;
use crate::graph::store::GraphStore;
use crate::kv::KvCache;
use crate::vector::VectorEngine;
pub use executor::QueryResult;

pub struct QueryEngine {
    parser: parser::NxrQlParser,
    planner: planner::QueryPlanner,
    executor: executor::QueryExecutor,
}

impl QueryEngine {
    pub fn new() -> Self {
        Self {
            parser: parser::NxrQlParser,
            planner: planner::QueryPlanner,
            executor: executor::QueryExecutor,
        }
    }

    pub fn execute(
        &self,
        query: &str,
        vector: &VectorEngine,
        graph: &mut GraphStore,
        kv: &KvCache,
    ) -> NxrResult<QueryResult> {
        let parsed = self.parser.parse(query)?;
        let plan = self.planner.plan(&parsed)?;
        self.executor.execute(&plan, vector, graph, kv)
    }

    pub fn execute_with_db(
        &self,
        query: &str,
        db: &mut crate::NxrDb,
    ) -> NxrResult<QueryResult> {
        let parsed = self.parser.parse(query)?;
        let plan = self.planner.plan(&parsed)?;
        self.executor.execute(&plan, &db.vector, &mut db.graph, &db.kv)
    }
}
