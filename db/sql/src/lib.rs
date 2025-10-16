//! Crate for accessing database(s) of a HOPR node.
//!
//! Functionality defined here is meant to be used mostly by other higher-level crates.
//! The crate provides database operations across multiple SQLite databases for scalability
//! and supports importing logs database snapshots for fast synchronization.

pub mod accounts;
pub mod channels;
// TODO: Refactor to use channel.corrupted_state field
// pub mod corrupted_channels;
pub mod db;
pub mod errors;
pub mod info;
pub mod logs;

use std::path::PathBuf;

use async_trait::async_trait;
pub use blokli_db_api as api;
use blokli_db_api::logs::BlokliDbLogOperations;
use futures::future::BoxFuture;
use sea_orm::TransactionTrait;
pub use sea_orm::{DatabaseConnection, DatabaseTransaction};

use crate::{
    accounts::BlokliDbAccountOperations,
    channels::BlokliDbChannelOperations,
    // corrupted_channels::BlokliDbCorruptedChannelOperations,
    db::BlokliDb,
    errors::{DbSqlError, Result},
    info::BlokliDbInfoOperations,
};

/// Primary key used in tables that contain only a single row.
pub const SINGULAR_TABLE_FIXED_ID: i32 = 1;

/// Shorthand for the `chrono` based timestamp type used in the database.
pub type DbTimestamp = chrono::DateTime<chrono::Utc>;

/// Represents an already opened transaction.
/// This is a thin wrapper over [DatabaseTransaction].
/// The wrapping behavior is needed to allow transaction agnostic functionalities
/// of the DB traits.
#[derive(Debug)]
pub struct OpenTransaction(DatabaseTransaction, TargetDb);

impl OpenTransaction {
    /// Executes the given `callback` inside the transaction
    /// and commits the transaction if it succeeds or rollbacks otherwise.
    #[tracing::instrument(level = "trace", name = "Sql::perform_in_transaction", skip_all, err)]
    pub async fn perform<F, T, E>(self, callback: F) -> std::result::Result<T, E>
    where
        F: for<'c> FnOnce(&'c OpenTransaction) -> BoxFuture<'c, std::result::Result<T, E>> + Send,
        T: Send,
        E: std::error::Error + From<DbSqlError>,
    {
        let start = std::time::Instant::now();
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
        Ok(self.0.commit().await?)
    }

    /// Rollbacks the transaction.
    pub async fn rollback(self) -> Result<()> {
        Ok(self.0.rollback().await?)
    }
}

impl AsRef<DatabaseTransaction> for OpenTransaction {
    fn as_ref(&self) -> &DatabaseTransaction {
        &self.0
    }
}

impl From<OpenTransaction> for DatabaseTransaction {
    fn from(value: OpenTransaction) -> Self {
        value.0
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

    /// Import logs database from a snapshot directory.
    ///
    /// Replaces all data in the current logs database with data from a snapshot's
    /// `hopr_logs.db` file. This is used for fast synchronization during node startup.
    ///
    /// # Process
    ///
    /// 1. Attaches the source database from the snapshot directory
    /// 2. Clears existing data from all logs-related tables
    /// 3. Copies all data from the snapshot database
    /// 4. Detaches the source database
    ///
    /// All operations are performed within a single transaction for atomicity.
    ///
    /// # Arguments
    ///
    /// * `src_dir` - Directory containing the extracted snapshot with `hopr_logs.db`
    ///
    /// # Returns
    ///
    /// `Ok(())` on successful import, or [`DbSqlError::Construction`] if the source
    /// database doesn't exist or the import operation fails.
    ///
    /// # Errors
    ///
    /// - Returns error if `hopr_logs.db` is not found in the source directory
    /// - Returns error if SQLite ATTACH, data transfer, or DETACH operations fail
    /// - All database errors are wrapped in [`DbSqlError::Construction`]
    ///
    /// # Example
    ///
    /// ```no_run
    /// # use std::path::PathBuf;
    /// # use blokli_db_sql::BlokliDbGeneralModelOperations;
    /// # async fn example(db: impl BlokliDbGeneralModelOperations) -> Result<(), Box<dyn std::error::Error>> {
    /// let snapshot_dir = PathBuf::from("/tmp/snapshot_extracted");
    /// db.import_logs_db(snapshot_dir).await?;
    /// # Ok(())
    /// # }
    /// ```
    async fn import_logs_db(self, src_dir: PathBuf) -> Result<()>;

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
            assert_eq!(t.1, target_db, "attempt to create nest into tx from a different db");
            Ok(OpenTransaction(t.as_ref().begin().await?, target_db))
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
        Ok(OpenTransaction(db_conn.begin_with_config(None, None).await?, target_db))
    }

    async fn import_logs_db(self, src_dir: PathBuf) -> Result<()> {
        use sea_orm::ConnectionTrait;

        let sql_path = src_dir.join("hopr_logs.sql");

        if !sql_path.exists() {
            return Err(DbSqlError::Construction(format!(
                "SQL dump file not found: {}",
                sql_path.display()
            )));
        }

        // Read the SQL dump file
        let sql_content = std::fs::read_to_string(&sql_path)
            .map_err(|e| DbSqlError::Construction(format!("Failed to read SQL dump file: {}", e)))?;

        // Execute within a transaction for atomicity
        let txn = self.db.begin().await?;

        // Clear existing data from log tables
        txn.execute_unprepared("TRUNCATE TABLE log, log_status, log_topic_info CASCADE")
            .await?;

        // Execute the SQL dump
        // Split on empty lines or statement terminators to handle multi-statement SQL
        for statement in sql_content.split(';') {
            let trimmed = statement.trim();
            if trimmed.is_empty() || trimmed.starts_with("--") {
                continue;
            }

            // Execute the statement
            let sql = format!("{};", trimmed);
            txn.execute_unprepared(&sql)
                .await
                .map_err(|e| DbSqlError::Construction(format!("Failed to execute SQL statement: {}", e)))?;
        }

        // Commit the transaction
        txn.commit().await?;

        Ok(())
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
{
}

#[doc(hidden)]
pub mod prelude {
    pub use blokli_db_api::logs::*;

    pub use super::*;
    pub use crate::{accounts::*, channels::*, /* corrupted_channels::*, */ db::*, errors::*, info::*};
}
