use blokli_chain_rpc::HoprIndexerRpcOperations;
use blokli_chain_types::AlloyAddressExt;
use blokli_db::{BlokliDbAllOperations, OpenTransaction};
use hopr_bindings::hopr_token::HoprToken::HoprTokenEvents;
use hopr_types::primitive::prelude::{Address, HoprBalance, IntoEndian};
use tracing::{debug, trace};

use super::ContractEventHandlers;
#[cfg(all(feature = "telemetry", not(test)))]
use super::increment_indexer_contract_log_count;
use crate::{errors::Result, state::IndexerEvent};

impl<T, Db> ContractEventHandlers<T, Db>
where
    T: HoprIndexerRpcOperations + Clone + Send + 'static,
    Db: BlokliDbAllOperations + Clone,
{
    pub(super) async fn on_token_event(
        &self,
        _tx: &OpenTransaction,
        event: HoprTokenEvents,
        _is_synced: bool,
    ) -> Result<Vec<IndexerEvent>> {
        #[cfg(all(feature = "telemetry", not(test)))]
        increment_indexer_contract_log_count("token");

        match event {
            HoprTokenEvents::Transfer(transferred) => {
                let from: Address = transferred.from.to_hopr_address();
                let to: Address = transferred.to.to_hopr_address();

                trace!(
                    %from, %to,
                    "on_token_transfer_event"
                );
            }
            HoprTokenEvents::Approval(approved) => {
                let owner: Address = approved.owner.to_hopr_address();
                let spender: Address = approved.spender.to_hopr_address();

                trace!(
                    %owner, %spender, allowance = %approved.value,
                    "on_token_approval_event",

                );

                return Ok(vec![IndexerEvent::HoprApprovalUpdated {
                    owner,
                    spender,
                    allowance: HoprBalance::from_be_bytes(approved.value.to_be_bytes::<32>()),
                }]);
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

        Ok(vec![])
    }
}

#[cfg(test)]
mod tests {
    use std::sync::Arc;

    use blokli_chain_types::AlloyAddressExt;
    use blokli_db::{BlokliDbGeneralModelOperations, db::BlokliDb};
    use hex_literal::hex;
    use hopr_bindings::{
        exports::alloy::primitives::{Address as AlloyAddress, U256},
        hopr_token::HoprToken::{Approval, Transfer},
    };
    use hopr_types::primitive::prelude::{Address, ToHex};

    use crate::{
        errors::CoreEthereumIndexerError,
        handlers::test_utils::test_helpers::{
            CHANNELS_ADDR, ClonableMockOperations, MockIndexerRpcOperations, SAFE_INSTANCE_ADDR, TOKEN_ADDR,
            XHOPR_TOKEN_ADDR, event_to_log, init_handlers_with_events,
        },
        state::IndexerEvent,
        traits::ChainLogHandler,
    };

    fn approval(value: U256) -> Approval {
        Approval {
            owner: AlloyAddress::from_hopr_address(*SAFE_INSTANCE_ADDR),
            spender: AlloyAddress::from_hopr_address(*CHANNELS_ADDR),
            value,
        }
    }

    #[tokio::test]
    async fn approval_preserves_absolute_uint256_values() -> anyhow::Result<()> {
        let db = BlokliDb::new_in_memory().await?;
        let rpc = ClonableMockOperations {
            inner: Arc::new(MockIndexerRpcOperations::new()),
        };
        let (handlers, _, mut receiver) = init_handlers_with_events(rpc, db);
        // Distinct high and low limbs catch truncation and byte-order mistakes.
        let asymmetric = U256::from_be_bytes(hex!("0123456789abcdef112233445566778899aabbccddeeff001020304050607080"));
        let mut updates = Vec::new();
        // Reduction, increase, revocation, >u128, and maximum uint256.
        for value in [
            U256::from(1000),
            U256::from(2),
            U256::from(9000),
            U256::ZERO,
            asymmetric,
            U256::MAX,
        ] {
            handlers
                .collect_log_event(event_to_log(approval(value), *TOKEN_ADDR), true)
                .await?;
            match receiver.try_recv()? {
                IndexerEvent::HoprApprovalUpdated {
                    owner,
                    spender,
                    allowance,
                } => {
                    updates.push((owner.to_hex(), spender.to_hex(), allowance.amount().to_string()));
                }
                event => panic!("Expected Approval update, got {event:?}"),
            }
        }
        insta::assert_yaml_snapshot!(updates);
        assert!(receiver.try_recv().is_err());
        Ok(())
    }

    #[tokio::test]
    async fn approval_handler_does_not_publish_before_commit_or_during_sync() -> anyhow::Result<()> {
        let db = BlokliDb::new_in_memory().await?;
        let rpc = ClonableMockOperations {
            inner: Arc::new(MockIndexerRpcOperations::new()),
        };
        let (handlers, _, mut receiver) = init_handlers_with_events(rpc, db.clone());
        let log = event_to_log(approval(U256::MAX), *TOKEN_ADDR);
        let tx = db.begin_transaction().await?;
        let events = handlers.process_log_event(&tx, log.clone(), true).await?;
        assert!(matches!(events.as_slice(), [IndexerEvent::HoprApprovalUpdated { .. }]));
        assert!(
            receiver.try_recv().is_err(),
            "handler must only return events inside the transaction"
        );
        tx.rollback().await?;
        assert!(receiver.try_recv().is_err(), "rolled back processing must not publish");

        handlers.collect_log_event(log.clone(), false).await?;
        assert!(
            receiver.try_recv().is_err(),
            "historical synchronization must not publish"
        );
        handlers.collect_log_event(log, true).await?;
        assert!(matches!(receiver.try_recv()?, IndexerEvent::HoprApprovalUpdated { .. }));
        assert!(receiver.try_recv().is_err());
        Ok(())
    }

    #[tokio::test]
    async fn approval_rejects_other_tokens_and_leaves_transfers_unchanged() -> anyhow::Result<()> {
        let db = BlokliDb::new_in_memory().await?;
        let rpc = ClonableMockOperations {
            inner: Arc::new(MockIndexerRpcOperations::new()),
        };
        let (handlers, _, mut receiver) = init_handlers_with_events(rpc, db);
        for token in [*XHOPR_TOKEN_ADDR, Address::from([0x42; 20])] {
            let result = handlers
                .collect_log_event(event_to_log(approval(U256::MAX), token), true)
                .await;
            assert!(matches!(result, Err(CoreEthereumIndexerError::UnknownContract(address)) if address == token));
            assert!(receiver.try_recv().is_err());
        }
        let transfer = Transfer {
            from: AlloyAddress::from_hopr_address(*SAFE_INSTANCE_ADDR),
            to: AlloyAddress::from_hopr_address(*CHANNELS_ADDR),
            value: U256::MAX,
        };
        handlers
            .collect_log_event(event_to_log(transfer, *TOKEN_ADDR), true)
            .await?;
        assert!(receiver.try_recv().is_err());
        Ok(())
    }
}
