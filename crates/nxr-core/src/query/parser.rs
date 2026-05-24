use crate::error::{NxrError, NxrResult};

#[derive(Debug, Clone)]
pub enum QueryType {
    Match,
    Select,
    Insert,
    Delete,
}

#[derive(Debug, Clone)]
pub struct MatchClause {
    pub node_alias: String,
    pub label: Option<String>,
    pub conditions: Vec<(String, String, String)>,
}

#[derive(Debug, Clone)]
pub struct EdgePattern {
    pub from_alias: String,
    pub to_alias: String,
    pub relation: Option<String>,
    pub direction: String,
}

#[derive(Debug, Clone)]
pub struct SelectField {
    pub name: String,
    pub alias: Option<String>,
}

#[derive(Debug, Clone)]
pub struct OrderBy {
    pub field: String,
    pub descending: bool,
}

#[derive(Debug, Clone)]
pub struct ParsedQuery {
    pub query_type: QueryType,
    pub match_clauses: Vec<MatchClause>,
    pub edge_patterns: Vec<EdgePattern>,
    pub select_fields: Vec<SelectField>,
    pub where_clause: Option<String>,
    pub order_by: Option<OrderBy>,
    pub limit: Option<usize>,
    pub return_fields: Vec<String>,
    pub insert_label: Option<String>,
    pub insert_properties: Vec<(String, String)>,
    pub delete_alias: Option<String>,
    pub delete_label: Option<String>,
}

pub struct NxrQlParser;

impl NxrQlParser {
    pub fn parse(&self, input: &str) -> NxrResult<ParsedQuery> {
        let trimmed = input.trim();
        if trimmed.is_empty() {
            return Err(NxrError::Query("Empty query".into()));
        }

        let upper = trimmed.to_uppercase();

        if upper.starts_with("SELECT") || upper.starts_with("INSERT") || upper.starts_with("DELETE") || upper.starts_with("MATCH") {
            self.parse_nxrql(trimmed)
        } else {
            self.parse_semantic(trimmed)
        }
    }

    fn parse_nxrql(&self, input: &str) -> NxrResult<ParsedQuery> {
        let stmt = nxr_ql::NxrQlParser::parse(input)
            .map_err(|e| NxrError::Query(format!("{}", e)))?;

        match stmt {
            nxr_ql::ast::Statement::Match(q) => self.convert_match(q),
            nxr_ql::ast::Statement::Select(fields) => self.convert_select(fields),
            nxr_ql::ast::Statement::Insert(q) => self.convert_insert(q),
            nxr_ql::ast::Statement::Delete(q) => self.convert_delete(q),
            nxr_ql::ast::Statement::VectorSearch(q) => self.convert_vector_search(q),
        }
    }

    fn convert_match(&self, q: nxr_ql::ast::MatchQuery) -> NxrResult<ParsedQuery> {
        let mut match_clauses = Vec::new();
        let mut edge_patterns = Vec::new();

        for p in &q.patterns {
            match_clauses.push(MatchClause {
                node_alias: p.from.alias.clone(),
                label: p.from.label.clone(),
                conditions: Vec::new(),
            });
            match_clauses.push(MatchClause {
                node_alias: p.to.alias.clone(),
                label: p.to.label.clone(),
                conditions: Vec::new(),
            });

            let direction = match p.edge.direction {
                nxr_ql::ast::Direction::Forward => "->",
                nxr_ql::ast::Direction::Backward => "<-",
                nxr_ql::ast::Direction::Bidirectional => "<->",
            };

            edge_patterns.push(EdgePattern {
                from_alias: p.from.alias.clone(),
                to_alias: p.to.alias.clone(),
                relation: p.edge.relation.clone(),
                direction: direction.to_string(),
            });
        }

        let where_clause = q.where_clause.map(|e| format!("{:?}", e));
        let return_fields: Vec<String> = q.return_fields.iter().map(|f| f.name.clone()).collect();
        let order_by = q.order_by.map(|o| OrderBy {
            field: o.field,
            descending: o.descending,
        });

        Ok(ParsedQuery {
            query_type: QueryType::Match,
            match_clauses,
            edge_patterns,
            select_fields: Vec::new(),
            where_clause,
            order_by,
            limit: q.limit,
            return_fields,
            insert_label: None,
            insert_properties: Vec::new(),
            delete_alias: None,
            delete_label: None,
        })
    }

    fn convert_select(&self, fields: Vec<String>) -> NxrResult<ParsedQuery> {
        Ok(ParsedQuery {
            query_type: QueryType::Select,
            match_clauses: Vec::new(),
            edge_patterns: Vec::new(),
            select_fields: fields.iter().map(|f| SelectField { name: f.clone(), alias: None }).collect(),
            where_clause: None,
            order_by: None,
            limit: None,
            return_fields: fields,
            insert_label: None,
            insert_properties: Vec::new(),
            delete_alias: None,
            delete_label: None,
        })
    }

    fn convert_insert(&self, q: nxr_ql::ast::InsertStatement) -> NxrResult<ParsedQuery> {
        Ok(ParsedQuery {
            query_type: QueryType::Insert,
            match_clauses: Vec::new(),
            edge_patterns: Vec::new(),
            select_fields: Vec::new(),
            where_clause: None,
            order_by: None,
            limit: None,
            return_fields: Vec::new(),
            insert_label: Some(q.label),
            insert_properties: q.properties,
            delete_alias: None,
            delete_label: None,
        })
    }

    fn convert_delete(&self, q: nxr_ql::ast::DeleteStatement) -> NxrResult<ParsedQuery> {
        Ok(ParsedQuery {
            query_type: QueryType::Delete,
            match_clauses: Vec::new(),
            edge_patterns: Vec::new(),
            select_fields: Vec::new(),
            where_clause: None,
            order_by: None,
            limit: None,
            return_fields: Vec::new(),
            insert_label: None,
            insert_properties: Vec::new(),
            delete_alias: Some(q.pattern.alias),
            delete_label: q.pattern.label,
        })
    }

    fn convert_vector_search(&self, q: nxr_ql::ast::VectorSearch) -> NxrResult<ParsedQuery> {
        let vec_str: Vec<String> = q.query_vector.iter().map(|v| v.to_string()).collect();
        Ok(ParsedQuery {
            query_type: QueryType::Select,
            match_clauses: Vec::new(),
            edge_patterns: Vec::new(),
            select_fields: Vec::new(),
            where_clause: Some(format!("[{}]", vec_str.join(","))),
            order_by: None,
            limit: Some(q.limit),
            return_fields: vec!["similarity".into()],
            insert_label: None,
            insert_properties: Vec::new(),
            delete_alias: None,
            delete_label: None,
        })
    }

    fn parse_semantic(&self, input: &str) -> NxrResult<ParsedQuery> {
        Ok(ParsedQuery {
            query_type: QueryType::Select,
            match_clauses: Vec::new(),
            edge_patterns: Vec::new(),
            select_fields: Vec::new(),
            where_clause: Some(input.to_string()),
            order_by: None,
            limit: None,
            return_fields: vec!["similarity".into()],
            insert_label: None,
            insert_properties: Vec::new(),
            delete_alias: None,
            delete_label: None,
        })
    }
}
