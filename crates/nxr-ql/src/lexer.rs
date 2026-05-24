#[derive(Debug, Clone, PartialEq)]
pub enum Token {
    Match,
    Select,
    Insert,
    Delete,
    Return,
    Where,
    Order,
    By,
    Limit,
    Desc,
    Asc,
    And,
    Or,
    As,
    On,
    Set,
    Create,
    To,
    From,
    Edge,
    Node,

    LParen,
    RParen,
    LBrack,
    RBrack,
    LBrace,
    RBrace,
    Colon,
    Comma,
    Dot,
    Semicolon,
    Arrow,
    LArrow,
    BiArrow,

    Eq,
    Neq,
    Lt,
    Gt,
    Lte,
    Gte,
    Plus,
    Minus,
    Star,
    Slash,

    Identifier(String),
    StringLiteral(String),
    NumberLiteral(f64),
    IntegerLiteral(i64),
    FloatLiteral(f64),
    EOF,
    Error(String),
}

pub struct Lexer {
    input: Vec<char>,
    pos: usize,
}

impl Lexer {
    pub fn new(input: &str) -> Self {
        Self {
            input: input.chars().collect(),
            pos: 0,
        }
    }

    fn peek(&self) -> Option<char> {
        self.input.get(self.pos).copied()
    }

    fn advance(&mut self) -> Option<char> {
        let ch = self.input.get(self.pos).copied();
        self.pos += 1;
        ch
    }

    fn skip_whitespace(&mut self) {
        while let Some(ch) = self.peek() {
            if ch.is_whitespace() {
                self.advance();
            } else {
                break;
            }
        }
    }

    fn read_identifier(&mut self, first: char) -> String {
        let mut ident = String::new();
        ident.push(first);
        while let Some(ch) = self.peek() {
            if ch.is_alphanumeric() || ch == '_' {
                ident.push(ch);
                self.advance();
            } else {
                break;
            }
        }
        ident
    }

    fn read_number(&mut self, first: char) -> Token {
        let mut num = String::new();
        num.push(first);
        let mut is_float = false;

        while let Some(ch) = self.peek() {
            if ch.is_ascii_digit() {
                num.push(ch);
                self.advance();
            } else if ch == '.' {
                is_float = true;
                num.push(ch);
                self.advance();
            } else {
                break;
            }
        }

        if is_float {
            Token::FloatLiteral(num.parse::<f64>().unwrap_or(0.0))
        } else {
            Token::IntegerLiteral(num.parse::<i64>().unwrap_or(0))
        }
    }

    fn read_string(&mut self) -> Token {
        let mut s = String::new();
        loop {
            match self.advance() {
                Some('"') => break,
                Some('\\') => {
                    if let Some(next) = self.advance() {
                        match next {
                            'n' => s.push('\n'),
                            't' => s.push('\t'),
                            '\\' => s.push('\\'),
                            '"' => s.push('"'),
                            c => s.push(c),
                        }
                    }
                }
                Some(c) => s.push(c),
                None => return Token::Error("Unterminated string".into()),
            }
        }
        Token::StringLiteral(s)
    }

    fn keyword_or_ident(&self, ident: &str) -> Token {
        match ident.to_uppercase().as_str() {
            "MATCH" => Token::Match,
            "SELECT" => Token::Select,
            "INSERT" => Token::Insert,
            "DELETE" => Token::Delete,
            "RETURN" => Token::Return,
            "WHERE" => Token::Where,
            "ORDER" => Token::Order,
            "BY" => Token::By,
            "LIMIT" => Token::Limit,
            "DESC" => Token::Desc,
            "ASC" => Token::Asc,
            "AND" => Token::And,
            "OR" => Token::Or,
            "AS" => Token::As,
            "ON" => Token::On,
            "SET" => Token::Set,
            "CREATE" => Token::Create,
            "TO" => Token::To,
            "FROM" => Token::From,
            "EDGE" => Token::Edge,
            "NODE" => Token::Node,
            _ => Token::Identifier(ident.to_string()),
        }
    }

    pub fn next_token(&mut self) -> Token {
        self.skip_whitespace();

        match self.advance() {
            None => Token::EOF,
            Some('(') => Token::LParen,
            Some(')') => Token::RParen,
            Some('[') => Token::LBrack,
            Some(']') => Token::RBrack,
            Some('{') => Token::LBrace,
            Some('}') => Token::RBrace,
            Some(':') => Token::Colon,
            Some(',') => Token::Comma,
            Some('.') => Token::Dot,
            Some(';') => Token::Semicolon,
            Some('+') => Token::Plus,
            Some('-') => {
                if self.peek() == Some('>') {
                    self.advance();
                    Token::Arrow
                } else if self.peek() == Some('-') {
                    self.advance();
                    if self.peek() == Some('>') {
                        self.advance();
                        Token::BiArrow
                    } else {
                        Token::Minus
                    }
                } else {
                    Token::Minus
                }
            }
            Some('*') => Token::Star,
            Some('/') => Token::Slash,
            Some('=') => {
                if self.peek() == Some('>') {
                    self.advance();
                    Token::Arrow
                } else {
                    Token::Eq
                }
            }
            Some('!') => {
                if self.peek() == Some('=') {
                    self.advance();
                    Token::Neq
                } else {
                    Token::Error("Expected = after !".into())
                }
            }
            Some('<') => {
                if self.peek() == Some('=') {
                    self.advance();
                    Token::Lte
                } else if self.peek() == Some('-') {
                    self.advance();
                    if self.peek() == Some('>') {
                        self.advance();
                        Token::BiArrow
                    } else {
                        Token::LArrow
                    }
                } else {
                    Token::Lt
                }
            }
            Some('>') => {
                if self.peek() == Some('=') {
                    self.advance();
                    Token::Gte
                } else {
                    Token::Gt
                }
            }
            Some('"') => self.read_string(),
            Some(ch) if ch.is_alphabetic() || ch == '_' => {
                let ident = self.read_identifier(ch);
                self.keyword_or_ident(&ident)
            }
            Some(ch) if ch.is_ascii_digit() => self.read_number(ch),
            Some(ch) => Token::Error(format!("Unexpected character: {}", ch)),
        }
    }
}

impl Iterator for Lexer {
    type Item = Token;

    fn next(&mut self) -> Option<Self::Item> {
        let token = self.next_token();
        if token == Token::EOF {
            None
        } else {
            Some(token)
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_empty_input() {
        let mut lex = Lexer::new("");
        assert_eq!(lex.next_token(), Token::EOF);
    }

    #[test]
    fn test_keywords() {
        let check = |input: &str, expected: Token| {
            let mut lex = Lexer::new(input);
            assert_eq!(lex.next_token(), expected);
        };
        check("MATCH", Token::Match);
        check("SELECT", Token::Select);
        check("INSERT", Token::Insert);
        check("DELETE", Token::Delete);
        check("RETURN", Token::Return);
        check("WHERE", Token::Where);
        check("ORDER", Token::Order);
        check("BY", Token::By);
        check("LIMIT", Token::Limit);
        check("DESC", Token::Desc);
        check("AND", Token::And);
        check("OR", Token::Or);
    }

    #[test]
    fn test_identifiers() {
        let mut lex = Lexer::new("myVar userName age_123");
        assert_eq!(lex.next_token(), Token::Identifier("myVar".into()));
        assert_eq!(lex.next_token(), Token::Identifier("userName".into()));
        assert_eq!(lex.next_token(), Token::Identifier("age_123".into()));
        assert_eq!(lex.next_token(), Token::EOF);
    }

    #[test]
    fn test_numbers() {
        let mut lex = Lexer::new("42 3.14");
        assert_eq!(lex.next_token(), Token::IntegerLiteral(42));
        assert_eq!(lex.next_token(), Token::FloatLiteral(3.14));
    }

    #[test]
    fn test_strings() {
        let mut lex = Lexer::new(r#""hello world""#);
        assert_eq!(lex.next_token(), Token::StringLiteral("hello world".into()));
    }

    #[test]
    fn test_punctuation() {
        let mut lex = Lexer::new("()[]{}:,.;");
        assert_eq!(lex.next_token(), Token::LParen);
        assert_eq!(lex.next_token(), Token::RParen);
        assert_eq!(lex.next_token(), Token::LBrack);
        assert_eq!(lex.next_token(), Token::RBrack);
        assert_eq!(lex.next_token(), Token::LBrace);
        assert_eq!(lex.next_token(), Token::RBrace);
        assert_eq!(lex.next_token(), Token::Colon);
        assert_eq!(lex.next_token(), Token::Comma);
        assert_eq!(lex.next_token(), Token::Dot);
        assert_eq!(lex.next_token(), Token::Semicolon);
    }

    #[test]
    fn test_arrows() {
        let mut lex = Lexer::new("-> --> <->");
        assert_eq!(lex.next_token(), Token::Arrow);
        assert_eq!(lex.next_token(), Token::BiArrow);
        assert_eq!(lex.next_token(), Token::BiArrow);
    }

    #[test]
    fn test_comparison() {
        let mut lex = Lexer::new("= != < > <= >=");
        assert_eq!(lex.next_token(), Token::Eq);
        assert_eq!(lex.next_token(), Token::Neq);
        assert_eq!(lex.next_token(), Token::Lt);
        assert_eq!(lex.next_token(), Token::Gt);
        assert_eq!(lex.next_token(), Token::Lte);
        assert_eq!(lex.next_token(), Token::Gte);
    }

    #[test]
    fn test_match_query_tokens() {
        let mut lex = Lexer::new("MATCH (u:User)-[r:PREFERS]->(t:Topic)");
        assert_eq!(lex.next_token(), Token::Match);
        assert_eq!(lex.next_token(), Token::LParen);
        assert_eq!(lex.next_token(), Token::Identifier("u".into()));
        assert_eq!(lex.next_token(), Token::Colon);
        assert_eq!(lex.next_token(), Token::Identifier("User".into()));
        assert_eq!(lex.next_token(), Token::RParen);
        assert_eq!(lex.next_token(), Token::Minus);
        assert_eq!(lex.next_token(), Token::LBrack);
        assert_eq!(lex.next_token(), Token::Identifier("r".into()));
        assert_eq!(lex.next_token(), Token::Colon);
        assert_eq!(lex.next_token(), Token::Identifier("PREFERS".into()));
        assert_eq!(lex.next_token(), Token::RBrack);
        assert_eq!(lex.next_token(), Token::Arrow);
        assert_eq!(lex.next_token(), Token::LParen);
        assert_eq!(lex.next_token(), Token::Identifier("t".into()));
        assert_eq!(lex.next_token(), Token::Colon);
        assert_eq!(lex.next_token(), Token::Identifier("Topic".into()));
        assert_eq!(lex.next_token(), Token::RParen);
    }
}
