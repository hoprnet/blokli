use std::ops::{Mul, Sub};

use async_trait::async_trait;
#[cfg(all(feature = "prometheus", not(test)))]
use hopr_metrics::metrics::SimpleHistogram;
use hopr_network_types::prelude::ResolvedTransportRouting;
use hopr_parallelize::cpu::spawn_fifo_blocking;
use hopr_path::{Path, PathAddressResolver, ValidatedPath, errors::PathError};
use hopr_primitive_types::{HoprBalance, WinningProbability};
use tracing::{instrument, trace, warn};

use crate::{
    api::{
        errors::Result, prelude::DbError, protocol::BlokliDbProtocolOperations, resolver::BlokliDbResolverOperations,
    },
    channels::BlokliDbChannelOperations,
    db::BlokliDb,
    errors::DbSqlError,
    info::BlokliDbInfoOperations,
    prelude::BlokliDbTicketOperations,
};

#[cfg(all(feature = "prometheus", not(test)))]
lazy_static::lazy_static! {
    static ref METRIC_INCOMING_WIN_PROB: SimpleHistogram =
        SimpleHistogram::new(
            "hopr_tickets_incoming_win_probability",
            "Observes the winning probabilities on incoming tickets",
            vec![0.0, 0.0001, 0.001, 0.01, 0.05, 0.1, 0.15, 0.25, 0.3, 0.5],
        ).unwrap();
}

const SLOW_OP_MS: u128 = 150;

#[async_trait]
impl BlokliDbProtocolOperations for BlokliDb {
    async fn get_network_winning_probability(&self) -> Result<WinningProbability> {
        Ok(self
            .get_indexer_data(None)
            .await
            .map(|data| data.minimum_incoming_ticket_winning_prob)?)
    }

    async fn get_network_ticket_price(&self) -> Result<HoprBalance> {
        Ok(self.get_indexer_data(None).await.and_then(|data| {
            data.ticket_price
                .ok_or(DbSqlError::LogicalError("missing ticket price".into()))
        })?)
    }
}
