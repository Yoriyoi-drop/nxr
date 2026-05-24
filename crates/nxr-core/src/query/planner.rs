use super::parser::{ParsedQuery, QueryType};
use crate::error::NxrResult;

#[derive(Debug)]
pub enum PlanStep {
    VectorSearch {
        query_text: String,
        limit: usize,
    },
    GraphTraverse {
        from_label: String,
        relation: String,
        to_label: String,
    },
    KvLookup {
        key: String,
    },
    GraphInsertNode {
        label: String,
        properties: Vec<(String, String)>,
    },
    GraphDeleteNode {
        label: Option<String>,
        alias: String,
    },
    MergeResults {
        strategy: String,
        source_count: usize,
    },
    Filter {
        condition: String,
    },
    Sort {
        field: String,
        descending: bool,
    },
    Limit {
        count: usize,
    },
}

#[derive(Debug)]
pub struct QueryPlan {
    pub steps: Vec<PlanStep>,
}

pub struct QueryPlanner;

impl QueryPlanner {
    pub fn plan(&self, parsed: &ParsedQuery) -> NxrResult<QueryPlan> {
        let mut steps = Vec::new();
        let mut source_count: usize = 0;

        match parsed.query_type {
            QueryType::Match => {
                if !parsed.edge_patterns.is_empty() {
                    for edge in &parsed.edge_patterns {
                        steps.push(PlanStep::GraphTraverse {
                            from_label: parsed.match_clauses.first()
                                .and_then(|m| m.label.clone())
                                .unwrap_or_default(),
                            relation: edge.relation.clone().unwrap_or_default(),
                            to_label: parsed.match_clauses.get(1)
                                .and_then(|m| m.label.clone())
                                .unwrap_or_default(),
                        });
                        source_count += 1;
                    }
                } else if !parsed.match_clauses.is_empty() {
                    // Simple node match: no edges, use first node as KV lookup
                    let alias = &parsed.match_clauses[0].node_alias;
                    steps.push(PlanStep::KvLookup {
                        key: alias.clone(),
                    });
                    source_count += 1;
                }
            }
            QueryType::Insert => {
                if let Some(ref label) = parsed.insert_label {
                    steps.push(PlanStep::GraphInsertNode {
                        label: label.clone(),
                        properties: parsed.insert_properties.clone(),
                    });
                    source_count += 1;
                }
            }
            QueryType::Delete => {
                steps.push(PlanStep::GraphDeleteNode {
                    alias: parsed.delete_alias.clone().unwrap_or_default(),
                    label: parsed.delete_label.clone(),
                });
                source_count += 1;
            }
            QueryType::Select => {
                if let Some(ref text) = parsed.where_clause {
                    steps.push(PlanStep::VectorSearch {
                        query_text: text.clone(),
                        limit: parsed.limit.unwrap_or(10),
                    });
                    source_count += 1;
                }
            }
        }

        // Add merge step if multiple sources
        if source_count > 1 {
            steps.push(PlanStep::MergeResults {
                strategy: "union".into(),
                source_count,
            });
        }

        if let Some(ref order) = parsed.order_by {
            steps.push(PlanStep::Sort {
                field: order.field.clone(),
                descending: order.descending,
            });
        }

        if let Some(limit) = parsed.limit {
            steps.push(PlanStep::Limit { count: limit });
        }

        Ok(QueryPlan { steps })
    }
}
