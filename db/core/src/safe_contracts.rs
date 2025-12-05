use async_trait::async_trait;
use blokli_db_entity::{codegen::prelude::*, hopr_safe_contract};
use hopr_primitive_types::prelude::*;
use sea_orm::*;

use crate::{BlokliDb, BlokliDbGeneralModelOperations, DbSqlError, OptTx, Result};

#[async_trait]
pub trait BlokliDbSafeContractOperations: BlokliDbGeneralModelOperations {
    /// Create safe contract entry from deployment event
    ///
    /// # Arguments
    /// * `safe_address` - Safe contract address from event
    /// * `module_address` - Module contract address from event
    /// * `chain_key` - Transaction sender (owner address)
    /// * `block` - Deployment block number
    /// * `tx_index` - Transaction index
    /// * `log_index` - Log index
    ///
    /// # Idempotency
    /// Uses check-then-insert logic with unique constraint on (deployed_block, deployed_tx_index, deployed_log_index)
    #[allow(clippy::too_many_arguments)]
    async fn create_safe_contract<'a>(
        &'a self,
        tx: OptTx<'a>,
        safe_address: Address,
        module_address: Address,
        chain_key: Address,
        block: u64,
        tx_index: u64,
        log_index: u64,
    ) -> Result<i64>;

    /// Verify safe contract exists and chain_key matches expected value
    ///
    /// Used by RegisteredNodeSafe handler for verification only
    ///
    /// # Arguments
    /// * `safe_address` - Safe contract address from event
    /// * `expected_chain_key` - Expected chain_key (from event.nodeAddress)
    ///
    /// # Returns
    /// * `Ok(true)` - Safe exists and chain_key matches
    /// * `Ok(false)` - Safe exists but chain_key does NOT match
    /// * `Err(_)` - Safe does not exist or query failed
    async fn verify_safe_contract<'a>(
        &'a self,
        tx: OptTx<'a>,
        safe_address: Address,
        expected_chain_key: Address,
    ) -> Result<bool>;
}

#[async_trait]
impl BlokliDbSafeContractOperations for BlokliDb {
    #[allow(clippy::too_many_arguments)]
    #[allow(clippy::cast_possible_wrap)]
    async fn create_safe_contract<'a>(
        &'a self,
        tx: OptTx<'a>,
        safe_address: Address,
        module_address: Address,
        chain_key: Address,
        block: u64,
        tx_index: u64,
        log_index: u64,
    ) -> Result<i64> {
        let tx = self.nest_transaction(tx).await?;

        let safe_model = hopr_safe_contract::ActiveModel {
            address: Set(safe_address.as_ref().to_vec()),
            module_address: Set(module_address.as_ref().to_vec()),
            chain_key: Set(chain_key.as_ref().to_vec()),
            deployed_block: Set(block as i64),
            deployed_tx_index: Set(tx_index as i64),
            deployed_log_index: Set(log_index as i64),
            ..Default::default()
        };

        // Check existence for idempotency
        // The unique constraint is on (deployed_block, deployed_tx_index, deployed_log_index).
        let existing = HoprSafeContract::find()
            .filter(hopr_safe_contract::Column::DeployedBlock.eq(block as i64))
            .filter(hopr_safe_contract::Column::DeployedTxIndex.eq(tx_index as i64))
            .filter(hopr_safe_contract::Column::DeployedLogIndex.eq(log_index as i64))
            .one(tx.as_ref())
            .await?;

        if let Some(existing) = existing {
            return Ok(existing.id);
        }

        let res = HoprSafeContract::insert(safe_model)
            .exec(tx.as_ref())
            .await
            .map_err(DbSqlError::from)?;

        Ok(res.last_insert_id)
    }

    async fn verify_safe_contract<'a>(
        &'a self,
        tx: OptTx<'a>,
        safe_address: Address,
        expected_chain_key: Address,
    ) -> Result<bool> {
        let query =
            HoprSafeContract::find().filter(hopr_safe_contract::Column::Address.eq(safe_address.as_ref().to_vec()));

        let safe = if let Some(t) = tx {
            query.one(t.as_ref()).await?
        } else {
            query.one(self.conn(crate::TargetDb::Index)).await?
        };

        match safe {
            Some(safe) => {
                let chain_key_match = safe.chain_key == expected_chain_key.as_ref().to_vec();
                Ok(chain_key_match)
            }
            None => Err(DbSqlError::EntityNotFound(format!(
                "Safe contract not found: {}",
                safe_address
            ))),
        }
    }
}
