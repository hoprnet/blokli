use std::time::Duration;

use blokli_db_entity::prelude::{Account, Announcement};
use migration::{MigratorChainLogs, MigratorIndex, MigratorTrait};
use sea_orm::{ConnectOptions, Database, EntityTrait};
use tracing::log::LevelFilter;
use validator::Validate;

use crate::{
    BlokliDbAllOperations,
    accounts::model_to_account_entry,
    errors::{DbSqlError, Result},
    events::EventBus,
};

#[derive(Debug, Clone, PartialEq, Eq, smart_default::SmartDefault, validator::Validate)]
pub struct BlokliDbConfig {
    #[default(10)]
    pub max_connections: u32,
    #[default(Duration::from_secs(5))]
    pub log_slow_queries: Duration,
}

/// Main database handle for HOPR node operations.
///
/// For PostgreSQL: Uses a single database connection for all tables.
/// For SQLite: Uses two separate database connections to avoid write lock contention:
///   - `db`: Index database (accounts, channels, announcements, node_info, chain_info)
///   - `logs_db`: Logs database (log, log_status, log_topic_info)
///
/// Supports database snapshot imports for fast synchronization via
/// [`BlokliDbGeneralModelOperations::import_logs_db`].
///
/// Includes event bus for real-time state change notifications.
#[derive(Clone)]
pub struct BlokliDb {
    /// Primary database connection (index tables for SQLite, all tables for PostgreSQL)
    pub(crate) db: sea_orm::DatabaseConnection,

    /// Logs database connection (only used for SQLite, None for PostgreSQL)
    pub(crate) logs_db: Option<sea_orm::DatabaseConnection>,

    /// Event bus for broadcasting state changes
    pub(crate) event_bus: EventBus,

    #[allow(dead_code)]
    pub(crate) cfg: BlokliDbConfig,
}

impl std::fmt::Debug for BlokliDb {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("BlokliDb")
            .field("db", &self.db)
            .field("logs_db", &self.logs_db)
            .field("event_bus_subscribers", &self.event_bus.subscriber_count())
            .field("cfg", &self.cfg)
            .finish()
    }
}

impl BlokliDb {
    /// Create a new database connection (PostgreSQL or SQLite).
    ///
    /// # Arguments
    ///
    /// * `database_url` - Primary database connection URL (index database for SQLite):
    ///   - PostgreSQL: "postgresql://user:pass@localhost:5432/bloklid"
    ///   - SQLite: "sqlite:///path/to/index.db?mode=rwc" or "sqlite://:memory:?mode=rwc"
    /// * `logs_database_url` - Logs database connection URL (only for SQLite, None for PostgreSQL)
    /// * `cfg` - Database configuration including connection pool settings
    ///
    /// # Process
    ///
    /// 1. Validates configuration
    /// 2. Establishes connection pool(s) to database(s)
    /// 3. Runs migrations for index and logs tables (on separate databases for SQLite)
    /// 4. Initializes account KeyId mappings
    ///
    /// # Returns
    ///
    /// A configured `BlokliDb` instance ready for use
    ///
    /// # Errors
    ///
    /// Returns `DbSqlError` if:
    /// - Configuration validation fails
    /// - Cannot connect to database
    /// - Migrations fail to apply
    /// - Account initialization encounters errors
    pub async fn new(database_url: &str, logs_database_url: Option<&str>, cfg: BlokliDbConfig) -> Result<Self> {
        cfg.validate()
            .map_err(|e| DbSqlError::Construction(format!("failed configuration validation: {e}")))?;

        let is_sqlite = database_url.starts_with("sqlite://");

        // Helper function to ensure parent directory exists for SQLite databases
        let ensure_sqlite_dir = |url: &str| -> Result<()> {
            if url.starts_with("sqlite://") && !url.contains(":memory:") {
                let path_part = url
                    .strip_prefix("sqlite://")
                    .and_then(|s| s.split('?').next())
                    .ok_or_else(|| DbSqlError::Construction("invalid SQLite URL format".to_string()))?;

                if let Some(parent) = std::path::Path::new(path_part).parent() {
                    std::fs::create_dir_all(parent)
                        .map_err(|e| DbSqlError::Construction(format!("failed to create database directory: {e}")))?;
                }
            }
            Ok(())
        };

        // Ensure directories exist for database files
        ensure_sqlite_dir(database_url)?;
        if let Some(logs_url) = logs_database_url {
            ensure_sqlite_dir(logs_url)?;
        }

        // Helper function to create connection options
        let create_connection_opts = |url: &str| -> ConnectOptions {
            let mut opt = ConnectOptions::new(url.to_string());
            opt.max_connections(cfg.max_connections)
                .min_connections(1)
                .connect_timeout(Duration::from_secs(8))
                .idle_timeout(Duration::from_secs(300))
                .max_lifetime(Duration::from_secs(1800))
                .sqlx_logging(cfg.log_slow_queries.as_secs() > 0)
                .sqlx_logging_level(LevelFilter::Warn);
            opt
        };

        // Establish primary database connection (index for SQLite, all tables for PostgreSQL)
        let db = Database::connect(create_connection_opts(database_url))
            .await
            .map_err(|e| DbSqlError::Construction(format!("failed to connect to index database: {e}")))?;

        // Establish logs database connection (only for SQLite)
        let logs_db = if let Some(logs_url) = logs_database_url {
            Some(
                Database::connect(create_connection_opts(logs_url))
                    .await
                    .map_err(|e| DbSqlError::Construction(format!("failed to connect to logs database: {e}")))?,
            )
        } else {
            None
        };

        // Apply migrations based on database backend
        if is_sqlite && logs_db.is_some() {
            // For SQLite with dual databases: run separate migrations on each database
            MigratorIndex::up(&db, None)
                .await
                .map_err(|e| DbSqlError::Construction(format!("cannot apply index migrations: {e}")))?;

            MigratorChainLogs::up(logs_db.as_ref().unwrap(), None)
                .await
                .map_err(|e| DbSqlError::Construction(format!("cannot apply logs migrations: {e}")))?;
        } else {
            // For PostgreSQL (or legacy single-file SQLite): run all migrations on single database
            MigratorIndex::up(&db, None)
                .await
                .map_err(|e| DbSqlError::Construction(format!("cannot apply index migrations: {e}")))?;

            MigratorChainLogs::up(&db, None)
                .await
                .map_err(|e| DbSqlError::Construction(format!("cannot apply logs migrations: {e}")))?;
        }

        // Initialize KeyId mapping for accounts
        Account::find()
            .find_with_related(Announcement)
            .all(&db)
            .await?
            .into_iter()
            .try_for_each(|(a, b)| match model_to_account_entry(a, b) {
                Ok(_account) => {
                    // FIXME: update key id mapper
                    Ok::<(), DbSqlError>(())
                }
                Err(error) => {
                    // Undecodeable accounts are skipped and will be unreachable
                    tracing::error!(%error, "undecodeable account");
                    Ok(())
                }
            })?;

        // Initialize event bus with capacity for 1000 events per subscriber
        let event_bus = EventBus::new(1000);

        Ok(Self {
            db,
            logs_db,
            event_bus,
            cfg,
        })
    }

    /// Create an in-memory SQLite database for testing.
    ///
    /// Uses SQLite's in-memory mode for fast testing without requiring external database services.
    /// Uses dual in-memory databases (one for index, one for logs) to match production behavior.
    ///
    /// # Returns
    ///
    /// A `BlokliDb` instance connected to in-memory databases
    ///
    /// # Errors
    ///
    /// Returns `DbSqlError` if connection or initialization fails
    pub async fn new_in_memory() -> Result<Self> {
        // Use SQLite in-memory databases for testing (dual databases like production)
        let index_url = "sqlite::memory:";
        let logs_url = "sqlite::memory:";
        Self::new(index_url, Some(logs_url), Default::default()).await
    }

    /// Get the appropriate database connection for log-related operations.
    ///
    /// For SQLite with dual databases, returns the logs database connection.
    /// For PostgreSQL or single-database mode, returns the primary database connection.
    pub(crate) fn logs_db(&self) -> &sea_orm::DatabaseConnection {
        self.logs_db.as_ref().unwrap_or(&self.db)
    }

    /// Get a reference to the event bus for subscribing to state changes.
    ///
    /// Subscribers will receive real-time notifications when account or channel state changes.
    ///
    /// # Example
    ///
    /// ```rust,ignore
    /// let mut subscriber = db.event_bus().subscribe();
    /// tokio::spawn(async move {
    ///     while let Ok(event) = subscriber.recv().await {
    ///         // Handle state change event
    ///     }
    /// });
    /// ```
    pub fn event_bus(&self) -> &EventBus {
        &self.event_bus
    }
}

impl BlokliDbAllOperations for BlokliDb {}

#[cfg(test)]
mod tests {
    use migration::{MigratorChainLogs, MigratorIndex, MigratorTrait};

    use crate::{BlokliDbGeneralModelOperations, TargetDb, db::BlokliDb};

    #[tokio::test]
    async fn test_basic_db_init() -> anyhow::Result<()> {
        let db = BlokliDb::new_in_memory().await?;

        // For SQLite, check the unified Migrator status
        use migration::Migrator;
        Migrator::status(db.conn(TargetDb::Index)).await?;

        Ok(())
    }
}
