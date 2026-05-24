use thiserror::Error;

#[derive(Error, Debug)]
pub enum QlError {
    #[error("Syntax error: {0}")]
    Syntax(String),

    #[error("Semantic error: {0}")]
    Semantic(String),

    #[error("Type error: {0}")]
    TypeError(String),

    #[error("Internal error: {0}")]
    Internal(String),
}

pub type QlResult<T> = Result<T, QlError>;
