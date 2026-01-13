use blokli_chain_rpc::HoprIndexerRpcOperations;
use blokli_chain_types::AlloyAddressExt;
use blokli_db::{BlokliDbAllOperations, OpenTransaction};
use hopr_bindings::hopr_token::HoprToken::HoprTokenEvents;
use hopr_primitive_types::prelude::Address;
use tracing::{debug, trace};

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
    pub(super) async fn on_token_event(
        &self,
        _tx: &OpenTransaction,
        event: HoprTokenEvents,
        _is_synced: bool,
    ) -> Result<Vec<IndexerEvent>> {
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

        Ok(vec![])
    }
}
