use super::BlokliClient;
use crate::api::*;

#[async_trait::async_trait]
impl BlokliTransactionClient for BlokliClient {
    async fn submit_signed_transaction(&self, _signed_tx: &[u8]) -> Result<TxReceipt> {
        unimplemented!()
    }

    async fn submit_tracked_transaction(&self, _signed_tx: &[u8]) -> Result<(TxReceipt, u64)> {
        unimplemented!()
    }

    async fn submit_and_confirm_transaction(&self, _signed_tx: &[u8], _num_confirmations: usize) -> Result<TxReceipt> {
        unimplemented!()
    }
}
