use cynic::MutationBuilder;
use hex::ToHex;

use super::{BlokliClient, response_to_data};
use crate::{
    api::{internal::*, types::*, *},
    errors::ErrorKind,
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

        Ok(tx?.transaction_hash.ok_or(ErrorKind::NoData).and_then(|d| {
            hex::decode(d.0)
                .map_err(|_| ErrorKind::ParseError)
                .and_then(|d| d.try_into().map_err(|_| ErrorKind::ParseError))
        })?)
    }
}
