use napi::bindgen_prelude::*;
use napi_derive::napi;

use nxr_core::config::Config;
use nxr_core::graph::GraphStore;
use nxr_core::kv::KvCache;
use nxr_core::query::QueryEngine;
use nxr_core::vector::VectorEngine;

#[napi]
pub struct NxrDatabase {
    config: Config,
    vector: VectorEngine,
    graph: GraphStore,
    kv: KvCache,
    query_engine: QueryEngine,
}

#[napi]
impl NxrDatabase {
    #[napi(constructor)]
    pub fn open(path: String) -> Result<Self> {
        let config = Config::load(&std::path::Path::new(&path).join("config.toml"))
            .unwrap_or_else(|_| Config::default().with_db_path(&path));

        let vector = VectorEngine::new(&config)
            .map_err(|e| Error::from_reason(format!("Vector init: {}", e)))?;
        let graph = GraphStore::new(&config)
            .map_err(|e| Error::from_reason(format!("Graph init: {}", e)))?;
        let kv = KvCache::new(&config)
            .map_err(|e| Error::from_reason(format!("KV init: {}", e)))?;

        Ok(Self { config, vector, graph, kv, query_engine: QueryEngine::new() })
    }

    #[napi]
    pub fn query(&mut self, sql: String) -> Result<QueryResult> {
        let result = self.query_engine.execute(&sql, &self.vector, &mut self.graph, &self.kv)
            .map_err(|e| Error::from_reason(format!("Query failed: {}", e)))?;
        Ok(QueryResult {
            columns: result.columns,
            rows: result.rows.iter().map(|r| {
                r.iter().map(|v| v.to_string()).collect()
            }).collect(),
            row_count: result.row_count as i64,
            elapsed_ms: result.elapsed_ms as f64,
            message: result.message.unwrap_or_default(),
        })
    }

    #[napi]
    pub fn vector_insert(&mut self, id: i64, vector: Vec<f64>, metadata: Option<String>) -> Result<()> {
        let vec_f32: Vec<f32> = vector.into_iter().map(|v| v as f32).collect();
        let meta = metadata.unwrap_or_default().into_bytes();
        self.vector.insert(id as u64, &vec_f32, &meta)
            .map_err(|e| Error::from_reason(format!("Vector insert failed: {}", e)))
    }

    #[napi]
    pub fn vector_search(&self, query: Vec<f64>, k: Option<i64>) -> Result<Vec<SearchResult>> {
        let vec_f32: Vec<f32> = query.into_iter().map(|v| v as f32).collect();
        let limit = k.unwrap_or(10).max(1) as usize;
        let results = self.vector.search(&vec_f32, limit)
            .map_err(|e| Error::from_reason(format!("Vector search failed: {}", e)))?;
        Ok(results.into_iter().map(|(id, score)| SearchResult {
            id: id as i64,
            score: score as f64,
        }).collect())
    }

    #[napi]
    pub fn vector_delete(&mut self, id: i64) -> Result<()> {
        self.vector.delete(id as u64)
            .map_err(|e| Error::from_reason(format!("Vector delete failed: {}", e)))
    }

    #[napi]
    pub fn graph_add_node(&mut self, label: String, properties: Option<Vec<String>>) -> Result<i64> {
        let props: Vec<(String, String)> = properties.unwrap_or_default()
            .chunks(2)
            .filter(|chunk| chunk.len() == 2)
            .map(|chunk| (chunk[0].clone(), chunk[1].clone()))
            .collect();
        let id = self.graph.add_node(&label, props)
            .map_err(|e| Error::from_reason(format!("Graph add node failed: {}", e)))?;
        Ok(id as i64)
    }

    #[napi]
    pub fn graph_add_edge(&mut self, from_node: i64, to_node: i64, relation: String, weight: f64) -> Result<i64> {
        let id = self.graph.add_edge(from_node as u64, to_node as u64, &relation, weight as f32)
            .map_err(|e| Error::from_reason(format!("Graph add edge failed: {}", e)))?;
        Ok(id as i64)
    }

    #[napi]
    pub fn graph_traverse(&self, from_label: String, relation: String, to_label: String) -> Result<Vec<TraverseResult>> {
        let results = self.graph.traverse(&from_label, &relation, &to_label);
        Ok(results.into_iter().map(|(from, to, weight)| TraverseResult {
            from_id: from as i64,
            to_id: to as i64,
            weight: weight as f64,
        }).collect())
    }

    #[napi]
    pub fn kv_set(&mut self, key: String, value: String, ttl: Option<i64>) -> Result<()> {
        self.kv.set(&key, value.as_bytes(), ttl.unwrap_or(0) as u32)
            .map_err(|e| Error::from_reason(format!("KV set failed: {}", e)))
    }

    #[napi]
    pub fn kv_get(&self, key: String) -> Result<Option<String>> {
        let val = self.kv.get(&key)
            .map_err(|e| Error::from_reason(format!("KV get failed: {}", e)))?;
        Ok(val.map(|v| String::from_utf8_lossy(&v).to_string()))
    }

    #[napi]
    pub fn kv_delete(&mut self, key: String) -> Result<()> {
        self.kv.delete(&key)
            .map_err(|e| Error::from_reason(format!("KV delete failed: {}", e)))
    }

    #[napi]
    pub fn len(&self) -> i64 {
        self.vector.len() as i64
    }
}

#[napi(object)]
#[derive(Clone)]
pub struct QueryResult {
    pub columns: Vec<String>,
    pub rows: Vec<Vec<String>>,
    pub row_count: i64,
    pub elapsed_ms: f64,
    pub message: String,
}

#[napi(object)]
#[derive(Clone)]
pub struct SearchResult {
    pub id: i64,
    pub score: f64,
}

#[napi(object)]
#[derive(Clone)]
pub struct TraverseResult {
    pub from_id: i64,
    pub to_id: i64,
    pub weight: f64,
}
