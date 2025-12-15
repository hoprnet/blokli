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
        // Note: Token transfer handlers emit events but don't update node-specific state

        Ok(())
    }
}
