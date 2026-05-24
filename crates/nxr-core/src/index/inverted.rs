use std::collections::HashMap;

pub struct InvertedIndex {
    postings: HashMap<String, Vec<(u64, f32)>>,
    total_docs: u64,
}

impl InvertedIndex {
    pub fn new() -> Self {
        Self {
            postings: HashMap::new(),
            total_docs: 0,
        }
    }

    pub fn index_document(&mut self, doc_id: u64, text: &str) {
        self.total_docs += 1;
        let tokens = tokenize(text);
        let mut term_freq: HashMap<String, u32> = HashMap::new();

        for token in &tokens {
            *term_freq.entry(token.clone()).or_default() += 1;
        }

        let max_freq = term_freq.values().max().copied().unwrap_or(1) as f32;

        for (term, freq) in term_freq {
            let tf = (freq as f32) / max_freq;
            self.postings
                .entry(term)
                .or_default()
                .push((doc_id, tf));
        }
    }

    pub fn search(&self, query: &str, top_k: usize) -> Vec<(u64, f32)> {
        let tokens = tokenize(query);
        let mut scores: HashMap<u64, f32> = HashMap::new();

        for token in &tokens {
            if let Some(postings) = self.postings.get(token) {
                let idf = ((self.total_docs as f32) / (postings.len() as f32 + 1.0)).ln() + 1.0;
                for &(doc_id, tf) in postings {
                    *scores.entry(doc_id).or_default() += tf * idf;
                }
            }
        }

        let mut results: Vec<(u64, f32)> = scores.into_iter().collect();
        results.sort_by(|a, b| b.1.partial_cmp(&a.1).unwrap());
        results.truncate(top_k);
        results
    }

    pub fn len(&self) -> usize {
        self.postings.len()
    }
}

pub fn tokenize(text: &str) -> Vec<String> {
    text.split_whitespace()
        .map(|s| {
            s.to_lowercase()
                .trim_matches(|c: char| !c.is_alphanumeric())
                .to_string()
        })
        .filter(|s| !s.is_empty() && s.len() >= 2)
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_empty_index() {
        let idx = InvertedIndex::new();
        assert_eq!(idx.len(), 0);
    }

    #[test]
    fn test_index_and_search() {
        let mut idx = InvertedIndex::new();
        idx.index_document(1, "the quick brown fox");
        idx.index_document(2, "the lazy dog");
        idx.index_document(3, "the quick dog");

        let results = idx.search("quick dog", 10);
        assert!(results.len() >= 2);
        assert!(results.iter().any(|(id, _)| *id == 3));
    }

    #[test]
    fn test_search_top_k() {
        let mut idx = InvertedIndex::new();
        for i in 0..10 {
            idx.index_document(i, &format!("document number {}", i));
        }
        let results = idx.search("document", 3);
        assert_eq!(results.len(), 3);
    }

    #[test]
    fn test_tokenize_empty() {
        let tokens = tokenize("");
        assert!(tokens.is_empty());
    }

    #[test]
    fn test_tokenize_normal() {
        let tokens = tokenize("Hello World! This is TEST.");
        assert!(tokens.contains(&"hello".into()));
        assert!(tokens.contains(&"world".into()));
        assert!(tokens.contains(&"test".into()));
    }
}
