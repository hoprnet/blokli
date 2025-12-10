use super::{DateTime, Hex32, schema};
use crate::{
    api::TxReceipt,
    errors::{BlokliClientError, ErrorKind},
};

#[derive(cynic::QueryVariables)]
pub struct TransactionsVariables {
    pub id: cynic::Id,
}

#[derive(cynic::QueryVariables)]
pub struct SendTransactionVariables {
    pub raw_transaction: String,
}

#[derive(cynic::QueryVariables)]
pub struct ConfirmTransactionVariables {
    pub raw_transaction: String,
    pub confirmations: i32,
}

#[derive(cynic::QueryFragment, Debug)]
#[cynic(graphql_type = "QueryRoot", variables = "TransactionsVariables")]
pub struct QueryTransaction {
    #[arguments(id: $id)]
    pub transaction: Option<TransactionResult>,
}

#[derive(cynic::QueryFragment, Debug)]
#[cynic(graphql_type = "SubscriptionRoot", variables = "TransactionsVariables")]
pub struct SubscribeTransaction {
    #[arguments(id: $id)]
    pub transaction_updated: Transaction,
}

#[derive(cynic::QueryFragment, Debug)]
#[cynic(graphql_type = "MutationRoot", variables = "ConfirmTransactionVariables")]
pub struct MutateConfirmTransaction {
    #[arguments(input: { rawTransaction: $raw_transaction }, confirmations: $confirmations)]
    pub send_transaction_sync: SendTransactionSyncResult,
}

#[derive(cynic::QueryFragment, Debug)]
#[cynic(graphql_type = "MutationRoot", variables = "SendTransactionVariables")]
pub struct MutateTrackTransaction {
    #[arguments(input: { rawTransaction: $raw_transaction })]
    pub send_transaction_async: SendTransactionAsyncResult,
}

#[derive(cynic::QueryFragment, Debug)]
#[cynic(graphql_type = "MutationRoot", variables = "SendTransactionVariables")]
pub struct MutateSendTransaction {
    #[arguments(input: { rawTransaction: $raw_transaction })]
    pub send_transaction: SendTransactionResult,
}

#[derive(cynic::QueryFragment, Debug, Clone, PartialEq, Eq)]
#[cfg_attr(feature = "serde", derive(serde::Serialize))]
pub struct Transaction {
    pub id: cynic::Id,
    pub status: TransactionStatus,
    pub submitted_at: DateTime,
    pub transaction_hash: Hex32,
}

#[derive(cynic::QueryFragment, Debug)]
pub struct InvalidTransactionIdError {
    pub __typename: String,
    pub code: String,
    pub message: String,
}

impl From<InvalidTransactionIdError> for BlokliClientError {
    fn from(value: InvalidTransactionIdError) -> Self {
        ErrorKind::BlokliError {
            kind: "invalid transaction id",
            code: value.code,
            message: value.message,
        }
        .into()
    }
}

#[derive(cynic::InlineFragments, Debug)]
pub enum TransactionResult {
    Transaction(Transaction),
    InvalidTransactionIdError(InvalidTransactionIdError),
    #[cynic(fallback)]
    Unknown,
}

impl From<TransactionResult> for Result<Transaction, BlokliClientError> {
    fn from(value: TransactionResult) -> Self {
        match value {
            TransactionResult::Transaction(t) => Ok(t),
            TransactionResult::InvalidTransactionIdError(e) => Err(e.into()),
            TransactionResult::Unknown => Err(ErrorKind::NoData.into()),
        }
    }
}

#[derive(cynic::Enum, Clone, Copy, Debug, PartialEq, Eq)]
pub enum TransactionStatus {
    #[cynic(rename = "CONFIRMED")]
    Confirmed,
    #[cynic(rename = "PENDING")]
    Pending,
    #[cynic(rename = "REVERTED")]
    Reverted,
    #[cynic(rename = "SUBMISSION_FAILED")]
    SubmissionFailed,
    #[cynic(rename = "SUBMITTED")]
    Submitted,
    #[cynic(rename = "TIMEOUT")]
    Timeout,
    #[cynic(rename = "VALIDATION_FAILED")]
    ValidationFailed,
}

#[derive(cynic::QueryFragment, Debug)]
pub struct TimeoutError {
    pub __typename: String,
    pub code: String,
    pub message: String,
}

impl From<TimeoutError> for BlokliClientError {
    fn from(value: TimeoutError) -> Self {
        ErrorKind::BlokliError {
            kind: "timeout",
            code: value.code,
            message: value.message,
        }
        .into()
    }
}

#[derive(cynic::QueryFragment, Debug)]
pub struct SendTransactionSuccess {
    pub __typename: String,
    pub transaction_hash: Hex32,
}

#[derive(cynic::QueryFragment, Debug)]
pub struct RpcError {
    pub __typename: String,
    pub code: String,
    pub message: String,
}

impl From<RpcError> for BlokliClientError {
    fn from(value: RpcError) -> Self {
        ErrorKind::BlokliError {
            kind: "rpc error",
            code: value.code,
            message: value.message,
        }
        .into()
    }
}

#[derive(cynic::QueryFragment, Debug)]
pub struct FunctionNotAllowedError {
    pub __typename: String,
    pub code: String,
    pub contract_address: String,
    pub function_selector: String,
    pub message: String,
}

impl From<FunctionNotAllowedError> for BlokliClientError {
    fn from(value: FunctionNotAllowedError) -> Self {
        ErrorKind::BlokliError {
            kind: "function not allowed",
            code: value.code,
            message: value.message,
        }
        .into()
    }
}

#[derive(cynic::QueryFragment, Debug)]
pub struct ContractNotAllowedError {
    pub __typename: String,
    pub code: String,
    pub contract_address: String,
    pub message: String,
}

impl From<ContractNotAllowedError> for BlokliClientError {
    fn from(value: ContractNotAllowedError) -> Self {
        ErrorKind::BlokliError {
            kind: "contract not allowed",
            code: value.code,
            message: value.message,
        }
        .into()
    }
}

#[derive(cynic::InlineFragments, Debug)]
pub enum SendTransactionAsyncResult {
    Transaction(Transaction),
    ContractNotAllowedError(ContractNotAllowedError),
    FunctionNotAllowedError(FunctionNotAllowedError),
    RpcError(RpcError),
    #[cynic(fallback)]
    Unknown,
}

impl From<SendTransactionAsyncResult> for Result<Transaction, BlokliClientError> {
    fn from(value: SendTransactionAsyncResult) -> Self {
        match value {
            SendTransactionAsyncResult::Transaction(t) => Ok(t),
            SendTransactionAsyncResult::ContractNotAllowedError(e) => Err(e.into()),
            SendTransactionAsyncResult::FunctionNotAllowedError(e) => Err(e.into()),
            SendTransactionAsyncResult::RpcError(e) => Err(e.into()),
            SendTransactionAsyncResult::Unknown => Err(ErrorKind::NoData.into()),
        }
    }
}

#[derive(cynic::InlineFragments, Debug)]
pub enum SendTransactionResult {
    SendTransactionSuccess(SendTransactionSuccess),
    ContractNotAllowedError(ContractNotAllowedError),
    FunctionNotAllowedError(FunctionNotAllowedError),
    RpcError(RpcError),
    #[cynic(fallback)]
    Unknown,
}

impl From<SendTransactionResult> for Result<TxReceipt, BlokliClientError> {
    fn from(value: SendTransactionResult) -> Self {
        match value {
            SendTransactionResult::SendTransactionSuccess(t) => {
                let hash = t.transaction_hash.0.to_lowercase();
                Ok(hex::decode(hash.trim_start_matches("0x"))
                    .map_err(|_| ErrorKind::ParseError)?
                    .try_into()
                    .map_err(|_| ErrorKind::ParseError)?)
            }
            SendTransactionResult::ContractNotAllowedError(e) => Err(e.into()),
            SendTransactionResult::FunctionNotAllowedError(e) => Err(e.into()),
            SendTransactionResult::RpcError(e) => Err(e.into()),
            SendTransactionResult::Unknown => Err(ErrorKind::NoData.into()),
        }
    }
}

#[derive(cynic::InlineFragments, Debug)]
pub enum SendTransactionSyncResult {
    Transaction(Transaction),
    ContractNotAllowedError(ContractNotAllowedError),
    FunctionNotAllowedError(FunctionNotAllowedError),
    RpcError(RpcError),
    TimeoutError(TimeoutError),
    #[cynic(fallback)]
    Unknown,
}

impl From<SendTransactionSyncResult> for Result<Transaction, BlokliClientError> {
    fn from(value: SendTransactionSyncResult) -> Self {
        match value {
            SendTransactionSyncResult::Transaction(t) => Ok(t),
            SendTransactionSyncResult::ContractNotAllowedError(e) => Err(e.into()),
            SendTransactionSyncResult::FunctionNotAllowedError(e) => Err(e.into()),
            SendTransactionSyncResult::RpcError(e) => Err(e.into()),
            SendTransactionSyncResult::TimeoutError(e) => Err(e.into()),
            SendTransactionSyncResult::Unknown => Err(ErrorKind::NoData.into()),
        }
    }
}
