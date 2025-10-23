use super::BlokliClient;
use crate::api::*;

#[async_trait::async_trait]
impl BlokliTransactionClient for BlokliClient {
    async fn submit_signed_transaction(&self, _signed_tx: &[u8]) -> Result<TxReceipt> {
        unimplemented!()
    }
}
