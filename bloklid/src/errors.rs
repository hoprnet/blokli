use validator::ValidationErrors;

#[derive(Debug, thiserror::Error)]
pub enum BloklidError {
    #[error(transparent)]
    Io(#[from] std::io::Error),

    #[error(transparent)]
    Config(#[from] ConfigError),

    #[error("Unspecified error: {0}")]
    NonSpecific(String),

    #[error("database error: {0}")]
    Database(#[from] blokli_db::errors::DbSqlError),

    #[error("chain error: {0}")]
    Chain(#[from] blokli_chain_api::errors::BlokliChainError),

    #[error("rpc error: {0}")]
    Rpc(#[from] blokli_chain_rpc::errors::RpcError),

    #[error("indexer error: {0}")]
    Indexer(#[from] blokli_chain_indexer::errors::CoreEthereumIndexerError),

    #[error("database not configured: {0}")]
    DatabaseNotConfigured(String),
}

#[derive(Debug, thiserror::Error)]
pub enum ConfigError {
    #[error("no configuration provided")]
    NoConfiguration,
    #[error("failed to parse config file: {0}")]
    Parse(String),
    #[error("failed to validate config: {0}")]
    Validation(ValidationErrors),
    #[error(
        "no database configuration provided. must provide either [database] section in config file or environment \
         variables: BLOKLI_DATABASE_TYPE, BLOKLI_DATABASE_URL"
    )]
    NoDatabaseConfiguration,
}

pub type Result<T> = std::result::Result<T, BloklidError>;
