use crate::ast::*;
use crate::error::{QlError, QlResult};
use crate::lexer::{Lexer, Token};

pub struct NxrQlParser;

impl NxrQlParser {
    pub fn parse(input: &str) -> QlResult<Statement> {
        let lexer = Lexer::new(input);
        let tokens: Vec<Token> = lexer.collect();
        let mut pos = 0;

        if tokens.is_empty() {
            return Err(QlError::Syntax("Empty query".into()));
        }

        match &tokens[0] {
            Token::Match => Self::parse_match(&tokens, &mut pos),
            Token::Select => Self::parse_select(&tokens, &mut pos),
            Token::Insert => Self::parse_insert(&tokens, &mut pos),
            Token::Delete => Self::parse_delete(&tokens, &mut pos),
            _ => Err(QlError::Syntax(format!(
                "Unexpected token {:?}, expected MATCH/SELECT/INSERT/DELETE",
                tokens[0]
            ))),
        }
    }

    fn parse_match(tokens: &[Token], pos: &mut usize) -> QlResult<Statement> {
        *pos += 1; // consume MATCH

        let mut patterns = Vec::new();

        // Parse (alias:Label)
        if *pos >= tokens.len() || tokens[*pos] != Token::LParen {
            return Err(QlError::Syntax("Expected ( after MATCH".into()));
        }
        *pos += 1;

        let from_alias = Self::expect_identifier(tokens, pos)?;
        let from_label = if *pos < tokens.len() && tokens[*pos] == Token::Colon {
            *pos += 1;
            Some(Self::expect_identifier_str(tokens, pos)?)
        } else {
            None
        };
        Self::expect_token(tokens, pos, Token::RParen)?;

        // Parse optional dash before edge [-:RELATION]->
        if *pos < tokens.len() && tokens[*pos] == Token::Minus {
            *pos += 1;
        }

        // Parse edge [-:RELATION]->
        let mut edge_alias = None;
        let mut relation = None;
        let direction;

        if *pos < tokens.len() && tokens[*pos] == Token::LBrack {
            *pos += 1;
            if *pos < tokens.len() && matches!(&tokens[*pos], Token::Identifier(_)) {
                let ident = Self::expect_identifier_str(tokens, pos)?;
                if *pos < tokens.len() && tokens[*pos] == Token::Colon {
                    *pos += 1;
                    edge_alias = Some(ident);
                    if *pos < tokens.len() && matches!(&tokens[*pos], Token::Identifier(_)) {
                        relation = Some(Self::expect_identifier_str(tokens, pos)?);
                    }
                } else {
                    relation = Some(ident);
                }
            } else if *pos < tokens.len() && tokens[*pos] == Token::Colon {
                *pos += 1;
                if *pos < tokens.len() && matches!(&tokens[*pos], Token::Identifier(_)) {
                    relation = Some(Self::expect_identifier_str(tokens, pos)?);
                }
            }
            Self::expect_token(tokens, pos, Token::RBrack)?;
        }

        let has_edge = edge_alias.is_some() || relation.is_some();

        if *pos < tokens.len() && tokens[*pos] == Token::Arrow {
            *pos += 1;
            direction = Direction::Forward;
        } else if *pos < tokens.len() && tokens[*pos] == Token::LArrow {
            *pos += 1;
            direction = Direction::Backward;
        } else {
            direction = Direction::Bidirectional;
        }

        // Parse (alias:Label) if edge or next token is LParen
        let to_alias;
        let to_label;
        if has_edge || (*pos < tokens.len() && tokens[*pos] == Token::LParen) {
            Self::expect_token(tokens, pos, Token::LParen)?;
            to_alias = Self::expect_identifier(tokens, pos)?;
            to_label = if *pos < tokens.len() && tokens[*pos] == Token::Colon {
                *pos += 1;
                Some(Self::expect_identifier_str(tokens, pos)?)
            } else {
                None
            };
            Self::expect_token(tokens, pos, Token::RParen)?;
        } else {
            to_alias = from_alias.clone();
            to_label = from_label.clone();
        }

        patterns.push(PathPattern {
            from: NodePattern { alias: from_alias, label: from_label },
            edge: EdgePattern {
                alias: edge_alias,
                relation,
                direction,
            },
            to: NodePattern { alias: to_alias, label: to_label },
        });

        // Parse optional WHERE
        let where_clause = if *pos < tokens.len() && tokens[*pos] == Token::Where {
            *pos += 1;
            Some(Self::parse_expression(tokens, pos)?)
        } else {
            None
        };

        // Parse RETURN
        let return_fields = if *pos < tokens.len() && tokens[*pos] == Token::Return {
            *pos += 1;
            Self::parse_field_list(tokens, pos)?
        } else {
            Vec::new()
        };

        // Parse ORDER BY
        let order_by = if *pos + 1 < tokens.len()
            && tokens[*pos] == Token::Order
            && tokens[*pos + 1] == Token::By
        {
            *pos += 2;
            let field = Self::expect_identifier_str(tokens, pos)?;
            let descending = if *pos < tokens.len() && tokens[*pos] == Token::Desc {
                *pos += 1;
                true
            } else {
                if *pos < tokens.len() && tokens[*pos] == Token::Asc {
                    *pos += 1;
                }
                false
            };
            Some(OrderBy { field, descending })
        } else {
            None
        };

        // Parse LIMIT
        let limit = if *pos < tokens.len() && tokens[*pos] == Token::Limit {
            *pos += 1;
            Some(Self::expect_integer(tokens, pos)? as usize)
        } else {
            None
        };

        Ok(Statement::Match(MatchQuery {
            patterns,
            where_clause,
            return_fields,
            order_by,
            limit,
        }))
    }

    fn parse_select(_tokens: &[Token], _pos: &mut usize) -> QlResult<Statement> {
        Err(QlError::Syntax("SELECT not fully implemented".into()))
    }

    fn parse_insert(tokens: &[Token], pos: &mut usize) -> QlResult<Statement> {
        *pos += 1; // consume INSERT
        if *pos >= tokens.len() || tokens[*pos] != Token::LParen {
            return Err(QlError::Syntax("Expected ( after INSERT".into()));
        }
        *pos += 1;
        let label = Self::expect_identifier_str(tokens, pos)?;

        let mut properties = Vec::new();
        if *pos < tokens.len() && tokens[*pos] == Token::LBrace {
            *pos += 1;
            loop {
                if *pos >= tokens.len() || tokens[*pos] == Token::RBrace {
                    break;
                }
                let key = Self::expect_identifier_str(tokens, pos)?;
                Self::expect_token(tokens, pos, Token::Colon)?;
                let value = match &tokens[*pos] {
                    Token::StringLiteral(s) => {
                        *pos += 1;
                        s.clone()
                    }
                    Token::IntegerLiteral(i) => {
                        *pos += 1;
                        i.to_string()
                    }
                    _ => return Err(QlError::Syntax("Expected property value".into())),
                };
                properties.push((key, value));
                if *pos < tokens.len() && tokens[*pos] == Token::Comma {
                    *pos += 1;
                }
            }
            Self::expect_token(tokens, pos, Token::RBrace)?;
        }
        Self::expect_token(tokens, pos, Token::RParen)?;

        Ok(Statement::Insert(InsertStatement { label, properties }))
    }

    fn parse_delete(tokens: &[Token], pos: &mut usize) -> QlResult<Statement> {
        *pos += 1; // consume DELETE
        Self::expect_token(tokens, pos, Token::LParen)?;
        let alias = Self::expect_identifier(tokens, pos)?;
        let label = if *pos < tokens.len() && tokens[*pos] == Token::Colon {
            *pos += 1;
            Some(Self::expect_identifier_str(tokens, pos)?)
        } else {
            None
        };
        Self::expect_token(tokens, pos, Token::RParen)?;
        Ok(Statement::Delete(DeleteStatement {
            pattern: NodePattern { alias, label },
            cascade: false,
        }))
    }

    fn parse_expression(tokens: &[Token], pos: &mut usize) -> QlResult<Expression> {
        let left = match &tokens[*pos] {
            Token::Identifier(_) => {
                let mut field = Self::expect_identifier_str(tokens, pos)?;
                while *pos < tokens.len() && tokens[*pos] == Token::Dot {
                    *pos += 1;
                    field = format!("{}.{}", field, Self::expect_identifier_str(tokens, pos)?);
                }
                if *pos < tokens.len() {
                    match &tokens[*pos] {
                        Token::Eq => {
                            *pos += 1;
                            let val = Self::expect_value(tokens, pos)?;
                            Expression::Comparison(field, "=".into(), val)
                        }
                        Token::Neq => {
                            *pos += 1;
                            let val = Self::expect_value(tokens, pos)?;
                            Expression::Comparison(field, "!=".into(), val)
                        }
                        Token::Gt => {
                            *pos += 1;
                            let val = Self::expect_value(tokens, pos)?;
                            Expression::Comparison(field, ">".into(), val)
                        }
                        Token::Lt => {
                            *pos += 1;
                            let val = Self::expect_value(tokens, pos)?;
                            Expression::Comparison(field, "<".into(), val)
                        }
                        _ => Expression::Literal(field),
                    }
                } else {
                    Expression::Literal(field)
                }
            }
            Token::StringLiteral(s) => {
                *pos += 1;
                Expression::Literal(s.clone())
            }
            _ => return Err(QlError::Syntax("Expected expression".into())),
        };

        if *pos < tokens.len() && tokens[*pos] == Token::And {
            *pos += 1;
            let right = Self::parse_expression(tokens, pos)?;
            return Ok(Expression::And(Box::new(left), Box::new(right)));
        }

        if *pos < tokens.len() && tokens[*pos] == Token::Or {
            *pos += 1;
            let right = Self::parse_expression(tokens, pos)?;
            return Ok(Expression::Or(Box::new(left), Box::new(right)));
        }

        Ok(left)
    }

    fn parse_field_list(tokens: &[Token], pos: &mut usize) -> QlResult<Vec<Field>> {
        let mut fields = Vec::new();
        loop {
            if *pos >= tokens.len() {
                break;
            }
            match &tokens[*pos] {
                Token::Identifier(name) => {
                    *pos += 1;
                    let alias = if *pos < tokens.len() && tokens[*pos] == Token::As {
                        *pos += 1;
                        Some(Self::expect_identifier_str(tokens, pos)?)
                    } else {
                        None
                    };
                    fields.push(Field {
                        name: name.clone(),
                        alias,
                    });
                }
                Token::Star => {
                    *pos += 1;
                    fields.push(Field {
                        name: "*".into(),
                        alias: None,
                    });
                }
                _ => break,
            }
            if *pos < tokens.len() && tokens[*pos] == Token::Comma {
                *pos += 1;
            } else {
                break;
            }
        }
        Ok(fields)
    }

    fn expect_identifier(tokens: &[Token], pos: &mut usize) -> QlResult<String> {
        if *pos >= tokens.len() {
            return Err(QlError::Syntax("Expected identifier".into()));
        }
        match &tokens[*pos] {
            Token::Identifier(s) => {
                *pos += 1;
                Ok(s.clone())
            }
            _ => Err(QlError::Syntax(format!(
                "Expected identifier, got {:?}",
                tokens[*pos]
            ))),
        }
    }

    fn expect_identifier_str(tokens: &[Token], pos: &mut usize) -> QlResult<String> {
        match &tokens[*pos] {
            Token::Identifier(s) | Token::StringLiteral(s) => {
                *pos += 1;
                Ok(s.clone())
            }
            _ => Err(QlError::Syntax(format!(
                "Expected identifier or string, got {:?}",
                tokens[*pos]
            ))),
        }
    }

    fn expect_value(tokens: &[Token], pos: &mut usize) -> QlResult<String> {
        if *pos >= tokens.len() {
            return Err(QlError::Syntax("Expected value".into()));
        }
        let val = match &tokens[*pos] {
            Token::StringLiteral(s) => s.clone(),
            Token::IntegerLiteral(i) => i.to_string(),
            Token::FloatLiteral(f) => f.to_string(),
            Token::Identifier(s) => s.clone(),
            _ => return Err(QlError::Syntax("Expected value".into())),
        };
        *pos += 1;
        Ok(val)
    }

    fn expect_token(tokens: &[Token], pos: &mut usize, expected: Token) -> QlResult<()> {
        if *pos >= tokens.len() {
            return Err(QlError::Syntax(format!("Expected {:?}, got EOF", expected)));
        }
        if tokens[*pos] != expected {
            return Err(QlError::Syntax(format!(
                "Expected {:?}, got {:?}",
                expected, tokens[*pos]
            )));
        }
        *pos += 1;
        Ok(())
    }

    fn expect_integer(tokens: &[Token], pos: &mut usize) -> QlResult<i64> {
        if *pos >= tokens.len() {
            return Err(QlError::Syntax("Expected integer".into()));
        }
        match &tokens[*pos] {
            Token::IntegerLiteral(i) => {
                *pos += 1;
                Ok(*i)
            }
            _ => Err(QlError::Syntax(format!(
                "Expected integer, got {:?}",
                tokens[*pos]
            ))),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn parse(input: &str) -> Statement {
        NxrQlParser::parse(input).unwrap()
    }

    #[test]
    fn test_parse_simple_match() {
        let stmt = parse("MATCH (u:User)");
        match stmt {
            Statement::Match(q) => {
                assert_eq!(q.patterns.len(), 1);
                assert_eq!(q.patterns[0].from.alias, "u");
                assert_eq!(q.patterns[0].from.label, Some("User".into()));
            }
            _ => panic!("Expected Match"),
        }
    }

    #[test]
    fn test_parse_match_with_edge() {
        let stmt = parse("MATCH (u:User)-[r:PREFERS]->(t:Topic)");
        match stmt {
            Statement::Match(q) => {
                assert_eq!(q.patterns.len(), 1);
                let p = &q.patterns[0];
                assert_eq!(p.from.alias, "u");
                assert_eq!(p.from.label, Some("User".into()));
                assert_eq!(p.edge.relation, Some("PREFERS".into()));
                assert_eq!(p.edge.direction, Direction::Forward);
                assert_eq!(p.to.alias, "t");
                assert_eq!(p.to.label, Some("Topic".into()));
            }
            _ => panic!("Expected Match"),
        }
    }

    #[test]
    fn test_parse_match_with_where() {
        let stmt = parse("MATCH (u:User) WHERE u.name = \"Alice\" RETURN u");
        match stmt {
            Statement::Match(q) => {
                assert!(q.where_clause.is_some());
                assert!(!q.return_fields.is_empty());
            }
            _ => panic!("Expected Match"),
        }
    }

    #[test]
    fn test_parse_match_with_limit() {
        let stmt = parse("MATCH (u:User) RETURN u LIMIT 10");
        match stmt {
            Statement::Match(q) => {
                assert_eq!(q.limit, Some(10));
            }
            _ => panic!("Expected Match"),
        }
    }

    #[test]
    fn test_parse_insert() {
        let stmt = parse("INSERT (User {name: \"Alice\", age: \"30\"})");
        match stmt {
            Statement::Insert(q) => {
                assert_eq!(q.label, "User");
                assert_eq!(q.properties.len(), 2);
            }
            _ => panic!("Expected Insert"),
        }
    }

    #[test]
    fn test_parse_delete() {
        let stmt = parse("DELETE (u:User)");
        match stmt {
            Statement::Delete(q) => {
                assert_eq!(q.pattern.alias, "u");
                assert_eq!(q.pattern.label, Some("User".into()));
            }
            _ => panic!("Expected Delete"),
        }
    }

    #[test]
    fn test_parse_empty_error() {
        let result = NxrQlParser::parse("");
        assert!(result.is_err());
    }

    #[test]
    fn test_parse_invalid_syntax() {
        let result = NxrQlParser::parse("INVALID QUERY HERE");
        assert!(result.is_err());
    }
}
