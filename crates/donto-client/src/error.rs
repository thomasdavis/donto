use thiserror::Error;

#[derive(Debug, Error)]
pub enum Error {
    #[error("postgres error: {0}")]
    Postgres(#[from] tokio_postgres::Error),

    #[error("pool error: {0}")]
    Pool(#[from] deadpool_postgres::PoolError),

    #[error("pool build error: {0}")]
    PoolBuild(#[from] deadpool_postgres::BuildError),

    #[error("pool create error: {0}")]
    PoolCreate(#[from] deadpool_postgres::CreatePoolError),

    #[error("serde_json: {0}")]
    Json(#[from] serde_json::Error),

    #[error("uuid: {0}")]
    Uuid(#[from] uuid::Error),

    #[error("invalid argument: {0}")]
    Invalid(String),

    #[error("not found: {0}")]
    NotFound(String),
}

pub type Result<T> = std::result::Result<T, Error>;
