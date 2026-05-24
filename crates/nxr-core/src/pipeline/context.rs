use std::collections::VecDeque;

#[derive(Debug, Clone)]
pub struct Message {
    pub role: String,
    pub content: String,
    pub token_count: usize,
    pub timestamp: i64,
}

pub struct ContextManager {
    max_tokens: u32,
    messages: VecDeque<Message>,
    strategy: ContextStrategy,
}

pub enum ContextStrategy {
    SlidingWindow,
    SummarizeOld,
}

impl ContextManager {
    pub fn new(max_tokens: u32) -> Self {
        Self {
            max_tokens,
            messages: VecDeque::new(),
            strategy: ContextStrategy::SlidingWindow,
        }
    }

    pub fn add_message(&mut self, role: &str, content: &str) {
        let token_count = estimate_tokens(content);
        let msg = Message {
            role: role.to_string(),
            content: content.to_string(),
            token_count,
            timestamp: chrono::Utc::now().timestamp(),
        };

        self.messages.push_back(msg);
        self.enforce_limit();
    }

    pub fn get_context(&self) -> Vec<&Message> {
        self.messages.iter().collect()
    }

    pub fn get_context_text(&self) -> String {
        self.messages
            .iter()
            .map(|m| format!("{}: {}", m.role, m.content))
            .collect::<Vec<_>>()
            .join("\n")
    }

    fn enforce_limit(&mut self) {
        let total: usize = self.messages.iter().map(|m| m.token_count).sum();

        while total > self.max_tokens as usize && self.messages.len() > 1 {
            match self.strategy {
                ContextStrategy::SlidingWindow => {
                    self.messages.pop_front();
                }
                ContextStrategy::SummarizeOld => {
                    if self.messages.len() > 2 {
                        let first = self.messages.pop_front().unwrap();
                        if self.messages.len() > 1 {
                            let second = self.messages.pop_front().unwrap();
                            let summary = format!(
                                "[Summarized: {} + {}]",
                                truncate(&first.content, 50),
                                truncate(&second.content, 50)
                            );
                            let token_count = estimate_tokens(&summary);
                            self.messages.push_front(Message {
                                role: "system".into(),
                                content: summary,
                                token_count,
                                timestamp: first.timestamp,
                            });
                        }
                    } else {
                        self.messages.pop_front();
                    }
                }
            }
        }
    }

    pub fn clear(&mut self) {
        self.messages.clear();
    }

    pub fn len(&self) -> usize {
        self.messages.len()
    }
}

fn estimate_tokens(text: &str) -> usize {
    // Rough estimate: 1 token ≈ 4 chars
    text.len() / 4 + 1
}

fn truncate(text: &str, max_chars: usize) -> String {
    if text.len() <= max_chars {
        text.to_string()
    } else {
        format!("{}...", &text[..max_chars])
    }
}
