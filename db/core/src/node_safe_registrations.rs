use async_trait::async_trait;
use blokli_db_entity::{hopr_node_safe_registration, prelude::HoprNodeSafeRegistration};
use hopr_primitive_types::prelude::Address;
use sea_orm::{ColumnTrait, DbErr, EntityTrait, ModelTrait, QueryFilter, Set};
use sea_query::OnConflict;

use crate::{BlokliDb, BlokliDbGeneralModelOperations, DbSqlError, OptTx, Result};

#[async_trait]
pub trait BlokliDbNodeSafeRegistrationOperations: BlokliDbGeneralModelOperations {
    /// Register a node to a safe
    ///
    /// Creates a node-safe registration entry. If a registration with the same event coordinates
    /// (registered_block, registered_tx_index, registered_log_index) already exists, returns the
    /// existing registration ID without modification.
    ///
    /// # Arguments
    /// * `safe_address` - Safe contract address
    /// * `node_address` - Node address to register
    /// * `block` - Registration event block number
    /// * `tx_index` - Registration event transaction index
    /// * `log_index` - Registration event log index
    ///
    /// # Idempotency
    /// Uses ON CONFLICT DO NOTHING on event coordinates for safe event replay.
    #[allow(clippy::too_many_arguments)]
    async fn register_node_to_safe<'a>(
        &'a self,
        tx: OptTx<'a>,
        safe_address: Address,
        node_address: Address,
        block: u64,
        tx_index: u64,
        log_index: u64,
    ) -> Result<i64>;

    /// Deregister a node from a safe
    ///
    /// Removes the node-safe registration entry. Does not delete the safe itself.
    ///
    /// # Arguments
    /// * `safe_address` - Safe contract address
    /// * `node_address` - Node address to deregister
    ///
    /// # Returns
    /// * `Ok(())` - Registration was deleted successfully
    /// * `Err(_)` - Registration does not exist or deletion failed
    async fn deregister_node_from_safe<'a>(
        &'a self,
        tx: OptTx<'a>,
        safe_address: Address,
        node_address: Address,
    ) -> Result<()>;

    /// Get all registered nodes for a safe
    ///
    /// Returns list of node addresses that have registered with the given safe.
    ///
    /// # Arguments
    /// * `safe_address` - Safe contract address
    ///
    /// # Returns
    /// * `Vec<Address>` - List of registered node addresses (may be empty)
    async fn get_registered_nodes_for_safe<'a>(&'a self, tx: OptTx<'a>, safe_address: Address) -> Result<Vec<Address>>;

    /// Get safe address for a registered node
    ///
    /// Finds the safe that a given node is registered to.
    ///
    /// # Arguments
    /// * `node_address` - Node address to query
    ///
    /// # Returns
    /// * `Ok(Some(Address))` - Node is registered to this safe
    /// * `Ok(None)` - Node is not registered to any safe
    async fn get_safe_for_registered_node<'a>(
        &'a self,
        tx: OptTx<'a>,
        node_address: Address,
    ) -> Result<Option<Address>>;
}

#[async_trait]
impl BlokliDbNodeSafeRegistrationOperations for BlokliDb {
    /// Registers a node to a safe by creating or updating a registration entry.
    ///
    /// Uses event coordinates for idempotency. If the same
    /// event is replayed with identical coordinates, no error occurs and the existing
    /// registration ID is returned.
    ///
    /// # Returns
    ///
    /// The database id of the existing or newly inserted registration.
    ///
    /// # Examples
    ///
    /// ```ignore
    /// # async fn example(db: &BlokliDb, safe: Address, node: Address) -> Result<(), Box<dyn std::error::Error>> {
    /// let id = db.register_node_to_safe(None, safe, node, 100, 0, 0).await?;
    /// println!("registration id: {}", id);
    /// # Ok(()) }
    /// ```
    #[allow(clippy::too_many_arguments)]
    #[allow(clippy::cast_possible_wrap)]
    async fn register_node_to_safe<'a>(
        &'a self,
        tx: OptTx<'a>,
        safe_address: Address,
        node_address: Address,
        block: u64,
        tx_index: u64,
        log_index: u64,
    ) -> Result<i64> {
        let tx = self.nest_transaction(tx).await?;

        let registration_model = hopr_node_safe_registration::ActiveModel {
            safe_address: Set(safe_address.as_ref().to_vec()),
            node_address: Set(node_address.as_ref().to_vec()),
            registered_block: Set(block as i64),
            registered_tx_index: Set(tx_index as i64),
            registered_log_index: Set(log_index as i64),
            ..Default::default()
        };

        match HoprNodeSafeRegistration::insert(registration_model)
            .on_conflict(
                OnConflict::columns([
                    hopr_node_safe_registration::Column::RegisteredBlock,
                    hopr_node_safe_registration::Column::RegisteredTxIndex,
                    hopr_node_safe_registration::Column::RegisteredLogIndex,
                ])
                .do_nothing()
                .to_owned(),
            )
            .exec(tx.as_ref())
            .await
        {
            Ok(_) | Err(DbErr::RecordNotInserted) => {
                // Success or already exists due to ON CONFLICT DO NOTHING
            }
            Err(e) => return Err(e.into()),
        }

        // Retrieve the ID (whether newly inserted or existing)
        let registration = HoprNodeSafeRegistration::find()
            .filter(hopr_node_safe_registration::Column::RegisteredBlock.eq(block as i64))
            .filter(hopr_node_safe_registration::Column::RegisteredTxIndex.eq(tx_index as i64))
            .filter(hopr_node_safe_registration::Column::RegisteredLogIndex.eq(log_index as i64))
            .one(tx.as_ref())
            .await?
            .ok_or_else(|| {
                DbSqlError::EntityNotFound(format!(
                    "Node safe registration not found after insert at block {} tx {} log {}",
                    block, tx_index, log_index
                ))
            })?;

        tx.commit().await?;
        Ok(registration.id)
    }

    /// Deletes a node-safe registration entry from the database.
    ///
    /// Used when a node is deregistered from a safe via DeregisteredNodeSafe event.
    /// Does not delete the safe itself.
    ///
    /// # Returns
    ///
    /// `Ok(())` if the registration was successfully deleted, or `DbSqlError::EntityNotFound` if no registration is
    /// found.
    ///
    /// # Examples
    ///
    /// ```ignore
    /// # use crate::db::BlokliDb;
    /// # use crate::types::Address;
    /// # async fn example(db: &BlokliDb, safe: Address, node: Address) -> Result<(), crate::db::DbSqlError> {
    /// db.deregister_node_from_safe(None, safe, node).await?;
    /// # Ok(())
    /// # }
    /// ```
    async fn deregister_node_from_safe<'a>(
        &'a self,
        tx: OptTx<'a>,
        safe_address: Address,
        node_address: Address,
    ) -> Result<()> {
        let tx = self.nest_transaction(tx).await?;

        // Find the registration first to ensure it exists
        let registration = HoprNodeSafeRegistration::find()
            .filter(hopr_node_safe_registration::Column::SafeAddress.eq(safe_address.as_ref().to_vec()))
            .filter(hopr_node_safe_registration::Column::NodeAddress.eq(node_address.as_ref().to_vec()))
            .one(tx.as_ref())
            .await?
            .ok_or_else(|| {
                DbSqlError::EntityNotFound(format!(
                    "Node safe registration not found: safe={}, node={}",
                    safe_address, node_address
                ))
            })?;

        // Delete the registration entry
        registration.delete(tx.as_ref()).await?;

        tx.commit().await?;
        Ok(())
    }

    /// Retrieves all node addresses registered to a specific safe.
    ///
    /// Returns a list of addresses of nodes that have registered with the given safe via
    /// RegisteredNodeSafe events. The list may be empty if no nodes are registered.
    ///
    /// # Returns
    ///
    /// `Vec<Address>` containing all registered node addresses.
    ///
    /// # Examples
    ///
    /// ```ignore
    /// # use crate::db::BlokliDb;
    /// # use crate::types::Address;
    /// # async fn example(db: &BlokliDb, safe: Address) -> Result<(), crate::db::DbSqlError> {
    /// let nodes = db.get_registered_nodes_for_safe(None, safe).await?;
    /// println!("Found {} registered nodes", nodes.len());
    /// # Ok(())
    /// # }
    /// ```
    async fn get_registered_nodes_for_safe<'a>(&'a self, tx: OptTx<'a>, safe_address: Address) -> Result<Vec<Address>> {
        let query = HoprNodeSafeRegistration::find()
            .filter(hopr_node_safe_registration::Column::SafeAddress.eq(safe_address.as_ref().to_vec()));

        let registrations = if let Some(t) = tx {
            query.all(t.as_ref()).await?
        } else {
            query.all(self.conn(crate::TargetDb::Index)).await?
        };

        let nodes: Vec<Address> = registrations
            .into_iter()
            .filter_map(|reg| Address::try_from(reg.node_address.as_slice()).ok())
            .collect();

        Ok(nodes)
    }

    /// Retrieves the safe address that a node is registered to.
    ///
    /// Looks up the registration by node address (which has a unique constraint) and returns
    /// the associated safe address if found.
    ///
    /// # Returns
    ///
    /// `Ok(Some(Address))` if the node is registered to a safe, `Ok(None)` if not registered.
    ///
    /// # Examples
    ///
    /// ```ignore
    /// # use crate::db::BlokliDb;
    /// # use crate::types::Address;
    /// # async fn example(db: &BlokliDb, node: Address) -> Result<(), crate::db::DbSqlError> {
    /// if let Some(safe) = db.get_safe_for_registered_node(None, node).await? {
    ///     println!("Node is registered to safe: {}", safe);
    /// }
    /// # Ok(())
    /// # }
    /// ```
    async fn get_safe_for_registered_node<'a>(
        &'a self,
        tx: OptTx<'a>,
        node_address: Address,
    ) -> Result<Option<Address>> {
        let query = HoprNodeSafeRegistration::find()
            .filter(hopr_node_safe_registration::Column::NodeAddress.eq(node_address.as_ref().to_vec()));

        let registration = if let Some(t) = tx {
            query.one(t.as_ref()).await?
        } else {
            query.one(self.conn(crate::TargetDb::Index)).await?
        };

        Ok(registration.and_then(|reg| Address::try_from(reg.safe_address.as_slice()).ok()))
    }
}

#[cfg(test)]
mod tests {
    use hopr_crypto_random::random_bytes;
    use sea_orm::PaginatorTrait;

    use super::*;
    use crate::db::BlokliDb;

    /// Generates a new random `Address`.
    fn random_address() -> Address {
        Address::from(random_bytes())
    }

    #[tokio::test]
    async fn test_register_node_to_safe() -> anyhow::Result<()> {
        let db = BlokliDb::new_in_memory().await?;

        let safe_address = random_address();
        let node_address = random_address();

        // Register node to safe
        let id = db
            .register_node_to_safe(None, safe_address, node_address, 100, 0, 0)
            .await?;

        // Verify it was created
        let registration = HoprNodeSafeRegistration::find_by_id(id)
            .one(db.conn(crate::TargetDb::Index))
            .await?
            .expect("registration should exist");

        assert_eq!(registration.safe_address, safe_address.as_ref().to_vec());
        assert_eq!(registration.node_address, node_address.as_ref().to_vec());
        assert_eq!(registration.registered_block, 100);

        Ok(())
    }

    #[tokio::test]
    async fn test_register_node_idempotency() -> anyhow::Result<()> {
        let db = BlokliDb::new_in_memory().await?;

        let safe_address = random_address();
        let node_address = random_address();

        // Register node
        let id1 = db
            .register_node_to_safe(None, safe_address, node_address, 100, 0, 0)
            .await?;

        // Try to register same node with same event coordinates
        let id2 = db
            .register_node_to_safe(None, safe_address, node_address, 100, 0, 0)
            .await?;

        // Should return same ID
        assert_eq!(id1, id2);

        // Verify only one record exists
        let count = HoprNodeSafeRegistration::find()
            .count(db.conn(crate::TargetDb::Index))
            .await?;
        assert_eq!(count, 1);

        Ok(())
    }

    #[tokio::test]
    async fn test_deregister_node_from_safe() -> anyhow::Result<()> {
        let db = BlokliDb::new_in_memory().await?;

        let safe_address = random_address();
        let node_address = random_address();

        // Case 1: Deregister non-existent registration
        let result = db.deregister_node_from_safe(None, safe_address, node_address).await;
        assert!(matches!(result, Err(DbSqlError::EntityNotFound(_))));

        // Register node
        let id = db
            .register_node_to_safe(None, safe_address, node_address, 100, 0, 0)
            .await?;

        // Verify registration exists
        let registration = HoprNodeSafeRegistration::find_by_id(id)
            .one(db.conn(crate::TargetDb::Index))
            .await?;
        assert!(registration.is_some());

        // Deregister the node
        db.deregister_node_from_safe(None, safe_address, node_address).await?;

        // Verify registration is deleted
        let registration = HoprNodeSafeRegistration::find_by_id(id)
            .one(db.conn(crate::TargetDb::Index))
            .await?;
        assert!(registration.is_none());

        Ok(())
    }

    #[tokio::test]
    async fn test_get_registered_nodes_for_safe() -> anyhow::Result<()> {
        let db = BlokliDb::new_in_memory().await?;

        let safe_address = random_address();
        let node1 = random_address();
        let node2 = random_address();
        let node3 = random_address();

        // Initially no nodes registered
        let nodes = db.get_registered_nodes_for_safe(None, safe_address).await?;
        assert_eq!(nodes.len(), 0);

        // Register three nodes to the same safe
        db.register_node_to_safe(None, safe_address, node1, 100, 0, 0).await?;
        db.register_node_to_safe(None, safe_address, node2, 100, 1, 0).await?;
        db.register_node_to_safe(None, safe_address, node3, 100, 2, 0).await?;

        // Get all registered nodes
        let nodes = db.get_registered_nodes_for_safe(None, safe_address).await?;
        assert_eq!(nodes.len(), 3);
        assert!(nodes.contains(&node1));
        assert!(nodes.contains(&node2));
        assert!(nodes.contains(&node3));

        Ok(())
    }

    #[tokio::test]
    async fn test_get_safe_for_registered_node() -> anyhow::Result<()> {
        let db = BlokliDb::new_in_memory().await?;

        let safe_address = random_address();
        let node_address = random_address();

        // Case 1: Node not registered
        let result = db.get_safe_for_registered_node(None, node_address).await?;
        assert!(result.is_none());

        // Register node to safe
        db.register_node_to_safe(None, safe_address, node_address, 100, 0, 0)
            .await?;

        // Case 2: Node is registered
        let result = db.get_safe_for_registered_node(None, node_address).await?;
        assert_eq!(result, Some(safe_address));

        Ok(())
    }

    #[tokio::test]
    async fn test_node_unique_constraint() -> anyhow::Result<()> {
        let db = BlokliDb::new_in_memory().await?;

        let safe1 = random_address();
        let safe2 = random_address();
        let node = random_address();

        // Register node to safe1
        db.register_node_to_safe(None, safe1, node, 100, 0, 0).await?;

        // Try to register same node to safe2 (should fail due to unique constraint on node_address)
        let result = db.register_node_to_safe(None, safe2, node, 100, 1, 0).await;
        assert!(result.is_err());

        Ok(())
    }
}
