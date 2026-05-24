use crate::config::Config;
use crate::error::{NxrError, NxrResult};
use crate::snapshot::BinaryExport;
use crate::wal::{Wal, WalLayer, WalOperation};
use serde::{Deserialize, Serialize};
use std::collections::{BTreeMap, HashMap};
use std::fs;
use std::path::Path;
use std::sync::Arc;

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct GraphNode {
    pub id: u64,
    pub label: String,
    pub properties: HashMap<String, String>,
    pub created: i64,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct GraphEdge {
    pub id: u64,
    pub from_node: u64,
    pub to_node: u64,
    pub relation: String,
    pub weight: f32,
    pub created: i64,
}

pub struct GraphStore {
    nodes: BTreeMap<u64, GraphNode>,
    edges: BTreeMap<u64, GraphEdge>,
    adjacency: HashMap<u64, Vec<(u64, u64, f32)>>,
    label_index: HashMap<String, Vec<u64>>,
    next_node_id: u64,
    next_edge_id: u64,
    config: Config,
    wal: Option<Arc<Wal>>,
}

impl GraphStore {
    pub fn new(config: &Config) -> NxrResult<Self> {
        fs::create_dir_all(config.graph.nodes_path.parent().unwrap())?;

        let mut store = Self {
            nodes: BTreeMap::new(),
            edges: BTreeMap::new(),
            adjacency: HashMap::new(),
            label_index: HashMap::new(),
            next_node_id: 1,
            next_edge_id: 1,
            config: config.clone(),
            wal: None,
        };

        store.load()?;
        Ok(store)
    }

    fn save_adj_list(&self) -> NxrResult<()> {
        let path = &self.config.graph.adj_list_path;
        let mut buf = Vec::new();
        buf.extend_from_slice(&(self.adjacency.len() as u64).to_le_bytes());
        for (&from_id, neighbors) in &self.adjacency {
            buf.extend_from_slice(&from_id.to_le_bytes());
            buf.extend_from_slice(&(neighbors.len() as u64).to_le_bytes());
            for &(to_id, edge_id, weight) in neighbors {
                buf.extend_from_slice(&to_id.to_le_bytes());
                buf.extend_from_slice(&edge_id.to_le_bytes());
                buf.extend_from_slice(&weight.to_le_bytes());
            }
        }
        fs::write(path, buf)?;
        Ok(())
    }

    fn load_adj_list(&mut self) -> NxrResult<bool> {
        let path = &self.config.graph.adj_list_path;
        if !path.exists() {
            return Ok(false);
        }
        let bytes = fs::read(path)?;
        if bytes.len() < 8 {
            return Ok(false);
        }
        let mut offset = 0;
        let n_entries = u64::from_le_bytes(bytes[offset..offset + 8].try_into().unwrap());
        offset += 8;
        for _ in 0..n_entries {
            if offset + 8 > bytes.len() {
                break;
            }
            let from_id = u64::from_le_bytes(bytes[offset..offset + 8].try_into().unwrap());
            offset += 8;
            if offset + 8 > bytes.len() {
                break;
            }
            let n_neighbors = u64::from_le_bytes(bytes[offset..offset + 8].try_into().unwrap());
            offset += 8;
            let mut neighbors = Vec::with_capacity(n_neighbors as usize);
            for _ in 0..n_neighbors {
                if offset + 20 > bytes.len() {
                    break;
                }
                let to_id = u64::from_le_bytes(bytes[offset..offset + 8].try_into().unwrap());
                let edge_id = u64::from_le_bytes(bytes[offset + 8..offset + 16].try_into().unwrap());
                let weight = f32::from_le_bytes(bytes[offset + 16..offset + 20].try_into().unwrap());
                neighbors.push((to_id, edge_id, weight));
                offset += 20;
            }
            self.adjacency.insert(from_id, neighbors);
        }
        Ok(true)
    }

    pub fn with_wal(mut self, wal: Arc<Wal>) -> Self {
        self.wal = Some(wal);
        self
    }

    fn load(&mut self) -> NxrResult<()> {
        if self.config.graph.nodes_path.exists() {
            let json = fs::read_to_string(&self.config.graph.nodes_path)?;
            if !json.is_empty() {
                let nodes: Vec<GraphNode> = serde_json::from_str(&json)
                    .map_err(|e| NxrError::Graph(format!("Deserialize nodes: {}", e)))?;
                for n in nodes {
                    let id = n.id;
                    self.nodes.insert(id, n);
                    if id >= self.next_node_id {
                        self.next_node_id = id + 1;
                    }
                }
            }
        }

        // Try loading adjacency list from binary index first
        if !self.load_adj_list()? {
            // Fallback: rebuild adjacency from edges
            if self.config.graph.edges_path.exists() {
                let json = fs::read_to_string(&self.config.graph.edges_path)?;
                if !json.is_empty() {
                    let edges: Vec<GraphEdge> = serde_json::from_str(&json)
                        .map_err(|e| NxrError::Graph(format!("Deserialize edges: {}", e)))?;
                    for e in edges {
                        let id = e.id;
                        self.adjacency
                            .entry(e.from_node)
                            .or_default()
                            .push((e.to_node, e.id, e.weight));
                        self.edges.insert(id, e);
                        if id >= self.next_edge_id {
                            self.next_edge_id = id + 1;
                        }
                    }
                }
            }
        } else {
            // adj_list.idx loaded, still need to populate edges map
            if self.config.graph.edges_path.exists() {
                let json = fs::read_to_string(&self.config.graph.edges_path)?;
                if !json.is_empty() {
                    let edges: Vec<GraphEdge> = serde_json::from_str(&json)
                        .map_err(|e| NxrError::Graph(format!("Deserialize edges: {}", e)))?;
                    for e in edges {
                        let id = e.id;
                        self.edges.insert(id, e);
                        if id >= self.next_edge_id {
                            self.next_edge_id = id + 1;
                        }
                    }
                }
            }
        }

        if self.config.graph.labels_path.exists() {
            let json = fs::read_to_string(&self.config.graph.labels_path)?;
            if !json.is_empty() {
                let idx: HashMap<String, Vec<u64>> = serde_json::from_str(&json)
                    .map_err(|e| NxrError::Graph(format!("Deserialize labels: {}", e)))?;
                self.label_index = idx;
            }
        }

        Ok(())
    }

    fn save(&self) -> NxrResult<()> {
        let node_list: Vec<&GraphNode> = self.nodes.values().collect();
        fs::write(
            &self.config.graph.nodes_path,
            serde_json::to_string_pretty(&node_list)
                .map_err(|e| NxrError::Graph(format!("Serialize: {}", e)))?,
        )?;

        let edge_list: Vec<&GraphEdge> = self.edges.values().collect();
        fs::write(
            &self.config.graph.edges_path,
            serde_json::to_string_pretty(&edge_list)
                .map_err(|e| NxrError::Graph(format!("Serialize: {}", e)))?,
        )?;

        fs::write(
            &self.config.graph.labels_path,
            serde_json::to_string_pretty(&self.label_index)
                .map_err(|e| NxrError::Graph(format!("Serialize: {}", e)))?,
        )?;

        self.save_adj_list()?;

        Ok(())
    }

    pub fn add_node(&mut self, label: &str, properties: Vec<(String, String)>) -> NxrResult<u64> {
        let properties: HashMap<String, String> = properties.into_iter().collect();
        let id = self.next_node_id;
        self.next_node_id += 1;
        let node = GraphNode {
            id,
            label: label.to_string(),
            properties,
            created: chrono::Utc::now().timestamp(),
        };

        if let Some(ref wal) = self.wal {
            let payload = serde_json::to_vec(&node)
                .map_err(|e| NxrError::Graph(format!("Serialize: {}", e)))?;
            wal.append(WalOperation::Insert, WalLayer::Graph, &payload)?;
        }

        self.label_index.entry(label.to_string()).or_default().push(id);
        self.nodes.insert(id, node);
        self.save()?;
        Ok(id)
    }

    pub fn add_edge(
        &mut self,
        from_node: u64,
        to_node: u64,
        relation: &str,
        weight: f32,
    ) -> NxrResult<u64> {
        if !self.nodes.contains_key(&from_node) {
            return Err(NxrError::NotFound(format!("Node {} not found", from_node)));
        }
        if !self.nodes.contains_key(&to_node) {
            return Err(NxrError::NotFound(format!("Node {} not found", to_node)));
        }

        let id = self.next_edge_id;
        self.next_edge_id += 1;
        let edge = GraphEdge {
            id,
            from_node,
            to_node,
            relation: relation.to_string(),
            weight,
            created: chrono::Utc::now().timestamp(),
        };

        if let Some(ref wal) = self.wal {
            let payload = serde_json::to_vec(&edge)
                .map_err(|e| NxrError::Graph(format!("Serialize: {}", e)))?;
            wal.append(WalOperation::Insert, WalLayer::Graph, &payload)?;
        }

        self.adjacency
            .entry(from_node)
            .or_default()
            .push((to_node, id, weight));
        self.edges.insert(id, edge);
        self.save()?;
        Ok(id)
    }

    pub fn get_node(&self, id: u64) -> Option<&GraphNode> {
        self.nodes.get(&id)
    }

    pub fn find_nodes_by_label(&self, label: &str) -> Vec<&GraphNode> {
        self.label_index
            .get(label)
            .map(|ids| ids.iter().filter_map(|id| self.nodes.get(id)).collect())
            .unwrap_or_default()
    }

    pub fn get_neighbors(&self, node_id: u64, relation: Option<&str>) -> Vec<(u64, f32)> {
        self.adjacency
            .get(&node_id)
            .map(|neighbors| {
                neighbors
                    .iter()
                    .filter(|(_, eid, _)| {
                        relation.map_or(true, |r| {
                            self.edges.get(eid).map(|e| e.relation == r).unwrap_or(false)
                        })
                    })
                    .map(|(to, _, w)| (*to, *w))
                    .collect()
            })
            .unwrap_or_default()
    }

    pub fn traverse(
        &self,
        from_label: &str,
        relation: &str,
        to_label: &str,
    ) -> Vec<(u64, u64, f32)> {
        let mut results = Vec::new();
        if let Some(from_ids) = self.label_index.get(from_label) {
            for &from_id in from_ids {
                if let Some(neighbors) = self.adjacency.get(&from_id) {
                    for &(to_id, edge_id, weight) in neighbors {
                        if let Some(edge) = self.edges.get(&edge_id) {
                            if edge.relation == relation {
                                if let Some(to_node) = self.nodes.get(&to_id) {
                                    if to_node.label == to_label {
                                        results.push((from_id, to_id, weight));
                                    }
                                }
                            }
                        }
                    }
                }
            }
        }
        results
    }

    pub fn remove_node(&mut self, id: u64) -> NxrResult<()> {
        let node = self.nodes.remove(&id)
            .ok_or_else(|| NxrError::NotFound(format!("Node {} not found", id)))?;

        self.label_index.entry(node.label.clone())
            .or_default()
            .retain(|&nid| nid != id);

        let removed_edges: Vec<u64> = self.edges.iter()
            .filter(|(_, e)| e.from_node == id || e.to_node == id)
            .map(|(eid, _)| *eid)
            .collect();

        for eid in removed_edges {
            if let Some(edge) = self.edges.remove(&eid) {
                if let Some(adj) = self.adjacency.get_mut(&edge.from_node) {
                    adj.retain(|(to, _, _)| *to != edge.to_node);
                }
            }
        }

        self.adjacency.remove(&id);
        self.save()?;
        Ok(())
    }

    pub fn node_count(&self) -> usize {
        self.nodes.len()
    }

    pub fn edge_count(&self) -> usize {
        self.edges.len()
    }
}

impl BinaryExport for GraphStore {
    fn export_binary(&self, path: &Path) -> NxrResult<()> {
        let node_list: Vec<&GraphNode> = self.nodes.values().collect();
        let edge_list: Vec<&GraphEdge> = self.edges.values().collect();
        let data = GraphSnapshot {
            nodes: node_list.into_iter().cloned().collect(),
            edges: edge_list.into_iter().cloned().collect(),
            label_index: self.label_index.clone(),
            next_node_id: self.next_node_id,
            next_edge_id: self.next_edge_id,
        };
        let bytes = bincode::serialize(&data)
            .map_err(|e| NxrError::Snapshot(format!("Serialize: {}", e)))?;
        fs::write(path, bytes)?;
        Ok(())
    }

    fn import_binary(&mut self, path: &Path) -> NxrResult<()> {
        if !path.exists() {
            return Ok(());
        }
        let bytes = fs::read(path)?;
        let data: GraphSnapshot = bincode::deserialize(&bytes)
            .map_err(|e| NxrError::Snapshot(format!("Deserialize: {}", e)))?;
        self.nodes = data.nodes.into_iter().map(|n| (n.id, n)).collect();
        self.edges = data.edges.into_iter().map(|e| (e.id, e)).collect();
        self.label_index = data.label_index;

        self.adjacency.clear();
        for (_, edge) in &self.edges {
            self.adjacency
                .entry(edge.from_node)
                .or_default()
                .push((edge.to_node, edge.id, edge.weight));
        }

        self.next_node_id = data.next_node_id;
        self.next_edge_id = data.next_edge_id;
        Ok(())
    }
}

#[derive(Debug, Serialize, Deserialize)]
struct GraphSnapshot {
    nodes: Vec<GraphNode>,
    edges: Vec<GraphEdge>,
    label_index: HashMap<String, Vec<u64>>,
    next_node_id: u64,
    next_edge_id: u64,
}

#[cfg(test)]
mod tests {
    use super::*;

    fn test_config(name: &str) -> Config {
        let mut config = Config::default();
        let tmp = std::env::temp_dir().join(format!("nxr_graph_{}", name));
        let _ = std::fs::remove_dir_all(&tmp);
        std::fs::create_dir_all(&tmp).unwrap();
        config.graph.nodes_path = tmp.join("nodes.dat");
        config.graph.edges_path = tmp.join("edges.dat");
        config.graph.adj_list_path = tmp.join("adj.idx");
        config.graph.labels_path = tmp.join("labels.idx");
        config
    }

    #[test]
    fn test_add_node() {
        let config = test_config("add_node");
        let mut store = GraphStore::new(&config).unwrap();
        let id = store.add_node("User", Vec::new()).unwrap();
        assert_eq!(id, 1);
        assert_eq!(store.node_count(), 1);
    }

    #[test]
    fn test_add_node_with_props() {
        let config = test_config("add_node_props");
        let mut store = GraphStore::new(&config).unwrap();
        let props = vec![("name".into(), "Alice".into())];
        let id = store.add_node("User", props).unwrap();
        let node = store.get_node(id).unwrap();
        assert_eq!(node.label, "User");
        assert_eq!(node.properties.get("name").unwrap(), "Alice");
    }

    #[test]
    fn test_add_edge() {
        let config = test_config("add_edge");
        let mut store = GraphStore::new(&config).unwrap();
        let alice = store.add_node("User", Vec::new()).unwrap();
        let jazz = store.add_node("Topic", Vec::new()).unwrap();
        let edge_id = store.add_edge(alice, jazz, "PREFERS", 0.9).unwrap();
        assert_eq!(edge_id, 1);
        assert_eq!(store.edge_count(), 1);
    }

    #[test]
    fn test_edge_missing_node() {
        let config = test_config("missing_node");
        let mut store = GraphStore::new(&config).unwrap();
        let result = store.add_edge(999, 1000, "LIKES", 1.0);
        assert!(result.is_err());
    }

    #[test]
    fn test_traverse() {
        let config = test_config("traverse");
        let mut store = GraphStore::new(&config).unwrap();
        let alice = store.add_node("User", Vec::new()).unwrap();
        let jazz = store.add_node("Topic", Vec::new()).unwrap();
        store.add_edge(alice, jazz, "PREFERS", 0.9).unwrap();

        let results = store.traverse("User", "PREFERS", "Topic");
        assert_eq!(results.len(), 1);
    }

    #[test]
    fn test_find_by_label() {
        let config = test_config("find_label");
        let mut store = GraphStore::new(&config).unwrap();
        store.add_node("User", Vec::new()).unwrap();
        store.add_node("User", Vec::new()).unwrap();
        store.add_node("Topic", Vec::new()).unwrap();

        let users = store.find_nodes_by_label("User");
        assert_eq!(users.len(), 2);

        let topics = store.find_nodes_by_label("Topic");
        assert_eq!(topics.len(), 1);
    }
}
