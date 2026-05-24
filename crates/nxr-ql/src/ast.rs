use serde::Serialize;

#[derive(Debug, Clone, PartialEq)]
pub enum Statement {
    Match(MatchQuery),
    Select(Vec<String>),
    Insert(InsertStatement),
    Delete(DeleteStatement),
    VectorSearch(VectorSearch),
}

#[derive(Debug, Clone, PartialEq)]
pub struct MatchQuery {
    pub patterns: Vec<PathPattern>,
    pub where_clause: Option<Expression>,
    pub return_fields: Vec<Field>,
    pub order_by: Option<OrderBy>,
    pub limit: Option<usize>,
}

#[derive(Debug, Clone, PartialEq)]
pub struct PathPattern {
    pub from: NodePattern,
    pub edge: EdgePattern,
    pub to: NodePattern,
}

#[derive(Debug, Clone, PartialEq)]
pub struct NodePattern {
    pub alias: String,
    pub label: Option<String>,
}

#[derive(Debug, Clone, PartialEq)]
pub struct EdgePattern {
    pub alias: Option<String>,
    pub relation: Option<String>,
    pub direction: Direction,
}

#[derive(Debug, Clone, PartialEq)]
pub enum Direction {
    Forward,
    Backward,
    Bidirectional,
}

#[derive(Debug, Clone, PartialEq)]
pub struct Field {
    pub name: String,
    pub alias: Option<String>,
}

#[derive(Debug, Clone, PartialEq)]
pub struct OrderBy {
    pub field: String,
    pub descending: bool,
}

#[derive(Debug, Clone, PartialEq)]
pub enum Expression {
    Comparison(String, String, String),
    And(Box<Expression>, Box<Expression>),
    Or(Box<Expression>, Box<Expression>),
    Literal(String),
}

#[derive(Debug, Clone, PartialEq)]
pub struct InsertStatement {
    pub label: String,
    pub properties: Vec<(String, String)>,
}

#[derive(Debug, Clone, PartialEq)]
pub struct DeleteStatement {
    pub pattern: NodePattern,
    pub cascade: bool,
}

#[derive(Debug, Clone, PartialEq)]
pub struct VectorSearch {
    pub query_vector: Vec<f32>,
    pub limit: usize,
    pub filters: Vec<Expression>,
}

#[derive(Debug, Serialize)]
pub struct QueryResult {
    pub columns: Vec<String>,
    pub rows: Vec<Vec<serde_json::Value>>,
    pub row_count: usize,
}
