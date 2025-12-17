use async_trait::async_trait;
use blokli_db_entity::{codegen::prelude::*, hopr_safe_contract};
use hopr_primitive_types::prelude::*;
use sea_orm::{ColumnTrait, EntityTrait, ModelTrait, QueryFilter, Set};
use sea_query::OnConflict;

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
    /// Uses ON CONFLICT DO NOTHING with unique constraint on (deployed_block, deployed_tx_index, deployed_log_index)
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

    /// Delete a safe contract entry
    ///
    /// Used by DeregisteredNodeSafe handler to remove safe when deregistered
    ///
    /// # Arguments
    /// * `safe_address` - Safe contract address from event
    ///
    /// # Returns
    /// * `Ok(())` - Safe was deleted successfully
    /// * `Err(_)` - Safe does not exist or deletion failed
    async fn delete_safe_contract<'a>(&'a self, tx: OptTx<'a>, safe_address: Address) -> Result<()>;
}

#[async_trait]
impl BlokliDbSafeContractOperations for BlokliDb {
    /// Creates a safe contract record for a deployment, inserting a new row or returning an existing one when the
    /// deployment indices match.
    ///
    /// If a record with the same deployment (deployed block, transaction index, and log index) already exists, the
    /// existing record's id is returned and no new row is inserted. The operation uses the provided optional
    /// transaction; a nested transaction will be created and committed on success when needed.
    ///
    /// # Returns
    ///
    /// The database id of the existing or newly inserted safe contract.
    ///
    /// # Examples
    ///
    /// ```ignore
    /// # async fn example(db: &BlokliDb, safe_addr: Address, module_addr: Address, chain_key: Address) -> Result<(), Box<dyn std::error::Error>> {
    /// let id = db.create_safe_contract(None, safe_addr, module_addr, chain_key, 100, 0, 0).await?;
    /// println!("safe id: {}", id);
    /// # Ok(()) }
    /// ```
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

        // Insert with ON CONFLICT DO NOTHING for idempotency
        // The unique constraint is on (deployed_block, deployed_tx_index, deployed_log_index).
        // We ignore the result because ON CONFLICT DO NOTHING may not insert anything.
        let _ = HoprSafeContract::insert(safe_model)
            .on_conflict(
                OnConflict::columns([
                    hopr_safe_contract::Column::DeployedBlock,
                    hopr_safe_contract::Column::DeployedTxIndex,
                    hopr_safe_contract::Column::DeployedLogIndex,
                ])
                .do_nothing()
                .to_owned(),
            )
            .exec(tx.as_ref())
            .await;

        // Retrieve the ID (whether newly inserted or existing)
        let safe = HoprSafeContract::find()
            .filter(hopr_safe_contract::Column::DeployedBlock.eq(block as i64))
            .filter(hopr_safe_contract::Column::DeployedTxIndex.eq(tx_index as i64))
            .filter(hopr_safe_contract::Column::DeployedLogIndex.eq(log_index as i64))
            .one(tx.as_ref())
            .await?
            .ok_or_else(|| {
                DbSqlError::EntityNotFound(format!(
                    "Safe contract not found after insert at block {} tx {} log {}",
                    block, tx_index, log_index
                ))
            })?;

        tx.commit().await?;
        Ok(safe.id)
    }

    /// Checks that a safe contract exists at `safe_address` and that its stored chain key equals `expected_chain_key`.
    ///
    /// # Returns
    ///
    /// `true` if the safe exists and the stored chain key equals `expected_chain_key`, `false` if the safe exists but
    /// the chain key differs. Returns `DbSqlError::EntityNotFound` if no safe contract is found.
    ///
    /// # Examples
    ///
    /// ```ignore
    /// # use crate::db::BlokliDb;
    /// # use crate::types::Address;
    /// # async fn example(db: &BlokliDb, addr: Address, key: Address) -> Result<(), crate::db::DbSqlError> {
    /// let matches = db.verify_safe_contract(None, addr, key).await?;
    /// # Ok(())
    /// # }
    /// ```
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

    /// Deletes a safe contract entry from the database.
    ///
    /// Used when a node is deregistered from a safe via DeregisteredNodeSafe event.
    ///
    /// # Returns
    ///
    /// `Ok(())` if the safe was successfully deleted, or `DbSqlError::EntityNotFound` if no safe contract is found.
    ///
    /// # Examples
    ///
    /// ```ignore
    /// # use crate::db::BlokliDb;
    /// # use crate::types::Address;
    /// # async fn example(db: &BlokliDb, addr: Address) -> Result<(), crate::db::DbSqlError> {
    /// db.delete_safe_contract(None, addr).await?;
    /// # Ok(())
    /// # }
    /// ```
    async fn delete_safe_contract<'a>(&'a self, tx: OptTx<'a>, safe_address: Address) -> Result<()> {
        let tx = self.nest_transaction(tx).await?;

        // Find the safe first to ensure it exists
        let safe = HoprSafeContract::find()
            .filter(hopr_safe_contract::Column::Address.eq(safe_address.as_ref().to_vec()))
            .one(tx.as_ref())
            .await?
            .ok_or_else(|| DbSqlError::EntityNotFound(format!("Safe contract not found: {}", safe_address)))?;

        // Delete the safe entry
        safe.delete(tx.as_ref()).await?;

        tx.commit().await?;
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use sea_orm::PaginatorTrait;

    use super::*;
    use crate::db::BlokliDb;

    // Helper to generate random address
    /// Generates a new random `Address` from cryptographically secure random bytes.
    ///
    /// # Examples
    ///
    /// ```
    /// let _addr: Address = random_address();
    /// ```
    fn random_address() -> Address {
        Address::from(hopr_crypto_random::random_bytes())
    }

    #[tokio::test]
    async fn test_create_safe_contract() -> anyhow::Result<()> {
        let db = BlokliDb::new_in_memory().await?;

        let safe_address = random_address();
        let module_address = random_address();
        let chain_key = random_address();

        // Create safe contract
        let id = db
            .create_safe_contract(None, safe_address, module_address, chain_key, 100, 0, 0)
            .await?;

        // Verify it was created
        let safe = HoprSafeContract::find_by_id(id)
            .one(db.conn(crate::TargetDb::Index))
            .await?
            .expect("safe should exist");

        assert_eq!(safe.address, safe_address.as_ref().to_vec());
        assert_eq!(safe.module_address, module_address.as_ref().to_vec());
        assert_eq!(safe.chain_key, chain_key.as_ref().to_vec());
        assert_eq!(safe.deployed_block, 100);

        Ok(())
    }

    #[tokio::test]
    async fn test_create_safe_contract_idempotency() -> anyhow::Result<()> {
        let db = BlokliDb::new_in_memory().await?;

        let safe_address = random_address();
        let module_address = random_address();
        let chain_key = random_address();

        // Create safe contract
        let id1 = db
            .create_safe_contract(None, safe_address, module_address, chain_key, 100, 0, 0)
            .await?;

        // Try to create same safe contract (same block/tx/log)
        let id2 = db
            .create_safe_contract(None, safe_address, module_address, chain_key, 100, 0, 0)
            .await?;

        // Should return same ID
        assert_eq!(id1, id2);

        // Verify only one record exists
        let count = HoprSafeContract::find().count(db.conn(crate::TargetDb::Index)).await?;
        assert_eq!(count, 1);

        Ok(())
    }

    #[tokio::test]
    async fn test_verify_safe_contract() -> anyhow::Result<()> {
        let db = BlokliDb::new_in_memory().await?;

        let safe_address = random_address();
        let module_address = random_address();
        let chain_key = random_address();
        let other_chain_key = random_address();

        // Case 1: Safe doesn't exist
        let result = db.verify_safe_contract(None, safe_address, chain_key).await;
        assert!(matches!(result, Err(DbSqlError::EntityNotFound(_))));

        // Create safe contract
        db.create_safe_contract(None, safe_address, module_address, chain_key, 100, 0, 0)
            .await?;

        // Case 2: Safe exists and chain_key matches
        let result = db.verify_safe_contract(None, safe_address, chain_key).await?;
        assert!(result);

        // Case 3: Safe exists but chain_key mismatch
        let result = db.verify_safe_contract(None, safe_address, other_chain_key).await?;
        assert!(!result);

        Ok(())
    }

    #[tokio::test]
    async fn test_delete_safe_contract() -> anyhow::Result<()> {
        let db = BlokliDb::new_in_memory().await?;

        let safe_address = random_address();
        let module_address = random_address();
        let chain_key = random_address();

        // Case 1: Safe doesn't exist
        let result = db.delete_safe_contract(None, safe_address).await;
        assert!(matches!(result, Err(DbSqlError::EntityNotFound(_))));

        // Create safe contract
        let id = db
            .create_safe_contract(None, safe_address, module_address, chain_key, 100, 0, 0)
            .await?;

        // Verify safe exists
        let safe = HoprSafeContract::find_by_id(id)
            .one(db.conn(crate::TargetDb::Index))
            .await?;
        assert!(safe.is_some());

        // Delete the safe
        db.delete_safe_contract(None, safe_address).await?;

        // Verify safe is deleted
        let safe = HoprSafeContract::find_by_id(id)
            .one(db.conn(crate::TargetDb::Index))
            .await?;
        assert!(safe.is_none());

        Ok(())
    }
}
