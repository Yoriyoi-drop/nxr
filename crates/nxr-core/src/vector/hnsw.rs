use crate::error::{NxrError, NxrResult};
use rand::SeedableRng;
use std::collections::{HashMap, HashSet};
use std::path::Path;
use std::sync::atomic::{AtomicUsize, Ordering};
use std::sync::Mutex;

#[derive(Debug, Clone)]
struct HnswNode {
    id: u64,
    vector: Vec<f32>,
    level: usize,
    neighbors: Vec<Vec<u64>>,
}

pub struct HnswIndex {
    dimension: usize,
    ef_construction: usize,
    m_max: usize,
    m_max0: usize,
    space: String,
    nodes: HashMap<u64, HnswNode>,
    max_level: AtomicUsize,
    entry_point: AtomicUsize,
    count: AtomicUsize,
    deleted: HashSet<u64>,
    levels: Vec<Vec<u64>>,
    rng: Mutex<rand::rngs::SmallRng>,
}

impl HnswIndex {
    pub fn new(
        dimension: usize,
        ef_construction: usize,
        m_max: usize,
        space: &str,
    ) -> Self {
        let seed = 42u64;
        Self {
            dimension,
            ef_construction,
            m_max,
            m_max0: m_max * 2,
            space: space.to_string(),
            nodes: HashMap::new(),
            max_level: AtomicUsize::new(0),
            entry_point: AtomicUsize::new(0),
            count: AtomicUsize::new(0),
            deleted: HashSet::new(),
            levels: Vec::new(),
            rng: Mutex::new(rand::SeedableRng::seed_from_u64(seed)),
        }
    }

    fn random_level(&self) -> usize {
        let mut rng = self.rng.lock().unwrap();
        use rand::Rng;
        let r: f64 = rng.r#gen();
        let ml = 1.0 / (self.m_max as f64).ln();
        (-r.ln() * ml).floor() as usize
    }

    fn distance(&self, a: &[f32], b: &[f32]) -> f32 {
        match self.space.as_str() {
            "cosine" => {
                let dot: f32 = a.iter().zip(b.iter()).map(|(x, y)| x * y).sum();
                let norm_a: f32 = a.iter().map(|x| x * x).sum::<f32>().sqrt();
                let norm_b: f32 = b.iter().map(|x| x * x).sum::<f32>().sqrt();
                1.0 - (dot / (norm_a * norm_b + 1e-10))
            }
            "euclidean" | _ => {
                let sum: f32 = a.iter().zip(b.iter()).map(|(x, y)| (x - y).powi(2)).sum();
                sum.sqrt()
            }
        }
    }

    fn search_layer(
        &self,
        query: &[f32],
        entry_id: u64,
        ef: usize,
        level: usize,
    ) -> Vec<(u64, f32)> {
        let mut visited = HashSet::new();
        let mut candidates = std::collections::BinaryHeap::new();
        let mut result = Vec::new();

        if let Some(entry) = self.nodes.get(&entry_id) {
            let dist = self.distance(query, &entry.vector);
            candidates.push(std::cmp::Reverse((ordered_float::OrderedFloat(dist), entry_id)));
            visited.insert(entry_id);
            result.push((entry_id, dist));
        }

        while let Some(std::cmp::Reverse((_dist, node_id))) = candidates.pop() {
            if let Some(node) = self.nodes.get(&node_id) {
                let mut found_better = false;
                if level < node.neighbors.len() {
                    for &neighbor_id in &node.neighbors[level] {
                        if visited.insert(neighbor_id) {
                            if let Some(neighbor) = self.nodes.get(&neighbor_id) {
                                let dist = self.distance(query, &neighbor.vector);
                                if result.len() < ef || dist < result.last().map(|r| r.1).unwrap_or(f32::MAX) {
                                    candidates.push(std::cmp::Reverse((
                                        ordered_float::OrderedFloat(dist),
                                        neighbor_id,
                                    )));
                                    result.push((neighbor_id, dist));
                                    result.sort_by(|a, b| a.1.partial_cmp(&b.1).unwrap());
                                    result.truncate(ef);
                                    found_better = true;
                                }
                            }
                        }
                    }
                }
                if !found_better {
                    break;
                }
            }
        }

        result.sort_by(|a, b| a.1.partial_cmp(&b.1).unwrap());
        result
    }

    pub fn insert(&mut self, id: u64, vector: &[f32]) -> NxrResult<()> {
        if vector.len() != self.dimension {
            return Err(NxrError::Vector(format!(
                "Expected {} dims, got {}", self.dimension, vector.len()
            )));
        }

        let level = self.random_level();
        let node = HnswNode {
            id,
            vector: vector.to_vec(),
            level,
            neighbors: vec![Vec::new(); level + 1],
        };

        if self.nodes.is_empty() {
            self.nodes.insert(id, node);
            self.max_level.store(level, Ordering::Relaxed);
            self.entry_point.store(id as usize, Ordering::Relaxed);
            self.count.fetch_add(1, Ordering::Relaxed);
            return Ok(());
        }

        let ep = self.entry_point.load(Ordering::Relaxed) as u64;
        let mut curr_entry = ep;

        let top_level = self.max_level.load(Ordering::Relaxed);
        for lvl in (level + 1..=top_level).rev() {
            let candidates = self.search_layer(vector, curr_entry, 1, lvl);
            if let Some(&(next_id, _)) = candidates.first() {
                curr_entry = next_id;
            }
        }

        self.nodes.insert(id, node);

        for lvl in (0..=level.min(top_level)).rev() {
            let candidates = self.search_layer(vector, curr_entry, self.ef_construction, lvl);
            let m = if lvl == 0 { self.m_max0 } else { self.m_max };
            let selected: Vec<u64> = candidates.iter().take(m).map(|(id, _)| *id).collect();

            if let Some(n) = self.nodes.get_mut(&id) {
                n.neighbors[lvl] = selected.clone();
            }

            for &neighbor_id in &selected {
                let (needs_trunc, neighbor_vec) = {
                    let neighbor = match self.nodes.get(&neighbor_id) {
                        Some(n) => n,
                        None => continue,
                    };
                    let m = self.m_max.max(self.m_max0);
                    (neighbor.neighbors.get(lvl).map_or(false, |n| n.len() > m), neighbor.vector.clone())
                };

                if !needs_trunc {
                    if let Some(neighbor) = self.nodes.get_mut(&neighbor_id) {
                        while neighbor.neighbors.len() <= lvl {
                            neighbor.neighbors.push(Vec::new());
                        }
                        neighbor.neighbors[lvl].push(id);
                    }
                    continue;
                }

                // Collect distances and sort
                let m = self.m_max.max(self.m_max0);
                let neighbor_entry = self.nodes.get(&neighbor_id).unwrap();
                let current_neighbors: Vec<u64> = neighbor_entry.neighbors.get(lvl)
                    .cloned()
                    .unwrap_or_default()
                    .into_iter()
                    .chain(std::iter::once(id))
                    .collect();

                let space = &self.space;
                let mut scored: Vec<(u64, f32)> = current_neighbors
                    .iter()
                    .filter(|&&nid| nid != id)
                    .map(|&nid| {
                        let vec = &self.nodes.get(&nid).unwrap().vector;
                        let dist = Self::calc_distance(space, vec, &neighbor_vec);
                        (nid, dist)
                    })
                    .collect();
                scored.sort_by(|a, b| a.1.partial_cmp(&b.1).unwrap());
                scored.truncate(m);

                if let Some(neighbor) = self.nodes.get_mut(&neighbor_id) {
                    while neighbor.neighbors.len() <= lvl {
                        neighbor.neighbors.push(Vec::new());
                    }
                    neighbor.neighbors[lvl] = scored.into_iter().map(|(id, _)| id).collect();
                }
            }
            curr_entry = candidates.first().map(|(id, _)| *id).unwrap_or(curr_entry);
        }

        if level > top_level {
            self.max_level.store(level, Ordering::Relaxed);
        }
        self.count.fetch_add(1, Ordering::Relaxed);
        Ok(())
    }

    pub fn search(&self, query: &[f32], k: usize) -> NxrResult<Vec<(u64, f32)>> {
        if self.nodes.is_empty() {
            return Ok(Vec::new());
        }

        let ep = self.entry_point.load(Ordering::Relaxed) as u64;
        let mut curr_entry = ep;
        let top_level = self.max_level.load(Ordering::Relaxed);

        for lvl in (1..=top_level).rev() {
            let candidates = self.search_layer(query, curr_entry, 1, lvl);
            if let Some(&(next_id, _)) = candidates.first() {
                curr_entry = next_id;
            }
        }

        let candidates = self.search_layer(query, curr_entry, k.max(self.ef_construction), 0);
        let results: Vec<(u64, f32)> = candidates
            .into_iter()
            .filter(|(id, _)| !self.deleted.contains(id))
            .take(k)
            .collect();

        Ok(results)
    }

    fn calc_distance(space: &str, a: &[f32], b: &[f32]) -> f32 {
        match space {
            "cosine" => {
                let dot: f32 = a.iter().zip(b.iter()).map(|(x, y)| x * y).sum();
                let norm_a: f32 = a.iter().map(|x| x * x).sum::<f32>().sqrt();
                let norm_b: f32 = b.iter().map(|x| x * x).sum::<f32>().sqrt();
                1.0 - (dot / (norm_a * norm_b + 1e-10))
            }
            _ => {
                let sum: f32 = a.iter().zip(b.iter()).map(|(x, y)| (x - y).powi(2)).sum();
                sum.sqrt()
            }
        }
    }

    pub fn delete(&mut self, id: u64) -> NxrResult<()> {
        if self.nodes.contains_key(&id) {
            self.deleted.insert(id);
            self.count.fetch_sub(1, Ordering::Relaxed);
            Ok(())
        } else {
            Err(NxrError::NotFound(format!("Vector {} not found", id)))
        }
    }

    pub fn len(&self) -> usize {
        self.count.load(Ordering::Relaxed)
    }

    pub fn is_empty(&self) -> bool {
        self.len() == 0
    }

    pub fn save<P: AsRef<Path>>(&self, path: P) -> NxrResult<()> {
        use std::io::Write;
        let mut buf = Vec::new();

        buf.extend_from_slice(b"HNSW");
        buf.extend_from_slice(&1u32.to_le_bytes());
        buf.extend_from_slice(&(self.dimension as u32).to_le_bytes());
        buf.extend_from_slice(&(self.ef_construction as u32).to_le_bytes());
        buf.extend_from_slice(&(self.m_max as u32).to_le_bytes());
        buf.extend_from_slice(&(self.m_max0 as u32).to_le_bytes());
        let space_bytes = self.space.as_bytes();
        buf.extend_from_slice(&(space_bytes.len() as u32).to_le_bytes());
        buf.extend_from_slice(space_bytes);
        buf.extend_from_slice(&(self.entry_point.load(Ordering::Relaxed) as u64).to_le_bytes());
        buf.extend_from_slice(&(self.max_level.load(Ordering::Relaxed) as u64).to_le_bytes());
        buf.extend_from_slice(&(self.count.load(Ordering::Relaxed) as u64).to_le_bytes());

        let mut node_ids: Vec<u64> = self.nodes.keys().copied().collect();
        node_ids.sort();
        buf.extend_from_slice(&(node_ids.len() as u32).to_le_bytes());
        for &id in &node_ids {
            let node = &self.nodes[&id];
            buf.extend_from_slice(&id.to_le_bytes());
            buf.extend_from_slice(&(node.level as u64).to_le_bytes());
            for &v in &node.vector {
                buf.extend_from_slice(&v.to_le_bytes());
            }
            buf.extend_from_slice(&(node.neighbors.len() as u32).to_le_bytes());
            for level in &node.neighbors {
                buf.extend_from_slice(&(level.len() as u32).to_le_bytes());
                for &nid in level {
                    buf.extend_from_slice(&nid.to_le_bytes());
                }
            }
        }

        let deleted_ids: Vec<u64> = self.deleted.iter().copied().collect();
        buf.extend_from_slice(&(deleted_ids.len() as u32).to_le_bytes());
        for &id in &deleted_ids {
            buf.extend_from_slice(&id.to_le_bytes());
        }

        if let Some(parent) = path.as_ref().parent() {
            std::fs::create_dir_all(parent)
                .map_err(|e| NxrError::Io(e))?;
        }
        let mut file = std::fs::File::create(path.as_ref())
            .map_err(|e| NxrError::Io(e))?;
        file.write_all(&buf).map_err(|e| NxrError::Io(e))?;
        Ok(())
    }

    pub fn load<P: AsRef<Path>>(path: P) -> NxrResult<Self> {
        use std::io::Read;
        let mut buf = Vec::new();
        std::fs::File::open(path.as_ref())
            .map_err(|e| NxrError::Io(e))?
            .read_to_end(&mut buf)
            .map_err(|e| NxrError::Io(e))?;

        let mut pos = 0;
        let read_u32 = |buf: &[u8], pos: &mut usize| -> u32 {
            let v = u32::from_le_bytes(buf[*pos..*pos + 4].try_into().unwrap());
            *pos += 4;
            v
        };
        let read_u64 = |buf: &[u8], pos: &mut usize| -> u64 {
            let v = u64::from_le_bytes(buf[*pos..*pos + 8].try_into().unwrap());
            *pos += 8;
            v
        };

        let magic = &buf[pos..pos + 4];
        pos += 4;
        if magic != b"HNSW" {
            return Err(NxrError::Vector("Invalid HNSW magic bytes".into()));
        }
        let _version = read_u32(&buf, &mut pos);
        let dimension = read_u32(&buf, &mut pos) as usize;
        let ef_construction = read_u32(&buf, &mut pos) as usize;
        let m_max = read_u32(&buf, &mut pos) as usize;
        let m_max0 = read_u32(&buf, &mut pos) as usize;
        let space_len = read_u32(&buf, &mut pos) as usize;
        let space = String::from_utf8(buf[pos..pos + space_len].to_vec())
            .map_err(|e| NxrError::Vector(format!("Invalid space name: {}", e)))?;
        pos += space_len;
        let entry_point = read_u64(&buf, &mut pos);
        let max_level = read_u64(&buf, &mut pos) as usize;
        let count = read_u64(&buf, &mut pos) as usize;

        let mut nodes = HashMap::new();
        let num_nodes = read_u32(&buf, &mut pos) as usize;
        for _ in 0..num_nodes {
            let id = read_u64(&buf, &mut pos);
            let level = read_u64(&buf, &mut pos) as usize;
            let mut vector = Vec::with_capacity(dimension);
            for _ in 0..dimension {
                let v = f32::from_le_bytes(buf[pos..pos + 4].try_into().unwrap());
                pos += 4;
                vector.push(v);
            }
            let num_levels = read_u32(&buf, &mut pos) as usize;
            let mut neighbors = Vec::with_capacity(num_levels);
            for _ in 0..num_levels {
                let num_nbrs = read_u32(&buf, &mut pos) as usize;
                let mut level_nbrs = Vec::with_capacity(num_nbrs);
                for _ in 0..num_nbrs {
                    level_nbrs.push(read_u64(&buf, &mut pos));
                }
                neighbors.push(level_nbrs);
            }
            nodes.insert(id, HnswNode { id, vector, level, neighbors });
        }

        let num_deleted = read_u32(&buf, &mut pos) as usize;
        let mut deleted = HashSet::new();
        for _ in 0..num_deleted {
            deleted.insert(read_u64(&buf, &mut pos));
        }

        Ok(Self {
            dimension,
            ef_construction,
            m_max,
            m_max0,
            space,
            nodes,
            max_level: AtomicUsize::new(max_level),
            entry_point: AtomicUsize::new(entry_point as usize),
            count: AtomicUsize::new(count),
            deleted,
            levels: Vec::new(),
            rng: Mutex::new(rand::SeedableRng::seed_from_u64(42)),
        })
    }
}

mod ordered_float {
    #[derive(Debug, Clone, Copy, PartialEq)]
    pub struct OrderedFloat(pub f32);

    impl Eq for OrderedFloat {}

    impl PartialOrd for OrderedFloat {
        fn partial_cmp(&self, other: &Self) -> Option<std::cmp::Ordering> {
            Some(self.cmp(other))
        }
    }

    impl Ord for OrderedFloat {
        fn cmp(&self, other: &Self) -> std::cmp::Ordering {
            self.0.partial_cmp(&other.0).unwrap_or(std::cmp::Ordering::Equal)
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_empty_index() {
        let idx = HnswIndex::new(4, 200, 16, "cosine");
        assert!(idx.is_empty());
        assert_eq!(idx.len(), 0);
    }

    #[test]
    fn test_insert_and_search() {
        let mut idx = HnswIndex::new(4, 200, 16, "cosine");
        idx.insert(1, &[1.0, 0.0, 0.0, 0.0]).unwrap();
        idx.insert(2, &[0.0, 1.0, 0.0, 0.0]).unwrap();
        idx.insert(3, &[0.0, 0.0, 1.0, 0.0]).unwrap();
        idx.insert(4, &[0.0, 0.0, 0.0, 1.0]).unwrap();

        assert_eq!(idx.len(), 4);
        assert!(!idx.is_empty());

        let results = idx.search(&[1.0, 0.0, 0.0, 0.0], 2).unwrap();
        assert!(!results.is_empty());
        assert_eq!(results[0].0, 1);
        assert!(results[0].1 < 0.01);
    }

    #[test]
    fn test_euclidean_distance() {
        let mut idx = HnswIndex::new(2, 200, 16, "euclidean");
        idx.insert(1, &[0.0, 0.0]).unwrap();
        idx.insert(2, &[3.0, 4.0]).unwrap();

        let results = idx.search(&[0.0, 0.0], 2).unwrap();
        assert_eq!(results[0].0, 1);
        assert!((results[0].1 - 0.0).abs() < 0.01);
    }

    #[test]
    fn test_delete() {
        let mut idx = HnswIndex::new(4, 200, 16, "cosine");
        idx.insert(1, &[1.0, 0.0, 0.0, 0.0]).unwrap();
        idx.insert(2, &[0.0, 1.0, 0.0, 0.0]).unwrap();

        assert_eq!(idx.len(), 2);
        idx.delete(1).unwrap();
        assert_eq!(idx.len(), 1);

        let results = idx.search(&[1.0, 0.0, 0.0, 0.0], 5).unwrap();
        assert!(!results.iter().any(|(id, _)| *id == 1));
    }

    #[test]
    fn test_dimension_mismatch() {
        let mut idx = HnswIndex::new(3, 200, 16, "cosine");
        let result = idx.insert(1, &[1.0, 0.0]);
        assert!(result.is_err());
    }

    #[test]
    fn test_large_insert() {
        let mut idx = HnswIndex::new(8, 100, 8, "cosine");
        for i in 0..100u64 {
            let v: Vec<f32> = (0..8).map(|j| ((i * 8 + j) as f32) / 100.0).collect();
            idx.insert(i, &v).unwrap();
        }
        assert_eq!(idx.len(), 100);

        let query: Vec<f32> = (0..8).map(|j| j as f32 / 10.0).collect();
        let results = idx.search(&query, 5).unwrap();
        assert_eq!(results.len(), 5);
    }
}
