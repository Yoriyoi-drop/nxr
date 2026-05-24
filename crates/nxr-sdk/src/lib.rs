use std::path::Path;
use std::sync::Mutex;

use nxr_core::config::Config;
use nxr_core::error::NxrResult;
use nxr_core::graph::store::GraphNode;
use nxr_core::query::QueryResult;
use nxr_core::query::QueryEngine;
use nxr_core::NxrDb;

/// High-level NXR database client.
///
/// Wraps `NxrDb` with a thread-safe, ergonomic API.
/// All operations use internal synchronization — no `&mut self` required.
pub struct NxrClient {
    db: Mutex<NxrDb>,
    query_engine: QueryEngine,
}

impl NxrClient {
    /// Open or create a database at the given path.
    pub fn open<P: AsRef<Path>>(path: P) -> NxrResult<Self> {
        let path_str = path.as_ref().to_str().unwrap_or("/var/nxr-db");
        let db = NxrDb::open(path_str)?;
        Ok(Self { db: Mutex::new(db), query_engine: QueryEngine::new() })
    }

    /// Execute an NXR-QL query string.
    pub fn query(&self, sql: &str) -> NxrResult<QueryResult> {
        let mut db = self.db.lock().unwrap();
        self.query_engine.execute_with_db(sql, &mut db)
    }

    // ── Vector operations ──

    /// Insert a vector with metadata.
    pub fn vector_insert(&self, id: u64, vector: &[f32], metadata: &[u8]) -> NxrResult<()> {
        let mut db = self.db.lock().unwrap();
        db.vector.insert(id, vector, metadata)
    }

    /// Search for the k nearest neighbors.
    pub fn vector_search(&self, query: &[f32], k: usize) -> NxrResult<Vec<(u64, f32)>> {
        let db = self.db.lock().unwrap();
        db.vector.search(query, k)
    }

    /// Delete a vector by id.
    pub fn vector_delete(&self, id: u64) -> NxrResult<()> {
        let mut db = self.db.lock().unwrap();
        db.vector.delete(id)
    }

    /// Number of vectors stored.
    pub fn vector_count(&self) -> usize {
        let db = self.db.lock().unwrap();
        db.vector.len()
    }

    /// Vector dimension.
    pub fn vector_dimension(&self) -> u32 {
        let db = self.db.lock().unwrap();
        db.vector.dimension()
    }

    // ── Graph operations ──

    /// Add a node to the graph.
    pub fn graph_add_node(&self, label: &str, properties: Vec<(String, String)>) -> NxrResult<u64> {
        let mut db = self.db.lock().unwrap();
        db.graph.add_node(label, properties)
    }

    /// Add an edge between two nodes.
    pub fn graph_add_edge(&self, from_node: u64, to_node: u64, relation: &str, weight: f32) -> NxrResult<u64> {
        let mut db = self.db.lock().unwrap();
        db.graph.add_edge(from_node, to_node, relation, weight)
    }

    /// Get a node by id.
    pub fn graph_get_node(&self, id: u64) -> Option<GraphNode> {
        let db = self.db.lock().unwrap();
        db.graph.get_node(id).cloned()
    }

    /// Find nodes by label.
    pub fn graph_find_by_label(&self, label: &str) -> Vec<GraphNode> {
        let db = self.db.lock().unwrap();
        db.graph.find_nodes_by_label(label).into_iter().cloned().collect()
    }

    /// Get neighbors of a node.
    pub fn graph_get_neighbors(&self, node_id: u64, relation: Option<&str>) -> Vec<(u64, f32)> {
        let db = self.db.lock().unwrap();
        db.graph.get_neighbors(node_id, relation)
    }

    /// Traverse the graph.
    pub fn graph_traverse(&self, from_label: &str, relation: &str, to_label: &str) -> Vec<(u64, u64, f32)> {
        let db = self.db.lock().unwrap();
        db.graph.traverse(from_label, relation, to_label)
    }

    /// Remove a node from the graph.
    pub fn graph_remove_node(&self, id: u64) -> NxrResult<()> {
        let mut db = self.db.lock().unwrap();
        db.graph.remove_node(id)
    }

    /// Number of nodes in the graph.
    pub fn graph_node_count(&self) -> usize {
        let db = self.db.lock().unwrap();
        db.graph.node_count()
    }

    /// Number of edges in the graph.
    pub fn graph_edge_count(&self) -> usize {
        let db = self.db.lock().unwrap();
        db.graph.edge_count()
    }

    // ── KV operations ──

    /// Set a key-value pair with optional TTL (seconds, 0 = no expiry).
    pub fn kv_set(&self, key: &str, value: &[u8], ttl: u32) -> NxrResult<()> {
        let mut db = self.db.lock().unwrap();
        db.kv.set(key, value, ttl)
    }

    /// Get a value by key.
    pub fn kv_get(&self, key: &str) -> NxrResult<Option<Vec<u8>>> {
        let db = self.db.lock().unwrap();
        db.kv.get(key)
    }

    /// Delete a key-value pair.
    pub fn kv_delete(&self, key: &str) -> NxrResult<()> {
        let mut db = self.db.lock().unwrap();
        db.kv.delete(key)
    }

    /// Number of KV entries.
    pub fn kv_count(&self) -> usize {
        let db = self.db.lock().unwrap();
        db.kv.len()
    }

    // ── Admin operations ──

    /// Run garbage collection.
    pub fn run_gc(&self) -> NxrResult<()> {
        let db = self.db.lock().unwrap();
        db.gc.run(None)
    }

    /// Rebuild all indexes if fragmentation exceeds threshold.
    pub fn rebuild_indexes(&self) -> NxrResult<()> {
        let mut db = self.db.lock().unwrap();
        let db_ref = &mut *db;
        db_ref.gc.rebuild_index(&mut db_ref.index)
    }

    /// Get the database config.
    pub fn config(&self) -> Config {
        let db = self.db.lock().unwrap();
        db.config.clone()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn test_client(name: &str) -> NxrClient {
        let dir = std::env::temp_dir().join(format!("nxr_sdk_{}", name));
        let _ = std::fs::remove_dir_all(&dir);
        NxrClient::open(&dir).unwrap()
    }

    #[test]
    fn test_vector_crud() {
        let client = test_client("vector_crud");
        let dim = client.vector_dimension();
        assert!(dim > 0);

        client.vector_insert(1, &vec![0.1; dim as usize], b"meta1").unwrap();
        client.vector_insert(2, &vec![0.9; dim as usize], b"meta2").unwrap();
        assert_eq!(client.vector_count(), 2);

        let results = client.vector_search(&vec![0.9; dim as usize], 5).unwrap();
        assert!(!results.is_empty());
        assert_eq!(results[0].0, 2);

        client.vector_delete(1).unwrap();
        assert_eq!(client.vector_count(), 1);
    }

    #[test]
    fn test_graph_operations() {
        let client = test_client("graph_ops");
        let alice = client.graph_add_node("User", vec![
            ("name".into(), "Alice".into()),
        ]).unwrap();
        let bob = client.graph_add_node("User", vec![
            ("name".into(), "Bob".into()),
        ]).unwrap();
        let topic = client.graph_add_node("Topic", vec![
            ("name".into(), "Rust".into()),
        ]).unwrap();

        client.graph_add_edge(alice, topic, "PREFERS", 0.9).unwrap();
        client.graph_add_edge(bob, topic, "PREFERS", 0.7).unwrap();

        assert_eq!(client.graph_node_count(), 3);
        assert_eq!(client.graph_edge_count(), 2);

        let node = client.graph_get_node(alice).unwrap();
        assert_eq!(node.label, "User");

        let users = client.graph_find_by_label("User");
        assert_eq!(users.len(), 2);

        let traverse = client.graph_traverse("User", "PREFERS", "Topic");
        assert_eq!(traverse.len(), 2);

        client.graph_remove_node(alice).unwrap();
        assert_eq!(client.graph_node_count(), 2);
    }

    #[test]
    fn test_kv_operations() {
        let client = test_client("kv_ops");
        client.kv_set("greeting", b"hello", 0).unwrap();
        let val = client.kv_get("greeting").unwrap().unwrap();
        assert_eq!(val, b"hello");

        client.kv_delete("greeting").unwrap();
        let val = client.kv_get("greeting").unwrap();
        assert!(val.is_none());
    }
}
