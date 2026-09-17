use async_trait::async_trait;
use blokli_db_entity::{
    conversions::node_safe_registration::fetch_registered_nodes_for_safe, hopr_node_safe_registration,
    prelude::HoprNodeSafeRegistration,
};
use hopr_types::primitive::prelude::Address;
use sea_orm::{
    ActiveModelTrait, ColumnTrait, ConnectionTrait, DbErr, EntityTrait, IntoActiveModel, ModelTrait, QueryFilter, Set,
};
use sea_query::OnConflict;
use tracing::trace;

use crate::{BlokliDb, BlokliDbGeneralModelOperations, DbSqlError, OptTx, Result, numeric::log_position_to_i64};

#[async_trait]
pub trait BlokliDbNodeSafeRegistrationOperations: BlokliDbGeneralModelOperations {
    /// Register a node to a safe
    ///
    /// Creates a node-safe registration entry, or moves an existing one to a new safe. A node can
    /// be registered to at most one safe at a time, which the schema enforces with a unique key on
    /// `node_address`.
    ///
    /// # Arguments
    /// * `safe_address` - Safe contract address
    /// * `node_address` - Node address to register
    /// * `block` - Registration event block number
    /// * `tx_index` - Registration event transaction index
    /// * `log_index` - Registration event log index
    ///
    /// # Idempotency
    /// The event coordinates (`registered_block`, `registered_tx_index`, `registered_log_index`)
    /// decide which registration a node ends up with, so the same set of events yields the same
    /// result whatever order or number of times they are applied: a registration newer than the
    /// stored one replaces it, and one that is equal or older is ignored.
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
    /// Event coordinates decide the outcome, so replaying events is safe in any order: a
    /// registration newer than the stored one replaces it, and one that is equal or older is
    /// ignored.
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
        let (registered_block, registered_tx_index, registered_log_index) =
            log_position_to_i64(block, tx_index, log_index)?;

        // A node holds at most one registration, so the row is keyed by node address and the event
        // coordinates decide which registration it carries. Keying the write on the coordinates
        // instead would make a replayed registration insert a second row for the node, which the
        // unique key on `node_address` rejects, aborting the whole block's transaction.
        let existing = HoprNodeSafeRegistration::find()
            .filter(hopr_node_safe_registration::Column::NodeAddress.eq(node_address.as_ref().to_vec()))
            .one(tx.as_ref())
            .await?;

        let incoming_position = (registered_block, registered_tx_index, registered_log_index);

        let registration_id = match existing {
            Some(existing) => {
                apply_to_existing_registration(tx.as_ref(), existing, safe_address, node_address, incoming_position)
                    .await?
            }
            None => {
                let registration_model = hopr_node_safe_registration::ActiveModel {
                    safe_address: Set(safe_address.as_ref().to_vec()),
                    node_address: Set(node_address.as_ref().to_vec()),
                    registered_block: Set(registered_block),
                    registered_tx_index: Set(registered_tx_index),
                    registered_log_index: Set(registered_log_index),
                    ..Default::default()
                };

                match HoprNodeSafeRegistration::insert(registration_model)
                    .on_conflict(
                        OnConflict::column(hopr_node_safe_registration::Column::NodeAddress)
                            .do_nothing()
                            .to_owned(),
                    )
                    .exec(tx.as_ref())
                    .await
                {
                    Ok(insert_result) => insert_result.last_insert_id,
                    // A registration for this node was inserted between the lookup and the insert.
                    // The coordinates still decide the outcome, so run the same comparison against
                    // the row that won the race rather than letting timing pick the safe.
                    Err(DbErr::RecordNotInserted) => {
                        let existing = HoprNodeSafeRegistration::find()
                            .filter(hopr_node_safe_registration::Column::NodeAddress.eq(node_address.as_ref().to_vec()))
                            .one(tx.as_ref())
                            .await?
                            .ok_or_else(|| {
                                DbSqlError::EntityNotFound(format!(
                                    "Node safe registration not found after insert at block {} tx {} log {}",
                                    block, tx_index, log_index
                                ))
                            })?;

                        apply_to_existing_registration(
                            tx.as_ref(),
                            existing,
                            safe_address,
                            node_address,
                            incoming_position,
                        )
                        .await?
                    }
                    Err(e) => return Err(e.into()),
                }
            }
        };

        tx.commit().await?;
        Ok(registration_id)
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
        if let Some(t) = tx {
            Ok(fetch_registered_nodes_for_safe(t.as_ref(), safe_address.as_ref()).await?)
        } else {
            Ok(fetch_registered_nodes_for_safe(self.conn(crate::TargetDb::Index), safe_address.as_ref()).await?)
        }
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

/// Moves an existing node-safe registration to `safe_address` when the incoming event supersedes
/// the one the row carries, and leaves it untouched otherwise.
///
/// Returns the row's id either way.
async fn apply_to_existing_registration<C: ConnectionTrait>(
    conn: &C,
    existing: hopr_node_safe_registration::Model,
    safe_address: Address,
    node_address: Address,
    incoming_position: (i64, i64, i64),
) -> Result<i64> {
    let stored_position = (
        existing.registered_block,
        existing.registered_tx_index,
        existing.registered_log_index,
    );

    if incoming_position <= stored_position {
        // A replay of the stored registration, or of one it has already superseded.
        trace!(
            node_address = %node_address,
            stored_safe_address = %hex::encode(&existing.safe_address),
            position = ?incoming_position,
            "ignoring node-safe registration that does not supersede the stored one"
        );
        return Ok(existing.id);
    }

    let registration_id = existing.id;
    let mut registration = existing.into_active_model();
    registration.safe_address = Set(safe_address.as_ref().to_vec());
    registration.registered_block = Set(incoming_position.0);
    registration.registered_tx_index = Set(incoming_position.1);
    registration.registered_log_index = Set(incoming_position.2);
    registration.update(conn).await?;

    Ok(registration_id)
}

#[cfg(test)]
mod tests {
    use blokli_db_entity::conversions::node_safe_registration::fetch_registered_nodes_for_safes;
    use hopr_types::crypto_random::random_bytes;
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
    async fn test_register_node_moves_it_to_a_newer_safe() -> anyhow::Result<()> {
        let db = BlokliDb::new_in_memory().await?;

        let first_safe = random_address();
        let second_safe = random_address();
        let node_address = random_address();

        let id = db
            .register_node_to_safe(None, first_safe, node_address, 100, 0, 0)
            .await?;
        let moved_id = db
            .register_node_to_safe(None, second_safe, node_address, 200, 0, 0)
            .await?;

        assert_eq!(id, moved_id, "the node keeps its single registration row");

        let registration = HoprNodeSafeRegistration::find_by_id(id)
            .one(db.conn(crate::TargetDb::Index))
            .await?
            .expect("registration should exist");
        assert_eq!(registration.safe_address, second_safe.as_ref().to_vec());
        assert_eq!(registration.registered_block, 200);

        Ok(())
    }

    #[tokio::test]
    async fn test_register_node_ignores_a_superseded_registration() -> anyhow::Result<()> {
        let db = BlokliDb::new_in_memory().await?;

        let first_safe = random_address();
        let second_safe = random_address();
        let node_address = random_address();

        let id = db
            .register_node_to_safe(None, first_safe, node_address, 100, 0, 0)
            .await?;
        db.register_node_to_safe(None, second_safe, node_address, 200, 0, 0)
            .await?;

        // Replaying the first registration must not resurrect it, nor fail on the unique key that
        // allows a node only one registration: this is what a re-indexed block looks like.
        let replayed_id = db
            .register_node_to_safe(None, first_safe, node_address, 100, 0, 0)
            .await?;

        assert_eq!(id, replayed_id);

        let registration = HoprNodeSafeRegistration::find_by_id(id)
            .one(db.conn(crate::TargetDb::Index))
            .await?
            .expect("registration should exist");
        assert_eq!(registration.safe_address, second_safe.as_ref().to_vec());
        assert_eq!(registration.registered_block, 200);

        let count = HoprNodeSafeRegistration::find()
            .count(db.conn(crate::TargetDb::Index))
            .await?;
        assert_eq!(count, 1);

        Ok(())
    }

    #[tokio::test]
    async fn test_register_node_orders_registrations_within_a_block() -> anyhow::Result<()> {
        let db = BlokliDb::new_in_memory().await?;

        let first_safe = random_address();
        let second_safe = random_address();
        let node_address = random_address();

        db.register_node_to_safe(None, first_safe, node_address, 100, 2, 1)
            .await?;
        // Same block and transaction, later log: newer.
        db.register_node_to_safe(None, second_safe, node_address, 100, 2, 7)
            .await?;
        // Same block and transaction, earlier log: older.
        db.register_node_to_safe(None, first_safe, node_address, 100, 2, 1)
            .await?;

        let registration = HoprNodeSafeRegistration::find()
            .one(db.conn(crate::TargetDb::Index))
            .await?
            .expect("registration should exist");
        assert_eq!(registration.safe_address, second_safe.as_ref().to_vec());
        assert_eq!(registration.registered_log_index, 7);

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
    async fn test_node_holds_a_single_registration() -> anyhow::Result<()> {
        let db = BlokliDb::new_in_memory().await?;

        let safe1 = random_address();
        let safe2 = random_address();
        let node = random_address();

        // Register node to safe1
        db.register_node_to_safe(None, safe1, node, 100, 0, 0).await?;

        // Registering the same node to safe2 moves it rather than adding a second row: the schema
        // allows a node only one registration, and the newer event is the one that counts.
        db.register_node_to_safe(None, safe2, node, 100, 1, 0).await?;

        let count = HoprNodeSafeRegistration::find()
            .count(db.conn(crate::TargetDb::Index))
            .await?;
        assert_eq!(count, 1);
        assert_eq!(db.get_safe_for_registered_node(None, node).await?, Some(safe2));

        Ok(())
    }

    #[tokio::test]
    async fn test_fetch_registered_nodes_for_safes_batches_and_filters_invalid_nodes() -> anyhow::Result<()> {
        let db = BlokliDb::new_in_memory().await?;
        let conn = db.conn(crate::TargetDb::Index);

        let empty = fetch_registered_nodes_for_safes(conn, &[]).await?;
        assert!(empty.is_empty());

        let safe1 = random_address();
        let safe2 = random_address();
        let safe3 = random_address();
        let node1 = random_address();
        let node2 = random_address();

        db.register_node_to_safe(None, safe1, node1, 100, 0, 0).await?;
        db.register_node_to_safe(None, safe2, node2, 101, 0, 0).await?;

        HoprNodeSafeRegistration::insert(hopr_node_safe_registration::ActiveModel {
            safe_address: Set(safe1.as_ref().to_vec()),
            node_address: Set(vec![0xAB; 19]),
            registered_block: Set(102),
            registered_tx_index: Set(0),
            registered_log_index: Set(0),
            ..Default::default()
        })
        .exec(conn)
        .await?;

        let result = fetch_registered_nodes_for_safes(
            conn,
            &[
                safe1.as_ref().to_vec(),
                safe2.as_ref().to_vec(),
                safe3.as_ref().to_vec(),
            ],
        )
        .await?;

        assert_eq!(result.get(safe1.as_ref()), Some(&vec![node1]));
        assert_eq!(result.get(safe2.as_ref()), Some(&vec![node2]));
        assert!(!result.contains_key(safe3.as_ref()));

        Ok(())
    }
}
