use std::num::TryFromIntError;

use blokli_chain_types::curvy_tree::TreeError;
use hopr_bindings::exports::alloy::{dyn_abi::Error as AbiError, sol_types::Error as SolTypeError};
use hopr_types::primitive::{errors::GeneralError, primitives::Address};
use thiserror::Error;

#[derive(Error, Debug)]
pub enum CoreEthereumIndexerError {
    #[error("{0}")]
    ProcessError(String),

    #[error(transparent)]
    DbApiError(#[from] blokli_db::errors::DbSqlError),

    #[error(transparent)]
    DbError(#[from] blokli_db::api::errors::DbError),

    #[error(transparent)]
    DbEntityError(#[from] blokli_db_entity::errors::DbEntityError),

    #[error(transparent)]
    AbiError(#[from] AbiError),

    #[error(transparent)]
    SolTypeError(#[from] SolTypeError),

    #[error(transparent)]
    GeneralError(#[from] GeneralError),

    #[error("{0}")]
    ValidationError(String),

    #[error(transparent)]
    CurvyTreeError(#[from] TreeError),

    #[error("Curvy derived-state invariant failed: {0}")]
    CurvyStateInvariant(String),

    #[error("Curvy index conversion failed: {0}")]
    CurvyIndexConversion(#[from] TryFromIntError),

    #[error("Address announcement without a preceding key binding.")]
    AnnounceBeforeKeyBinding,

    #[error("Address announcement contains empty Multiaddr.")]
    AnnounceEmptyMultiaddr,

    #[error("Node has already announced a key binding. Reassigning keys is not supported.")]
    UnsupportedKeyRebinding,

    #[error("Address revocation before key binding.")]
    RevocationBeforeKeyBinding,

    #[error("Could not verify account entry signature. Maybe a cross-signing issue?")]
    AccountEntrySignatureVerification,

    #[error("Received an event for a channel that is closed or for which we haven't seen an OPEN event.")]
    ChannelDoesNotExist,

    #[error("Cannot deregister non-existent MFA module")]
    MFAModuleDoesNotExist,

    #[error("Unknown smart contract. Received event from {0}")]
    UnknownContract(Address),

    #[error(transparent)]
    MultiaddrParseError(#[from] multiaddr::Error),

    #[error(transparent)]
    RpcError(#[from] blokli_chain_rpc::errors::RpcError),

    #[error("Snapshot error: {0}")]
    SnapshotError(String),
}

pub type Result<T> = core::result::Result<T, CoreEthereumIndexerError>;
