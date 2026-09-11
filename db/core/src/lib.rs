//! Crate for accessing database(s) of a HOPR node.
//!
//! Functionality defined here is meant to be used mostly by other higher-level crates.
//! The crate provides database operations across multiple SQLite databases for scalability
//! and supports importing logs database snapshots for fast synchronization.

pub mod accounts;
pub mod api;
pub mod channels;
// TODO: Refactor to use channel.corrupted_state field
// pub mod corrupted_channels;
pub mod db;
pub mod errors;
pub mod events;
pub mod info;
pub mod logs;
pub mod node_safe_registrations;
mod numeric;
pub mod safe_contracts;
pub mod safe_history;
pub mod safe_redeemed_stats;
pub mod services;
pub mod snapshot;
pub mod state_queries;
pub mod utils;
pub mod version;

use std::{
    path::PathBuf,
    sync::{Arc, Mutex, MutexGuard},
    time::Instant,
};

use async_trait::async_trait;
use futures::future::BoxFuture;
use sea_orm::TransactionTrait;
pub use sea_orm::{DatabaseConnection, DatabaseTransaction};

use crate::{
    accounts::BlokliDbAccountOperations,
    api::logs::BlokliDbLogOperations,
    channels::BlokliDbChannelOperations,
    // corrupted_channels::BlokliDbCorruptedChannelOperations,
    db::BlokliDb,
    errors::{DbSqlError, Result},
    events::{EventBus, StateChange},
    info::BlokliDbInfoOperations,
    node_safe_registrations::BlokliDbNodeSafeRegistrationOperations,
    safe_contracts::BlokliDbSafeContractOperations,
    safe_history::BlokliDbSafeHistoryOperations,
    safe_redeemed_stats::BlokliDbSafeRedeemedStatsOperations,
    services::BlokliDbServiceOperations,
    snapshot::LogsSnapshotInfo,
};

/// Primary key used in tables that contain only a single row.
pub const SINGULAR_TABLE_FIXED_ID: i64 = 1;

/// Shorthand for the `chrono` based timestamp type used in the database.
pub type DbTimestamp = chrono::DateTime<chrono::Utc>;

/// Represents an already opened transaction.
/// This is a thin wrapper over [DatabaseTransaction].
/// The wrapping behavior is needed to allow transaction agnostic functionalities
/// of the DB traits.
#[derive(Debug)]
pub struct OpenTransaction {
    transaction: DatabaseTransaction,
    target: TargetDb,
    deferred_events: Arc<Mutex<Vec<StateChange>>>,
    parent_deferred_events: Option<Arc<Mutex<Vec<StateChange>>>>,
    event_bus: Option<EventBus>,
}

impl OpenTransaction {
    pub(crate) fn new(transaction: DatabaseTransaction, target: TargetDb, event_bus: EventBus) -> Self {
        Self {
            transaction,
            target,
            deferred_events: Arc::new(Mutex::new(Vec::new())),
            parent_deferred_events: None,
            event_bus: Some(event_bus),
        }
    }

    fn nested(
        transaction: DatabaseTransaction,
        target: TargetDb,
        parent_deferred_events: Arc<Mutex<Vec<StateChange>>>,
    ) -> Self {
        Self {
            transaction,
            target,
            deferred_events: Arc::new(Mutex::new(Vec::new())),
            parent_deferred_events: Some(parent_deferred_events),
            event_bus: None,
        }
    }

    /// Queues a database state-change event for publication after the root transaction commits.
    ///
    /// Events are deliberately not broadcast while the transaction is open: a later rollback
    /// must not expose state changes which never became durable.
    fn deferred_events_lock(&self) -> MutexGuard<'_, Vec<StateChange>> {
        match self.deferred_events.lock() {
            Ok(events) => events,
            Err(error) => {
                tracing::warn!("deferred event queue lock poisoned; continuing with recovered queue");
                error.into_inner()
            }
        }
    }

    pub fn defer_event(&self, event: StateChange) {
        self.deferred_events_lock().push(event);
    }

    /// Executes the given `callback` inside the transaction
    /// and commits the transaction if it succeeds or rollbacks otherwise.
    #[tracing::instrument(level = "trace", name = "Sql::perform_in_transaction", skip_all, err)]
    pub async fn perform<F, T, E>(self, callback: F) -> std::result::Result<T, E>
    where
        F: for<'c> FnOnce(&'c OpenTransaction) -> BoxFuture<'c, std::result::Result<T, E>> + Send,
        T: Send,
        E: std::error::Error + From<DbSqlError>,
    {
        let start = Instant::now();
        let res = callback(&self).await;

        if res.is_ok() {
            self.commit().await?;
        } else {
            self.rollback().await?;
        }

        tracing::trace!(
            elapsed_ms = start.elapsed().as_millis(),
            was_successful = res.is_ok(),
            "transaction completed",
        );

        res
    }

    /// Commits the transaction.
    pub async fn commit(self) -> Result<()> {
        let Self {
            transaction,
            deferred_events,
            parent_deferred_events,
            event_bus,
            ..
        } = self;
        transaction.commit().await?;

        let mut deferred_events = match deferred_events.lock() {
            Ok(events) => events,
            Err(error) => {
                tracing::warn!("deferred event queue lock poisoned; continuing with recovered queue");
                error.into_inner()
            }
        };
        let events = std::mem::take(&mut *deferred_events);
        drop(deferred_events);

        if let Some(parent_deferred_events) = parent_deferred_events {
            let mut parent_events = match parent_deferred_events.lock() {
                Ok(events) => events,
                Err(error) => {
                    tracing::warn!("deferred event queue lock poisoned; continuing with recovered queue");
                    error.into_inner()
                }
            };
            parent_events.extend(events);
        } else if let Some(event_bus) = event_bus {
            for event in events {
                if let Err(error) = event_bus.publish(event) {
                    tracing::warn!(%error, "failed to publish deferred database state change event");
                }
            }
        }

        Ok(())
    }

    /// Rollbacks the transaction.
    pub async fn rollback(self) -> Result<()> {
        Ok(self.transaction.rollback().await?)
    }
}

impl AsRef<DatabaseTransaction> for OpenTransaction {
    fn as_ref(&self) -> &DatabaseTransaction {
        &self.transaction
    }
}

impl From<OpenTransaction> for DatabaseTransaction {
    fn from(value: OpenTransaction) -> Self {
        value.transaction
    }
}

/// Shorthand for optional transaction.
/// Useful for transaction nesting (see [`BlokliDbGeneralModelOperations::nest_transaction`]).
pub type OptTx<'a> = Option<&'a OpenTransaction>;

/// When Sqlite is used as a backend, model needs to be split
/// into 2 different databases to avoid locking the database.
/// On Postgres backend, these should actually point to the same database.
#[derive(Copy, Clone, Debug, PartialEq, Eq, Default)]
pub enum TargetDb {
    #[default]
    /// Indexer database.
    Index,
    /// RPC logs database
    Logs,
}

#[async_trait]
pub trait BlokliDbGeneralModelOperations {
    /// Returns reference to the database connection.
    /// Can be used in case transaction is not needed, but
    /// users should aim to use [`BlokliDbGeneralModelOperations::begin_transaction`]
    /// and [`BlokliDbGeneralModelOperations::nest_transaction`] as much as possible.
    fn conn(&self, target_db: TargetDb) -> &DatabaseConnection;

    /// Creates a new transaction.
    async fn begin_transaction_in_db(&self, target: TargetDb) -> Result<OpenTransaction>;

    /// Import logs snapshot SQL data from a snapshot directory.
    ///
    /// Replaces all data in the current logs database with data from a snapshot's
    /// `hopr_logs.sql` file. This is used for fast synchronization during node startup.
    ///
    /// # Process
    ///
    /// 1. Reads `hopr_logs.sql` from the snapshot directory
    /// 2. Clears existing data from all logs-related tables
    /// 3. Parses the `COPY ... FROM stdin` sections for `log`, `log_status`, and `log_topic_info`
    /// 4. Inserts the parsed rows into the logs tables
    /// 5. Commits the transaction
    ///
    /// All operations are performed within a single transaction for atomicity.
    ///
    /// # Arguments
    ///
    /// * `src_dir` - Directory containing the extracted snapshot with `hopr_logs.sql`
    ///
    /// # Returns
    ///
    /// [`LogsSnapshotInfo`] on successful import.
    ///
    /// # Errors
    ///
    /// - Returns error if `hopr_logs.sql` is not found in the source directory
    /// - Returns error if reading or inserting snapshot rows fails
    /// - All database errors are wrapped in [`DbSqlError::Construction`]
    ///
    /// # Example
    ///
    /// ```no_run
    /// # use std::path::PathBuf;
    /// # use blokli_db::BlokliDbGeneralModelOperations;
    /// # async fn example(db: impl BlokliDbGeneralModelOperations) -> Result<(), Box<dyn std::error::Error>> {
    /// let snapshot_dir = PathBuf::from("/tmp/snapshot_extracted");
    /// db.import_logs_snapshot(snapshot_dir).await?;
    /// # Ok(())
    /// # }
    /// ```
    async fn import_logs_snapshot(self, src_dir: PathBuf) -> Result<LogsSnapshotInfo>;

    /// Export logs snapshot SQL data into a target directory.
    ///
    /// Writes `hopr_logs.sql` into `target_dir`.
    ///
    /// # Arguments
    ///
    /// * `target_dir` - Directory where `hopr_logs.sql` should be written
    ///
    /// # Returns
    ///
    /// [`LogsSnapshotInfo`] describing the exported snapshot, including row counts
    /// and the latest block number captured in the exported logs.
    ///
    /// # Errors
    ///
    /// Returns an error if the target directory cannot be created, the snapshot
    /// file cannot be written, or exporting rows from the logs database fails.
    ///
    /// # Example
    ///
    /// ```no_run
    /// # use std::path::PathBuf;
    /// # use blokli_db::BlokliDbGeneralModelOperations;
    /// # async fn example(db: impl BlokliDbGeneralModelOperations) -> Result<(), Box<dyn std::error::Error>> {
    /// let target_dir = PathBuf::from("/tmp/snapshot");
    /// let info = db.export_logs_snapshot(target_dir).await?;
    /// println!("exported {} logs", info.log_count);
    /// # Ok(())
    /// # }
    /// ```
    async fn export_logs_snapshot(&self, target_dir: PathBuf) -> Result<LogsSnapshotInfo>;

    /// Same as [`BlokliDbGeneralModelOperations::begin_transaction_in_db`] with default [TargetDb].
    async fn begin_transaction(&self) -> Result<OpenTransaction> {
        self.begin_transaction_in_db(Default::default()).await
    }

    /// Creates a nested transaction inside the given transaction.
    ///
    /// If `None` is given, behaves exactly as [`BlokliDbGeneralModelOperations::begin_transaction`].
    ///
    /// This method is useful for creating APIs that should be agnostic whether they are being
    /// run from an existing transaction or without it (via [OptTx]).
    ///
    /// If `tx` is `Some`, the `target_db` must match with the one in `tx`. In other words,
    /// nesting across different databases is forbidden and the method will panic.
    async fn nest_transaction_in_db(&self, tx: OptTx<'_>, target_db: TargetDb) -> Result<OpenTransaction> {
        if let Some(t) = tx {
            assert_eq!(
                t.target, target_db,
                "attempt to create nest into tx from a different db"
            );
            Ok(OpenTransaction::nested(
                t.as_ref().begin().await?,
                target_db,
                t.deferred_events.clone(),
            ))
        } else {
            self.begin_transaction_in_db(target_db).await
        }
    }

    /// Same as [`BlokliDbGeneralModelOperations::nest_transaction_in_db`] with default [TargetDb].
    async fn nest_transaction(&self, tx: OptTx<'_>) -> Result<OpenTransaction> {
        self.nest_transaction_in_db(tx, Default::default()).await
    }
}

#[async_trait]
impl BlokliDbGeneralModelOperations for BlokliDb {
    /// Retrieves raw database connection for the specified target.
    ///
    /// For PostgreSQL: both Index and Logs use the same database connection.
    /// For SQLite with dual databases: Index uses `db`, Logs uses `logs_db`.
    fn conn(&self, target_db: TargetDb) -> &DatabaseConnection {
        match target_db {
            TargetDb::Index => &self.db,
            TargetDb::Logs => self.logs_db(),
        }
    }

    /// Starts a new transaction on the appropriate database.
    ///
    /// For PostgreSQL: both Index and Logs use the same database connection.
    /// For SQLite with dual databases: uses the appropriate connection based on `target_db`.
    async fn begin_transaction_in_db(&self, target_db: TargetDb) -> Result<OpenTransaction> {
        let db_conn = self.conn(target_db);
        Ok(OpenTransaction::new(
            db_conn.begin_with_config(None, None).await?,
            target_db,
            self.event_bus.clone(),
        ))
    }

    async fn import_logs_snapshot(self, src_dir: PathBuf) -> Result<LogsSnapshotInfo> {
        snapshot::import_logs_snapshot_from_dir(&self, &src_dir).await
    }

    async fn export_logs_snapshot(&self, target_dir: PathBuf) -> Result<LogsSnapshotInfo> {
        snapshot::export_logs_snapshot_to_dir(self, &target_dir).await
    }
}

/// Convenience trait that contain all HOPR DB operations crates.
pub trait BlokliDbAllOperations:
    BlokliDbGeneralModelOperations
    + BlokliDbAccountOperations
    + BlokliDbChannelOperations
    // + BlokliDbCorruptedChannelOperations
    + BlokliDbInfoOperations
    + BlokliDbLogOperations
    + BlokliDbNodeSafeRegistrationOperations
    + BlokliDbSafeRedeemedStatsOperations
    + BlokliDbSafeContractOperations
    + BlokliDbSafeHistoryOperations
    + BlokliDbServiceOperations
{
}

#[doc(hidden)]
pub mod prelude {
    pub use super::*;
    pub use crate::api::logs::*;
    pub use crate::{
        accounts::*,
        api,
        channels::*, // corrupted_channels::*,
        db::*,
        errors::*,
        events::*,
        info::*,
        safe_contracts::*,
        safe_history::*,
        safe_redeemed_stats::*,
        services::*,
        state_queries::*,
    };
}
