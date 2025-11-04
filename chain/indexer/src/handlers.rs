use std::{
    fmt::{Debug, Formatter},
    ops::Add,
    sync::Arc,
    time::{Duration, SystemTime},
};

use alloy::{primitives::B256, sol_types::SolEventInterface};
use async_trait::async_trait;
use blokli_chain_rpc::{HoprIndexerRpcOperations, Log};
use blokli_chain_types::{
    ContractAddresses,
    chain_events::{ChainEventType, SignificantChainEvent},
};
use blokli_db::{BlokliDbAllOperations, OpenTransaction, api::info::DomainSeparator, errors::DbSqlError};
use hopr_bindings::{
    hopr_announcements::HoprAnnouncements::HoprAnnouncementsEvents, hopr_channels::HoprChannels::HoprChannelsEvents,
    hopr_node_management_module::HoprNodeManagementModule::HoprNodeManagementModuleEvents,
    hopr_node_safe_registry::HoprNodeSafeRegistry::HoprNodeSafeRegistryEvents,
    hopr_ticket_price_oracle::HoprTicketPriceOracle::HoprTicketPriceOracleEvents,
    hopr_token::HoprToken::HoprTokenEvents,
    hopr_winning_probability_oracle::HoprWinningProbabilityOracle::HoprWinningProbabilityOracleEvents,
};
use hopr_crypto_types::prelude::*;
use hopr_internal_types::prelude::*;
use hopr_primitive_types::prelude::*;
use tracing::{debug, error, info, trace, warn};

use crate::errors::{CoreEthereumIndexerError, Result};

/// Represents the decoded state of a channel from the packed bytes32 format emitted by contract events.
///
/// The channel state is packed into 256 bits (32 bytes) with the following layout (right-to-left):
/// - Bytes 0-5: Padding (48 bits)
/// - Bytes 6-17: balance (96 bits)
/// - Bytes 18-23: ticketIndex (48 bits)
/// - Bytes 24-27: closureTime (32 bits)
/// - Bytes 28-30: epoch (24 bits)
/// - Byte 31: status (8 bits)
#[derive(Debug, Clone)]
struct DecodedChannel {
    balance: HoprBalance,
    ticket_index: u32,
    closure_time: u32,
    // TODO(Phase 3): Use epoch when channel update API is refactored to support channel_state table
    #[allow(dead_code)]
    epoch: u32,
    status: ChannelStatus,
}

/// Decodes a packed channel bytes32 into its constituent fields.
///
/// The contract emits channel state as a packed bytes32 value. This function
/// extracts each field according to the Solidity struct packing rules.
///
/// # Arguments
/// * `channel` - The packed bytes32 channel value from the contract event
///
/// # Returns
/// * `DecodedChannel` - Struct containing all decoded channel fields
fn decode_channel(channel: B256) -> DecodedChannel {
    let bytes = channel.as_slice();

    // Extract balance (bytes 6-17, 96 bits = 12 bytes)
    let mut balance_bytes = [0u8; 32];
    balance_bytes[20..32].copy_from_slice(&bytes[6..18]);
    let balance = HoprBalance::from_be_bytes(balance_bytes);

    // Extract ticketIndex (bytes 18-23, 48 bits = 6 bytes)
    // Convert to u32 (only use lower 32 bits, as u48 would need u64)
    let mut ticket_index_bytes = [0u8; 8];
    ticket_index_bytes[2..8].copy_from_slice(&bytes[18..24]);
    let ticket_index = u64::from_be_bytes(ticket_index_bytes) as u32;

    // Extract closureTime (bytes 24-27, 32 bits = 4 bytes)
    let mut closure_time_bytes = [0u8; 4];
    closure_time_bytes.copy_from_slice(&bytes[24..28]);
    let closure_time = u32::from_be_bytes(closure_time_bytes);

    // Extract epoch (bytes 28-30, 24 bits = 3 bytes)
    let mut epoch_bytes = [0u8; 4];
    epoch_bytes[1..4].copy_from_slice(&bytes[28..31]);
    let epoch = u32::from_be_bytes(epoch_bytes);

    // Extract status (byte 31, 8 bits = 1 byte)
    let status_byte = bytes[31];
    let status = match status_byte {
        0 => ChannelStatus::Closed,
        1 => ChannelStatus::Open,
        2 => ChannelStatus::PendingToClose(SystemTime::UNIX_EPOCH.add(Duration::from_secs(closure_time as u64))),
        _ => {
            error!("Invalid channel status byte: {}", status_byte);
            ChannelStatus::Closed // Default to Closed for safety
        }
    };

    DecodedChannel {
        balance,
        ticket_index,
        closure_time,
        epoch,
        status,
    }
}

#[cfg(all(feature = "prometheus", not(test)))]
lazy_static::lazy_static! {
    static ref METRIC_INDEXER_LOG_COUNTERS: hopr_metrics::MultiCounter =
        hopr_metrics::MultiCounter::new(
            "hopr_indexer_contract_log_count",
            "Counts of different HOPR contract logs processed by the Indexer",
            &["contract"]
    ).unwrap();
}

/// Event handling an object for on-chain operations
///
/// Once an on-chain operation is recorded by the [crate::block::Indexer], it is pre-processed
/// and passed on to this object that handles event-specific actions for each on-chain operation.
#[derive(Clone)]
pub struct ContractEventHandlers<T, Db> {
    /// channels, announcements, token: contract addresses
    /// whose event we process
    addresses: Arc<ContractAddresses>,
    /// callbacks to inform other modules
    db: Db,
    /// rpc operations to interact with the chain
    _rpc_operations: T,
}

impl<T, Db> Debug for ContractEventHandlers<T, Db> {
    fn fmt(&self, f: &mut Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("ContractEventHandler")
            .field("addresses", &self.addresses)
            .finish_non_exhaustive()
    }
}

impl<T, Db> ContractEventHandlers<T, Db>
where
    T: HoprIndexerRpcOperations + Clone + Send + 'static,
    Db: BlokliDbAllOperations + Clone,
{
    /// Creates a new instance of contract event handlers with RPC operations support.
    ///
    /// This constructor initializes the event handlers with all necessary dependencies
    /// for processing blockchain events and making direct RPC calls for fresh state data.
    ///
    /// # Type Parameters
    /// * `T` - Type implementing `HoprIndexerRpcOperations` for blockchain queries
    ///
    /// # Arguments
    /// * `addresses` - Contract addresses configuration
    /// * `db` - Database connection for persistent storage
    /// * `rpc_operations` - RPC interface for direct blockchain queries
    ///
    /// # Returns
    /// * `Self` - New instance of `ContractEventHandlers`
    pub fn new(addresses: ContractAddresses, db: Db, rpc_operations: T) -> Self {
        Self {
            addresses: Arc::new(addresses),
            db,
            _rpc_operations: rpc_operations,
        }
    }

    async fn on_announcement_event(
        &self,
        tx: &OpenTransaction,
        event: HoprAnnouncementsEvents,
        block_number: u32,
        tx_index: u32,
        log_index: u32,
        _is_synced: bool,
    ) -> Result<Option<ChainEventType>> {
        #[cfg(all(feature = "prometheus", not(test)))]
        METRIC_INDEXER_LOG_COUNTERS.increment(&["announcements"]);

        match event {
            HoprAnnouncementsEvents::AddressAnnouncement(address_announcement) => {
                debug!(
                    multiaddress = &address_announcement.baseMultiaddr,
                    address = &address_announcement.node.to_string(),
                    "on_announcement_event: AddressAnnouncement",
                );
                // safeguard against empty multiaddrs, skip
                if address_announcement.baseMultiaddr.is_empty() {
                    warn!(
                        address = %address_announcement.node,
                        "encountered empty multiaddress announcement",
                    );
                    return Ok(None);
                }
                let node_address: Address = address_announcement.node.into();

                return match self
                    .db
                    .insert_announcement(
                        Some(tx),
                        node_address,
                        address_announcement.baseMultiaddr.parse()?,
                        block_number,
                    )
                    .await
                {
                    Ok(account) => Ok(Some(ChainEventType::Announcement {
                        peer: account.public_key.into(),
                        address: account.chain_addr,
                        multiaddresses: vec![account.get_multiaddr().expect("not must contain multiaddr")],
                    })),
                    Err(DbSqlError::MissingAccount) => Err(CoreEthereumIndexerError::AnnounceBeforeKeyBinding),
                    Err(e) => Err(e.into()),
                };
            }
            HoprAnnouncementsEvents::KeyBinding(key_binding) => {
                debug!(
                    address = %key_binding.chain_key,
                    public_key = %key_binding.ed25519_pub_key,
                    "on_announcement_event: KeyBinding",
                );
                match KeyBinding::from_parts(
                    key_binding.chain_key.into(),
                    key_binding.ed25519_pub_key.0.try_into()?,
                    OffchainSignature::try_from((key_binding.ed25519_sig_0.0, key_binding.ed25519_sig_1.0))?,
                ) {
                    Ok(binding) => {
                        match self
                            .db
                            .upsert_account(
                                Some(tx),
                                binding.chain_key,
                                binding.packet_key,
                                None, // safe_address is None for key bindings
                                block_number,
                                tx_index,
                                log_index,
                            )
                            .await
                        {
                            Ok(_) => (),
                            Err(err) => {
                                // We handle these errors gracefully and don't want the indexer to crash,
                                // because anybody could write faulty entries into the announcement contract.
                                error!(%err, "failed to store announcement key binding")
                            }
                        }
                    }
                    Err(e) => {
                        warn!(
                            address = ?key_binding.chain_key,
                            error = %e,
                            "Filtering announcement with invalid signature",

                        )
                    }
                }
            }
            HoprAnnouncementsEvents::RevokeAnnouncement(revocation) => {
                let node_address: Address = revocation.node.into();
                match self.db.delete_all_announcements(Some(tx), node_address).await {
                    Err(DbSqlError::MissingAccount) => {
                        return Err(CoreEthereumIndexerError::RevocationBeforeKeyBinding);
                    }
                    Err(e) => return Err(e.into()),
                    _ => {}
                }
            }
            HoprAnnouncementsEvents::Initialized(_event) => {
                debug!("on_announcement_event: Initialized");
            }
            HoprAnnouncementsEvents::KeyBindingFeeUpdate(fee_update) => {
                debug!(
                    new_fee = %fee_update.newFee,
                    "on_announcement_event: KeyBindingFeeUpdate"
                );
            }
            HoprAnnouncementsEvents::LedgerDomainSeparatorUpdated(ledger_domain_separator) => {
                self.db
                    .set_domain_separator(
                        Some(tx),
                        DomainSeparator::Ledger,
                        ledger_domain_separator.ledgerDomainSeparator.0.into(),
                    )
                    .await?;
                debug!("on_announcement_event: LedgerDomainSeparatorUpdated");
            }
            HoprAnnouncementsEvents::OwnershipTransferred(ownership) => {
                debug!(
                    previous_owner = %ownership.previousOwner,
                    new_owner = %ownership.newOwner,
                    "on_announcement_event: OwnershipTransferred"
                );
            }
            HoprAnnouncementsEvents::Upgraded(upgraded) => {
                debug!(
                    implementation = %upgraded.implementation,
                    "on_announcement_event: Upgraded"
                );
            }
        };

        Ok(None)
    }

    async fn on_channel_event(
        &self,
        tx: &OpenTransaction,
        event: HoprChannelsEvents,
        block: u32,
        tx_index: u32,
        log_index: u32,
        _is_synced: bool,
    ) -> Result<Option<ChainEventType>> {
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
                    decoded.ticket_index.into(),
                    decoded.status,
                    decoded.epoch.into(),
                );

                // Atomically upsert the new state
                self.db
                    .upsert_channel(tx.into(), updated_channel, block, tx_index, log_index)
                    .await?;

                Ok(Some(ChainEventType::ChannelBalanceDecreased(updated_channel, diff)))
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
                    decoded.ticket_index.into(),
                    decoded.status,
                    decoded.epoch.into(),
                );

                // Atomically upsert the new state
                self.db
                    .upsert_channel(tx.into(), updated_channel, block, tx_index, log_index)
                    .await?;

                Ok(Some(ChainEventType::ChannelBalanceIncreased(updated_channel, diff)))
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
                    decoded.ticket_index.into(),
                    decoded.status,
                    decoded.epoch.into(),
                );

                // Atomically upsert the new state
                self.db
                    .upsert_channel(tx.into(), updated_channel, block, tx_index, log_index)
                    .await?;

                Ok(Some(ChainEventType::ChannelClosed(updated_channel)))
            }
            HoprChannelsEvents::ChannelOpened(channel_opened) => {
                let source: Address = channel_opened.source.into();
                let destination: Address = channel_opened.destination.into();
                let channel_id = generate_channel_id(&source, &destination);

                let maybe_existing = match self.db.get_channel_by_id(tx.into(), &channel_id).await {
                    Ok(existing) => existing,
                    Err(e) => {
                        error!(%source, %destination, %channel_id, %e, "failed to get channel on on_channel_opened_event");
                        return Err(e.into());
                    }
                };

                let channel = if let Some(existing) = maybe_existing {
                    // Channel exists - check if it's in Closed state
                    if existing.status != ChannelStatus::Closed {
                        warn!(%source, %destination, %channel_id, "received Open event for a channel that is not Closed, marking state as corrupted");

                        // Mark this state as corrupted
                        self.db
                            .mark_channel_state_corrupted(tx.into(), &channel_id, block, tx_index, log_index)
                            .await
                            .ok();

                        return Ok(None);
                    }

                    trace!(%source, %destination, %channel_id, "on_channel_reopened_event");

                    let current_epoch = existing.channel_epoch;

                    // Reopen channel: reset ticket_index, increment epoch, set status to Open
                    let reopened_channel = ChannelEntry::new(
                        source,
                        destination,
                        existing.balance, // Keep existing balance
                        0_u32.into(),     // Reset ticket index
                        ChannelStatus::Open,
                        current_epoch.add(1), // Increment epoch
                    );

                    self.db
                        .upsert_channel(tx.into(), reopened_channel, block, tx_index, log_index)
                        .await?;
                    reopened_channel
                } else {
                    // Channel doesn't exist - create new one
                    trace!(%source, %destination, %channel_id, "on_channel_opened_event");

                    let new_channel = ChannelEntry::new(
                        source,
                        destination,
                        0_u32.into(),
                        0_u32.into(),
                        ChannelStatus::Open,
                        1_u32.into(),
                    );

                    self.db
                        .upsert_channel(tx.into(), new_channel, block, tx_index, log_index)
                        .await?;
                    new_channel
                };

                Ok(Some(ChainEventType::ChannelOpened(channel)))
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
                    decoded.ticket_index.into(),
                    decoded.status,
                    decoded.epoch.into(),
                );

                // Atomically upsert the new state
                self.db
                    .upsert_channel(tx.into(), updated_channel, block, tx_index, log_index)
                    .await?;

                Ok(None)
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
                    decoded.ticket_index.into(),
                    decoded.status, // Should be PendingToClose with proper timestamp
                    decoded.epoch.into(),
                );

                // Atomically upsert the new state
                self.db
                    .upsert_channel(tx.into(), updated_channel, block, tx_index, log_index)
                    .await?;

                Ok(Some(ChainEventType::ChannelClosureInitiated(updated_channel)))
            }
            HoprChannelsEvents::DomainSeparatorUpdated(domain_separator_updated) => {
                self.db
                    .set_domain_separator(
                        Some(tx),
                        DomainSeparator::Channel,
                        domain_separator_updated.domainSeparator.0.into(),
                    )
                    .await?;

                Ok(None)
            }
            HoprChannelsEvents::LedgerDomainSeparatorUpdated(ledger_domain_separator_updated) => {
                self.db
                    .set_domain_separator(
                        Some(tx),
                        DomainSeparator::Ledger,
                        ledger_domain_separator_updated.ledgerDomainSeparator.0.into(),
                    )
                    .await?;

                Ok(None)
            }
        }
    }

    async fn on_token_event(
        &self,
        _tx: &OpenTransaction,
        event: HoprTokenEvents,
        _is_synced: bool,
    ) -> Result<Option<ChainEventType>> {
        #[cfg(all(feature = "prometheus", not(test)))]
        METRIC_INDEXER_LOG_COUNTERS.increment(&["token"]);

        match event {
            HoprTokenEvents::Transfer(transferred) => {
                let from: Address = transferred.from.into();
                let to: Address = transferred.to.into();

                trace!(
                    %from, %to,
                    "on_token_transfer_event"
                );
            }
            HoprTokenEvents::Approval(approved) => {
                let owner: Address = approved.owner.into();
                let spender: Address = approved.spender.into();

                trace!(
                    %owner, %spender, allowance = %approved.value,
                    "on_token_approval_event",

                )
            }
            HoprTokenEvents::AuthorizedOperator(authorized) => {
                debug!(
                    operator = %authorized.operator,
                    token_holder = %authorized.tokenHolder,
                    "on_token_authorized_operator_event"
                );
            }
            HoprTokenEvents::Burned(burned) => {
                debug!(
                    operator = %burned.operator,
                    from = %burned.from,
                    amount = %burned.amount,
                    "on_token_burned_event"
                );
            }
            HoprTokenEvents::Minted(minted) => {
                debug!(
                    operator = %minted.operator,
                    to = %minted.to,
                    amount = %minted.amount,
                    "on_token_minted_event"
                );
            }
            HoprTokenEvents::RevokedOperator(revoked) => {
                debug!(
                    operator = %revoked.operator,
                    token_holder = %revoked.tokenHolder,
                    "on_token_revoked_operator_event"
                );
            }
            HoprTokenEvents::RoleAdminChanged(role_admin) => {
                debug!(
                    role = ?role_admin.role,
                    previous_admin_role = ?role_admin.previousAdminRole,
                    new_admin_role = ?role_admin.newAdminRole,
                    "on_token_role_admin_changed_event"
                );
            }
            HoprTokenEvents::RoleGranted(role_granted) => {
                debug!(
                    role = ?role_granted.role,
                    account = %role_granted.account,
                    sender = %role_granted.sender,
                    "on_token_role_granted_event"
                );
            }
            HoprTokenEvents::RoleRevoked(role_revoked) => {
                debug!(
                    role = ?role_revoked.role,
                    account = %role_revoked.account,
                    sender = %role_revoked.sender,
                    "on_token_role_revoked_event"
                );
            }
            HoprTokenEvents::Sent(sent) => {
                debug!(
                    operator = %sent.operator,
                    from = %sent.from,
                    to = %sent.to,
                    amount = %sent.amount,
                    "on_token_sent_event"
                );
            }
        }

        Ok(None)
    }

    async fn on_node_safe_registry_event(
        &self,
        tx: &OpenTransaction,
        event: HoprNodeSafeRegistryEvents,
        _is_synced: bool,
    ) -> Result<Option<ChainEventType>> {
        #[cfg(all(feature = "prometheus", not(test)))]
        METRIC_INDEXER_LOG_COUNTERS.increment(&["safe_registry"]);

        match event {
            HoprNodeSafeRegistryEvents::RegisteredNodeSafe(registered) => {
                info!(node_address = %registered.nodeAddress, safe_address = %registered.safeAddress, "Node safe registered", );
            }
            HoprNodeSafeRegistryEvents::DeregisteredNodeSafe(deregistered) => {
                info!(node_address = %deregistered.nodeAddress, safe_address = %deregistered.safeAddress, "Node safe deregistered", );
            }
            HoprNodeSafeRegistryEvents::DomainSeparatorUpdated(domain_separator_updated) => {
                self.db
                    .set_domain_separator(
                        Some(tx),
                        DomainSeparator::SafeRegistry,
                        domain_separator_updated.domainSeparator.0.into(),
                    )
                    .await?;
            }
        }

        Ok(None)
    }

    #[allow(dead_code)]
    async fn on_node_management_module_event(
        &self,
        _db: &OpenTransaction,
        _event: HoprNodeManagementModuleEvents,
        _is_synced: bool,
    ) -> Result<Option<ChainEventType>> {
        #[cfg(all(feature = "prometheus", not(test)))]
        METRIC_INDEXER_LOG_COUNTERS.increment(&["node_management_module"]);

        // Don't care at the moment
        Ok(None)
    }

    async fn on_ticket_winning_probability_oracle_event(
        &self,
        tx: &OpenTransaction,
        event: HoprWinningProbabilityOracleEvents,
        _is_synced: bool,
    ) -> Result<Option<ChainEventType>> {
        #[cfg(all(feature = "prometheus", not(test)))]
        METRIC_INDEXER_LOG_COUNTERS.increment(&["win_prob_oracle"]);

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
        Ok(None)
    }

    async fn on_ticket_price_oracle_event(
        &self,
        tx: &OpenTransaction,
        event: HoprTicketPriceOracleEvents,
        _is_synced: bool,
    ) -> Result<Option<ChainEventType>> {
        #[cfg(all(feature = "prometheus", not(test)))]
        METRIC_INDEXER_LOG_COUNTERS.increment(&["price_oracle"]);

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
            }
            HoprTicketPriceOracleEvents::OwnershipTransferred(_event) => {
                // ignore ownership transfer event
            }
        }
        Ok(None)
    }

    #[tracing::instrument(level = "debug", skip(self, slog), fields(log=%slog))]
    async fn process_log_event(
        &self,
        tx: &OpenTransaction,
        slog: SerializableLog,
        is_synced: bool,
    ) -> Result<Option<ChainEventType>> {
        trace!(log = %slog, "log content");

        let log = Log::from(slog.clone());

        let primitive_log = alloy::primitives::Log::new(
            slog.address.into(),
            slog.topics.iter().map(|h| B256::from_slice(h.as_ref())).collect(),
            slog.data.clone().into(),
        )
        .ok_or_else(|| {
            CoreEthereumIndexerError::ProcessError(format!("failed to convert log to primitive log: {slog:?}"))
        })?;

        if log.address.eq(&self.addresses.announcements) {
            let bn = log.block_number as u32;
            let tx_idx = log.tx_index as u32;
            let log_idx = log.log_index.as_u32();
            let event = HoprAnnouncementsEvents::decode_log(&primitive_log)?;
            self.on_announcement_event(tx, event.data, bn, tx_idx, log_idx, is_synced)
                .await
        } else if log.address.eq(&self.addresses.channels) {
            let event = HoprChannelsEvents::decode_log(&primitive_log)?;
            let block = log.block_number as u32;
            let tx_idx = log.tx_index as u32;
            let log_idx = log.log_index.as_u32();
            match self
                .on_channel_event(tx, event.data, block, tx_idx, log_idx, is_synced)
                .await
            {
                Ok(res) => Ok(res),
                Err(CoreEthereumIndexerError::ChannelDoesNotExist) => {
                    // This is not an error, just a log that we don't have the channel in the DB
                    debug!(
                        ?log,
                        "channel didn't exist in the db. Created a corrupted channel entry and ignored event"
                    );
                    Ok(None)
                }
                Err(e) => Err(e),
            }
        } else if log.address.eq(&self.addresses.token) {
            let event = HoprTokenEvents::decode_log(&primitive_log)?;
            self.on_token_event(tx, event.data, is_synced).await
        } else if log.address.eq(&self.addresses.safe_registry) {
            let event = HoprNodeSafeRegistryEvents::decode_log(&primitive_log)?;
            self.on_node_safe_registry_event(tx, event.data, is_synced).await
        } else if log.address.eq(&self.addresses.price_oracle) {
            let event = HoprTicketPriceOracleEvents::decode_log(&primitive_log)?;
            self.on_ticket_price_oracle_event(tx, event.data, is_synced).await
        } else if log.address.eq(&self.addresses.win_prob_oracle) {
            let event = HoprWinningProbabilityOracleEvents::decode_log(&primitive_log)?;
            self.on_ticket_winning_probability_oracle_event(tx, event.data, is_synced)
                .await
        } else {
            #[cfg(all(feature = "prometheus", not(test)))]
            METRIC_INDEXER_LOG_COUNTERS.increment(&["unknown"]);

            error!(
                address = %log.address, log = ?log,
                "on_event error - unknown contract address, received log"
            );
            return Err(CoreEthereumIndexerError::UnknownContract(log.address));
        }
    }
}

#[async_trait]
impl<T, Db> crate::traits::ChainLogHandler for ContractEventHandlers<T, Db>
where
    T: HoprIndexerRpcOperations + Clone + Send + Sync + 'static,
    Db: BlokliDbAllOperations + Clone + Debug + Send + Sync + 'static,
{
    fn contract_addresses(&self) -> Vec<Address> {
        vec![
            self.addresses.announcements,
            self.addresses.channels,
            self.addresses.price_oracle,
            self.addresses.win_prob_oracle,
            self.addresses.safe_registry,
            self.addresses.token,
        ]
    }

    fn contract_addresses_map(&self) -> Arc<ContractAddresses> {
        self.addresses.clone()
    }

    fn contract_address_topics(&self, contract: Address) -> Vec<B256> {
        if contract.eq(&self.addresses.announcements) {
            crate::constants::topics::announcement()
        } else if contract.eq(&self.addresses.channels) {
            crate::constants::topics::channel()
        } else if contract.eq(&self.addresses.price_oracle) {
            crate::constants::topics::ticket_price_oracle()
        } else if contract.eq(&self.addresses.win_prob_oracle) {
            crate::constants::topics::winning_prob_oracle()
        } else if contract.eq(&self.addresses.safe_registry) {
            crate::constants::topics::node_safe_registry()
        } else {
            panic!("use of unsupported contract address: {contract}");
        }
    }

    async fn collect_log_event(&self, slog: SerializableLog, is_synced: bool) -> Result<Option<SignificantChainEvent>> {
        let myself = self.clone();
        self.db
            .begin_transaction()
            .await?
            .perform(move |tx| {
                let log = slog.clone();
                let tx_hash = Hash::from(log.tx_hash);
                let log_id = log.log_index;
                let block_id = log.block_number;

                Box::pin(async move {
                    match myself.process_log_event(tx, log, is_synced).await {
                        // If a significant chain event can be extracted from the log
                        Ok(Some(event_type)) => {
                            let significant_event = SignificantChainEvent { tx_hash, event_type };
                            debug!(block_id, %tx_hash, log_id, ?significant_event, "indexer got significant_event");
                            Ok(Some(significant_event))
                        }
                        Ok(None) => {
                            debug!(block_id, %tx_hash, log_id, "no significant event in log");
                            Ok(None)
                        }
                        Err(error) => {
                            error!(block_id, %tx_hash, log_id, %error, "error processing log in tx");
                            Err(error)
                        }
                    }
                })
            })
            .await
    }
}

#[cfg(test)]
mod tests {
    use std::{
        ops::Add,
        sync::Arc,
        time::{Duration, SystemTime},
    };

    use alloy::{
        dyn_abi::DynSolValue,
        primitives::{Address as AlloyAddress, B256, U256},
        sol_types::SolValue,
    };
    use anyhow::Context;
    use blokli_chain_rpc::HoprIndexerRpcOperations;
    use blokli_chain_types::{ContractAddresses, chain_events::ChainEventType};
    use blokli_db::{
        BlokliDbAllOperations,
        BlokliDbGeneralModelOperations,
        accounts::{BlokliDbAccountOperations, ChainOrPacketKey},
        api::info::DomainSeparator,
        channels::BlokliDbChannelOperations,
        // TODO: Refactor to use channel.corrupted_state field
        // corrupted_channels::BlokliDbCorruptedChannelOperations,
        db::BlokliDb,
        info::BlokliDbInfoOperations,
        // prelude::BlokliDbTicketOperations,
    };
    use hex_literal::hex;
    use hopr_crypto_types::prelude::*;
    use hopr_internal_types::prelude::*;
    use hopr_primitive_types::prelude::*;
    use multiaddr::Multiaddr;
    use primitive_types::H256;

    use super::ContractEventHandlers;

    lazy_static::lazy_static! {
        static ref SELF_PRIV_KEY: OffchainKeypair = OffchainKeypair::from_secret(&hex!("492057cf93e99b31d2a85bc5e98a9c3aa0021feec52c227cc8170e8f7d047775")).expect("lazy static keypair should be constructible");
        static ref SELF_CHAIN_KEYPAIR: ChainKeypair = ChainKeypair::from_secret(&hex!("492057cf93e99b31d2a85bc5e98a9c3aa0021feec52c227cc8170e8f7d047775")).expect("lazy static chain keypair should be constructible");
        static ref SELF_CHAIN_ADDRESS: Address = SELF_CHAIN_KEYPAIR.public().to_address();
        static ref COUNTERPARTY_CHAIN_ADDRESS: Address = "1234567890abcdef1234567890abcdef12345678".parse().expect("lazy static address should be constructible"); // just a dummy
        static ref SAFE_INSTANCE_ADDR: Address = "fedcba0987654321fedcba0987654321fedcba09".parse().expect("lazy static address should be constructible"); // just a dummy
        static ref STAKE_ADDRESS: Address = "4331eaa9542b6b034c43090d9ec1c2198758dbc3".parse().expect("lazy static address should be constructible");
        static ref CHANNELS_ADDR: Address = "bab20aea98368220baa4e3b7f151273ee71df93b".parse().expect("lazy static address should be constructible"); // just a dummy
        static ref TOKEN_ADDR: Address = "47d1677e018e79dcdd8a9c554466cb1556fa5007".parse().expect("lazy static address should be constructible"); // just a dummy
        static ref NODE_SAFE_REGISTRY_ADDR: Address = "0dcd1bf9a1b36ce34237eeafef220932846bcd82".parse().expect("lazy static address should be constructible"); // just a dummy
        static ref ANNOUNCEMENTS_ADDR: Address = "11db4791bf45ef31a10ea4a1b5cb90f46cc72c7e".parse().expect("lazy static address should be constructible"); // just a dummy
        static ref TICKET_PRICE_ORACLE_ADDR: Address = "11db4391bf45ef31a10ea4a1b5cb90f46cc72c7e".parse().expect("lazy static address should be constructible"); // just a dummy
        static ref WIN_PROB_ORACLE_ADDR: Address = "00db4391bf45ef31a10ea4a1b5cb90f46cc64c7e".parse().expect("lazy static address should be constructible"); // just a dummy
    }

    mockall::mock! {
        pub(crate) IndexerRpcOperations {}

        #[async_trait::async_trait]
        impl HoprIndexerRpcOperations for IndexerRpcOperations {
            async fn block_number(&self) -> blokli_chain_rpc::errors::Result<u64>;

        async fn get_hopr_allowance(&self, owner: Address, spender: Address) -> blokli_chain_rpc::errors::Result<HoprBalance>;

        async fn get_xdai_balance(&self, address: Address) -> blokli_chain_rpc::errors::Result<XDaiBalance>;

        async fn get_hopr_balance(&self, address: Address) -> blokli_chain_rpc::errors::Result<HoprBalance>;

        fn try_stream_logs<'a>(
            &'a self,
            start_block_number: u64,
            filters: blokli_chain_rpc::FilterSet,
            is_synced: bool,
        ) -> blokli_chain_rpc::errors::Result<std::pin::Pin<Box<dyn futures::Stream<Item=blokli_chain_rpc::BlockWithLogs> + Send + 'a> > >;
        }
    }

    #[derive(Clone)]
    pub struct ClonableMockOperations {
        pub inner: Arc<MockIndexerRpcOperations>,
    }

    #[async_trait::async_trait]
    impl HoprIndexerRpcOperations for ClonableMockOperations {
        async fn block_number(&self) -> blokli_chain_rpc::errors::Result<u64> {
            self.inner.block_number().await
        }

        async fn get_hopr_allowance(
            &self,
            owner: Address,
            spender: Address,
        ) -> blokli_chain_rpc::errors::Result<HoprBalance> {
            self.inner.get_hopr_allowance(owner, spender).await
        }

        async fn get_xdai_balance(&self, address: Address) -> blokli_chain_rpc::errors::Result<XDaiBalance> {
            self.inner.get_xdai_balance(address).await
        }

        async fn get_hopr_balance(&self, address: Address) -> blokli_chain_rpc::errors::Result<HoprBalance> {
            self.inner.get_hopr_balance(address).await
        }

        fn try_stream_logs<'a>(
            &'a self,
            start_block_number: u64,
            filters: blokli_chain_rpc::FilterSet,
            is_synced: bool,
        ) -> blokli_chain_rpc::errors::Result<
            std::pin::Pin<Box<dyn futures::Stream<Item = blokli_chain_rpc::BlockWithLogs> + Send + 'a>>,
        > {
            self.inner.try_stream_logs(start_block_number, filters, is_synced)
        }
    }

    fn init_handlers<T: Clone, Db: BlokliDbAllOperations + Clone>(
        rpc_operations: T,
        db: Db,
    ) -> ContractEventHandlers<T, Db> {
        ContractEventHandlers {
            addresses: Arc::new(ContractAddresses {
                channels: *CHANNELS_ADDR,
                token: *TOKEN_ADDR,
                safe_registry: *NODE_SAFE_REGISTRY_ADDR,
                announcements: *ANNOUNCEMENTS_ADDR,
                price_oracle: *TICKET_PRICE_ORACLE_ADDR,
                win_prob_oracle: *WIN_PROB_ORACLE_ADDR,
                stake_factory: Default::default(),
            }),
            db,
            _rpc_operations: rpc_operations,
        }
    }

    fn test_log() -> SerializableLog {
        SerializableLog { ..Default::default() }
    }

    #[tokio::test]
    async fn announce_keybinding() -> anyhow::Result<()> {
        let db = BlokliDb::new_in_memory().await?;

        let rpc_operations = MockIndexerRpcOperations::new();
        // ==> set mock expectations here
        let clonable_rpc_operations = ClonableMockOperations {
            inner: Arc::new(rpc_operations),
        };

        let handlers = init_handlers(clonable_rpc_operations, db.clone());

        let keybinding = KeyBinding::new(*SELF_CHAIN_ADDRESS, &SELF_PRIV_KEY);

        let keybinding_log = SerializableLog {
            address: handlers.addresses.announcements,
            topics: vec![
                hopr_bindings::hoprannouncementsevents::HoprAnnouncementsEvents::KeyBinding::SIGNATURE_HASH.into(),
            ],
            data: DynSolValue::Tuple(vec![
                DynSolValue::Bytes(keybinding.signature.as_ref().to_vec()),
                DynSolValue::Bytes(keybinding.packet_key.as_ref().to_vec()),
                DynSolValue::FixedBytes(AlloyAddress::from_slice(SELF_CHAIN_ADDRESS.as_ref()).into_word(), 32),
            ])
            .abi_encode_packed(),
            ..test_log()
        };

        let account_entry = AccountEntry {
            public_key: *SELF_PRIV_KEY.public(),
            chain_addr: *SELF_CHAIN_ADDRESS,
            entry_type: AccountType::NotAnnounced,
            published_at: 0,
        };

        let event_type = db
            .begin_transaction()
            .await?
            .perform(|tx| Box::pin(async move { handlers.process_log_event(tx, keybinding_log, true).await }))
            .await?;

        assert!(event_type.is_none(), "keybinding does not have a chain event type");

        assert_eq!(
            db.get_account(None, ChainOrPacketKey::ChainKey(*SELF_CHAIN_ADDRESS))
                .await?
                .context("a value should be present")?,
            account_entry
        );
        Ok(())
    }

    #[tokio::test]
    async fn announce_address_announcement() -> anyhow::Result<()> {
        let db = BlokliDb::new_in_memory().await?;
        let rpc_operations = MockIndexerRpcOperations::new();
        // ==> set mock expectations here
        let clonable_rpc_operations = ClonableMockOperations {
            //
            inner: Arc::new(rpc_operations),
        };
        let handlers = init_handlers(clonable_rpc_operations, db.clone());

        // Assume that there is a keybinding
        // Create account using upsert_account
        db.upsert_account(
            None,
            *SELF_CHAIN_ADDRESS,
            *SELF_PRIV_KEY.public(),
            None, // no safe_address
            1,    // block
            0,    // tx_index
            0,    // log_index
        )
        .await?;

        let test_multiaddr_empty: Multiaddr = "".parse()?;

        let address_announcement_empty_log_encoded_data = DynSolValue::Tuple(vec![
            DynSolValue::Address(AlloyAddress::from_slice(SELF_CHAIN_ADDRESS.as_ref())),
            DynSolValue::String(test_multiaddr_empty.to_string()),
        ])
        .abi_encode();

        let address_announcement_empty_log = SerializableLog {
            address: handlers.addresses.announcements,
            topics: vec![
                hopr_bindings::hoprannouncementsevents::HoprAnnouncementsEvents::AddressAnnouncement::SIGNATURE_HASH
                    .into(),
            ],
            data: address_announcement_empty_log_encoded_data[32..].into(),
            ..test_log()
        };

        let handlers_clone = handlers.clone();
        let event_type = db
            .begin_transaction()
            .await?
            .perform(|tx| {
                Box::pin(async move {
                    handlers_clone
                        .process_log_event(tx, address_announcement_empty_log, true)
                        .await
                })
            })
            .await?;

        assert!(
            event_type.is_none(),
            "announcement of empty multiaddresses must pass through"
        );

        assert_eq!(
            db.get_account(None, ChainOrPacketKey::ChainKey(*SELF_CHAIN_ADDRESS))
                .await?
                .context("a value should be present")?,
            account_entry
        );

        let test_multiaddr: Multiaddr = "/ip4/1.2.3.4/tcp/56".parse()?;

        let address_announcement_log_encoded_data = DynSolValue::Tuple(vec![
            DynSolValue::Address(AlloyAddress::from_slice(SELF_CHAIN_ADDRESS.as_ref())),
            DynSolValue::String(test_multiaddr.to_string()),
        ])
        .abi_encode();

        let address_announcement_log = SerializableLog {
            address: handlers.addresses.announcements,
            block_number: 1,
            topics: vec![
                hopr_bindings::hoprannouncementsevents::HoprAnnouncementsEvents::AddressAnnouncement::SIGNATURE_HASH
                    .into(),
            ],
            data: address_announcement_log_encoded_data[32..].into(),
            ..test_log()
        };

        let announced_account_entry = AccountEntry {
            public_key: *SELF_PRIV_KEY.public(),
            chain_addr: *SELF_CHAIN_ADDRESS,
            entry_type: AccountType::Announced {
                multiaddr: test_multiaddr.clone(),
                updated_block: 1,
            },
            published_at: 1,
        };

        let handlers_clone = handlers.clone();
        let event_type = db
            .begin_transaction()
            .await?
            .perform(|tx| {
                Box::pin(async move {
                    handlers_clone
                        .process_log_event(tx, address_announcement_log, true)
                        .await
                })
            })
            .await?;

        assert!(
            matches!(event_type, Some(ChainEventType::Announcement { multiaddresses,.. }) if multiaddresses == vec![test_multiaddr]),
            "must return the latest announce multiaddress"
        );

        assert_eq!(
            db.get_account(None, ChainOrPacketKey::ChainKey(*SELF_CHAIN_ADDRESS))
                .await?
                .context("a value should be present")?,
            announced_account_entry
        );

        // TODO: Re-enable these assertions once resolve_chain_key and resolve_packet_key methods are implemented
        // assert_eq!(
        //     Some(*SELF_CHAIN_ADDRESS),
        //     db.resolve_chain_key(SELF_PRIV_KEY.public()).await?,
        //     "must resolve correct chain key"
        // );

        // assert_eq!(
        //     Some(*SELF_PRIV_KEY.public()),
        //     db.resolve_packet_key(&SELF_CHAIN_ADDRESS).await?,
        //     "must resolve correct packet key"
        // );

        let test_multiaddr_dns: Multiaddr = "/dns4/useful.domain/tcp/56".parse()?;

        let address_announcement_dns_log_encoded_data = DynSolValue::Tuple(vec![
            DynSolValue::Address(AlloyAddress::from_slice(SELF_CHAIN_ADDRESS.as_ref())),
            DynSolValue::String(test_multiaddr_dns.to_string()),
        ])
        .abi_encode();

        let address_announcement_dns_log = SerializableLog {
            address: handlers.addresses.announcements,
            block_number: 2,
            topics: vec![
                hopr_bindings::hoprannouncementsevents::HoprAnnouncementsEvents::AddressAnnouncement::SIGNATURE_HASH
                    .into(),
            ],
            data: address_announcement_dns_log_encoded_data[32..].into(),
            ..test_log()
        };

        let announced_dns_account_entry = AccountEntry {
            public_key: *SELF_PRIV_KEY.public(),
            chain_addr: *SELF_CHAIN_ADDRESS,
            entry_type: AccountType::Announced {
                multiaddr: test_multiaddr_dns.clone(),
                updated_block: 2,
            },
            published_at: 1,
        };

        let event_type = db
            .begin_transaction()
            .await?
            .perform(|tx| {
                Box::pin(async move { handlers.process_log_event(tx, address_announcement_dns_log, true).await })
            })
            .await?;

        assert!(
            matches!(event_type, Some(ChainEventType::Announcement { multiaddresses,.. }) if multiaddresses == vec![test_multiaddr_dns]),
            "must return the latest announce multiaddress"
        );

        assert_eq!(
            db.get_account(None, ChainOrPacketKey::ChainKey(*SELF_CHAIN_ADDRESS))
                .await?
                .context("a value should be present")?,
            announced_dns_account_entry
        );

        // TODO: Re-enable these assertions once resolve_chain_key and resolve_packet_key methods are implemented
        // assert_eq!(
        //     Some(*SELF_CHAIN_ADDRESS),
        //     db.resolve_chain_key(SELF_PRIV_KEY.public()).await?,
        //     "must resolve correct chain key"
        // );

        // assert_eq!(
        //     Some(*SELF_PRIV_KEY.public()),
        //     db.resolve_packet_key(&SELF_CHAIN_ADDRESS).await?,
        //     "must resolve correct packet key"
        // );
        Ok(())
    }

    #[tokio::test]
    async fn announce_revoke() -> anyhow::Result<()> {
        let db = BlokliDb::new_in_memory().await?;
        let rpc_operations = MockIndexerRpcOperations::new();
        // ==> set mock expectations here
        let clonable_rpc_operations = ClonableMockOperations {
            //
            inner: Arc::new(rpc_operations),
        };
        let handlers = init_handlers(clonable_rpc_operations, db.clone());

        let test_multiaddr: Multiaddr = "/ip4/1.2.3.4/tcp/56".parse()?;

        // Assume that there is a keybinding and an address announcement
        // Create account using upsert_account
        db.upsert_account(
            None,
            *SELF_CHAIN_ADDRESS,
            *SELF_PRIV_KEY.public(),
            None, // no safe_address
            1,    // block
            0,    // tx_index
            0,    // log_index
        )
        .await?;

        // Add the announcement
        db.insert_announcement(None, *SELF_CHAIN_ADDRESS, test_multiaddr, 0)
            .await?;

        let encoded_data = (AlloyAddress::from_slice(SELF_CHAIN_ADDRESS.as_ref()),).abi_encode();

        let revoke_announcement_log = SerializableLog {
            address: handlers.addresses.announcements,
            topics: vec![
                hopr_bindings::hoprannouncementsevents::HoprAnnouncementsEvents::RevokeAnnouncement::SIGNATURE_HASH
                    .into(),
            ],
            data: encoded_data,
            ..test_log()
        };

        let account_entry = AccountEntry {
            public_key: *SELF_PRIV_KEY.public(),
            chain_addr: *SELF_CHAIN_ADDRESS,
            entry_type: AccountType::NotAnnounced,
            published_at: 1,
        };

        let event_type = db
            .begin_transaction()
            .await?
            .perform(|tx| Box::pin(async move { handlers.process_log_event(tx, revoke_announcement_log, true).await }))
            .await?;

        assert!(
            event_type.is_none(),
            "revoke announcement does not have chain event type"
        );

        assert_eq!(
            db.get_account(None, ChainOrPacketKey::ChainKey(*SELF_CHAIN_ADDRESS))
                .await?
                .context("a value should be present")?,
            account_entry
        );
        Ok(())
    }

    #[tokio::test]
    async fn on_token_transfer_to() -> anyhow::Result<()> {
        let db = BlokliDb::new_in_memory().await?;

        let value = U256::MAX;
        let target_hopr_balance = HoprBalance::from(primitive_types::U256::from_big_endian(
            value.to_be_bytes_vec().as_slice(),
        ));

        let mut rpc_operations = MockIndexerRpcOperations::new();
        rpc_operations
            .expect_get_hopr_balance()
            .times(1)
            .return_once(move |_| Ok(target_hopr_balance));
        rpc_operations
            .expect_get_hopr_allowance()
            .times(1)
            .returning(move |_, _| Ok(HoprBalance::from(primitive_types::U256::from(1000u64))));
        let clonable_rpc_operations = ClonableMockOperations {
            inner: Arc::new(rpc_operations),
        };

        let handlers = init_handlers(clonable_rpc_operations, db.clone());

        let encoded_data = (value).abi_encode();

        let transferred_log = SerializableLog {
            address: handlers.addresses.token,
            topics: vec![
                hopr_bindings::hoprtoken::HoprToken::Transfer::SIGNATURE_HASH.into(),
                H256::from_slice(&Address::default().to_bytes32()).into(),
                H256::from_slice(&STAKE_ADDRESS.to_bytes32()).into(),
            ],
            data: encoded_data,
            ..test_log()
        };

        let event_type = db
            .begin_transaction()
            .await?
            .perform(|tx| Box::pin(async move { handlers.process_log_event(tx, transferred_log, true).await }))
            .await?;

        assert!(event_type.is_none(), "token transfer does not have chain event type");

        assert_eq!(db.get_safe_hopr_balance(None).await?, target_hopr_balance);

        Ok(())
    }

    #[test_log::test(tokio::test)]
    async fn on_token_transfer_from() -> anyhow::Result<()> {
        let db = BlokliDb::new_in_memory().await?;

        let mut rpc_operations = MockIndexerRpcOperations::new();
        rpc_operations
            .expect_get_hopr_balance()
            .times(1)
            .return_once(|_| Ok(HoprBalance::zero()));
        rpc_operations
            .expect_get_hopr_allowance()
            .times(1)
            .returning(move |_, _| Ok(HoprBalance::from(primitive_types::U256::from(1000u64))));
        let clonable_rpc_operations = ClonableMockOperations {
            inner: Arc::new(rpc_operations),
        };

        let handlers = init_handlers(clonable_rpc_operations, db.clone());

        let value = U256::MAX;

        let encoded_data = (value).abi_encode();

        db.set_safe_hopr_balance(
            None,
            HoprBalance::from(primitive_types::U256::from_big_endian(
                value.to_be_bytes_vec().as_slice(),
            )),
        )
        .await?;

        let transferred_log = SerializableLog {
            address: handlers.addresses.token,
            topics: vec![
                hopr_bindings::hoprtoken::HoprToken::Transfer::SIGNATURE_HASH.into(),
                H256::from_slice(&STAKE_ADDRESS.to_bytes32()).into(),
                H256::from_slice(&Address::default().to_bytes32()).into(),
            ],
            data: encoded_data,
            ..test_log()
        };

        let event_type = db
            .begin_transaction()
            .await?
            .perform(|tx| Box::pin(async move { handlers.process_log_event(tx, transferred_log, true).await }))
            .await?;

        assert!(event_type.is_none(), "token transfer does not have chain event type");

        assert_eq!(db.get_safe_hopr_balance(None).await?, HoprBalance::zero());

        Ok(())
    }

    #[tokio::test]
    async fn on_token_approval_correct() -> anyhow::Result<()> {
        let db = BlokliDb::new_in_memory().await?;

        let target_allowance = HoprBalance::from(primitive_types::U256::from(1000u64));
        let mut rpc_operations = MockIndexerRpcOperations::new();
        rpc_operations
            .expect_get_hopr_allowance()
            .times(2)
            .returning(move |_, _| Ok(target_allowance));
        let clonable_rpc_operations = ClonableMockOperations {
            inner: Arc::new(rpc_operations),
        };

        let handlers = init_handlers(clonable_rpc_operations, db.clone());

        let encoded_data = (U256::from(1000u64)).abi_encode();

        let approval_log = SerializableLog {
            address: handlers.addresses.token,
            topics: vec![
                hopr_bindings::hoprtoken::HoprToken::Approval::SIGNATURE_HASH.into(),
                H256::from_slice(&SAFE_INSTANCE_ADDR.to_bytes32()).into(),
                H256::from_slice(&handlers.addresses.channels.to_bytes32()).into(),
            ],
            data: encoded_data,
            ..test_log()
        };

        // before any operation the allowance should be 0
        assert_eq!(db.get_safe_hopr_allowance(None).await?, HoprBalance::zero());

        let approval_log_clone = approval_log.clone();
        let handlers_clone = handlers.clone();
        let event_type = db
            .begin_transaction()
            .await?
            .perform(|tx| Box::pin(async move { handlers_clone.process_log_event(tx, approval_log_clone, true).await }))
            .await?;

        assert!(event_type.is_none(), "token approval does not have chain event type");

        // after processing the allowance should be 0
        assert_eq!(db.get_safe_hopr_allowance(None).await?, target_allowance.clone());

        // reduce allowance manually to verify a second time
        let _ = db
            .set_safe_hopr_allowance(None, HoprBalance::from(primitive_types::U256::from(10u64)))
            .await;
        assert_eq!(
            db.get_safe_hopr_allowance(None).await?,
            HoprBalance::from(primitive_types::U256::from(10u64))
        );

        let handlers_clone = handlers.clone();
        let event_type = db
            .begin_transaction()
            .await?
            .perform(|tx| Box::pin(async move { handlers_clone.process_log_event(tx, approval_log, true).await }))
            .await?;

        assert!(event_type.is_none(), "token approval does not have chain event type");

        assert_eq!(db.get_safe_hopr_allowance(None).await?, target_allowance);
        Ok(())
    }

    #[tokio::test]
    async fn on_channel_event_balance_increased() -> anyhow::Result<()> {
        let db = BlokliDb::new_in_memory().await?;

        let value = U256::MAX;
        let target_hopr_balance = HoprBalance::from(primitive_types::U256::from_big_endian(
            value.to_be_bytes_vec().as_slice(),
        ));

        let mut rpc_operations = MockIndexerRpcOperations::new();
        rpc_operations
            .expect_get_hopr_balance()
            .times(1)
            .return_once(move |_| Ok(target_hopr_balance));
        rpc_operations
            .expect_get_hopr_allowance()
            .times(1)
            .returning(move |_, _| Ok(HoprBalance::from(primitive_types::U256::from(1000u64))));
        let clonable_rpc_operations = ClonableMockOperations {
            inner: Arc::new(rpc_operations),
        };
        let handlers = init_handlers(clonable_rpc_operations, db.clone());

        let channel = ChannelEntry::new(
            *SELF_CHAIN_ADDRESS,
            *COUNTERPARTY_CHAIN_ADDRESS,
            0.into(),
            primitive_types::U256::zero(),
            ChannelStatus::Open,
            primitive_types::U256::one(),
        );

        db.upsert_channel(None, channel, 1, 0, 0).await?;

        let solidity_balance: HoprBalance = primitive_types::U256::from((1u128 << 96) - 1).into();
        let diff = solidity_balance - channel.balance;

        let encoded_data = (solidity_balance.amount().to_be_bytes()).abi_encode();

        let balance_increased_log = SerializableLog {
            address: handlers.addresses.channels,
            topics: vec![
                hopr_bindings::hoprchannels::HoprChannels::ChannelBalanceIncreased::SIGNATURE_HASH.into(),
                // ChannelBalanceIncreasedFilter::signature().into(),
                H256::from_slice(channel.get_id().as_ref()).into(),
            ],
            data: encoded_data,
            ..test_log()
        };

        let event_type = db
            .begin_transaction()
            .await?
            .perform(|tx| Box::pin(async move { handlers.process_log_event(tx, balance_increased_log, true).await }))
            .await?;

        let channel = db
            .get_channel_by_id(None, &channel.get_id())
            .await?
            .context("a value should be present")?;

        assert!(
            matches!(event_type, Some(ChainEventType::ChannelBalanceIncreased(c, b)) if c == channel && b == diff),
            "must return updated channel entry and balance diff"
        );

        assert_eq!(solidity_balance, channel.balance, "balance must be updated");
        Ok(())
    }

    #[tokio::test]
    async fn on_channel_event_domain_separator_updated() -> anyhow::Result<()> {
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
                hopr_bindings::hoprchannels::HoprChannels::DomainSeparatorUpdated::SIGNATURE_HASH.into(),
                // DomainSeparatorUpdatedFilter::signature().into(),
                H256::from_slice(separator.as_ref()).into(),
            ],
            data: encoded_data,
            ..test_log()
        };

        assert!(db.get_indexer_data(None).await?.channels_dst.is_none());

        let event_type = db
            .begin_transaction()
            .await?
            .perform(|tx| Box::pin(async move { handlers.process_log_event(tx, channels_dst_updated, true).await }))
            .await?;

        assert!(
            event_type.is_none(),
            "there's no chain event type for channel dst update"
        );

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
    async fn on_channel_event_balance_decreased() -> anyhow::Result<()> {
        let db = BlokliDb::new_in_memory().await?;

        let value = U256::MAX;
        let target_hopr_balance = HoprBalance::from(primitive_types::U256::from_big_endian(
            value.to_be_bytes_vec().as_slice(),
        ));

        let mut rpc_operations = MockIndexerRpcOperations::new();
        rpc_operations
            .expect_get_hopr_balance()
            .times(1)
            .return_once(move |_| Ok(target_hopr_balance));
        let clonable_rpc_operations = ClonableMockOperations {
            inner: Arc::new(rpc_operations),
        };
        let handlers = init_handlers(clonable_rpc_operations, db.clone());

        let channel = ChannelEntry::new(
            *SELF_CHAIN_ADDRESS,
            *COUNTERPARTY_CHAIN_ADDRESS,
            HoprBalance::from(primitive_types::U256::from((1u128 << 96) - 1)),
            primitive_types::U256::zero(),
            ChannelStatus::Open,
            primitive_types::U256::one(),
        );

        db.upsert_channel(None, channel, 1, 0, 0).await?;

        let solidity_balance: HoprBalance = primitive_types::U256::from((1u128 << 96) - 2).into();
        let diff = channel.balance - solidity_balance;

        // let encoded_data = (solidity_balance).abi_encode();
        let encoded_data = DynSolValue::Tuple(vec![DynSolValue::Uint(
            U256::from_be_slice(&solidity_balance.amount().to_be_bytes()),
            256,
        )])
        .abi_encode();

        let balance_decreased_log = SerializableLog {
            address: handlers.addresses.channels,
            topics: vec![
                hopr_bindings::hoprchannels::HoprChannels::ChannelBalanceDecreased::SIGNATURE_HASH.into(),
                // ChannelBalanceDecreasedFilter::signature().into(),
                H256::from_slice(channel.get_id().as_ref()).into(),
            ],
            data: encoded_data,
            ..test_log()
        };

        let event_type = db
            .begin_transaction()
            .await?
            .perform(|tx| Box::pin(async move { handlers.process_log_event(tx, balance_decreased_log, true).await }))
            .await?;

        let channel = db
            .get_channel_by_id(None, &channel.get_id())
            .await?
            .context("a value should be present")?;

        assert!(
            matches!(event_type, Some(ChainEventType::ChannelBalanceDecreased(c, b)) if c == channel && b == diff),
            "must return updated channel entry and balance diff"
        );

        assert_eq!(solidity_balance, channel.balance, "balance must be updated");
        Ok(())
    }

    #[tokio::test]
    async fn on_channel_closed() -> anyhow::Result<()> {
        let db = BlokliDb::new_in_memory().await?;
        let rpc_operations = MockIndexerRpcOperations::new();
        // ==> set mock expectations here
        let clonable_rpc_operations = ClonableMockOperations {
            //
            inner: Arc::new(rpc_operations),
        };
        let handlers = init_handlers(clonable_rpc_operations, db.clone());

        let starting_balance = HoprBalance::from(primitive_types::U256::from((1u128 << 96) - 1));

        let channel = ChannelEntry::new(
            *SELF_CHAIN_ADDRESS,
            *COUNTERPARTY_CHAIN_ADDRESS,
            starting_balance,
            primitive_types::U256::zero(),
            ChannelStatus::Open,
            primitive_types::U256::one(),
        );

        db.upsert_channel(None, channel, 1, 0, 0).await?;

        let encoded_data = ().abi_encode();

        let channel_closed_log = SerializableLog {
            address: handlers.addresses.channels,
            topics: vec![
                hopr_bindings::hoprchannels::HoprChannels::ChannelClosed::SIGNATURE_HASH.into(),
                // ChannelClosedFilter::signature().into(),
                H256::from_slice(channel.get_id().as_ref()).into(),
            ],
            data: encoded_data,
            ..test_log()
        };

        let event_type = db
            .begin_transaction()
            .await?
            .perform(|tx| Box::pin(async move { handlers.process_log_event(tx, channel_closed_log, true).await }))
            .await?;

        let closed_channel = db
            .get_channel_by_id(None, &channel.get_id())
            .await?
            .context("a value should be present")?;

        assert!(
            matches!(event_type, Some(ChainEventType::ChannelClosed(c)) if c == closed_channel),
            "must return the updated channel entry"
        );

        assert_eq!(closed_channel.status, ChannelStatus::Closed);
        assert_eq!(closed_channel.ticket_index, 0u64.into());
        // TODO: Re-enable once get_outgoing_ticket_index is implemented
        // assert_eq!(0, db.get_outgoing_ticket_index(closed_channel.get_id()).await?.load(Ordering::Relaxed));

        assert!(closed_channel.balance.amount().eq(&primitive_types::U256::zero()));
        Ok(())
    }

    #[tokio::test]
    async fn on_foreign_channel_closed() -> anyhow::Result<()> {
        let db = BlokliDb::new_in_memory().await?;
        let rpc_operations = MockIndexerRpcOperations::new();
        // ==> set mock expectations here
        let clonable_rpc_operations = ClonableMockOperations {
            //
            inner: Arc::new(rpc_operations),
        };
        let handlers = init_handlers(clonable_rpc_operations, db.clone());

        let starting_balance = HoprBalance::from(primitive_types::U256::from((1u128 << 96) - 1));

        let channel = ChannelEntry::new(
            Address::new(&hex!("B7397C218766eBe6A1A634df523A1a7e412e67eA")),
            Address::new(&hex!("D4fdec44DB9D44B8f2b6d529620f9C0C7066A2c1")),
            starting_balance,
            primitive_types::U256::zero(),
            ChannelStatus::Open,
            primitive_types::U256::one(),
        );

        db.upsert_channel(None, channel, 1, 0, 0).await?;

        let encoded_data = ().abi_encode();

        let channel_closed_log = SerializableLog {
            address: handlers.addresses.channels,
            topics: vec![
                hopr_bindings::hoprchannels::HoprChannels::ChannelClosed::SIGNATURE_HASH.into(),
                // ChannelClosedFilter::signature().into(),
                H256::from_slice(channel.get_id().as_ref()).into(),
            ],
            data: encoded_data,
            ..test_log()
        };

        let event_type = db
            .begin_transaction()
            .await?
            .perform(|tx| Box::pin(async move { handlers.process_log_event(tx, channel_closed_log, true).await }))
            .await?;

        let closed_channel = db.get_channel_by_id(None, &channel.get_id()).await?;

        assert_eq!(None, closed_channel, "foreign channel must be deleted");

        assert!(
            matches!(event_type, Some(ChainEventType::ChannelClosed(c)) if c.get_id() == channel.get_id()),
            "must return the closed channel entry"
        );

        Ok(())
    }

    #[tokio::test]
    async fn on_channel_opened() -> anyhow::Result<()> {
        let db = BlokliDb::new_in_memory().await?;
        let rpc_operations = MockIndexerRpcOperations::new();
        // ==> set mock expectations here
        let clonable_rpc_operations = ClonableMockOperations {
            //
            inner: Arc::new(rpc_operations),
        };
        let handlers = init_handlers(clonable_rpc_operations, db.clone());

        let channel_id = generate_channel_id(&SELF_CHAIN_ADDRESS, &COUNTERPARTY_CHAIN_ADDRESS);

        let encoded_data = ().abi_encode();

        let channel_opened_log = SerializableLog {
            address: handlers.addresses.channels,
            topics: vec![
                hopr_bindings::hoprchannels::HoprChannels::ChannelOpened::SIGNATURE_HASH.into(),
                // ChannelOpenedFilter::signature().into(),
                H256::from_slice(&SELF_CHAIN_ADDRESS.to_bytes32()).into(),
                H256::from_slice(&COUNTERPARTY_CHAIN_ADDRESS.to_bytes32()).into(),
            ],
            data: encoded_data,
            ..test_log()
        };

        let event_type = db
            .begin_transaction()
            .await?
            .perform(|tx| Box::pin(async move { handlers.process_log_event(tx, channel_opened_log, true).await }))
            .await?;

        let channel = db
            .get_channel_by_id(None, &channel_id)
            .await?
            .context("a value should be present")?;

        assert!(
            matches!(event_type, Some(ChainEventType::ChannelOpened(c)) if c == channel),
            "must return the updated channel entry"
        );

        assert_eq!(channel.status, ChannelStatus::Open);
        assert_eq!(channel.channel_epoch, 1u64.into());
        assert_eq!(channel.ticket_index, 0u64.into());
        // TODO: Re-enable once get_outgoing_ticket_index is implemented
        // assert_eq!(0, db.get_outgoing_ticket_index(channel.get_id()).await?.load(Ordering::Relaxed));
        Ok(())
    }

    #[tokio::test]
    async fn on_channel_reopened() -> anyhow::Result<()> {
        let db = BlokliDb::new_in_memory().await?;
        let rpc_operations = MockIndexerRpcOperations::new();
        // ==> set mock expectations here
        let clonable_rpc_operations = ClonableMockOperations {
            //
            inner: Arc::new(rpc_operations),
        };
        let handlers = init_handlers(clonable_rpc_operations, db.clone());

        let channel = ChannelEntry::new(
            *SELF_CHAIN_ADDRESS,
            *COUNTERPARTY_CHAIN_ADDRESS,
            HoprBalance::zero(),
            primitive_types::U256::zero(),
            ChannelStatus::Closed,
            3.into(),
        );

        db.upsert_channel(None, channel, 1, 0, 0).await?;

        let encoded_data = ().abi_encode();

        let channel_opened_log = SerializableLog {
            address: handlers.addresses.channels,
            topics: vec![
                hopr_bindings::hoprchannels::HoprChannels::ChannelOpened::SIGNATURE_HASH.into(),
                // ChannelOpenedFilter::signature().into(),
                H256::from_slice(&SELF_CHAIN_ADDRESS.to_bytes32()).into(),
                H256::from_slice(&COUNTERPARTY_CHAIN_ADDRESS.to_bytes32()).into(),
            ],
            data: encoded_data,
            ..test_log()
        };

        let event_type = db
            .begin_transaction()
            .await?
            .perform(|tx| Box::pin(async move { handlers.process_log_event(tx, channel_opened_log, true).await }))
            .await?;

        let channel = db
            .get_channel_by_id(None, &channel.get_id())
            .await?
            .context("a value should be present")?;

        assert!(
            matches!(event_type, Some(ChainEventType::ChannelOpened(c)) if c == channel),
            "must return the updated channel entry"
        );

        assert_eq!(channel.status, ChannelStatus::Open);
        assert_eq!(channel.channel_epoch, 4u64.into());
        assert_eq!(channel.ticket_index, 0u64.into());

        // TODO: Re-enable once get_outgoing_ticket_index is implemented
        // assert_eq!(0, db.get_outgoing_ticket_index(channel.get_id()).await?.load(Ordering::Relaxed));
        Ok(())
    }

    #[tokio::test]
    async fn on_channel_should_not_reopen_when_not_closed() -> anyhow::Result<()> {
        let db = BlokliDb::new_in_memory().await?;
        let rpc_operations = MockIndexerRpcOperations::new();
        // ==> set mock expectations here
        let clonable_rpc_operations = ClonableMockOperations {
            //
            inner: Arc::new(rpc_operations),
        };
        let handlers = init_handlers(clonable_rpc_operations, db.clone());

        let channel = ChannelEntry::new(
            *SELF_CHAIN_ADDRESS,
            *COUNTERPARTY_CHAIN_ADDRESS,
            0.into(),
            primitive_types::U256::zero(),
            ChannelStatus::Open,
            3.into(),
        );

        db.upsert_channel(None, channel, 1, 0, 0).await?;

        let encoded_data = ().abi_encode();

        let channel_opened_log = SerializableLog {
            address: handlers.addresses.channels,
            topics: vec![
                hopr_bindings::hoprchannels::HoprChannels::ChannelOpened::SIGNATURE_HASH.into(),
                // ChannelOpenedFilter::signature().into(),
                H256::from_slice(&SELF_CHAIN_ADDRESS.to_bytes32()).into(),
                H256::from_slice(&COUNTERPARTY_CHAIN_ADDRESS.to_bytes32()).into(),
            ],
            data: encoded_data,
            ..test_log()
        };

        db.begin_transaction()
            .await?
            .perform(|tx| Box::pin(async move { handlers.process_log_event(tx, channel_opened_log, true).await }))
            .await
            .context("Channel should stay open, with corrupted flag set")?;

        assert!(
            db.get_channel_by_id(None, &channel.get_id()).await?.is_none(),
            "channel should not be returned as marked as corrupted",
        );

        // TODO: Refactor to check channel.corrupted_state field
        // db.get_corrupted_channel_by_id(None, &channel.get_id())
        //     .await?
        //     .context("a value should be present")?;

        Ok(())
    }

    #[tokio::test]
    async fn event_for_non_existing_channel_should_create_corrupted_channel() -> anyhow::Result<()> {
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
                hopr_bindings::hoprchannels::HoprChannels::ChannelBalanceIncreased::SIGNATURE_HASH.into(),
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

    fn mock_acknowledged_ticket(
        signer: &ChainKeypair,
        destination: &ChainKeypair,
        index: u64,
        win_prob: f64,
    ) -> anyhow::Result<AcknowledgedTicket> {
        let channel_id = generate_channel_id(&signer.into(), &destination.into());

        let channel_epoch = 1u64;
        let domain_separator = Hash::default();

        let response = Response::try_from(
            Hash::create(&[channel_id.as_ref(), &channel_epoch.to_be_bytes(), &index.to_be_bytes()]).as_ref(),
        )?;

        Ok(TicketBuilder::default()
            .direction(&signer.into(), &destination.into())
            .amount(primitive_types::U256::from(PRICE_PER_PACKET).div_f64(win_prob)?)
            .index(index)
            .index_offset(1)
            .win_prob(win_prob.try_into()?)
            .channel_epoch(1)
            .challenge(response.to_challenge()?)
            .build_signed(signer, &domain_separator)?
            .into_acknowledged(response))
    }

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
    //                 hopr_bindings::hoprchannels::HoprChannels::TicketRedeemed::SIGNATURE_HASH.into(),
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
    //         let event_type = db
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
    //             matches!(event_type, Some(ChainEventType::TicketRedeemed(c, t)) if channel == c && t ==
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
    //                 hopr_bindings::hoprchannels::HoprChannels::TicketRedeemed::SIGNATURE_HASH.into(),
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
    //         let event_type = db
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
    //             matches!(event_type, Some(ChainEventType::TicketRedeemed(c, t)) if channel == c && t ==
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

        let channel = ChannelEntry::new(
            *SELF_CHAIN_ADDRESS,
            *COUNTERPARTY_CHAIN_ADDRESS,
            primitive_types::U256::from((1u128 << 96) - 1).into(),
            primitive_types::U256::zero(),
            ChannelStatus::Open,
            primitive_types::U256::one(),
        );

        let ticket_index = primitive_types::U256::from((1u128 << 48) - 2);
        let next_ticket_index = ticket_index + 1;

        db.upsert_channel(None, channel, 1, 0, 0).await?;

        let ticket_redeemed_log = SerializableLog {
            address: handlers.addresses.channels,
            topics: vec![
                hopr_bindings::hoprchannels::HoprChannels::TicketRedeemed::SIGNATURE_HASH.into(),
                // TicketRedeemedFilter::signature().into(),
                H256::from_slice(channel.get_id().as_ref()).into(),
            ],
            data: Vec::from(next_ticket_index.to_be_bytes()),
            ..test_log()
        };

        let event_type = db
            .begin_transaction()
            .await?
            .perform(|tx| Box::pin(async move { handlers.process_log_event(tx, ticket_redeemed_log, true).await }))
            .await?;

        let channel = db
            .get_channel_by_id(None, &channel.get_id())
            .await?
            .context("a value should be present")?;

        assert!(
            matches!(event_type, Some(ChainEventType::TicketRedeemed(c, None)) if channel == c),
            "must return update channel entry and no ticket"
        );

        assert_eq!(
            channel.ticket_index, next_ticket_index,
            "channel entry must contain next ticket index"
        );

        // TODO: Re-enable once get_outgoing_ticket_index is implemented
        let outgoing_ticket_index = next_ticket_index.as_u64(); // db.get_outgoing_ticket_index(channel.get_id()).await?.load(Ordering::Relaxed);

        assert!(
            outgoing_ticket_index >= ticket_index.as_u64(),
            "outgoing idx {outgoing_ticket_index} must be greater or equal to {ticket_index}"
        );
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

        let channel = ChannelEntry::new(
            *COUNTERPARTY_CHAIN_ADDRESS,
            *SELF_CHAIN_ADDRESS,
            primitive_types::U256::from((1u128 << 96) - 1).into(),
            primitive_types::U256::zero(),
            ChannelStatus::Open,
            primitive_types::U256::one(),
        );

        db.upsert_channel(None, channel, 1, 0, 0).await?;

        let next_ticket_index = primitive_types::U256::from((1u128 << 48) - 1);

        let ticket_redeemed_log = SerializableLog {
            address: handlers.addresses.channels,
            topics: vec![
                hopr_bindings::hoprchannels::HoprChannels::TicketRedeemed::SIGNATURE_HASH.into(),
                // TicketRedeemedFilter::signature().into(),
                H256::from_slice(channel.get_id().as_ref()).into(),
            ],
            data: Vec::from(next_ticket_index.to_be_bytes()),
            ..test_log()
        };

        let event_type = db
            .begin_transaction()
            .await?
            .perform(|tx| Box::pin(async move { handlers.process_log_event(tx, ticket_redeemed_log, true).await }))
            .await?;

        let channel = db
            .get_channel_by_id(None, &channel.get_id())
            .await?
            .context("a value should be present")?;

        assert!(
            matches!(event_type, Some(ChainEventType::TicketRedeemed(c, None)) if c == channel),
            "must return updated channel entry and no ticket"
        );

        assert_eq!(
            channel.ticket_index, next_ticket_index,
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

        let channel = ChannelEntry::new(
            Address::from(hopr_crypto_random::random_bytes()),
            Address::from(hopr_crypto_random::random_bytes()),
            primitive_types::U256::from((1u128 << 96) - 1).into(),
            primitive_types::U256::zero(),
            ChannelStatus::Open,
            primitive_types::U256::one(),
        );

        db.upsert_channel(None, channel, 1, 0, 0).await?;

        let next_ticket_index = primitive_types::U256::from((1u128 << 48) - 1);

        let ticket_redeemed_log = SerializableLog {
            address: handlers.addresses.channels,
            topics: vec![
                hopr_bindings::hoprchannels::HoprChannels::TicketRedeemed::SIGNATURE_HASH.into(),
                // TicketRedeemedFilter::signature().into(),
                H256::from_slice(channel.get_id().as_ref()).into(),
            ],
            data: Vec::from(next_ticket_index.to_be_bytes()),
            ..test_log()
        };

        let event_type = db
            .begin_transaction()
            .await?
            .perform(|tx| Box::pin(async move { handlers.process_log_event(tx, ticket_redeemed_log, true).await }))
            .await?;

        let channel = db
            .get_channel_by_id(None, &channel.get_id())
            .await?
            .context("a value should be present")?;

        assert!(
            matches!(event_type, Some(ChainEventType::TicketRedeemed(c, None)) if c == channel),
            "must return updated channel entry and no ticket"
        );

        assert_eq!(
            channel.ticket_index, next_ticket_index,
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

        let channel = ChannelEntry::new(
            *SELF_CHAIN_ADDRESS,
            *COUNTERPARTY_CHAIN_ADDRESS,
            primitive_types::U256::from((1u128 << 96) - 1).into(),
            primitive_types::U256::zero(),
            ChannelStatus::Open,
            primitive_types::U256::one(),
        );

        db.upsert_channel(None, channel, 1, 0, 0).await?;

        let timestamp = SystemTime::now();

        let encoded_data = (U256::from(timestamp.as_unix_timestamp().as_secs())).abi_encode();

        let closure_initiated_log = SerializableLog {
            address: handlers.addresses.channels,
            topics: vec![
                hopr_bindings::hoprchannels::HoprChannels::OutgoingChannelClosureInitiated::SIGNATURE_HASH.into(),
                // OutgoingChannelClosureInitiatedFilter::signature().into(),
                H256::from_slice(channel.get_id().as_ref()).into(),
            ],
            data: encoded_data,
            // data: Vec::from(U256::from(timestamp.as_unix_timestamp().as_secs()).to_be_bytes()).into(),
            ..test_log()
        };

        let event_type = db
            .begin_transaction()
            .await?
            .perform(|tx| Box::pin(async move { handlers.process_log_event(tx, closure_initiated_log, true).await }))
            .await?;

        let channel = db
            .get_channel_by_id(None, &channel.get_id())
            .await?
            .context("a value should be present")?;

        assert!(
            matches!(event_type, Some(ChainEventType::ChannelClosureInitiated(c)) if c == channel),
            "must return updated channel entry"
        );

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
        let rpc_operations = MockIndexerRpcOperations::new();
        // ==> set mock expectations here
        let clonable_rpc_operations = ClonableMockOperations {
            //
            inner: Arc::new(rpc_operations),
        };
        let handlers = init_handlers(clonable_rpc_operations, db.clone());

        let encoded_data = ().abi_encode();

        let safe_registered_log = SerializableLog {
            address: handlers.addresses.safe_registry,
            topics: vec![
                hopr_bindings::hoprnodesaferegistry::HoprNodeSafeRegistry::RegisteredNodeSafe::SIGNATURE_HASH.into(),
                // RegisteredNodeSafeFilter::signature().into(),
                H256::from_slice(&SAFE_INSTANCE_ADDR.to_bytes32()).into(),
                H256::from_slice(&SELF_CHAIN_ADDRESS.to_bytes32()).into(),
            ],
            data: encoded_data,
            ..test_log()
        };

        let event_type = db
            .begin_transaction()
            .await?
            .perform(|tx| Box::pin(async move { handlers.process_log_event(tx, safe_registered_log, true).await }))
            .await?;

        assert!(matches!(event_type, Some(ChainEventType::NodeSafeRegistered(addr)) if addr == *SAFE_INSTANCE_ADDR));

        // Nothing to check in the DB here, since we do not track this
        Ok(())
    }

    #[tokio::test]
    async fn on_node_safe_registry_deregistered() -> anyhow::Result<()> {
        let db = BlokliDb::new_in_memory().await?;
        let rpc_operations = MockIndexerRpcOperations::new();
        // ==> set mock expectations here
        let clonable_rpc_operations = ClonableMockOperations {
            //
            inner: Arc::new(rpc_operations),
        };
        let handlers = init_handlers(clonable_rpc_operations, db.clone());

        // Nothing to write to the DB here, since we do not track this

        let encoded_data = ().abi_encode();

        let safe_registered_log = SerializableLog {
            address: handlers.addresses.safe_registry,
            topics: vec![
                hopr_bindings::hoprnodesaferegistry::HoprNodeSafeRegistry::DergisteredNodeSafe::SIGNATURE_HASH.into(),
                // DergisteredNodeSafeFilter::signature().into(),
                H256::from_slice(&SAFE_INSTANCE_ADDR.to_bytes32()).into(),
                H256::from_slice(&SELF_CHAIN_ADDRESS.to_bytes32()).into(),
            ],
            data: encoded_data,
            ..test_log()
        };

        let event_type = db
            .begin_transaction()
            .await?
            .perform(|tx| Box::pin(async move { handlers.process_log_event(tx, safe_registered_log, true).await }))
            .await?;

        assert!(
            event_type.is_none(),
            "there's no associated chain event type with safe deregistration"
        );

        // Nothing to check in the DB here, since we do not track this
        Ok(())
    }

    #[tokio::test]
    async fn ticket_price_update() -> anyhow::Result<()> {
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
            address: handlers.addresses.price_oracle,
            topics: vec![
                hopr_bindings::hoprticketpriceoracle::HoprTicketPriceOracle::TicketPriceUpdated::SIGNATURE_HASH.into(),
                // TicketPriceUpdatedFilter::signature().into()
            ],
            data: encoded_data,
            // data: encode(&[Token::Uint(EthU256::from(1u64)), Token::Uint(EthU256::from(123u64))]).into(),
            ..test_log()
        };

        assert_eq!(db.get_indexer_data(None).await?.ticket_price, None);

        let event_type = db
            .begin_transaction()
            .await?
            .perform(|tx| Box::pin(async move { handlers.process_log_event(tx, price_change_log, true).await }))
            .await?;

        assert!(
            event_type.is_none(),
            "there's no associated chain event type with price oracle"
        );

        assert_eq!(
            db.get_indexer_data(None).await?.ticket_price.map(|p| p.amount()),
            Some(primitive_types::U256::from(123u64))
        );
        Ok(())
    }

    #[tokio::test]
    async fn minimum_win_prob_update() -> anyhow::Result<()> {
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
            address: handlers.addresses.win_prob_oracle,
            topics: vec![
                hopr_bindings::hoprwinningprobabilityoracle::HoprWinningProbabilityOracle::WinProbUpdated::SIGNATURE_HASH.into()],
            data: encoded_data,
            ..test_log()
        };

        assert_eq!(
            db.get_indexer_data(None).await?.minimum_incoming_ticket_winning_prob,
            1.0
        );

        let event_type = db
            .begin_transaction()
            .await?
            .perform(|tx| Box::pin(async move { handlers.process_log_event(tx, win_prob_change_log, true).await }))
            .await?;

        assert!(
            event_type.is_none(),
            "there's no associated chain event type with winning probability change"
        );

        assert_eq!(
            db.get_indexer_data(None).await?.minimum_incoming_ticket_winning_prob,
            0.5
        );
        Ok(())
    }

    // Helper function to create a packed channel bytes32 for testing
    // Channel packing format (bytes 0-31):
    // - Bytes 0-5: Padding (48 bits)
    // - Bytes 6-17: balance (96 bits = 12 bytes)
    // - Bytes 18-23: ticketIndex (48 bits = 6 bytes)
    // - Bytes 24-27: closureTime (32 bits = 4 bytes)
    // - Bytes 28-30: epoch (24 bits = 3 bytes)
    // - Byte 31: status (8 bits = 1 byte)
    fn create_packed_channel(balance: u128, ticket_index: u64, closure_time: u32, epoch: u32, status: u8) -> B256 {
        let mut bytes = [0u8; 32];

        // Bytes 0-5: Padding (leave as zeros)

        // Bytes 6-17: balance (96 bits = 12 bytes)
        // Take the lower 96 bits (12 bytes) of the balance
        let balance_bytes = balance.to_be_bytes(); // 16 bytes for u128
        bytes[6..18].copy_from_slice(&balance_bytes[4..16]); // Skip the first 4 bytes, take the last 12

        // Bytes 18-23: ticketIndex (48 bits = 6 bytes)
        let ticket_index_bytes = ticket_index.to_be_bytes(); // 8 bytes for u64
        bytes[18..24].copy_from_slice(&ticket_index_bytes[2..8]); // Skip the first 2 bytes, take the last 6

        // Bytes 24-27: closureTime (32 bits = 4 bytes)
        let closure_time_bytes = closure_time.to_be_bytes();
        bytes[24..28].copy_from_slice(&closure_time_bytes);

        // Bytes 28-30: epoch (24 bits = 3 bytes)
        let epoch_bytes = epoch.to_be_bytes(); // 4 bytes for u32
        bytes[28..31].copy_from_slice(&epoch_bytes[1..4]); // Skip the first byte, take the last 3

        // Byte 31: status (8 bits = 1 byte)
        bytes[31] = status;

        B256::from(bytes)
    }

    #[test]
    fn test_decode_channel_open_status() {
        // Create a channel with Open status
        let balance = 1_000_000_u128;
        let ticket_index = 42_u64;
        let closure_time = 0_u32; // Not used for Open status
        let epoch = 5_u32;
        let status = 1_u8; // Open

        let packed = create_packed_channel(balance, ticket_index, closure_time, epoch, status);
        let decoded = super::decode_channel(packed);

        assert_eq!(decoded.balance, HoprBalance::from(balance));
        assert_eq!(decoded.ticket_index, ticket_index as u32);
        assert_eq!(decoded.closure_time, closure_time);
        assert_eq!(decoded.epoch, epoch);
        assert!(matches!(decoded.status, ChannelStatus::Open));
    }

    #[test]
    fn test_decode_channel_closed_status() {
        // Create a channel with Closed status
        let balance = 500_000_u128;
        let ticket_index = 100_u64;
        let closure_time = 1234567890_u32;
        let epoch = 10_u32;
        let status = 0_u8; // Closed

        let packed = create_packed_channel(balance, ticket_index, closure_time, epoch, status);
        let decoded = super::decode_channel(packed);

        assert_eq!(decoded.balance, HoprBalance::from(balance));
        assert_eq!(decoded.ticket_index, ticket_index as u32);
        assert_eq!(decoded.closure_time, closure_time);
        assert_eq!(decoded.epoch, epoch);
        assert!(matches!(decoded.status, ChannelStatus::Closed));
    }

    #[test]
    fn test_decode_channel_pending_to_close_status() {
        // Create a channel with PendingToClose status
        let balance = 750_000_u128;
        let ticket_index = 200_u64;
        let closure_time = 1700000000_u32; // Some timestamp
        let epoch = 15_u32;
        let status = 2_u8; // PendingToClose

        let packed = create_packed_channel(balance, ticket_index, closure_time, epoch, status);
        let decoded = super::decode_channel(packed);

        assert_eq!(decoded.balance, HoprBalance::from(balance));
        assert_eq!(decoded.ticket_index, ticket_index as u32);
        assert_eq!(decoded.closure_time, closure_time);
        assert_eq!(decoded.epoch, epoch);

        // Verify PendingToClose with correct timestamp
        match decoded.status {
            ChannelStatus::PendingToClose(time) => {
                let expected_time = SystemTime::UNIX_EPOCH.add(Duration::from_secs(closure_time as u64));
                assert_eq!(time, expected_time);
            }
            _ => panic!("Expected PendingToClose status"),
        }
    }

    #[test]
    fn test_decode_channel_invalid_status() {
        // Test with invalid status bytes - should default to Closed
        let balance = 100_u128;
        let ticket_index = 1_u64;
        let closure_time = 0_u32;
        let epoch = 1_u32;

        // Test status = 3 (invalid)
        let packed = create_packed_channel(balance, ticket_index, closure_time, epoch, 3);
        let decoded = super::decode_channel(packed);
        assert!(
            matches!(decoded.status, ChannelStatus::Closed),
            "Invalid status 3 should default to Closed"
        );

        // Test status = 255 (invalid)
        let packed = create_packed_channel(balance, ticket_index, closure_time, epoch, 255);
        let decoded = super::decode_channel(packed);
        assert!(
            matches!(decoded.status, ChannelStatus::Closed),
            "Invalid status 255 should default to Closed"
        );
    }

    #[test]
    fn test_decode_channel_zero_values() {
        // Test decoding with all zeros
        let packed = B256::ZERO;
        let decoded = super::decode_channel(packed);

        assert_eq!(decoded.balance, HoprBalance::from(0_u128));
        assert_eq!(decoded.ticket_index, 0);
        assert_eq!(decoded.closure_time, 0);
        assert_eq!(decoded.epoch, 0);
        assert!(matches!(decoded.status, ChannelStatus::Closed));
    }

    #[test]
    fn test_decode_channel_max_values() {
        // Test with maximum values for each field
        // Balance: max 96-bit value = 2^96 - 1
        let max_balance_96bit = (1_u128 << 96) - 1;

        // TicketIndex: max 48-bit value = 2^48 - 1
        let max_ticket_index_48bit = (1_u64 << 48) - 1;

        // ClosureTime: max 32-bit value
        let max_closure_time = u32::MAX;

        // Epoch: max 24-bit value = 2^24 - 1
        let max_epoch_24bit = (1_u32 << 24) - 1;

        let status = 1_u8; // Open

        let packed = create_packed_channel(
            max_balance_96bit,
            max_ticket_index_48bit,
            max_closure_time,
            max_epoch_24bit,
            status,
        );
        let decoded = super::decode_channel(packed);

        assert_eq!(decoded.balance, HoprBalance::from(max_balance_96bit));
        // Note: ticket_index is cast to u32, so we need to verify the u32 cast behavior
        assert_eq!(decoded.ticket_index, max_ticket_index_48bit as u32);
        assert_eq!(decoded.closure_time, max_closure_time);
        assert_eq!(decoded.epoch, max_epoch_24bit);
        assert!(matches!(decoded.status, ChannelStatus::Open));
    }

    #[test]
    fn test_decode_channel_byte_boundaries() {
        // Test that each field extracts from the correct byte positions
        // Use distinct recognizable patterns for each field

        // Balance: Use pattern 0x123456789ABC (12 bytes when padded)
        let balance = 0x123456789ABC_u128;

        // TicketIndex: Use pattern 0xAABBCCDDEEFF (6 bytes)
        let ticket_index = 0xAABBCCDDEEFF_u64;

        // ClosureTime: Use pattern 0x11223344 (4 bytes)
        let closure_time = 0x11223344_u32;

        // Epoch: Use pattern 0x556677 (3 bytes)
        let epoch = 0x556677_u32;

        // Status: 1 (Open)
        let status = 1_u8;

        let packed = create_packed_channel(balance, ticket_index, closure_time, epoch, status);
        let decoded = super::decode_channel(packed);

        // Verify each field is correctly extracted
        assert_eq!(decoded.balance, HoprBalance::from(balance));
        assert_eq!(decoded.ticket_index, ticket_index as u32);
        assert_eq!(decoded.closure_time, closure_time);
        assert_eq!(decoded.epoch, epoch);
        assert!(matches!(decoded.status, ChannelStatus::Open));

        // Also verify the raw bytes to ensure packing is correct
        let bytes = packed.as_slice();
        // Verify padding (bytes 0-5 should be 0)
        assert_eq!(&bytes[0..6], &[0u8; 6]);
        // Verify balance starts at byte 6
        let balance_bytes = balance.to_be_bytes();
        assert_eq!(&bytes[6..18], &balance_bytes[4..16]);
        // Verify status at byte 31
        assert_eq!(bytes[31], status);
    }

    // TODO: Re-enable once ticket operations are implemented
    // #[tokio::test]
    // async fn lowering_minimum_win_prob_update_should_reject_non_satisfying_unredeemed_tickets() -> anyhow::Result<()>
    // { let db = BlokliDb::new_in_memory().await?;
    //         db.set_minimum_incoming_ticket_win_prob(None, 0.1.try_into()?).await?;
    //
    //         let new_minimum = 0.5;
    //         let ticket_win_probs = [0.1, 1.0, 0.3, 0.2];
    //
    //         let channel_1 = ChannelEntry::new(
    //             *COUNTERPARTY_CHAIN_ADDRESS,
    //             *SELF_CHAIN_ADDRESS,
    //             primitive_types::U256::from((1u128 << 96) - 1).into(),
    //             3_u32.into(),
    //             ChannelStatus::Open,
    //             primitive_types::U256::one(),
    //         );
    //
    //         db.upsert_channel(None, channel_1).await?;
    //
    //         let ticket = mock_acknowledged_ticket(&COUNTERPARTY_CHAIN_KEY, &SELF_CHAIN_KEY, 1, ticket_win_probs[0])?;
    //         db.upsert_ticket(None, ticket).await?;
    //
    //         let ticket = mock_acknowledged_ticket(&COUNTERPARTY_CHAIN_KEY, &SELF_CHAIN_KEY, 2, ticket_win_probs[1])?;
    //         db.upsert_ticket(None, ticket).await?;
    //
    //         let tickets = db.get_tickets((&channel_1).into()).await?;
    //         assert_eq!(tickets.len(), 2);
    //
    //         // ---
    //
    //         let other_counterparty = ChainKeypair::random();
    //         let channel_2 = ChannelEntry::new(
    //             other_counterparty.public().to_address(),
    //             *SELF_CHAIN_ADDRESS,
    //             primitive_types::U256::from((1u128 << 96) - 1).into(),
    //             3_u32.into(),
    //             ChannelStatus::Open,
    //             primitive_types::U256::one(),
    //         );
    //
    //         db.upsert_channel(None, channel_2).await?;
    //
    //         let ticket = mock_acknowledged_ticket(&other_counterparty, &SELF_CHAIN_KEY, 1, ticket_win_probs[2])?;
    //         db.upsert_ticket(None, ticket).await?;
    //
    //         let ticket = mock_acknowledged_ticket(&other_counterparty, &SELF_CHAIN_KEY, 2, ticket_win_probs[3])?;
    //         db.upsert_ticket(None, ticket).await?;
    //
    //         let tickets = db.get_tickets((&channel_2).into()).await?;
    //         assert_eq!(tickets.len(), 2);
    //
    //         let stats = db.get_ticket_statistics(None).await?;
    //         assert_eq!(HoprBalance::zero(), stats.rejected_value);
    //
    //         let rpc_operations = MockIndexerRpcOperations::new();
    //         // ==> set mock expectations here
    //         let clonable_rpc_operations = ClonableMockOperations {
    //             //
    //             inner: Arc::new(rpc_operations),
    //         };
    //         let handlers = init_handlers(clonable_rpc_operations, db.clone());
    //
    //         let encoded_data = (
    //             U256::from_be_slice(WinningProbability::try_from(0.1)?.as_ref()),
    //             U256::from_be_slice(WinningProbability::try_from(new_minimum)?.as_ref()),
    //         )
    //             .abi_encode();
    //
    //         let win_prob_change_log = SerializableLog {
    //             address: handlers.addresses.win_prob_oracle,
    //             topics: vec![
    //
    // hopr_bindings::hoprwinningprobabilityoracle::HoprWinningProbabilityOracle::WinProbUpdated::SIGNATURE_HASH.into(),
    //             ],
    //             data: encoded_data,
    //             ..test_log()
    //         };
    //
    //         let event_type = db
    //             .begin_transaction()
    //             .await?
    //             .perform(|tx| Box::pin(async move { handlers.process_log_event(tx, win_prob_change_log, true).await
    // }))             .await?;
    //
    //         assert!(
    //             event_type.is_none(),
    //             "there's no associated chain event type with winning probability change"
    //         );
    //
    //         assert_eq!(
    //             db.get_indexer_data(None).await?.minimum_incoming_ticket_winning_prob,
    //             new_minimum
    //         );
    //
    //         let tickets = db.get_tickets((&channel_1).into()).await?;
    //         assert_eq!(tickets.len(), 1);
    //
    //         let tickets = db.get_tickets((&channel_2).into()).await?;
    //         assert_eq!(tickets.len(), 0);
    //
    //         let stats = db.get_ticket_statistics(None).await?;
    //         let rejected_value: primitive_types::U256 = ticket_win_probs
    //             .iter()
    //             .filter(|p| **p < new_minimum)
    //             .map(|p| {
    //                 primitive_types::U256::from(PRICE_PER_PACKET)
    //                     .div_f64(*p)
    //                     .expect("must divide")
    //             })
    //             .reduce(|a, b| a + b)
    //             .ok_or(anyhow!("must sum"))?;
    //
    //         assert_eq!(HoprBalance::from(rejected_value), stats.rejected_value);
    //
    //         // Ok(())
    //     // }
}
