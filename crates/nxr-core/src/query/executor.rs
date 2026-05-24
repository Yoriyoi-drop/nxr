use crate::error::{NxrError, NxrResult};
use crate::graph::store::GraphStore;
use crate::kv::KvCache;
use crate::vector::VectorEngine;
use super::planner::{PlanStep, QueryPlan};
use serde::Serialize;

#[derive(Debug, Serialize)]
pub struct QueryResult {
    pub rows: Vec<Vec<serde_json::Value>>,
    pub columns: Vec<String>,
    pub row_count: usize,
    pub elapsed_ms: u64,
    pub message: Option<String>,
}

pub struct QueryExecutor;

impl QueryExecutor {
    pub fn execute(
        &self,
        plan: &QueryPlan,
        vector: &VectorEngine,
        graph: &mut GraphStore,
        kv: &KvCache,
    ) -> NxrResult<QueryResult> {
        let start = std::time::Instant::now();
        let mut rows: Vec<Vec<serde_json::Value>> = Vec::new();
        let mut columns: Vec<String> = Vec::new();
        let mut message: Option<String> = None;

        for step in &plan.steps {
            match step {
                PlanStep::GraphTraverse { from_label, relation, to_label } => {
                    columns = vec![
                        "from_id".into(),
                        "to_id".into(),
                        "weight".into(),
                    ];
                    let results = graph.traverse(from_label, relation, to_label);
                    rows = results
                        .into_iter()
                        .map(|(f, t, w)| {
                            vec![
                                serde_json::Value::Number((f as i64).into()),
                                serde_json::Value::Number((t as i64).into()),
                                serde_json::Value::Number(
                                    serde_json::Number::from_f64(w as f64).unwrap_or(0.into()),
                                ),
                            ]
                        })
                        .collect();
                }
                PlanStep::VectorSearch { query_text, limit } => {
                    columns = vec!["id".into(), "similarity".into()];
                    if let Ok(query_vec) = parse_vector(query_text) {
                        let results = vector.search(&query_vec, *limit)?;
                        rows = results
                            .into_iter()
                            .map(|(id, score)| {
                                vec![
                                    serde_json::Value::Number((id as i64).into()),
                                    serde_json::Value::Number(
                                        serde_json::Number::from_f64(score as f64)
                                            .unwrap_or(0.into()),
                                    ),
                                ]
                            })
                            .collect();
                    } else {
                        return Err(NxrError::Query(
                            "Vector query must be an array of floats".into(),
                        ));
                    }
                }
                PlanStep::KvLookup { key } => {
                    columns = vec!["key".into(), "value".into()];
                    if let Some(val) = kv.get(key)? {
                        let val_str = String::from_utf8_lossy(&val).to_string();
                        rows.push(vec![
                            serde_json::Value::String(key.clone()),
                            serde_json::Value::String(val_str),
                        ]);
                    }
                }
                PlanStep::GraphInsertNode { label, properties } => {
                    graph.add_node(label, properties.clone());
                    message = Some(format!("Created node with label '{}'", label));
                }
                PlanStep::GraphDeleteNode { alias: _, label } => {
                    let node_ids: Vec<u64> = graph
                        .find_nodes_by_label(label.as_deref().unwrap_or(""))
                        .iter()
                        .map(|n| n.id)
                        .collect();
                    for id in &node_ids {
                        let _ = graph.remove_node(*id);
                    }
                    message = Some(format!("Deleted {} node(s)", node_ids.len()));
                }
                PlanStep::Filter { condition: _ } => {}
                PlanStep::Sort { field: _, descending } => {
                    if rows.len() > 1 && columns.len() > 1 {
                        if *descending {
                            rows.sort_by(|a, b| {
                                let va = a.get(1).and_then(|v| v.as_f64()).unwrap_or(0.0);
                                let vb = b.get(1).and_then(|v| v.as_f64()).unwrap_or(0.0);
                                vb.partial_cmp(&va).unwrap_or(std::cmp::Ordering::Equal)
                            });
                        } else {
                            rows.sort_by(|a, b| {
                                let va = a.get(1).and_then(|v| v.as_f64()).unwrap_or(0.0);
                                let vb = b.get(1).and_then(|v| v.as_f64()).unwrap_or(0.0);
                                va.partial_cmp(&vb).unwrap_or(std::cmp::Ordering::Equal)
                            });
                        }
                    }
                }
                PlanStep::Limit { count } => {
                    rows.truncate(*count);
                }
                PlanStep::MergeResults { strategy, source_count } => {
                    if rows.is_empty() || columns.is_empty() {
                        continue;
                    }
                    match strategy.as_str() {
                        "union" => {
                            // rows already accumulated from sequential execution
                        }
                        "intersection" => {
                            // Keep rows whose first column (id) appears in all source groups
                            let n_sources = *source_count;
                            if n_sources <= 1 {
                                continue;
                            }
                            let chunk_size = rows.len() / n_sources;
                            if chunk_size == 0 {
                                continue;
                            }
                            // Collect ids from first source
                            let first_ids: Vec<serde_json::Value> = rows[..chunk_size]
                                .iter()
                                .filter_map(|r| r.first().cloned())
                                .collect();
                            let mut merged: Vec<Vec<serde_json::Value>> = Vec::new();
                            for id_val in &first_ids {
                                let mut all_present = true;
                                for s in 1..n_sources {
                                    let start = s * chunk_size;
                                    let end = (start + chunk_size).min(rows.len());
                                    let found = rows[start..end]
                                        .iter()
                                        .any(|r| r.first() == Some(id_val));
                                    if !found {
                                        all_present = false;
                                        break;
                                    }
                                }
                                if all_present {
                                    if let Some(row) = rows.iter().find(|r| r.first() == Some(id_val)) {
                                        merged.push(row.clone());
                                    }
                                }
                            }
                            rows = merged;
                        }
                        "weighted" => {
                            // Average scores from duplicate ids
                            let mut id_map: std::collections::HashMap<String, (Vec<serde_json::Value>, usize)> = std::collections::HashMap::new();
                            for row in &rows {
                                if let Some(id_val) = row.first() {
                                    let key = serde_json::to_string(id_val).unwrap_or_default();
                                    let entry = id_map.entry(key).or_insert_with(|| (row.clone(), 0));
                                    entry.1 += 1;
                                    // Average numeric fields
                                    for (i, val) in row.iter().enumerate() {
                                        if let (Some(a), Some(b)) = (entry.0.get(i).and_then(|v| v.as_f64()), val.as_f64()) {
                                            if i > 0 {
                                                entry.0[i] = serde_json::Value::Number(
                                                    serde_json::Number::from_f64((a + b) / 2.0).unwrap_or(0.into())
                                                );
                                            }
                                        }
                                    }
                                }
                            }
                            rows = id_map.into_values().map(|(r, _)| r).collect();
                        }
                        _ => {}
                    }
                }
            }
        }

        let elapsed = start.elapsed().as_millis() as u64;
        let row_count = rows.len();

        Ok(QueryResult {
            rows,
            columns,
            row_count,
            elapsed_ms: elapsed,
            message,
        })
    }
}

fn parse_vector(input: &str) -> Result<Vec<f32>, String> {
    let trimmed = input.trim_matches(|c| c == '[' || c == ']' || c == '(' || c == ')');
    trimmed
        .split(',')
        .map(|s| {
            s.trim()
                .parse::<f32>()
                .map_err(|e| format!("Float parse error: {}", e))
        })
        .collect()
}
