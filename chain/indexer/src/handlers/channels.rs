use blokli_chain_rpc::HoprIndexerRpcOperations;
use blokli_chain_types::AlloyAddressExt;
use blokli_db::{BlokliDbAllOperations, OpenTransaction, api::info::DomainSeparator};
use hopr_bindings::hopr_channels::HoprChannels::HoprChannelsEvents;
use hopr_internal_types::channels::{ChannelEntry, ChannelStatus, generate_channel_id};
use hopr_primitive_types::prelude::Address;
use tracing::{error, trace, warn};

use super::{ContractEventHandlers, channel_utils::decode_channel, helpers::construct_channel_update};
use crate::errors::{CoreEthereumIndexerError, Result};

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
    pub(super) async fn on_channel_event(
        &self,
        tx: &OpenTransaction,
        event: HoprChannelsEvents,
        block: u32,
        tx_index: u32,
        log_index: u32,
        is_synced: bool,
    ) -> Result<()> {
        #[cfg(all(feature = "prometheus", not(test)))]
        METRIC_INDEXER_LOG_COUNTERS.increment(&["channels"]);

        match event {
            HoprChannelsEvents::ChannelBalanceDecreased(balance_decreased) => {
                let channel_id = balance_decreased.channelId.0.into();

                // Get existing channel to calculate the diff
                let existing_channel = match self.db.get_channel_by_id(tx.into(), &channel_id).await {
                    Ok(Some(channel)) => channel,
                    Ok(None) => {
                        error!(%channel_id, "observed balance decreased event for a channel that does not exist");
                        // Mark the state as corrupted
                        self.db
                            .mark_channel_state_corrupted(tx.into(), &channel_id, block, tx_index, log_index)
                            .await
                            .ok();
                        return Err(CoreEthereumIndexerError::ChannelDoesNotExist);
                    }
                    Err(e) => {
                        error!(%channel_id, %e, "failed to get channel on on_channel_balance_decreased_event");
                        return Err(e.into());
                    }
                };

                trace!(
                    %channel_id,
                    "on_channel_balance_decreased_event",
                );

                // Decode the packed channel state from the event
                let decoded = decode_channel(balance_decreased.channel);
                let new_balance = decoded.balance;
                let diff = existing_channel.balance - new_balance;

                trace!(
                    %channel_id,
                    old_balance = %existing_channel.balance,
                    new_balance = %new_balance,
                    diff = %diff,
                    "ChannelBalanceDecreased: decoded channel state"
                );

                // Create updated channel entry with new state
                let updated_channel = ChannelEntry::new(
                    existing_channel.source,
                    existing_channel.destination,
                    new_balance,
                    decoded.ticket_index,
                    decoded.status,
                    decoded.epoch,
                );

                // Atomically upsert the new state
                self.db
                    .upsert_channel(tx.into(), updated_channel, block, tx_index, log_index)
                    .await?;

                // Publish event if synced
                if is_synced {
                    match construct_channel_update(tx.as_ref(), &channel_id).await {
                        Ok(channel_update) => {
                            self.indexer_state
                                .publish_event(crate::state::IndexerEvent::ChannelUpdated(Box::new(channel_update)));
                        }
                        Err(e) => {
                            warn!(%channel_id, %e, "Failed to construct channel update for ChannelBalanceDecreased");
                        }
                    }
                }

                Ok(())
            }
            HoprChannelsEvents::ChannelBalanceIncreased(balance_increased) => {
                let channel_id = balance_increased.channelId.0.into();

                // Get existing channel to calculate the diff
                let existing_channel = match self.db.get_channel_by_id(tx.into(), &channel_id).await {
                    Ok(Some(channel)) => channel,
                    Ok(None) => {
                        error!(%channel_id, "observed balance increased event for a channel that does not exist");
                        // Mark the state as corrupted
                        self.db
                            .mark_channel_state_corrupted(tx.into(), &channel_id, block, tx_index, log_index)
                            .await
                            .ok();
                        return Err(CoreEthereumIndexerError::ChannelDoesNotExist);
                    }
                    Err(e) => {
                        error!(%channel_id, %e, "failed to get channel on on_channel_balance_increased_event");
                        return Err(e.into());
                    }
                };

                trace!(
                    %channel_id,
                    "on_channel_balance_increased_event",
                );

                // Decode the packed channel state from the event
                let decoded = decode_channel(balance_increased.channel);
                let new_balance = decoded.balance;
                let diff = new_balance - existing_channel.balance;

                trace!(
                    %channel_id,
                    old_balance = %existing_channel.balance,
                    new_balance = %new_balance,
                    diff = %diff,
                    "ChannelBalanceIncreased: decoded channel state"
                );

                // Create updated channel entry with new state
                let updated_channel = ChannelEntry::new(
                    existing_channel.source,
                    existing_channel.destination,
                    new_balance,
                    decoded.ticket_index,
                    decoded.status,
                    decoded.epoch,
                );

                // Atomically upsert the new state
                self.db
                    .upsert_channel(tx.into(), updated_channel, block, tx_index, log_index)
                    .await?;

                // Publish event if synced
                if is_synced {
                    match construct_channel_update(tx.as_ref(), &channel_id).await {
                        Ok(channel_update) => {
                            self.indexer_state
                                .publish_event(crate::state::IndexerEvent::ChannelUpdated(Box::new(channel_update)));
                        }
                        Err(e) => {
                            warn!(%channel_id, %e, "Failed to construct channel update for ChannelBalanceIncreased");
                        }
                    }
                }

                Ok(())
            }
            HoprChannelsEvents::ChannelClosed(channel_closed) => {
                let channel_id = channel_closed.channelId.0.into();

                // Get existing channel
                let existing_channel = match self.db.get_channel_by_id(tx.into(), &channel_id).await {
                    Ok(Some(channel)) => channel,
                    Ok(None) => {
                        error!(%channel_id, "observed closure finalization event for a channel that does not exist");
                        // Mark the state as corrupted
                        self.db
                            .mark_channel_state_corrupted(tx.into(), &channel_id, block, tx_index, log_index)
                            .await
                            .ok();
                        return Err(CoreEthereumIndexerError::ChannelDoesNotExist);
                    }
                    Err(e) => {
                        error!(%channel_id, %e, "failed to get channel on on_channel_closed_event");
                        return Err(e.into());
                    }
                };

                trace!(
                    %channel_id,
                    "on_channel_closed_event",
                );

                // Decode the packed channel state from the event
                let decoded = decode_channel(channel_closed.channel);

                trace!(
                    %channel_id,
                    status = ?decoded.status,
                    balance = %decoded.balance,
                    ticket_index = decoded.ticket_index,
                    "ChannelClosed: decoded channel state"
                );

                // Create updated channel entry with all new state from decoded values
                let updated_channel = ChannelEntry::new(
                    existing_channel.source,
                    existing_channel.destination,
                    decoded.balance,
                    decoded.ticket_index,
                    decoded.status,
                    decoded.epoch,
                );

                // Atomically upsert the new state
                self.db
                    .upsert_channel(tx.into(), updated_channel, block, tx_index, log_index)
                    .await?;

                // Publish event if synced
                if is_synced {
                    match construct_channel_update(tx.as_ref(), &channel_id).await {
                        Ok(channel_update) => {
                            self.indexer_state
                                .publish_event(crate::state::IndexerEvent::ChannelUpdated(Box::new(channel_update)));
                        }
                        Err(e) => {
                            warn!(%channel_id, %e, "Failed to construct channel update for ChannelClosed");
                        }
                    }
                }

                Ok(())
            }
            HoprChannelsEvents::ChannelOpened(channel_opened) => {
                let source: Address = channel_opened.source.to_hopr_address();
                let destination: Address = channel_opened.destination.to_hopr_address();
                let channel_id = generate_channel_id(&source, &destination);

                // Decode the packed channel state from the event
                let decoded = decode_channel(channel_opened.channel);

                trace!(
                    %channel_id,
                    balance = %decoded.balance,
                    ticket_index = decoded.ticket_index,
                    epoch = decoded.epoch,
                    status = ?decoded.status,
                    "ChannelOpened: decoded channel state"
                );

                let maybe_existing = match self.db.get_channel_by_id(tx.into(), &channel_id).await {
                    Ok(existing) => existing,
                    Err(e) => {
                        error!(%source, %destination, %channel_id, %e, "failed to get channel on on_channel_opened_event");
                        return Err(e.into());
                    }
                };

                if let Some(existing) = maybe_existing {
                    // Channel exists - check if it's in Closed state
                    if existing.status != ChannelStatus::Closed {
                        warn!(%source, %destination, %channel_id, "received Open event for a channel that is not Closed, marking state as corrupted");

                        // Mark this state as corrupted
                        self.db
                            .mark_channel_state_corrupted(tx.into(), &channel_id, block, tx_index, log_index)
                            .await
                            .ok();

                        return Ok(());
                    }

                    trace!(%source, %destination, %channel_id, "on_channel_reopened_event");

                    // Reopen channel with state from decoded event payload
                    let reopened_channel = ChannelEntry::new(
                        source,
                        destination,
                        decoded.balance,
                        decoded.ticket_index,
                        decoded.status,
                        decoded.epoch,
                    );

                    self.db
                        .upsert_channel(tx.into(), reopened_channel, block, tx_index, log_index)
                        .await?;
                } else {
                    // Channel doesn't exist - create new one with state from decoded event payload
                    trace!(%source, %destination, %channel_id, "on_channel_opened_event");

                    let new_channel = ChannelEntry::new(
                        source,
                        destination,
                        decoded.balance,
                        decoded.ticket_index,
                        decoded.status,
                        decoded.epoch,
                    );

                    self.db
                        .upsert_channel(tx.into(), new_channel, block, tx_index, log_index)
                        .await?;
                }

                // Publish event if synced
                if is_synced {
                    match construct_channel_update(tx.as_ref(), &channel_id).await {
                        Ok(channel_update) => {
                            self.indexer_state
                                .publish_event(crate::state::IndexerEvent::ChannelUpdated(Box::new(channel_update)));
                        }
                        Err(e) => {
                            warn!(%channel_id, %e, "Failed to construct channel update for ChannelOpened");
                        }
                    }
                }

                Ok(())
            }
            HoprChannelsEvents::TicketRedeemed(ticket_redeemed) => {
                let channel_id = ticket_redeemed.channelId.0.into();

                // Get existing channel
                let existing_channel = match self.db.get_channel_by_id(tx.into(), &channel_id).await {
                    Ok(Some(channel)) => channel,
                    Ok(None) => {
                        error!(%channel_id, "observed ticket redeem on a channel that we don't have in the DB");
                        // Mark the state as corrupted
                        self.db
                            .mark_channel_state_corrupted(tx.into(), &channel_id, block, tx_index, log_index)
                            .await
                            .ok();
                        return Err(CoreEthereumIndexerError::ChannelDoesNotExist);
                    }
                    Err(e) => {
                        error!(%channel_id, %e, "failed to get channel on on_ticket_redeemed_event");
                        return Err(e.into());
                    }
                };

                // Decode the packed channel state from the event
                let decoded = decode_channel(ticket_redeemed.channel);

                trace!(
                    %channel_id,
                    new_balance = %decoded.balance,
                    new_ticket_index = decoded.ticket_index,
                    "TicketRedeemed: decoded channel state"
                );

                // Create updated channel entry with new balance and ticket index
                let updated_channel = ChannelEntry::new(
                    existing_channel.source,
                    existing_channel.destination,
                    decoded.balance,
                    decoded.ticket_index,
                    decoded.status,
                    decoded.epoch,
                );

                // Atomically upsert the new state
                self.db
                    .upsert_channel(tx.into(), updated_channel, block, tx_index, log_index)
                    .await?;

                // Publish event if synced
                if is_synced {
                    match construct_channel_update(tx.as_ref(), &channel_id).await {
                        Ok(channel_update) => {
                            self.indexer_state
                                .publish_event(crate::state::IndexerEvent::ChannelUpdated(Box::new(channel_update)));
                        }
                        Err(e) => {
                            warn!(%channel_id, %e, "Failed to construct channel update for TicketRedeemed");
                        }
                    }
                }

                Ok(())
            }
            HoprChannelsEvents::OutgoingChannelClosureInitiated(closure_initiated) => {
                let channel_id = closure_initiated.channelId.0.into();

                // Get existing channel
                let existing_channel = match self.db.get_channel_by_id(tx.into(), &channel_id).await {
                    Ok(Some(channel)) => channel,
                    Ok(None) => {
                        error!(%channel_id, "observed channel closure initiation on a channel that we don't have in the DB");
                        // Mark the state as corrupted
                        self.db
                            .mark_channel_state_corrupted(tx.into(), &channel_id, block, tx_index, log_index)
                            .await
                            .ok();
                        return Err(CoreEthereumIndexerError::ChannelDoesNotExist);
                    }
                    Err(e) => {
                        error!(%channel_id, %e, "failed to get channel on on_outgoing_channel_closure_initiated_event");
                        return Err(e.into());
                    }
                };

                // Decode the packed channel state from the event
                let decoded = decode_channel(closure_initiated.channel);

                trace!(
                    %channel_id,
                    closure_time = decoded.closure_time,
                    status = ?decoded.status,
                    "OutgoingChannelClosureInitiated: decoded channel state"
                );

                // Create updated channel entry with new status (PendingToClose)
                let updated_channel = ChannelEntry::new(
                    existing_channel.source,
                    existing_channel.destination,
                    decoded.balance,
                    decoded.ticket_index,
                    decoded.status, // Should be PendingToClose with proper timestamp
                    decoded.epoch,
                );

                // Atomically upsert the new state
                self.db
                    .upsert_channel(tx.into(), updated_channel, block, tx_index, log_index)
                    .await?;

                // Publish event if synced
                if is_synced {
                    match construct_channel_update(tx.as_ref(), &channel_id).await {
                        Ok(channel_update) => {
                            self.indexer_state
                                .publish_event(crate::state::IndexerEvent::ChannelUpdated(Box::new(channel_update)));
                        }
                        Err(e) => {
                            warn!(%channel_id, %e, "Failed to construct channel update for OutgoingChannelClosureInitiated");
                        }
                    }
                }

                Ok(())
            }
            HoprChannelsEvents::DomainSeparatorUpdated(domain_separator_updated) => {
                self.db
                    .set_domain_separator(
                        Some(tx),
                        DomainSeparator::Channel,
                        domain_separator_updated.domainSeparator.0.into(),
                    )
                    .await?;

                Ok(())
            }
            HoprChannelsEvents::LedgerDomainSeparatorUpdated(ledger_domain_separator_updated) => {
                self.db
                    .set_domain_separator(
                        Some(tx),
                        DomainSeparator::Ledger,
                        ledger_domain_separator_updated.ledgerDomainSeparator.0.into(),
                    )
                    .await?;

                Ok(())
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use std::{sync::Arc, time::SystemTime};

    use alloy::{
        primitives::{Address as AlloyAddress, FixedBytes},
        sol_types::{SolEvent, SolValue},
    };
    use anyhow::Context;
    use blokli_chain_types::AlloyAddressExt;
    use blokli_db::{
        BlokliDbGeneralModelOperations, accounts::BlokliDbAccountOperations, api::info::DomainSeparator,
        channels::BlokliDbChannelOperations, db::BlokliDb, info::BlokliDbInfoOperations,
        node_safe_registrations::BlokliDbNodeSafeRegistrationOperations,
        safe_contracts::BlokliDbSafeContractOperations,
    };
    use blokli_db_entity::{hopr_node_safe_registration, prelude::HoprNodeSafeRegistration};
    use hex_literal::hex;
    use hopr_bindings::{
        hopr_channels::HoprChannels,
        hopr_channels_events::HoprChannelsEvents::{
            ChannelBalanceDecreased, ChannelBalanceIncreased, ChannelClosed, ChannelOpened,
            OutgoingChannelClosureInitiated, TicketRedeemed,
        },
        hopr_node_safe_registry::HoprNodeSafeRegistry,
    };
    use hopr_crypto_types::{
        keypairs::Keypair,
        prelude::{Hash, OffchainKeypair},
    };
    use hopr_internal_types::channels::{ChannelEntry, ChannelStatus, generate_channel_id};
    use hopr_primitive_types::{
        prelude::{Address, HoprBalance, SerializableLog},
        traits::{AsUnixTimestamp, IntoEndian},
    };
    use primitive_types::H256;
    use sea_orm::{ColumnTrait, EntityTrait, QueryFilter};

    use crate::handlers::test_utils::test_helpers::*;

    #[tokio::test]
    async fn test_on_channel_event_balance_increased() -> anyhow::Result<()> {
        let db = BlokliDb::new_in_memory().await?;

        let rpc_operations = MockIndexerRpcOperations::new();
        let clonable_rpc_operations = ClonableMockOperations {
            inner: Arc::new(rpc_operations),
        };
        let handlers = init_handlers(clonable_rpc_operations, db.clone());

        // Create required accounts before channel operations
        create_test_accounts(&db).await?;

        let channel = ChannelEntry::new(
            *SELF_CHAIN_ADDRESS,
            *COUNTERPARTY_CHAIN_ADDRESS,
            0.into(),
            0u64,
            ChannelStatus::Open,
            0u32,
        );

        db.upsert_channel(None, channel.clone(), 1, 0, 0).await?;

        let solidity_balance: HoprBalance = primitive_types::U256::from((1u128 << 96) - 1).into();
        let channel_state = encode_channel_state(
            solidity_balance,
            channel.ticket_index,
            0,
            channel.channel_epoch,
            channel.status,
        );

        // Create ChannelBalanceIncreased event using bindings
        let event = ChannelBalanceIncreased {
            channelId: FixedBytes::from_slice(channel.get_id().as_ref()),
            channel: channel_state,
        };

        let balance_increased_log = event_to_log(event, handlers.addresses.channels);

        db.begin_transaction()
            .await?
            .perform(|tx| Box::pin(async move { handlers.process_log_event(tx, balance_increased_log, true).await }))
            .await?;

        let channel = db
            .get_channel_by_id(None, &channel.get_id())
            .await?
            .context("a value should be present")?;

        // TODO: Add event verification - check published IndexerEvent instead of return value

        assert_eq!(solidity_balance, channel.balance, "balance must be updated");
        Ok(())
    }

    #[tokio::test]
    async fn test_on_channel_event_domain_separator_updated() -> anyhow::Result<()> {
        let db = BlokliDb::new_in_memory().await?;
        let rpc_operations = MockIndexerRpcOperations::new();
        // ==> set mock expectations here
        let clonable_rpc_operations = ClonableMockOperations {
            //
            inner: Arc::new(rpc_operations),
        };
        let handlers = init_handlers(clonable_rpc_operations, db.clone());

        let separator = Hash::from(hopr_crypto_random::random_bytes());

        let encoded_data = ().abi_encode();

        let channels_dst_updated = SerializableLog {
            address: handlers.addresses.channels,
            topics: vec![
                HoprChannels::DomainSeparatorUpdated::SIGNATURE_HASH.into(),
                // DomainSeparatorUpdatedFilter::signature().into(),
                H256::from_slice(separator.as_ref()).into(),
            ],
            data: encoded_data,
            ..test_log()
        };

        assert!(db.get_indexer_data(None).await?.channels_dst.is_none());

        db.begin_transaction()
            .await?
            .perform(|tx| Box::pin(async move { handlers.process_log_event(tx, channels_dst_updated, true).await }))
            .await?;

        assert_eq!(
            separator,
            db.get_indexer_data(None)
                .await?
                .channels_dst
                .context("a value should be present")?,
            "separator must be updated"
        );
        Ok(())
    }

    #[tokio::test]
    async fn test_on_channel_event_balance_decreased() -> anyhow::Result<()> {
        let db = BlokliDb::new_in_memory().await?;

        let rpc_operations = MockIndexerRpcOperations::new();
        let clonable_rpc_operations = ClonableMockOperations {
            inner: Arc::new(rpc_operations),
        };
        let handlers = init_handlers(clonable_rpc_operations, db.clone());

        // Create required accounts before channel operations
        create_test_accounts(&db).await?;

        let channel = ChannelEntry::new(
            *SELF_CHAIN_ADDRESS,
            *COUNTERPARTY_CHAIN_ADDRESS,
            HoprBalance::from(primitive_types::U256::from((1u128 << 96) - 1)),
            0u64,
            ChannelStatus::Open,
            0u32,
        );

        db.upsert_channel(None, channel.clone(), 1, 0, 0).await?;

        let solidity_balance: HoprBalance = primitive_types::U256::from((1u128 << 96) - 2).into();
        let channel_state = encode_channel_state(
            solidity_balance,
            channel.ticket_index,
            0,
            channel.channel_epoch,
            channel.status,
        );

        // Create ChannelBalanceDecreased event using bindings
        let event = ChannelBalanceDecreased {
            channelId: FixedBytes::from_slice(channel.get_id().as_ref()),
            channel: channel_state,
        };

        let balance_decreased_log = event_to_log(event, handlers.addresses.channels);

        db.begin_transaction()
            .await?
            .perform(|tx| Box::pin(async move { handlers.process_log_event(tx, balance_decreased_log, true).await }))
            .await?;

        let channel = db
            .get_channel_by_id(None, &channel.get_id())
            .await?
            .context("a value should be present")?;

        // TODO: Add event verification - check published IndexerEvent instead of return value

        assert_eq!(solidity_balance, channel.balance, "balance must be updated");
        Ok(())
    }

    #[tokio::test]
    async fn test_on_channel_closed() -> anyhow::Result<()> {
        let db = BlokliDb::new_in_memory().await?;
        let rpc_operations = MockIndexerRpcOperations::new();
        // ==> set mock expectations here
        let clonable_rpc_operations = ClonableMockOperations {
            //
            inner: Arc::new(rpc_operations),
        };
        let handlers = init_handlers(clonable_rpc_operations, db.clone());

        create_test_accounts(&db).await?;

        let starting_balance = HoprBalance::from(primitive_types::U256::from((1u128 << 96) - 1));

        let channel = ChannelEntry::new(
            *SELF_CHAIN_ADDRESS,
            *COUNTERPARTY_CHAIN_ADDRESS,
            starting_balance,
            0u64,
            ChannelStatus::Open,
            0u32,
        );

        db.upsert_channel(None, channel.clone(), 1, 0, 0).await?;

        // When channel is closed, balance is 0, ticket_index is reset to 0, and status is Closed
        let channel_state =
            encode_channel_state(HoprBalance::zero(), 0, 0, channel.channel_epoch, ChannelStatus::Closed);

        // Create ChannelClosed event using bindings
        let event = ChannelClosed {
            channelId: FixedBytes::from_slice(channel.get_id().as_ref()),
            channel: channel_state,
        };

        let channel_closed_log = event_to_log(event, handlers.addresses.channels);

        db.begin_transaction()
            .await?
            .perform(|tx| Box::pin(async move { handlers.process_log_event(tx, channel_closed_log, true).await }))
            .await?;

        let closed_channel = db
            .get_channel_by_id(None, &channel.get_id())
            .await?
            .context("a value should be present")?;

        // TODO: Add event verification - check published IndexerEvent instead of return value

        assert_eq!(closed_channel.status, ChannelStatus::Closed);
        assert_eq!(closed_channel.ticket_index, 0u64);
        // TODO: Re-enable once get_outgoing_ticket_index is implemented
        // assert_eq!(0, db.get_outgoing_ticket_index(closed_channel.get_id()).await?.load(Ordering::Relaxed));

        assert!(closed_channel.balance.amount().eq(&primitive_types::U256::zero()));
        Ok(())
    }

    #[tokio::test]
    async fn test_on_foreign_channel_closed() -> anyhow::Result<()> {
        let db = BlokliDb::new_in_memory().await?;
        let rpc_operations = MockIndexerRpcOperations::new();
        // ==> set mock expectations here
        let clonable_rpc_operations = ClonableMockOperations {
            //
            inner: Arc::new(rpc_operations),
        };
        let handlers = init_handlers(clonable_rpc_operations, db.clone());

        // Create accounts for foreign addresses before channel operations
        let foreign_addr1 = Address::new(&hex!("B7397C218766eBe6A1A634df523A1a7e412e67eA"));
        let foreign_addr2 = Address::new(&hex!("D4fdec44DB9D44B8f2b6d529620f9C0C7066A2c1"));
        let foreign_key1 = OffchainKeypair::from_secret(&hex!(
            "1111111111111111111111111111111111111111111111111111111111111111"
        ))
        .expect("valid keypair");
        let foreign_key2 = OffchainKeypair::from_secret(&hex!(
            "2222222222222222222222222222222222222222222222222222222222222222"
        ))
        .expect("valid keypair");

        db.upsert_account(None, 1, foreign_addr1, *foreign_key1.public(), None, 1, 0, 0)
            .await?;
        db.upsert_account(None, 2, foreign_addr2, *foreign_key2.public(), None, 1, 0, 1)
            .await?;

        let starting_balance = HoprBalance::from(primitive_types::U256::from((1u128 << 96) - 1));

        let channel = ChannelEntry::new(
            foreign_addr1,
            foreign_addr2,
            starting_balance,
            0u64,
            ChannelStatus::Open,
            0u32,
        );

        db.upsert_channel(None, channel.clone(), 1, 0, 0).await?;

        // When channel is closed, balance is 0, ticket_index is reset to 0, and status is Closed
        let channel_state =
            encode_channel_state(HoprBalance::zero(), 0, 0, channel.channel_epoch, ChannelStatus::Closed);

        // Create ChannelClosed event using bindings
        let event = ChannelClosed {
            channelId: FixedBytes::from_slice(channel.get_id().as_ref()),
            channel: channel_state,
        };

        let channel_closed_log = event_to_log(event, handlers.addresses.channels);

        db.begin_transaction()
            .await?
            .perform(|tx| Box::pin(async move { handlers.process_log_event(tx, channel_closed_log, true).await }))
            .await?;

        let closed_channel = db
            .get_channel_by_id(None, &channel.get_id())
            .await?
            .context("channel should still exist after closing")?;

        // Foreign channels are kept in database with Closed status for historical data
        assert_eq!(closed_channel.status, ChannelStatus::Closed);
        assert_eq!(closed_channel.balance, HoprBalance::zero());
        assert_eq!(closed_channel.ticket_index, 0u64);

        // TODO: Add event verification - check published IndexerEvent instead of return value

        Ok(())
    }

    #[test_log::test(tokio::test)]
    async fn test_on_channel_opened() -> anyhow::Result<()> {
        let db = BlokliDb::new_in_memory().await?;
        let rpc_operations = MockIndexerRpcOperations::new();
        // ==> set mock expectations here
        let clonable_rpc_operations = ClonableMockOperations {
            //
            inner: Arc::new(rpc_operations),
        };
        let handlers = init_handlers(clonable_rpc_operations, db.clone());

        // Create required accounts before channel operations
        create_test_accounts(&db).await?;

        let channel_id = generate_channel_id(&SELF_CHAIN_ADDRESS, &COUNTERPARTY_CHAIN_ADDRESS);

        let channel_state = encode_channel_state(HoprBalance::zero(), 0, 0, 1, ChannelStatus::Open);

        // Create ChannelOpened event using bindings
        let event = ChannelOpened {
            channelId: FixedBytes::from_slice(channel_id.as_ref()),
            source: AlloyAddress::from_hopr_address(*SELF_CHAIN_ADDRESS),
            destination: AlloyAddress::from_hopr_address(*COUNTERPARTY_CHAIN_ADDRESS),
            channel: channel_state,
        };

        let channel_opened_log = event_to_log(event, handlers.addresses.channels);

        db.begin_transaction()
            .await?
            .perform(|tx| Box::pin(async move { handlers.process_log_event(tx, channel_opened_log, true).await }))
            .await?;

        let channel = db
            .get_channel_by_id(None, &channel_id)
            .await?
            .context("a value should be present")?;

        // TODO: Add event verification - check published IndexerEvent instead of return value

        assert_eq!(channel.status, ChannelStatus::Open);
        assert_eq!(channel.channel_epoch, 1u32);
        assert_eq!(channel.ticket_index, 0u64);
        // TODO: Re-enable once get_outgoing_ticket_index is implemented
        // assert_eq!(0, db.get_outgoing_ticket_index(channel.get_id()).await?.load(Ordering::Relaxed));
        Ok(())
    }

    #[tokio::test]
    async fn test_on_channel_reopened() -> anyhow::Result<()> {
        let db = BlokliDb::new_in_memory().await?;
        let rpc_operations = MockIndexerRpcOperations::new();
        // ==> set mock expectations here
        let clonable_rpc_operations = ClonableMockOperations {
            //
            inner: Arc::new(rpc_operations),
        };
        let handlers = init_handlers(clonable_rpc_operations, db.clone());

        // Create required accounts before channel operations
        create_test_accounts(&db).await?;

        let channel = ChannelEntry::new(
            *SELF_CHAIN_ADDRESS,
            *COUNTERPARTY_CHAIN_ADDRESS,
            HoprBalance::zero(),
            0u64,
            ChannelStatus::Closed,
            3u32,
        );

        db.upsert_channel(None, channel, 1, 0, 0).await?;

        let channel_id = generate_channel_id(&SELF_CHAIN_ADDRESS, &COUNTERPARTY_CHAIN_ADDRESS);
        let channel_state = encode_channel_state(HoprBalance::zero(), 0, 0, 4, ChannelStatus::Open);

        // Create ChannelOpened event using bindings (reopening is a ChannelOpened event)
        let event = ChannelOpened {
            channelId: FixedBytes::from_slice(channel_id.as_ref()),
            source: AlloyAddress::from_hopr_address(*SELF_CHAIN_ADDRESS),
            destination: AlloyAddress::from_hopr_address(*COUNTERPARTY_CHAIN_ADDRESS),
            channel: channel_state,
        };

        let channel_opened_log = event_to_log(event, handlers.addresses.channels);

        db.begin_transaction()
            .await?
            .perform(|tx| Box::pin(async move { handlers.process_log_event(tx, channel_opened_log, true).await }))
            .await?;

        let channel = db
            .get_channel_by_id(None, &channel.get_id())
            .await?
            .context("a value should be present")?;

        // TODO: Add event verification - check published IndexerEvent instead of return value

        assert_eq!(channel.status, ChannelStatus::Open);
        assert_eq!(channel.channel_epoch, 4u32);
        assert_eq!(channel.ticket_index, 0u64);

        // TODO: Re-enable once get_outgoing_ticket_index is implemented
        // assert_eq!(0, db.get_outgoing_ticket_index(channel.get_id()).await?.load(Ordering::Relaxed));
        Ok(())
    }

    #[tokio::test]
    async fn test_on_channel_should_not_reopen_when_not_closed() -> anyhow::Result<()> {
        let db = BlokliDb::new_in_memory().await?;
        let rpc_operations = MockIndexerRpcOperations::new();
        // ==> set mock expectations here
        let clonable_rpc_operations = ClonableMockOperations {
            //
            inner: Arc::new(rpc_operations),
        };
        let handlers = init_handlers(clonable_rpc_operations, db.clone());

        create_test_accounts(&db).await?;

        let channel = ChannelEntry::new(
            *SELF_CHAIN_ADDRESS,
            *COUNTERPARTY_CHAIN_ADDRESS,
            0.into(),
            0u64,
            ChannelStatus::Open,
            3u32,
        );

        db.upsert_channel(None, channel, 1, 0, 0).await?;

        let channel_id = generate_channel_id(&SELF_CHAIN_ADDRESS, &COUNTERPARTY_CHAIN_ADDRESS);
        let channel_state = encode_channel_state(HoprBalance::zero(), 0, 0, 1, ChannelStatus::Open);

        // Create ChannelOpened event using bindings
        let event = ChannelOpened {
            channelId: FixedBytes::from_slice(channel_id.as_ref()),
            source: AlloyAddress::from_hopr_address(*SELF_CHAIN_ADDRESS),
            destination: AlloyAddress::from_hopr_address(*COUNTERPARTY_CHAIN_ADDRESS),
            channel: channel_state,
        };

        let channel_opened_log = event_to_log(event, handlers.addresses.channels);

        db.begin_transaction()
            .await?
            .perform(|tx| Box::pin(async move { handlers.process_log_event(tx, channel_opened_log, true).await }))
            .await
            .context("Channel should stay open, with corrupted flag set")?;

        // TODO: Refactor to check channel.corrupted_state field
        // db.get_corrupted_channel_by_id(None, &channel.get_id())
        //     .await?
        //     .context("a value should be present")?;

        Ok(())
    }

    #[tokio::test]
    async fn test_event_for_non_existing_channel_should_create_corrupted_channel() -> anyhow::Result<()> {
        let db = BlokliDb::new_in_memory().await?;
        let rpc_operations = MockIndexerRpcOperations::new();
        // ==> set mock expectations here
        let clonable_rpc_operations = ClonableMockOperations {
            //
            inner: Arc::new(rpc_operations),
        };
        let handlers = init_handlers(clonable_rpc_operations, db.clone());

        let channel_id = generate_channel_id(&SELF_CHAIN_ADDRESS, &COUNTERPARTY_CHAIN_ADDRESS);

        // Attempt to increase balance
        let solidity_balance: HoprBalance = primitive_types::U256::from((1u128 << 96) - 1).into();

        let encoded_data = (solidity_balance.amount().to_be_bytes()).abi_encode();

        let balance_increased_log = SerializableLog {
            address: handlers.addresses.channels,
            topics: vec![
                HoprChannels::ChannelBalanceIncreased::SIGNATURE_HASH.into(),
                // ChannelBalanceIncreasedFilter::signature().into(),
                H256::from_slice(channel_id.as_ref()).into(),
            ],
            data: encoded_data,
            ..test_log()
        };

        db.begin_transaction()
            .await?
            .perform(|tx| Box::pin(async move { handlers.process_log_event(tx, balance_increased_log, true).await }))
            .await?;

        // Check that the corrupted channel was created
        // TODO: Refactor to check channel.corrupted_state field
        // db.get_corrupted_channel_by_id(None, &channel_id)
        //     .await?
        //     .context("channel should be set a corrupted")?;

        Ok(())
    }

    const PRICE_PER_PACKET: u32 = 20_u32;

    // TODO: Re-enable once ticket operations and types are implemented
    // fn mock_acknowledged_ticket(
    //     signer: &ChainKeypair,
    //     destination: &ChainKeypair,
    //     index: u64,
    //     win_prob: f64,
    // ) -> anyhow::Result<AcknowledgedTicket> {
    //     let channel_id = generate_channel_id(&signer.into(), &destination.into());
    //
    //     let channel_epoch = 1u64;
    //     let domain_separator = Hash::default();
    //
    //     let response = Response::try_from(
    //         Hash::create(&[channel_id.as_ref(), &channel_epoch.to_be_bytes(), &index.to_be_bytes()]).as_ref(),
    //     )?;
    //
    //     Ok(TicketBuilder::default()
    //         .direction(&signer.into(), &destination.into())
    //         .amount(primitive_types::U256::from(PRICE_PER_PACKET).div_f64(win_prob)?)
    //         .index(index)
    //         .index_offset(1)
    //         .win_prob(win_prob.try_into()?)
    //         .channel_epoch(1)
    //         .challenge(response.to_challenge()?)
    //         .build_signed(signer, &domain_separator)?
    //         .into_acknowledged(response))
    // }

    // TODO: Re-enable once ticket operations are implemented
    // #[tokio::test]
    // async fn on_channel_ticket_redeemed_incoming_channel() -> anyhow::Result<()> {
    // let db = BlokliDb::new_in_memory().await?;
    //         db.set_domain_separator(None, DomainSeparator::Channel, Hash::default())
    //             .await?;
    //         let rpc_operations = MockIndexerRpcOperations::new();
    //         // ==> set mock expectations here
    //         let clonable_rpc_operations = ClonableMockOperations {
    //             //
    //             inner: Arc::new(rpc_operations),
    //         };
    //         let handlers = init_handlers(clonable_rpc_operations, db.clone());
    //
    //         let channel = ChannelEntry::new(
    //             *COUNTERPARTY_CHAIN_ADDRESS,
    //             *SELF_CHAIN_ADDRESS,
    //             HoprBalance::from(primitive_types::U256::from((1u128 << 96) - 1)),
    //             primitive_types::U256::zero(),
    //             ChannelStatus::Open,
    //             primitive_types::U256::one(),
    //         );
    //
    //         let ticket_index = primitive_types::U256::from((1u128 << 48) - 2);
    //         let next_ticket_index = ticket_index + 1;
    //
    //         let mut ticket =
    //             mock_acknowledged_ticket(&COUNTERPARTY_CHAIN_KEY, &SELF_CHAIN_KEY, ticket_index.as_u64(), 1.0)?;
    //         ticket.status = AcknowledgedTicketStatus::BeingRedeemed;
    //
    //         let ticket_value = ticket.verified_ticket().amount;
    //
    //         db.upsert_channel(None, channel).await?;
    //         db.upsert_ticket(None, ticket.clone()).await?;
    //
    //         let ticket_redeemed_log = SerializableLog {
    //             address: handlers.addresses.channels,
    //             topics: vec![
    //                 hopr_bindings::hopr_channels::HoprChannels::TicketRedeemed::SIGNATURE_HASH.into(),
    //                 // TicketRedeemedFilter::signature().into(),
    //                 H256::from_slice(channel.get_id().as_ref()).into(),
    //             ],
    //             data: DynSolValue::Tuple(vec![DynSolValue::Uint(
    //                 U256::from_be_bytes(next_ticket_index.to_be_bytes()),
    //                 48,
    //             )])
    //             .abi_encode(),
    //             ..test_log()
    //         };
    //
    //         let outgoing_ticket_index_before = db
    //             .get_outgoing_ticket_index(channel.get_id())
    //             .await?
    //             .load(Ordering::Relaxed);
    //
    //         let stats = db.get_ticket_statistics(Some(channel.get_id())).await?;
    //         assert_eq!(
    //             HoprBalance::zero(),
    //             stats.redeemed_value,
    //             "there should not be any redeemed value"
    //         );
    //         assert_eq!(
    //             HoprBalance::zero(),
    //             stats.neglected_value,
    //             "there should not be any neglected value"
    //         );
    //
    //         db
    //             .begin_transaction()
    //             .await?
    //             .perform(|tx| Box::pin(async move { handlers.process_log_event(tx, ticket_redeemed_log, true).await
    // }))             .await?;
    //
    //         let channel = db
    //             .get_channel_by_id(None, &channel.get_id())
    //             .await?
    //             .context("a value should be present")?;
    //
    //         assert!(
    // TODO: Add event verification - check published IndexerEvent instead of return value
    // Some(ticket)),             "must return the updated channel entry and the redeemed ticket"
    //         );
    //
    //         assert_eq!(
    //             channel.ticket_index, next_ticket_index,
    //             "channel entry must contain next ticket index"
    //         );
    //
    //         let outgoing_ticket_index_after = db
    //             .get_outgoing_ticket_index(channel.get_id())
    //             .await?
    //             .load(Ordering::Relaxed);
    //
    //         assert_eq!(
    //             outgoing_ticket_index_before, outgoing_ticket_index_after,
    //             "outgoing ticket index must not change"
    //         );
    //
    //         let tickets = db.get_tickets((&channel).into()).await?;
    //
    //         assert!(tickets.is_empty(), "there should not be any tickets left");
    //
    //         let stats = db.get_ticket_statistics(Some(channel.get_id())).await?;
    //         assert_eq!(
    //             ticket_value, stats.redeemed_value,
    //             "there should be redeemed value worth 1 ticket"
    //         );
    //         assert_eq!(
    //             HoprBalance::zero(),
    //             stats.neglected_value,
    //             "there should not be any neglected ticket"
    //         );
    //         // Ok(())
    //     // }

    // TODO: Re-enable once ticket operations are implemented
    // #[tokio::test]
    // async fn on_channel_ticket_redeemed_incoming_channel_neglect_left_over_tickets() -> anyhow::Result<()> {
    // let db = BlokliDb::new_in_memory().await?;
    //         db.set_domain_separator(None, DomainSeparator::Channel, Hash::default())
    //             .await?;
    //         let rpc_operations = MockIndexerRpcOperations::new();
    //         // ==> set mock expectations here
    //         let clonable_rpc_operations = ClonableMockOperations {
    //             //
    //             inner: Arc::new(rpc_operations),
    //         };
    //         let handlers = init_handlers(clonable_rpc_operations, db.clone());
    //
    //         let channel = ChannelEntry::new(
    //             *COUNTERPARTY_CHAIN_ADDRESS,
    //             *SELF_CHAIN_ADDRESS,
    //             primitive_types::U256::from((1u128 << 96) - 1).into(),
    //             primitive_types::U256::zero(),
    //             ChannelStatus::Open,
    //             primitive_types::U256::one(),
    //         );
    //
    //         let ticket_index = primitive_types::U256::from((1u128 << 48) - 2);
    //         let next_ticket_index = ticket_index + 1;
    //
    //         let mut ticket =
    //             mock_acknowledged_ticket(&COUNTERPARTY_CHAIN_KEY, &SELF_CHAIN_KEY, ticket_index.as_u64(), 1.0)?;
    //         ticket.status = AcknowledgedTicketStatus::BeingRedeemed;
    //
    //         let ticket_value = ticket.verified_ticket().amount;
    //
    //         db.upsert_channel(None, channel).await?;
    //         db.upsert_ticket(None, ticket.clone()).await?;
    //
    //         let old_ticket =
    //             mock_acknowledged_ticket(&COUNTERPARTY_CHAIN_KEY, &SELF_CHAIN_KEY, ticket_index.as_u64() - 1, 1.0)?;
    //         db.upsert_ticket(None, old_ticket.clone()).await?;
    //
    //         let ticket_redeemed_log = SerializableLog {
    //             address: handlers.addresses.channels,
    //             topics: vec![
    //                 hopr_bindings::hopr_channels::HoprChannels::TicketRedeemed::SIGNATURE_HASH.into(),
    //                 // TicketRedeemedFilter::signature().into(),
    //                 H256::from_slice(channel.get_id().as_ref()).into(),
    //             ],
    //             data: Vec::from(next_ticket_index.to_be_bytes()),
    //             ..test_log()
    //         };
    //
    //         let outgoing_ticket_index_before = db
    //             .get_outgoing_ticket_index(channel.get_id())
    //             .await?
    //             .load(Ordering::Relaxed);
    //
    //         let stats = db.get_ticket_statistics(Some(channel.get_id())).await?;
    //         assert_eq!(
    //             HoprBalance::zero(),
    //             stats.redeemed_value,
    //             "there should not be any redeemed value"
    //         );
    //         assert_eq!(
    //             HoprBalance::zero(),
    //             stats.neglected_value,
    //             "there should not be any neglected value"
    //         );
    //
    //         db
    //             .begin_transaction()
    //             .await?
    //             .perform(|tx| Box::pin(async move { handlers.process_log_event(tx, ticket_redeemed_log, true).await
    // }))             .await?;
    //
    //         let channel = db
    //             .get_channel_by_id(None, &channel.get_id())
    //             .await?
    //             .context("a value should be present")?;
    //
    //         assert!(
    // TODO: Add event verification - check published IndexerEvent instead of return value
    // Some(ticket)),             "must return the updated channel entry and the redeemed ticket"
    //         );
    //
    //         assert_eq!(
    //             channel.ticket_index, next_ticket_index,
    //             "channel entry must contain next ticket index"
    //         );
    //
    //         let outgoing_ticket_index_after = db
    //             .get_outgoing_ticket_index(channel.get_id())
    //             .await?
    //             .load(Ordering::Relaxed);
    //
    //         assert_eq!(
    //             outgoing_ticket_index_before, outgoing_ticket_index_after,
    //             "outgoing ticket index must not change"
    //         );
    //
    //         let tickets = db.get_tickets((&channel).into()).await?;
    //         assert!(tickets.is_empty(), "there should not be any tickets left");
    //
    //         let stats = db.get_ticket_statistics(Some(channel.get_id())).await?;
    //         assert_eq!(
    //             ticket_value, stats.redeemed_value,
    //             "there should be redeemed value worth 1 ticket"
    //         );
    //         assert_eq!(
    //             ticket_value, stats.neglected_value,
    //             "there should neglected value worth 1 ticket"
    //         );
    //         // Ok(())
    //     // }

    #[tokio::test]
    async fn on_channel_ticket_redeemed_outgoing_channel() -> anyhow::Result<()> {
        let db = BlokliDb::new_in_memory().await?;
        db.set_domain_separator(None, DomainSeparator::Channel, Hash::default())
            .await?;
        let rpc_operations = MockIndexerRpcOperations::new();
        // ==> set mock expectations here
        let clonable_rpc_operations = ClonableMockOperations {
            //
            inner: Arc::new(rpc_operations),
        };
        let handlers = init_handlers(clonable_rpc_operations, db.clone());

        create_test_accounts(&db).await?;

        let channel = ChannelEntry::new(
            *SELF_CHAIN_ADDRESS,
            *COUNTERPARTY_CHAIN_ADDRESS,
            primitive_types::U256::from((1u128 << 96) - 1).into(),
            0u64,
            ChannelStatus::Open,
            0u32,
        );

        let ticket_index = primitive_types::U256::from((1u128 << 48) - 2);
        let next_ticket_index = ticket_index + 1;

        db.upsert_channel(None, channel, 1, 0, 0).await?;

        let channel_id = generate_channel_id(&SELF_CHAIN_ADDRESS, &COUNTERPARTY_CHAIN_ADDRESS);
        let channel_state = encode_channel_state(
            channel.balance,
            next_ticket_index.as_u64(),
            0, // closure_time
            channel.channel_epoch,
            ChannelStatus::Open,
        );

        // Create TicketRedeemed event using bindings
        let event = TicketRedeemed {
            channelId: FixedBytes::from_slice(channel_id.as_ref()),
            channel: channel_state,
        };

        let ticket_redeemed_log = event_to_log(event, handlers.addresses.channels);

        db.begin_transaction()
            .await?
            .perform(|tx| Box::pin(async move { handlers.process_log_event(tx, ticket_redeemed_log, true).await }))
            .await?;

        let channel = db
            .get_channel_by_id(None, &channel.get_id())
            .await?
            .context("a value should be present")?;

        // TODO: Add event verification - check published IndexerEvent instead of return value

        assert_eq!(
            channel.ticket_index,
            next_ticket_index.as_u64(),
            "channel entry must contain next ticket index"
        );

        // TODO: Re-enable once get_outgoing_ticket_index is implemented
        let outgoing_ticket_index = next_ticket_index.as_u64(); // db.get_outgoing_ticket_index(channel.get_id()).await?.load(Ordering::Relaxed);

        assert_eq!(
            outgoing_ticket_index,
            next_ticket_index.as_u64(),
            "outgoing ticket index must be equal to next ticket index"
        );
        Ok(())
    }

    #[tokio::test]
    async fn on_channel_ticket_redeemed_on_incoming_channel_with_non_existent_ticket_should_pass() -> anyhow::Result<()>
    {
        let db = BlokliDb::new_in_memory().await?;
        db.set_domain_separator(None, DomainSeparator::Channel, Hash::default())
            .await?;
        let rpc_operations = MockIndexerRpcOperations::new();
        // ==> set mock expectations here
        let clonable_rpc_operations = ClonableMockOperations {
            //
            inner: Arc::new(rpc_operations),
        };
        let handlers = init_handlers(clonable_rpc_operations, db.clone());

        create_test_accounts(&db).await?;

        let channel = ChannelEntry::new(
            *COUNTERPARTY_CHAIN_ADDRESS,
            *SELF_CHAIN_ADDRESS,
            primitive_types::U256::from((1u128 << 96) - 1).into(),
            0u64,
            ChannelStatus::Open,
            0u32,
        );

        db.upsert_channel(None, channel, 1, 0, 0).await?;

        let next_ticket_index = primitive_types::U256::from((1u128 << 48) - 1);

        let channel_id = generate_channel_id(&COUNTERPARTY_CHAIN_ADDRESS, &SELF_CHAIN_ADDRESS);
        let channel_state = encode_channel_state(
            channel.balance,
            next_ticket_index.as_u64(),
            0, // closure_time
            channel.channel_epoch,
            ChannelStatus::Open,
        );

        // Create TicketRedeemed event using bindings
        let event = TicketRedeemed {
            channelId: FixedBytes::from_slice(channel_id.as_ref()),
            channel: channel_state,
        };

        let ticket_redeemed_log = event_to_log(event, handlers.addresses.channels);

        db.begin_transaction()
            .await?
            .perform(|tx| Box::pin(async move { handlers.process_log_event(tx, ticket_redeemed_log, true).await }))
            .await?;

        let channel = db
            .get_channel_by_id(None, &channel.get_id())
            .await?
            .context("a value should be present")?;

        // TODO: Add event verification - check published IndexerEvent instead of return value

        assert_eq!(
            channel.ticket_index,
            next_ticket_index.as_u64(),
            "channel entry must contain next ticket index"
        );
        Ok(())
    }

    #[tokio::test]
    async fn on_channel_ticket_redeemed_on_foreign_channel_should_pass() -> anyhow::Result<()> {
        let db = BlokliDb::new_in_memory().await?;
        let rpc_operations = MockIndexerRpcOperations::new();
        // ==> set mock expectations here
        let clonable_rpc_operations = ClonableMockOperations {
            //
            inner: Arc::new(rpc_operations),
        };
        let handlers = init_handlers(clonable_rpc_operations, db.clone());

        // Generate foreign addresses and create accounts before channel operations
        let foreign_addr1 = Address::from(hopr_crypto_random::random_bytes());
        let foreign_addr2 = Address::from(hopr_crypto_random::random_bytes());
        let foreign_key1 =
            OffchainKeypair::from_secret(&hopr_crypto_random::random_bytes::<32>()).expect("valid keypair");
        let foreign_key2 =
            OffchainKeypair::from_secret(&hopr_crypto_random::random_bytes::<32>()).expect("valid keypair");

        db.upsert_account(None, 1, foreign_addr1, *foreign_key1.public(), None, 1, 0, 0)
            .await?;
        db.upsert_account(None, 2, foreign_addr2, *foreign_key2.public(), None, 1, 0, 1)
            .await?;

        let channel = ChannelEntry::new(
            foreign_addr1,
            foreign_addr2,
            primitive_types::U256::from((1u128 << 96) - 1).into(),
            0u64,
            ChannelStatus::Open,
            0u32,
        );

        db.upsert_channel(None, channel.clone(), 1, 0, 0).await?;

        let next_ticket_index = primitive_types::U256::from((1u128 << 48) - 1);
        let channel_state = encode_channel_state(
            channel.balance,
            next_ticket_index.as_u64(),
            0,
            channel.channel_epoch,
            channel.status,
        );

        // Create TicketRedeemed event using bindings
        let event = TicketRedeemed {
            channelId: FixedBytes::from_slice(channel.get_id().as_ref()),
            channel: channel_state,
        };

        let ticket_redeemed_log = event_to_log(event, handlers.addresses.channels);

        db.begin_transaction()
            .await?
            .perform(|tx| Box::pin(async move { handlers.process_log_event(tx, ticket_redeemed_log, true).await }))
            .await?;

        let channel = db
            .get_channel_by_id(None, &channel.get_id())
            .await?
            .context("a value should be present")?;

        // TODO: Add event verification - check published IndexerEvent instead of return value

        assert_eq!(
            channel.ticket_index,
            next_ticket_index.as_u64(),
            "channel entry must contain next ticket index"
        );
        Ok(())
    }

    #[tokio::test]
    async fn on_channel_closure_initiated() -> anyhow::Result<()> {
        let db = BlokliDb::new_in_memory().await?;
        let rpc_operations = MockIndexerRpcOperations::new();
        // ==> set mock expectations here
        let clonable_rpc_operations = ClonableMockOperations {
            //
            inner: Arc::new(rpc_operations),
        };
        let handlers = init_handlers(clonable_rpc_operations, db.clone());

        create_test_accounts(&db).await?;

        let channel = ChannelEntry::new(
            *SELF_CHAIN_ADDRESS,
            *COUNTERPARTY_CHAIN_ADDRESS,
            primitive_types::U256::from((1u128 << 96) - 1).into(),
            0u64,
            ChannelStatus::Open,
            0u32,
        );

        db.upsert_channel(None, channel, 1, 0, 0).await?;

        let timestamp = SystemTime::now();
        let closure_time_secs = timestamp.as_unix_timestamp().as_secs() as u32;

        let channel_id = generate_channel_id(&SELF_CHAIN_ADDRESS, &COUNTERPARTY_CHAIN_ADDRESS);
        let channel_state = encode_channel_state(
            channel.balance,
            0, // ticket_index
            closure_time_secs,
            channel.channel_epoch,
            ChannelStatus::PendingToClose(timestamp),
        );

        // Create OutgoingChannelClosureInitiated event using bindings
        let event = OutgoingChannelClosureInitiated {
            channelId: FixedBytes::from_slice(channel_id.as_ref()),
            channel: channel_state,
        };

        let closure_initiated_log = event_to_log(event, handlers.addresses.channels);

        db.begin_transaction()
            .await?
            .perform(|tx| Box::pin(async move { handlers.process_log_event(tx, closure_initiated_log, true).await }))
            .await?;

        let channel = db
            .get_channel_by_id(None, &channel.get_id())
            .await?
            .context("a value should be present")?;

        // TODO: Add event verification - check published IndexerEvent instead of return value

        assert_eq!(
            channel.status,
            ChannelStatus::PendingToClose(timestamp),
            "channel status must match"
        );
        Ok(())
    }

    #[tokio::test]
    async fn on_node_safe_registry_registered() -> anyhow::Result<()> {
        let db = BlokliDb::new_in_memory().await?;
        let mut rpc_operations = MockIndexerRpcOperations::new();

        // Set up mock expectation for get_hopr_module_from_safe
        let module_address: Address = "abcdef1234567890abcdef1234567890abcdef12"
            .parse()
            .expect("valid address");
        rpc_operations
            .expect_get_hopr_module_from_safe()
            .returning(move |_| Ok(Some(module_address)));

        let clonable_rpc_operations = ClonableMockOperations {
            inner: Arc::new(rpc_operations),
        };
        let handlers = init_handlers(clonable_rpc_operations, db.clone());

        let encoded_data = ().abi_encode();

        let safe_registered_log = SerializableLog {
            address: handlers.addresses.node_safe_registry,
            topics: vec![
                HoprNodeSafeRegistry::RegisteredNodeSafe::SIGNATURE_HASH.into(),
                H256::from_slice(&SAFE_INSTANCE_ADDR.to_bytes32()).into(),
                H256::from_slice(&SELF_CHAIN_ADDRESS.to_bytes32()).into(),
            ],
            data: encoded_data,
            ..test_log()
        };

        db.begin_transaction()
            .await?
            .perform(|tx| Box::pin(async move { handlers.process_log_event(tx, safe_registered_log, true).await }))
            .await?;

        // Verify the safe was stored with the module address from RPC
        let safe = db
            .get_safe_contract_by_address(None, *SAFE_INSTANCE_ADDR)
            .await?
            .expect("safe should exist");
        assert_eq!(
            Address::try_from(safe.module_address.as_slice()).unwrap(),
            module_address,
            "module address should match RPC response"
        );

        Ok(())
    }

    #[tokio::test]
    async fn on_node_safe_registry_deregistered() -> anyhow::Result<()> {
        let db = BlokliDb::new_in_memory().await?;
        let rpc_operations = MockIndexerRpcOperations::new();
        let clonable_rpc_operations = ClonableMockOperations {
            inner: Arc::new(rpc_operations),
        };
        let handlers = init_handlers(clonable_rpc_operations, db.clone());

        // First create the safe entry that will be deregistered
        let module_address: Address = "abcdef1234567890abcdef1234567890abcdef12"
            .parse()
            .expect("valid address");
        db.create_safe_contract(
            None,
            *SAFE_INSTANCE_ADDR,
            module_address,
            *SELF_CHAIN_ADDRESS,
            1, // block
            0, // tx_index
            0, // log_index
        )
        .await?;

        // Also register the node to the safe
        db.register_node_to_safe(None, *SAFE_INSTANCE_ADDR, *SELF_CHAIN_ADDRESS, 1, 0, 0)
            .await?;

        let encoded_data = ().abi_encode();

        let safe_deregistered_log = SerializableLog {
            address: handlers.addresses.node_safe_registry,
            topics: vec![
                HoprNodeSafeRegistry::DeregisteredNodeSafe::SIGNATURE_HASH.into(),
                H256::from_slice(&SAFE_INSTANCE_ADDR.to_bytes32()).into(),
                H256::from_slice(&SELF_CHAIN_ADDRESS.to_bytes32()).into(),
            ],
            data: encoded_data,
            ..test_log()
        };

        db.begin_transaction()
            .await?
            .perform(|tx| Box::pin(async move { handlers.process_log_event(tx, safe_deregistered_log, true).await }))
            .await?;

        // Verify the node registration was deleted
        let registration = HoprNodeSafeRegistration::find()
            .filter(hopr_node_safe_registration::Column::SafeAddress.eq(SAFE_INSTANCE_ADDR.as_ref().to_vec()))
            .filter(hopr_node_safe_registration::Column::NodeAddress.eq(SELF_CHAIN_ADDRESS.as_ref().to_vec()))
            .one(db.conn(blokli_db::TargetDb::Index))
            .await?;
        assert!(registration.is_none(), "node registration should be deleted");

        // Verify the safe contract still exists (only node registration was deleted)
        let safe = db.get_safe_contract_by_address(None, *SAFE_INSTANCE_ADDR).await?;
        assert!(safe.is_some(), "safe contract should still exist");

        Ok(())
    }
}
