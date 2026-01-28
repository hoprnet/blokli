use blokli_api_types::{TicketParameters, TokenValueString};
use blokli_chain_rpc::HoprIndexerRpcOperations;
use blokli_db::{BlokliDbAllOperations, OpenTransaction};
use hopr_bindings::{
    hopr_ticket_price_oracle::HoprTicketPriceOracle::HoprTicketPriceOracleEvents,
    hopr_winning_probability_oracle::HoprWinningProbabilityOracle::HoprWinningProbabilityOracleEvents,
};
use hopr_internal_types::tickets::WinningProbability;
use hopr_primitive_types::{prelude::HoprBalance, traits::IntoEndian};
use tracing::{debug, info, trace};

use super::ContractEventHandlers;
use crate::{errors::Result, state::IndexerEvent};

#[cfg(all(feature = "prometheus", not(test)))]
lazy_static::lazy_static! {
    static ref METRIC_INDEXER_LOG_COUNTERS: hopr_metrics::MultiCounter =
        hopr_metrics::MultiCounter::new(
            "hopr_indexer_contract_log_count",
            "Counts of different HOPR contract logs processed by the Indexer",
            &["contract"]
    ).unwrap();
}

impl<T, Db> ContractEventHandlers<T, Db>
where
    T: HoprIndexerRpcOperations + Clone + Send + 'static,
    Db: BlokliDbAllOperations + Clone,
{
    pub(super) async fn on_ticket_winning_probability_oracle_event(
        &self,
        tx: &OpenTransaction,
        event: HoprWinningProbabilityOracleEvents,
        _is_synced: bool,
    ) -> Result<Vec<IndexerEvent>> {
        #[cfg(all(feature = "prometheus", not(test)))]
        METRIC_INDEXER_LOG_COUNTERS.increment(&["winning_probability_oracle"]);

        match event {
            HoprWinningProbabilityOracleEvents::WinProbUpdated(update) => {
                let old_minimum_win_prob: WinningProbability = update.oldWinProb.to_be_bytes().into();
                let new_minimum_win_prob: WinningProbability = update.newWinProb.to_be_bytes().into();

                trace!(
                    %old_minimum_win_prob,
                    %new_minimum_win_prob,
                    "on_ticket_minimum_win_prob_updated",
                );

                self.db
                    .set_minimum_incoming_ticket_win_prob(Some(tx), new_minimum_win_prob)
                    .await?;

                info!(
                    %old_minimum_win_prob,
                    %new_minimum_win_prob,
                    "minimum ticket winning probability updated"
                );

                // Fetch current ticket parameters to publish event
                let indexer_data = self.db.get_indexer_data(Some(tx)).await?;
                let ticket_params = TicketParameters {
                    min_ticket_winning_probability: f64::from(indexer_data.minimum_incoming_ticket_winning_prob),
                    ticket_price: TokenValueString(
                        indexer_data
                            .ticket_price
                            .map(|p| p.to_string())
                            .unwrap_or_else(|| "0 wxHOPR".to_string()),
                    ),
                };

                return Ok(vec![IndexerEvent::TicketParametersUpdated(ticket_params)]);
            }
            HoprWinningProbabilityOracleEvents::OwnershipTransferStarted(transfer_started) => {
                debug!(
                    previous_owner = %transfer_started.previousOwner,
                    new_owner = %transfer_started.newOwner,
                    "on_winning_prob_oracle_ownership_transfer_started"
                );
            }
            HoprWinningProbabilityOracleEvents::OwnershipTransferred(transferred) => {
                debug!(
                    previous_owner = %transferred.previousOwner,
                    new_owner = %transferred.newOwner,
                    "on_winning_prob_oracle_ownership_transferred"
                );
            }
        }
        Ok(vec![])
    }

    pub(super) async fn on_ticket_price_oracle_event(
        &self,
        tx: &OpenTransaction,
        event: HoprTicketPriceOracleEvents,
        _is_synced: bool,
    ) -> Result<Vec<IndexerEvent>> {
        #[cfg(all(feature = "prometheus", not(test)))]
        METRIC_INDEXER_LOG_COUNTERS.increment(&["ticket_price_oracle"]);

        match event {
            HoprTicketPriceOracleEvents::TicketPriceUpdated(update) => {
                trace!(
                    old = update._0.to_string(),
                    new = update._1.to_string(),
                    "on_ticket_price_updated",
                );

                self.db
                    .update_ticket_price(Some(tx), HoprBalance::from_be_bytes(update._1.to_be_bytes::<32>()))
                    .await?;

                info!(price = %update._1, "ticket price updated");

                // Fetch current ticket parameters to publish event
                let indexer_data = self.db.get_indexer_data(Some(tx)).await?;
                let ticket_params = TicketParameters {
                    min_ticket_winning_probability: f64::from(indexer_data.minimum_incoming_ticket_winning_prob),
                    ticket_price: TokenValueString(
                        indexer_data
                            .ticket_price
                            .map(|p| p.to_string())
                            .unwrap_or_else(|| "0 wxHOPR".to_string()),
                    ),
                };

                return Ok(vec![IndexerEvent::TicketParametersUpdated(ticket_params)]);
            }
            HoprTicketPriceOracleEvents::OwnershipTransferred(_event) => {
                // ignore ownership transfer event
            }
        }
        Ok(vec![])
    }
}

#[cfg(test)]
mod tests {
    use std::sync::Arc;

    use blokli_db::{BlokliDbGeneralModelOperations, db::BlokliDb, info::BlokliDbInfoOperations};
    use hopr_bindings::exports::alloy::{
        primitives::U256,
        sol_types::{SolEvent, SolValue},
    };
    use hopr_internal_types::tickets::WinningProbability;
    use hopr_primitive_types::prelude::SerializableLog;

    use crate::handlers::test_utils::test_helpers::*;

    #[tokio::test]
    async fn test_ticket_price_update() -> anyhow::Result<()> {
        let db = BlokliDb::new_in_memory().await?;
        let rpc_operations = MockIndexerRpcOperations::new();
        // ==> set mock expectations here
        let clonable_rpc_operations = ClonableMockOperations {
            //
            inner: Arc::new(rpc_operations),
        };
        let handlers = init_handlers(clonable_rpc_operations, db.clone());

        let encoded_data = (U256::from(1u64), U256::from(123u64)).abi_encode();

        let price_change_log = SerializableLog {
            address: handlers.addresses.ticket_price_oracle,
            topics: vec![
                hopr_bindings::hopr_ticket_price_oracle::HoprTicketPriceOracle::TicketPriceUpdated::SIGNATURE_HASH
                    .into(),
                // TicketPriceUpdatedFilter::signature().into()
            ],
            data: encoded_data,
            // data: encode(&[Token::Uint(EthU256::from(1u64)), Token::Uint(EthU256::from(123u64))]).into(),
            ..test_log()
        };

        assert_eq!(db.get_indexer_data(None).await?.ticket_price, None);

        db.begin_transaction()
            .await?
            .perform(|tx| Box::pin(async move { handlers.process_log_event(tx, price_change_log, true).await }))
            .await?;

        assert_eq!(
            db.get_indexer_data(None).await?.ticket_price.map(|p| p.amount()),
            Some(primitive_types::U256::from(123u64))
        );
        Ok(())
    }

    #[tokio::test]
    async fn test_minimum_win_prob_update() -> anyhow::Result<()> {
        let db = BlokliDb::new_in_memory().await?;
        let rpc_operations = MockIndexerRpcOperations::new();
        // ==> set mock expectations here
        let clonable_rpc_operations = ClonableMockOperations {
            //
            inner: Arc::new(rpc_operations),
        };
        let handlers = init_handlers(clonable_rpc_operations, db.clone());

        let encoded_data = (
            U256::from_be_slice(WinningProbability::ALWAYS.as_ref()),
            U256::from_be_slice(WinningProbability::try_from_f64(0.5)?.as_ref()),
        )
            .abi_encode();

        let win_prob_change_log = SerializableLog {
            address: handlers.addresses.winning_probability_oracle,
            topics: vec![
                hopr_bindings::hopr_winning_probability_oracle::HoprWinningProbabilityOracle::WinProbUpdated::SIGNATURE_HASH.into()],
            data: encoded_data,
            ..test_log()
        };

        assert_eq!(
            db.get_indexer_data(None).await?.minimum_incoming_ticket_winning_prob,
            1.0
        );

        db.begin_transaction()
            .await?
            .perform(|tx| Box::pin(async move { handlers.process_log_event(tx, win_prob_change_log, true).await }))
            .await?;

        assert_eq!(
            db.get_indexer_data(None).await?.minimum_incoming_ticket_winning_prob,
            0.5
        );
        Ok(())
    }
}
