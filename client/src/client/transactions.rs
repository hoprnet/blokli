use std::time::Duration;

use cynic::{MutationBuilder, SubscriptionBuilder};
use futures::TryStreamExt;
use futures_time::future::FutureExt;
use hex::ToHex;

use super::{BlokliClient, GraphQlQueries, response_to_data};
use crate::{
    api::{
        BlokliTransactionClient, Result, TxId, TxReceipt,
        internal::{
            ConfirmTransactionVariables, MutateConfirmTransaction, MutateSendTransaction, MutateTrackTransaction,
            SendTransactionVariables, SubscribeTransaction, TransactionsVariables,
        },
        types::{Transaction, TransactionStatus},
    },
    errors::{ErrorKind, TrackingErrorKind},
};

impl GraphQlQueries {
    /// `SendTransaction` GraphQL mutation.
    pub fn mutate_submit_transaction(
        signed_tx: &[u8],
    ) -> cynic::Operation<MutateSendTransaction, SendTransactionVariables> {
        MutateSendTransaction::build(SendTransactionVariables {
            raw_transaction: signed_tx.encode_hex(),
        })
    }

    /// `TrackTransaction` GraphQL mutation.
    pub fn mutate_submit_and_track_transaction(
        signed_tx: &[u8],
    ) -> cynic::Operation<MutateTrackTransaction, SendTransactionVariables> {
        MutateTrackTransaction::build(SendTransactionVariables {
            raw_transaction: signed_tx.encode_hex(),
        })
    }

    /// `ConfirmTransaction` GraphQL mutation.
    pub fn mutate_submit_and_confirm_transaction(
        signed_tx: &[u8],
        confirmations: usize,
    ) -> cynic::Operation<MutateConfirmTransaction, ConfirmTransactionVariables> {
        MutateConfirmTransaction::build(ConfirmTransactionVariables {
            raw_transaction: signed_tx.encode_hex(),
            confirmations: confirmations.min(128) as i32,
        })
    }

    /// `TrackTransaction` GraphQL subscription.
    pub fn subscribe_track_transaction(
        tx_id: TxId,
    ) -> cynic::StreamingOperation<SubscribeTransaction, TransactionsVariables> {
        SubscribeTransaction::build(TransactionsVariables { id: tx_id.into() })
    }
}

#[async_trait::async_trait]
impl BlokliTransactionClient for BlokliClient {
    async fn submit_transaction(&self, signed_tx: &[u8]) -> Result<TxReceipt> {
        let resp = self
            .build_query(GraphQlQueries::mutate_submit_transaction(signed_tx))?
            .await?;

        response_to_data(resp)?.send_transaction.into()
    }

    async fn submit_and_track_transaction(&self, signed_tx: &[u8]) -> Result<TxId> {
        let resp = self
            .build_query(GraphQlQueries::mutate_submit_and_track_transaction(signed_tx))?
            .await?;

        let tx: Result<Transaction> = response_to_data(resp)?.send_transaction_async.into();

        Ok(tx?.id.into_inner())
    }

    async fn submit_and_confirm_transaction(&self, signed_tx: &[u8], num_confirmations: usize) -> Result<TxReceipt> {
        let resp = self
            .build_query(GraphQlQueries::mutate_submit_and_confirm_transaction(
                signed_tx,
                num_confirmations,
            ))?
            .await?;

        let tx: Result<Transaction> = response_to_data(resp)?.send_transaction_sync.into();

        let hash = tx?.transaction_hash.0.to_lowercase();
        Ok(hex::decode(hash.trim_start_matches("0x"))
            .map_err(|_| ErrorKind::ParseError)
            .and_then(|d| d.try_into().map_err(|_| ErrorKind::ParseError))?)
    }

    async fn track_transaction(&self, tx_id: TxId, client_timeout: Duration) -> Result<Transaction> {
        self.build_subscription_stream(GraphQlQueries::subscribe_track_transaction(tx_id))?
            .try_filter_map(|item| {
                futures::future::ready(match &item.transaction_updated.status {
                    TransactionStatus::Confirmed => Ok(Some(item.transaction_updated)),
                    TransactionStatus::Pending | TransactionStatus::Submitted => Ok(None),
                    TransactionStatus::Reverted => Err(ErrorKind::TrackingError(TrackingErrorKind::Reverted).into()),
                    TransactionStatus::SubmissionFailed => {
                        Err(ErrorKind::TrackingError(TrackingErrorKind::SubmissionFailed).into())
                    }
                    TransactionStatus::Timeout => Err(ErrorKind::TrackingError(TrackingErrorKind::Timeout).into()),
                    TransactionStatus::ValidationFailed => {
                        Err(ErrorKind::TrackingError(TrackingErrorKind::ValidationFailed).into())
                    }
                })
            })
            .try_next()
            .timeout(futures_time::time::Duration::from(client_timeout))
            .await
            .map_err(|_| ErrorKind::Timeout)??
            .ok_or(ErrorKind::NoData.into())
    }
}
