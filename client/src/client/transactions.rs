use std::time::Duration;

use cynic::MutationBuilder;
use futures::TryStreamExt;
use futures_time::future::FutureExt;
use hex::ToHex;
use hopr_crypto_types::types::Hash;
use hopr_primitive_types::traits::ToHex as PrimitiveToHex;

use super::{BlokliClient, response_to_data};
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

#[async_trait::async_trait]
impl BlokliTransactionClient for BlokliClient {
    async fn submit_transaction(&self, signed_tx: &[u8]) -> Result<TxReceipt> {
        let resp = self
            .build_query(MutateSendTransaction::build(SendTransactionVariables {
                raw_transaction: signed_tx.encode_hex(),
            }))?
            .await?;

        response_to_data(resp)?.send_transaction.into()
    }

    async fn submit_and_track_transaction(&self, signed_tx: &[u8]) -> Result<TxId> {
        let resp = self
            .build_query(MutateTrackTransaction::build(SendTransactionVariables {
                raw_transaction: signed_tx.encode_hex(),
            }))?
            .await?;

        let tx: Result<Transaction> = response_to_data(resp)?.send_transaction_async.into();

        Ok(tx?.id.into_inner())
    }

    async fn submit_and_confirm_transaction(&self, signed_tx: &[u8], num_confirmations: usize) -> Result<TxReceipt> {
        let resp = self
            .build_query(MutateConfirmTransaction::build(ConfirmTransactionVariables {
                raw_transaction: signed_tx.encode_hex(),
                confirmations: num_confirmations.min(128) as i32,
            }))?
            .await?;

        let tx: Result<Transaction> = response_to_data(resp)?.send_transaction_sync.into();

        let hash_hex = tx?.transaction_hash.ok_or(ErrorKind::NoData)?;
        let hash = Hash::from_hex(&hash_hex.0).map_err(|_| ErrorKind::ParseError)?;
        let bytes: [u8; 32] = hash.as_ref().try_into().map_err(|_| ErrorKind::ParseError)?;
        Ok(bytes)
    }

    async fn track_transaction(&self, tx_id: TxId, client_timeout: Duration) -> Result<Transaction> {
        use cynic::SubscriptionBuilder;
        self.build_subscription_stream(SubscribeTransaction::build(TransactionsVariables { id: tx_id.into() }))?
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
