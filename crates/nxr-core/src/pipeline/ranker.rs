use crate::vector::VectorEngine;
use super::context::Message;
use std::collections::HashMap;

pub struct MemoryRanker {
    similarity_weight: f32,
    recency_weight: f32,
    importance_weight: f32,
    last_access: HashMap<u64, i64>,
    access_count: HashMap<u64, u64>,
    max_access_count: u64,
}

impl MemoryRanker {
    pub fn new(similarity_weight: f32, recency_weight: f32, importance_weight: f32) -> Self {
        Self {
            similarity_weight,
            recency_weight,
            importance_weight,
            last_access: HashMap::new(),
            access_count: HashMap::new(),
            max_access_count: 0,
        }
    }

    pub fn rank(
        &self,
        query: &str,
        vector: &VectorEngine,
        _context: Vec<&Message>,
    ) -> Vec<(u64, f32)> {
        let now = chrono::Utc::now().timestamp();

        if let Ok(query_vec) = simple_embed(query) {
            if let Ok(results) = vector.search(&query_vec, 50) {
                let mut scored: Vec<(u64, f32)> = results
                    .into_iter()
                    .map(|(id, similarity)| {
                        let recency = self.compute_recency(id, now);
                        let importance = self.compute_importance(id);
                        let score = similarity * self.similarity_weight
                            + recency * self.recency_weight
                            + importance * self.importance_weight;
                        (id, score)
                    })
                    .collect();

                scored.sort_by(|a, b| b.1.partial_cmp(&a.1).unwrap());
                return scored;
            }
        }

        Vec::new()
    }

    /// Record that a memory was accessed (for recency/importance tracking)
    pub fn record_access(&mut self, id: u64) {
        let now = chrono::Utc::now().timestamp();
        self.last_access.insert(id, now);
        let count = self.access_count.get(&id).copied().unwrap_or(0) + 1;
        self.access_count.insert(id, count);
        if count > self.max_access_count {
            self.max_access_count = count;
        }
    }

    /// Record accesses from search results
    pub fn record_accesses(&mut self, ids: &[u64]) {
        for &id in ids {
            self.record_access(id);
        }
    }

    fn compute_recency(&self, id: u64, now: i64) -> f32 {
        match self.last_access.get(&id) {
            Some(&last) => {
                let elapsed = (now - last).max(0) as f64;
                let decay_days = 30.0;
                let decay_secs = decay_days * 86400.0;
                (1.0 - (elapsed / decay_secs).min(1.0)) as f32
            }
            None => 0.0,
        }
    }

    fn compute_importance(&self, id: u64) -> f32 {
        let count = self.access_count.get(&id).copied().unwrap_or(0);
        if self.max_access_count == 0 {
            return 0.0;
        }
        count as f32 / self.max_access_count as f32
    }
}

fn simple_embed(text: &str) -> Result<Vec<f32>, ()> {
    let mut vec = vec![0.0f32; 128];
    for (i, ch) in text.bytes().enumerate() {
        vec[i % 128] += ch as f32 / 255.0;
    }
    let norm: f32 = vec.iter().map(|x| x * x).sum::<f32>().sqrt();
    if norm > 0.0 {
        for v in vec.iter_mut() {
            *v /= norm;
        }
    }
    Ok(vec)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::config::Config;

    fn setup_ranker() -> (MemoryRanker, VectorEngine) {
        let mut config = Config::default();
        config.vector.dimension = 4;
        config.vector.segments_path = std::env::temp_dir().join("nxr_ranker_test").join("segments");
        config.vector.hnsw_path = std::env::temp_dir().join("nxr_ranker_test").join("index.hnsw");

        let vector = VectorEngine::new(&config).unwrap();
        let ranker = MemoryRanker::new(0.6, 0.25, 0.15);
        (ranker, vector)
    }

    #[test]
    fn test_record_access() {
        let (mut ranker, _) = setup_ranker();
        ranker.record_access(1);
        ranker.record_access(1);
        ranker.record_access(2);

        let now = chrono::Utc::now().timestamp();
        let recency_1 = ranker.compute_recency(1, now);
        let importance_1 = ranker.compute_importance(1);
        let importance_2 = ranker.compute_importance(2);

        assert!(recency_1 > 0.9, "Recently accessed should have high recency");
        assert!(importance_1 >= importance_2, "More accesses = higher importance");
    }

    #[test]
    fn test_recency_decay() {
        let mut ranker = MemoryRanker::new(0.6, 0.25, 0.15);
        ranker.record_access(1);

        // Simulate 15 days later
        let future = chrono::Utc::now().timestamp() + 15 * 86400;
        let recency = ranker.compute_recency(1, future);
        assert!(recency < 0.6, "15 days later should have decayed");
        assert!(recency > 0.4, "15 days should be ~50% decay");

        // Simulate 60 days later (beyond decay window)
        let far_future = chrono::Utc::now().timestamp() + 60 * 86400;
        let recency_far = ranker.compute_recency(1, far_future);
        assert!(recency_far < 0.1, "60 days later should be nearly 0");
    }

    #[test]
    fn test_importance_normalized() {
        let mut ranker = MemoryRanker::new(0.6, 0.25, 0.15);
        ranker.record_access(1);
        ranker.record_access(1);
        ranker.record_access(1);
        ranker.record_access(2);

        let imp_1 = ranker.compute_importance(1);
        let imp_2 = ranker.compute_importance(2);

        assert!((imp_1 - 1.0).abs() < 0.01, "Most accessed should be 1.0");
        assert!((imp_2 - 1.0 / 3.0).abs() < 0.01, "1 access vs 3 max should be ~0.33");

        // Id with no access
        let imp_3 = ranker.compute_importance(3);
        assert!((imp_3 - 0.0).abs() < 0.01, "No access should be 0");
    }
}
