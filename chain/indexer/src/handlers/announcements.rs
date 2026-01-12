use blokli_api_types::TokenValueString;
use blokli_chain_rpc::HoprIndexerRpcOperations;
use blokli_chain_types::AlloyAddressExt;
use blokli_db::{BlokliDbAllOperations, OpenTransaction, api::info::DomainSeparator, errors::DbSqlError};
use hopr_bindings::hopr_announcements::HoprAnnouncements::HoprAnnouncementsEvents;
use hopr_crypto_types::prelude::OffchainSignature;
use hopr_internal_types::announcement::KeyBinding;
use hopr_primitive_types::{
    prelude::{Address, HoprBalance},
    traits::IntoEndian,
};
use tracing::{debug, error, warn};

use super::{ContractEventHandlers, helpers::construct_account_update};
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
    pub(super) async fn on_announcement_event(
        &self,
        tx: &OpenTransaction,
        event: HoprAnnouncementsEvents,
        block_number: u32,
        tx_index: u32,
        log_index: u32,
        is_synced: bool,
    ) -> Result<()> {
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
                    return Ok(());
                }
                let node_address: Address = address_announcement.node.to_hopr_address();

                self.db
                    .insert_announcement(
                        Some(tx),
                        node_address,
                        address_announcement.baseMultiaddr.parse()?,
                        block_number,
                    )
                    .await
                    .map_err(|e| match e {
                        DbSqlError::MissingAccount => CoreEthereumIndexerError::AnnounceBeforeKeyBinding,
                        other => other.into(),
                    })?;

                // Publish AccountUpdated event if synced
                if is_synced {
                    match construct_account_update(tx.as_ref(), &node_address).await {
                        Ok(account) => {
                            self.indexer_state
                                .publish_event(crate::state::IndexerEvent::AccountUpdated(account));
                        }
                        Err(e) => {
                            warn!("Failed to construct account update for AddressAnnouncement: {}", e);
                        }
                    }
                }
            }
            HoprAnnouncementsEvents::KeyBinding(key_binding) => {
                debug!(
                    address = %key_binding.chain_key,
                    public_key = %key_binding.ed25519_pub_key,
                    "on_announcement_event: KeyBinding",
                );
                match KeyBinding::from_parts(
                    key_binding.chain_key.to_hopr_address(),
                    key_binding.ed25519_pub_key.0.try_into()?,
                    OffchainSignature::try_from((key_binding.ed25519_sig_0.0, key_binding.ed25519_sig_1.0))?,
                ) {
                    Ok(binding) => {
                        let chain_key = binding.chain_key;
                        // key_id is a U256, but we only support u32 for now as it maps to the account ID
                        // This should be safe as long as we don't have more than 2^32 accounts
                        let key_id: u32 = key_binding.key_id.try_into().unwrap_or_default();

                        // Check if a safe is already registered for this node
                        let safe_address = match self.db.get_safe_for_registered_node(Some(tx), chain_key).await {
                            Ok(safe) => safe,
                            Err(e) => {
                                warn!(
                                    chain_key = %chain_key,
                                    error = %e,
                                    "Failed to lookup safe registration for node, proceeding without safe address"
                                );
                                None
                            }
                        };

                        match self
                            .db
                            .upsert_account(
                                Some(tx),
                                key_id,
                                chain_key,
                                binding.packet_key,
                                safe_address,
                                block_number,
                                tx_index,
                                log_index,
                            )
                            .await
                        {
                            Ok(_) => {
                                // Publish AccountUpdated event if synced
                                if is_synced {
                                    match construct_account_update(tx.as_ref(), &chain_key).await {
                                        Ok(account) => {
                                            self.indexer_state
                                                .publish_event(crate::state::IndexerEvent::AccountUpdated(account));
                                        }
                                        Err(e) => {
                                            warn!("Failed to construct account update for KeyBinding: {}", e);
                                        }
                                    }
                                }
                            }
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
                let node_address: Address = revocation.node.to_hopr_address();
                match self.db.delete_all_announcements(Some(tx), node_address).await {
                    Ok(_) => {
                        // Publish AccountUpdated event if synced
                        if is_synced {
                            match construct_account_update(tx.as_ref(), &node_address).await {
                                Ok(account) => {
                                    self.indexer_state
                                        .publish_event(crate::state::IndexerEvent::AccountUpdated(account));
                                }
                                Err(e) => {
                                    warn!("Failed to construct account update for RevokeAnnouncement: {}", e);
                                }
                            }
                        }
                    }
                    Err(DbSqlError::MissingAccount) => {
                        return Err(CoreEthereumIndexerError::RevocationBeforeKeyBinding);
                    }
                    Err(e) => return Err(e.into()),
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

                let fee = HoprBalance::from_be_bytes(fee_update.newFee.to_be_bytes::<32>());

                self.db.update_key_binding_fee(Some(tx), fee).await?;

                // Publish subscription event only when indexer is in synced mode
                if is_synced {
                    let fee_str = fee.amount().to_string();
                    self.indexer_state
                        .publish_event(crate::state::IndexerEvent::KeyBindingFeeUpdated(TokenValueString(
                            fee_str,
                        )));
                }
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
        }

        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use std::sync::Arc;

    use anyhow::Context;
    use blokli_db::{
        BlokliDbGeneralModelOperations,
        accounts::{BlokliDbAccountOperations, ChainOrPacketKey},
        db::BlokliDb,
        info::BlokliDbInfoOperations,
    };
    use hopr_bindings::{
        exports::alloy::{
            dyn_abi::DynSolValue,
            primitives::{Address as AlloyAddress, FixedBytes},
            sol_types::{SolEvent, SolValue},
        },
        hopr_announcements_events::HoprAnnouncementsEvents::{
            KeyBinding as KeyBindingEvent, KeyBindingFeeUpdate as KeyBindingFeeUpdateEvent,
        },
    };
    use hopr_crypto_types::keypairs::Keypair;
    use hopr_internal_types::{
        account::{AccountEntry, AccountType},
        announcement::KeyBinding,
    };
    use hopr_primitive_types::prelude::{SerializableLog, U256};
    use multiaddr::Multiaddr;

    use super::*;
    use crate::{handlers::test_utils::test_helpers::*, state::IndexerEvent};

    #[tokio::test]
    async fn test_announce_keybinding() -> anyhow::Result<()> {
        let db = BlokliDb::new_in_memory().await?;

        let rpc_operations = MockIndexerRpcOperations::new();
        // ==> set mock expectations here
        let clonable_rpc_operations = ClonableMockOperations {
            inner: Arc::new(rpc_operations),
        };

        let (handlers, _state, mut event_receiver) = init_handlers_with_events(clonable_rpc_operations, db.clone());

        let keybinding = KeyBinding::new(*SELF_CHAIN_ADDRESS, &SELF_PRIV_KEY);

        // KeyBinding event has 4 non-indexed parameters: chain_key, ed25519_pub_key, ed25519_sig_0, ed25519_sig_1
        // ed25519_pub_key and signatures are bytes32
        let packet_key_bytes = keybinding.packet_key.as_ref();
        let sig_bytes = keybinding.signature.as_ref();

        // Create KeyBinding event using bindings
        let event = KeyBindingEvent {
            key_id: hopr_bindings::exports::alloy::primitives::U256::ZERO,
            chain_key: AlloyAddress::from_hopr_address(*SELF_CHAIN_ADDRESS),
            ed25519_pub_key: FixedBytes::<32>::from_slice(packet_key_bytes),
            ed25519_sig_0: FixedBytes::<32>::from_slice(&sig_bytes[..32]),
            ed25519_sig_1: FixedBytes::<32>::from_slice(&sig_bytes[32..64]),
        };

        let keybinding_log = event_to_log(event, handlers.addresses.announcements);

        let account_entry = AccountEntry {
            public_key: *SELF_PRIV_KEY.public(),
            chain_addr: *SELF_CHAIN_ADDRESS,
            entry_type: AccountType::NotAnnounced,
            safe_address: None,
            key_id: 0.into(),
        };

        db.begin_transaction()
            .await?
            .perform(|tx| Box::pin(async move { handlers.process_log_event(tx, keybinding_log, true).await }))
            .await?;

        // Verify AccountUpdated event was published
        let _event = tokio::time::timeout(std::time::Duration::from_millis(100), event_receiver.recv())
            .await
            .expect("Timeout waiting for AccountUpdated event")
            .expect("Expected AccountUpdated event to be published");

        assert_eq!(
            db.get_account(None, ChainOrPacketKey::ChainKey(*SELF_CHAIN_ADDRESS))
                .await?
                .context("a value should be present")?,
            account_entry
        );
        Ok(())
    }

    #[tokio::test]
    async fn test_announce_address_announcement() -> anyhow::Result<()> {
        let db = BlokliDb::new_in_memory().await?;
        let rpc_operations = MockIndexerRpcOperations::new();
        // ==> set mock expectations here
        let clonable_rpc_operations = ClonableMockOperations {
            //
            inner: Arc::new(rpc_operations),
        };
        let (handlers, _indexer_state, mut event_receiver) =
            init_handlers_with_events(clonable_rpc_operations, db.clone());

        // Assume that there is a keybinding
        // Create account using upsert_account
        db.upsert_account(
            None,
            1,
            *SELF_CHAIN_ADDRESS,
            *SELF_PRIV_KEY.public(),
            None, // no safe_address
            1,    // block
            0,    // tx_index
            0,    // log_index
        )
        .await?;

        let account_entry = AccountEntry {
            public_key: *SELF_PRIV_KEY.public(),
            chain_addr: *SELF_CHAIN_ADDRESS,
            entry_type: AccountType::NotAnnounced,
            safe_address: None,
            key_id: 1.into(),
        };

        let test_multiaddr_empty: Multiaddr = "".parse()?;

        let address_announcement_empty_log_encoded_data = DynSolValue::Tuple(vec![
            DynSolValue::Address(AlloyAddress::from_hopr_address(*SELF_CHAIN_ADDRESS)),
            DynSolValue::String(test_multiaddr_empty.to_string()),
        ])
        .abi_encode();

        let address_announcement_empty_log = SerializableLog {
            address: handlers.addresses.announcements,
            topics: vec![
                hopr_bindings::hopr_announcements_events::HoprAnnouncementsEvents::AddressAnnouncement::SIGNATURE_HASH
                    .into(),
            ],
            data: address_announcement_empty_log_encoded_data[32..].into(),
            ..test_log()
        };

        let handlers_clone = handlers.clone();
        db.begin_transaction()
            .await?
            .perform(|tx| {
                Box::pin(async move {
                    handlers_clone
                        .process_log_event(tx, address_announcement_empty_log, true)
                        .await
                })
            })
            .await?;

        // Empty multiaddress announcement should not publish an event

        assert_eq!(
            db.get_account(None, ChainOrPacketKey::ChainKey(*SELF_CHAIN_ADDRESS))
                .await?
                .context("a value should be present")?,
            account_entry
        );

        let test_multiaddr: Multiaddr = "/ip4/1.2.3.4/tcp/56".parse()?;

        let address_announcement_log_encoded_data = DynSolValue::Tuple(vec![
            DynSolValue::Address(AlloyAddress::from_hopr_address(*SELF_CHAIN_ADDRESS)),
            DynSolValue::String(test_multiaddr.to_string()),
        ])
        .abi_encode();

        let address_announcement_log = SerializableLog {
            address: handlers.addresses.announcements,
            block_number: 1,
            topics: vec![
                hopr_bindings::hopr_announcements_events::HoprAnnouncementsEvents::AddressAnnouncement::SIGNATURE_HASH
                    .into(),
            ],
            data: address_announcement_log_encoded_data[32..].into(),
            ..test_log()
        };

        let announced_account_entry = AccountEntry {
            public_key: *SELF_PRIV_KEY.public(),
            chain_addr: *SELF_CHAIN_ADDRESS,
            entry_type: AccountType::Announced(vec![test_multiaddr.clone()]),
            safe_address: None,
            key_id: 1.into(),
        };

        let handlers_clone = handlers.clone();
        db.begin_transaction()
            .await?
            .perform(|tx| {
                Box::pin(async move {
                    handlers_clone
                        .process_log_event(tx, address_announcement_log, true)
                        .await
                })
            })
            .await?;

        // Verify AccountUpdated event was published with the multiaddress
        let event = try_recv_event(&mut event_receiver).expect("Expected AccountUpdated event to be published");
        match event {
            IndexerEvent::AccountUpdated(account) => {
                assert_eq!(account.multi_addresses.len(), 1, "Should have one multiaddress");
                assert_eq!(
                    account.multi_addresses[0],
                    test_multiaddr.to_string(),
                    "Published multiaddress should match"
                );
            }
            _ => panic!("Expected AccountUpdated event, got {:?}", event),
        }

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
            DynSolValue::Address(AlloyAddress::from_hopr_address(*SELF_CHAIN_ADDRESS)),
            DynSolValue::String(test_multiaddr_dns.to_string()),
        ])
        .abi_encode();

        let address_announcement_dns_log = SerializableLog {
            address: handlers.addresses.announcements,
            block_number: 2,
            topics: vec![
                hopr_bindings::hopr_announcements_events::HoprAnnouncementsEvents::AddressAnnouncement::SIGNATURE_HASH
                    .into(),
            ],
            data: address_announcement_dns_log_encoded_data[32..].into(),
            ..test_log()
        };

        let announced_dns_account_entry = AccountEntry {
            public_key: *SELF_PRIV_KEY.public(),
            chain_addr: *SELF_CHAIN_ADDRESS,
            entry_type: AccountType::Announced(vec![test_multiaddr_dns.clone()]),
            safe_address: None,
            key_id: 1.into(),
        };

        db.begin_transaction()
            .await?
            .perform(|tx| {
                Box::pin(async move { handlers.process_log_event(tx, address_announcement_dns_log, true).await })
            })
            .await?;

        // TODO: Add event verification - check published IndexerEvent instead of return value

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
    async fn test_announce_revoke() -> anyhow::Result<()> {
        let db = BlokliDb::new_in_memory().await?;
        let rpc_operations = MockIndexerRpcOperations::new();
        // ==> set mock expectations here
        let clonable_rpc_operations = ClonableMockOperations {
            //
            inner: Arc::new(rpc_operations),
        };
        let (handlers, _state, mut event_receiver) = init_handlers_with_events(clonable_rpc_operations, db.clone());

        let test_multiaddr: Multiaddr = "/ip4/1.2.3.4/tcp/56".parse()?;

        // Assume that there is a keybinding and an address announcement
        // Create account using upsert_account
        db.upsert_account(
            None,
            1,
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

        let encoded_data = (AlloyAddress::from_hopr_address(*SELF_CHAIN_ADDRESS),).abi_encode();

        let revoke_announcement_log = SerializableLog {
            address: handlers.addresses.announcements,
            topics: vec![
                hopr_bindings::hopr_announcements_events::HoprAnnouncementsEvents::RevokeAnnouncement::SIGNATURE_HASH
                    .into(),
            ],
            data: encoded_data,
            ..test_log()
        };

        let account_entry = AccountEntry {
            public_key: *SELF_PRIV_KEY.public(),
            chain_addr: *SELF_CHAIN_ADDRESS,
            entry_type: AccountType::NotAnnounced,
            safe_address: None,
            key_id: 1.into(),
        };

        db.begin_transaction()
            .await?
            .perform(|tx| Box::pin(async move { handlers.process_log_event(tx, revoke_announcement_log, true).await }))
            .await?;

        // Verify AccountUpdated event was published
        let _event = try_recv_event(&mut event_receiver).expect("Expected AccountUpdated event to be published");

        assert_eq!(
            db.get_account(None, ChainOrPacketKey::ChainKey(*SELF_CHAIN_ADDRESS))
                .await?
                .context("a value should be present")?,
            account_entry
        );
        Ok(())
    }

    #[tokio::test]
    async fn test_key_binding_fee_update() -> anyhow::Result<()> {
        let db = BlokliDb::new_in_memory().await?;
        let rpc_operations = MockIndexerRpcOperations::new();
        let clonable_rpc_operations = ClonableMockOperations {
            inner: Arc::new(rpc_operations),
        };
        let (handlers, _state, mut event_receiver) = init_handlers_with_events(clonable_rpc_operations, db.clone());

        // Initial fee should be empty/none
        let data = db.get_indexer_data(None).await?;
        assert!(data.key_binding_fee.is_none());

        // Create KeyBindingFeeUpdate event
        let new_fee_value: u128 = 123456;

        let event = KeyBindingFeeUpdateEvent {
            newFee: hopr_bindings::exports::alloy::primitives::U256::from(new_fee_value),
            oldFee: hopr_bindings::exports::alloy::primitives::U256::ZERO,
        };

        let log = event_to_log(event, handlers.addresses.announcements);

        db.begin_transaction()
            .await?
            .perform(|tx| Box::pin(async move { handlers.process_log_event(tx, log, true).await }))
            .await?;

        // Verify KeyBindingFeeUpdated event was published
        let event = try_recv_event(&mut event_receiver).expect("Expected KeyBindingFeeUpdated event to be published");
        match event {
            IndexerEvent::KeyBindingFeeUpdated(fee) => {
                assert_eq!(fee.0, new_fee_value.to_string(), "Published fee should match");
            }
            _ => panic!("Expected KeyBindingFeeUpdated event, got {:?}", event),
        }

        // Verify DB was updated
        let data = db.get_indexer_data(None).await?;
        assert_eq!(
            data.key_binding_fee.expect("Fee should be set").amount(),
            U256::from(new_fee_value)
        );

        Ok(())
    }
}
