use async_trait::async_trait;
use hopr_primitive_types::balance::HoprBalance;

use crate::errors::Result;

/// Trait defining all DB functionality needed by a packet/acknowledgement processing pipeline.
#[async_trait]
pub trait BlokliDbProtocolOperations {
    /// Loads (presumably cached) value of the network's minimum winning probability from the DB.
    async fn get_network_winning_probability(&self) -> Result<WinningProbability>;

    /// Loads (presumably cached) value of the network's minimum ticket price from the DB.
    async fn get_network_ticket_price(&self) -> Result<HoprBalance>;
}
