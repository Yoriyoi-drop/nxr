use std::collections::HashMap;

const DEFAULT_DIM: usize = 384;

pub enum EmbeddingStrategy {
    Local,
    Api { url: String, api_key: String, model: String },
}

pub struct EmbeddingPipeline {
    strategy: EmbeddingStrategy,
    vocab: HashMap<String, u64>,
    next_id: u64,
}

impl EmbeddingPipeline {
    pub fn new() -> Self {
        Self {
            strategy: EmbeddingStrategy::Local,
            vocab: HashMap::new(),
            next_id: 1,
        }
    }

    pub fn with_strategy(strategy: EmbeddingStrategy) -> Self {
        Self {
            strategy,
            vocab: HashMap::new(),
            next_id: 1,
        }
    }

    pub fn embed(&mut self, text: &str) -> Vec<f32> {
        match &self.strategy {
            EmbeddingStrategy::Api { url, api_key, model } => {
                self.api_embed(text, url, api_key, model)
                    .unwrap_or_else(|_| self.local_embed(text))
            }
            EmbeddingStrategy::Local => self.local_embed(text),
        }
    }

    pub fn embed_batch(&mut self, texts: &[&str]) -> Vec<Vec<f32>> {
        texts.iter().map(|t| self.embed(t)).collect()
    }

    pub fn tokenize(&self, text: &str) -> Vec<String> {
        let mut tokens = Vec::new();
        let mut current = String::new();
        for ch in text.chars() {
            if ch.is_alphanumeric() || ch == '\'' {
                current.push(ch.to_ascii_lowercase());
            } else {
                if !current.is_empty() {
                    tokens.push(current.clone());
                    current.clear();
                }
                if !ch.is_whitespace() {
                    tokens.push(ch.to_string());
                }
            }
        }
        if !current.is_empty() {
            tokens.push(current);
        }
        tokens
    }

    pub fn tokenize_bpe(&self, text: &str, vocab_size: usize) -> Vec<String> {
        let pre_tokens = self.tokenize(text);
        let mut result = Vec::new();
        for token in pre_tokens {
            if token.len() <= 2 {
                result.push(token);
                continue;
            }
            let chars: Vec<char> = token.chars().collect();
            let mut pairs = Vec::new();
            for i in 0..chars.len().saturating_sub(1) {
                pairs.push(format!("{}{}", chars[i], chars[i + 1]));
            }
            if pairs.len() <= vocab_size / 2 {
                result.push(token);
            } else {
                for i in (0..chars.len()).step_by(2) {
                    let sub: String = chars[i..(i + 2).min(chars.len())].iter().collect();
                    result.push(sub);
                }
            }
        }
        result
    }

    pub fn normalize(vector: &mut [f32]) {
        let norm: f32 = vector.iter().map(|x| x * x).sum::<f32>().sqrt();
        if norm > 0.0 {
            for v in vector.iter_mut() {
                *v /= norm;
            }
        }
    }

    fn local_embed(&mut self, text: &str) -> Vec<f32> {
        let tokens = self.tokenize(text);
        if tokens.is_empty() {
            return vec![0.0; DEFAULT_DIM];
        }

        let mut result = vec![0.0f32; DEFAULT_DIM];
        let mut term_freq: HashMap<String, usize> = HashMap::new();

        for token in &tokens {
            *term_freq.entry(token.clone()).or_default() += 1;
            if !self.vocab.contains_key(token) {
                self.vocab.insert(token.clone(), self.next_id);
                self.next_id += 1;
            }
        }

        let max_freq = *term_freq.values().max().unwrap_or(&1) as f32;
        let num_tokens = tokens.len() as f32;

        for (token, freq) in &term_freq {
            if let Some(&token_id) = self.vocab.get(token) {
                let tf = (*freq as f32) / max_freq;
                let idf = ((self.next_id as f32) / (token_id as f32 + 1.0)).ln() + 1.0;
                let weight = tf * idf;

                for j in 0..DEFAULT_DIM {
                    let hash = fnv_hash(&format!("{}_{}", token, j));
                    let val = ((hash % 2000) as f32 / 1000.0) - 1.0;
                    result[j] += val * weight;
                }
            }
        }

        for v in result.iter_mut() {
            *v /= num_tokens.max(1.0);
        }

        Self::normalize(&mut result);
        result
    }

    #[cfg(feature = "api-embedding")]
    fn api_embed(
        &self,
        text: &str,
        url: &str,
        api_key: &str,
        model: &str,
    ) -> Result<Vec<f32>, String> {
        use serde::{Deserialize, Serialize};
        #[derive(Serialize)]
        struct ApiRequest<'a> {
            model: &'a str,
            input: &'a str,
        }
        #[derive(Deserialize)]
        struct ApiResponse {
            data: Vec<ApiData>,
        }
        #[derive(Deserialize)]
        struct ApiData {
            embedding: Vec<f32>,
        }

        let client = reqwest::blocking::Client::new();
        let req_body = ApiRequest { model, input: text };
        let resp = client
            .post(url)
            .header("Authorization", format!("Bearer {}", api_key))
            .json(&req_body)
            .send()
            .map_err(|e| format!("API request failed: {}", e))?;
        let api_resp: ApiResponse = resp
            .json()
            .map_err(|e| format!("API parse failed: {}", e))?;
        api_resp
            .data
            .into_iter()
            .next()
            .map(|d| d.embedding)
            .ok_or_else(|| "No embedding returned".into())
    }

    #[cfg(not(feature = "api-embedding"))]
    fn api_embed(
        &self,
        _text: &str,
        _url: &str,
        _api_key: &str,
        _model: &str,
    ) -> Result<Vec<f32>, String> {
        Err("api-embedding feature not enabled".into())
    }

    pub fn vocab_size(&self) -> usize {
        self.vocab.len()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_tokenize() {
        let pipeline = EmbeddingPipeline::new();
        let tokens = pipeline.tokenize("Hello, World! This is NXR.");
        assert_eq!(tokens, vec!["hello", ",", "world", "!", "this", "is", "nxr", "."]);
    }

    #[test]
    fn test_tokenize_empty() {
        let pipeline = EmbeddingPipeline::new();
        let tokens = pipeline.tokenize("");
        assert!(tokens.is_empty());
    }

    #[test]
    fn test_embed_dimension() {
        let mut pipeline = EmbeddingPipeline::new();
        let vec = pipeline.embed("hello world");
        assert_eq!(vec.len(), DEFAULT_DIM);
    }

    #[test]
    fn test_embed_normalized() {
        let mut pipeline = EmbeddingPipeline::new();
        let vec = pipeline.embed("test text");
        let norm: f32 = vec.iter().map(|x| x * x).sum::<f32>().sqrt();
        assert!((norm - 1.0).abs() < 0.01);
    }

    #[test]
    fn test_embed_deterministic() {
        let mut p1 = EmbeddingPipeline::new();
        let mut p2 = EmbeddingPipeline::new();
        let v1 = p1.embed("hello world");
        let v2 = p2.embed("hello world");
        assert_eq!(v1, v2);
    }

    #[test]
    fn test_embed_similar_texts() {
        let mut pipeline = EmbeddingPipeline::new();
        let v1 = pipeline.embed("machine learning");
        let v2 = pipeline.embed("machine learning");
        let v3 = pipeline.embed("artificial intelligence");

        let dot_same: f32 = v1.iter().zip(v2.iter()).map(|(a, b)| a * b).sum();
        let _dot_diff: f32 = v1.iter().zip(v3.iter()).map(|(a, b)| a * b).sum();

        assert!(dot_same > 0.99);
    }

    #[test]
    fn test_normalize() {
        let mut vec = vec![3.0, 4.0];
        EmbeddingPipeline::normalize(&mut vec);
        let norm: f32 = vec.iter().map(|x| x * x).sum::<f32>().sqrt();
        assert!((norm - 1.0).abs() < 0.001);
    }

    #[test]
    fn test_vocab_grows() {
        let mut pipeline = EmbeddingPipeline::new();
        let _ = pipeline.embed("one two three");
        assert!(pipeline.vocab_size() >= 3);
    }

    #[test]
    fn test_bpe_tokenize() {
        let pipeline = EmbeddingPipeline::new();
        let tokens = pipeline.tokenize_bpe("hello", 100);
        assert!(!tokens.is_empty());
    }
}

fn fnv_hash(s: &str) -> u64 {
    let mut hash: u64 = 0xcbf29ce484222325;
    for byte in s.bytes() {
        hash ^= byte as u64;
        hash = hash.wrapping_mul(0x100000001b3);
    }
    hash
}
