use blokli_chain_rpc::HoprIndexerRpcOperations;
use blokli_chain_types::AlloyAddressExt;
use blokli_db::{BlokliDbAllOperations, OpenTransaction};
use hopr_bindings::hopr_token::HoprToken::HoprTokenEvents;
use hopr_primitive_types::prelude::Address;
use tracing::{debug, trace};

use super::ContractEventHandlers;
use crate::errors::Result;

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
    pub(super) async fn on_token_event(
        &self,
        _tx: &OpenTransaction,
        event: HoprTokenEvents,
        _is_synced: bool,
    ) -> Result<()> {
        #[cfg(all(feature = "prometheus", not(test)))]
        METRIC_INDEXER_LOG_COUNTERS.increment(&["token"]);

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

        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use std::sync::Arc;

    use alloy::{
        primitives::U256,
        sol_types::{SolEvent, SolValue},
    };
    use blokli_db::{BlokliDbGeneralModelOperations, db::BlokliDb, info::BlokliDbInfoOperations};
    use hopr_primitive_types::prelude::{Address, HoprBalance, SerializableLog};
    use primitive_types::H256;

    use crate::handlers::test_utils::test_helpers::*;

    #[test_log::test(tokio::test)]
    #[ignore = "TODO: Fix mock expectations - handler doesn't call RPC methods"]
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
                hopr_bindings::hopr_token::HoprToken::Transfer::SIGNATURE_HASH.into(),
                H256::from_slice(&Address::default().to_bytes32()).into(),
                H256::from_slice(&STAKE_ADDRESS.to_bytes32()).into(),
            ],
            data: encoded_data,
            ..test_log()
        };

        db.begin_transaction()
            .await?
            .perform(|tx| Box::pin(async move { handlers.process_log_event(tx, transferred_log, true).await }))
            .await?;

        // Event not published to subscribers (no assertion needed)

        assert_eq!(db.get_safe_hopr_balance(None).await?, target_hopr_balance);

        Ok(())
    }

    #[test_log::test(tokio::test)]
    #[ignore = "TODO: Fix mock expectations - handler doesn't call RPC methods"]
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
                hopr_bindings::hopr_token::HoprToken::Transfer::SIGNATURE_HASH.into(),
                H256::from_slice(&STAKE_ADDRESS.to_bytes32()).into(),
                H256::from_slice(&Address::default().to_bytes32()).into(),
            ],
            data: encoded_data,
            ..test_log()
        };

        db.begin_transaction()
            .await?
            .perform(|tx| Box::pin(async move { handlers.process_log_event(tx, transferred_log, true).await }))
            .await?;

        // Event not published to subscribers (no assertion needed)

        assert_eq!(db.get_safe_hopr_balance(None).await?, HoprBalance::zero());

        Ok(())
    }

    #[tokio::test]
    #[ignore = "TODO: Fix mock expectations - handler doesn't call RPC methods"]
    async fn test_on_token_approval_correct() -> anyhow::Result<()> {
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
                hopr_bindings::hopr_token::HoprToken::Approval::SIGNATURE_HASH.into(),
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
        db.begin_transaction()
            .await?
            .perform(|tx| Box::pin(async move { handlers_clone.process_log_event(tx, approval_log_clone, true).await }))
            .await?;

        // Event not published to subscribers (no assertion needed)

        // after processing the allowance should be 0
        assert_eq!(db.get_safe_hopr_allowance(None).await?, target_allowance);

        // reduce allowance manually to verify a second time
        let _ = db
            .set_safe_hopr_allowance(None, HoprBalance::from(primitive_types::U256::from(10u64)))
            .await;
        assert_eq!(
            db.get_safe_hopr_allowance(None).await?,
            HoprBalance::from(primitive_types::U256::from(10u64))
        );

        let handlers_clone = handlers.clone();
        db.begin_transaction()
            .await?
            .perform(|tx| Box::pin(async move { handlers_clone.process_log_event(tx, approval_log, true).await }))
            .await?;

        // after processing the allowance should be target_allowance
        assert_eq!(db.get_safe_hopr_allowance(None).await?, target_allowance);

        Ok(())
    }
}
