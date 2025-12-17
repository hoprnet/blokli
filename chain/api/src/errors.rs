use thiserror::Error;

/// Error representing all possible erroneous states of the entire HOPR
/// on-chain interactions.
#[derive(Error, Debug)]
pub enum BlokliChainError {
    #[error("API error: {0}")]
    Api(String),

    #[error("rpc error: {0}")]
    Rpc(#[from] blokli_chain_rpc::errors::RpcError),

    #[error("indexer error: {0}")]
    Indexer(#[from] blokli_chain_indexer::errors::CoreEthereumIndexerError),

    #[error(transparent)]
    DbError(#[from] blokli_db::errors::DbSqlError),

    #[error("configuration error: {0}")]
    Configuration(String),

    #[error("contract verification error: {0}")]
    Verification(String),
}

/// The default [Result] object translating errors in the [BlokliChainError] type
pub type Result<T> = core::result::Result<T, BlokliChainError>;
