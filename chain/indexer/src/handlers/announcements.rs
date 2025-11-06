use blokli_chain_rpc::HoprIndexerRpcOperations;
use blokli_db::{BlokliDbAllOperations, OpenTransaction, api::info::DomainSeparator, errors::DbSqlError};
use hopr_bindings::hopr_announcements::HoprAnnouncements::HoprAnnouncementsEvents;
use hopr_crypto_types::prelude::OffchainSignature;
use hopr_internal_types::announcement::KeyBinding;
use hopr_primitive_types::prelude::Address;
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
                let node_address: Address = address_announcement.node.into();

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
                    key_binding.chain_key.into(),
                    key_binding.ed25519_pub_key.0.try_into()?,
                    OffchainSignature::try_from((key_binding.ed25519_sig_0.0, key_binding.ed25519_sig_1.0))?,
                ) {
                    Ok(binding) => {
                        let chain_key = binding.chain_key;
                        match self
                            .db
                            .upsert_account(
                                Some(tx),
                                chain_key,
                                binding.packet_key,
                                None, // safe_address is None for key bindings
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
                let node_address: Address = revocation.node.into();
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

    use alloy::{
        dyn_abi::DynSolValue,
        primitives::Address as AlloyAddress,
        sol_types::{SolEvent, SolValue},
    };
    use anyhow::Context;
    use blokli_db::{
        BlokliDbGeneralModelOperations,
        accounts::{BlokliDbAccountOperations, ChainOrPacketKey},
        db::BlokliDb,
    };
    use hopr_crypto_types::keypairs::Keypair;
    use hopr_internal_types::{
        account::{AccountEntry, AccountType},
        announcement::KeyBinding,
    };
    use hopr_primitive_types::prelude::SerializableLog;
    use multiaddr::Multiaddr;

    use super::*;
    use crate::{handlers::test_utils::test_helpers::*, state::IndexerEvent};

    #[tokio::test]
    async fn announce_keybinding() -> anyhow::Result<()> {
        let db = BlokliDb::new_in_memory().await?;

        let rpc_operations = MockIndexerRpcOperations::new();
        // ==> set mock expectations here
        let clonable_rpc_operations = ClonableMockOperations {
            inner: Arc::new(rpc_operations),
        };

        let (handlers, _state, mut event_receiver) = init_handlers_with_events(clonable_rpc_operations, db.clone());

        let keybinding = KeyBinding::new(*SELF_CHAIN_ADDRESS, &SELF_PRIV_KEY);

        let keybinding_log = SerializableLog {
            address: handlers.addresses.announcements,
            topics: vec![
                hopr_bindings::hopr_announcements_events::HoprAnnouncementsEvents::KeyBinding::SIGNATURE_HASH.into(),
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

        db.begin_transaction()
            .await?
            .perform(|tx| Box::pin(async move { handlers.process_log_event(tx, keybinding_log, true).await }))
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
    async fn announce_address_announcement() -> anyhow::Result<()> {
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
            published_at: 1,
        };

        let test_multiaddr_empty: Multiaddr = "".parse()?;

        let address_announcement_empty_log_encoded_data = DynSolValue::Tuple(vec![
            DynSolValue::Address(AlloyAddress::from_slice(SELF_CHAIN_ADDRESS.as_ref())),
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
            DynSolValue::Address(AlloyAddress::from_slice(SELF_CHAIN_ADDRESS.as_ref())),
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
            entry_type: AccountType::Announced {
                multiaddr: test_multiaddr.clone(),
                updated_block: 1,
            },
            published_at: 1,
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
            DynSolValue::Address(AlloyAddress::from_slice(SELF_CHAIN_ADDRESS.as_ref())),
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
            entry_type: AccountType::Announced {
                multiaddr: test_multiaddr_dns.clone(),
                updated_block: 2,
            },
            published_at: 1,
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
    async fn announce_revoke() -> anyhow::Result<()> {
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
            published_at: 1,
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
}
