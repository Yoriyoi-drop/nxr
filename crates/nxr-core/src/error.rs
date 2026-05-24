use thiserror::Error;

#[derive(Error, Debug)]
pub enum NxrError {
    #[error("IO error: {0}")]
    Io(#[from] std::io::Error),

    #[error("Config error: {0}")]
    Config(String),

    #[error("Serialization error: {0}")]
    Serialization(String),

    #[error("WAL error: {0}")]
    Wal(String),

    #[error("Vector error: {0}")]
    Vector(String),

    #[error("Graph error: {0}")]
    Graph(String),

    #[error("KV error: {0}")]
    Kv(String),

    #[error("Query error: {0}")]
    Query(String),

    #[error("Index error: {0}")]
    Index(String),

    #[error("Pipeline error: {0}")]
    Pipeline(String),

    #[error("Not found: {0}")]
    NotFound(String),

    #[error("Invalid argument: {0}")]
    InvalidArgument(String),

    #[error("Snapshot error: {0}")]
    Snapshot(String),
}

pub type NxrResult<T> = Result<T, NxrError>;
