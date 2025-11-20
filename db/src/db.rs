use std::time::Duration;

use blokli_db_entity::prelude::{Account, Announcement};
use migration::{MigratorChainLogs, MigratorIndex, MigratorTrait};
use sea_orm::{ConnectOptions, Database, EntityTrait, SqlxSqliteConnector};
use sqlx::sqlite::{SqliteConnectOptions, SqlitePoolOptions};
use tracing::log::LevelFilter;
use validator::Validate;

use crate::{
    BlokliDbAllOperations,
    accounts::model_to_account_entry,
    errors::{DbSqlError, Result},
    events::EventBus,
    notifications::{SqliteHookSender, SqliteNotification, SqliteNotificationManager},
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

    /// SQLite notification manager for update hooks (None for PostgreSQL)
    pub(crate) sqlite_notification_manager: Option<SqliteNotificationManager>,

    #[allow(dead_code)]
    pub(crate) cfg: BlokliDbConfig,
}

impl std::fmt::Debug for BlokliDb {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("BlokliDb")
            .field("db", &self.db)
            .field("logs_db", &self.logs_db)
            .field("event_bus_subscribers", &self.event_bus.subscriber_count())
            .field("sqlite_notification_manager", &self.sqlite_notification_manager)
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

        let is_sqlite = database_url.starts_with("sqlite:");

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

        // Helper function to create connection options for PostgreSQL
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

        // Create database connections with notification support
        let (db, logs_db, sqlite_notification_manager) = if is_sqlite {
            // Create SQLite notification manager
            let (manager, sync_sender) = SqliteNotificationManager::new(100);
            let hook_sender = SqliteHookSender::new(sync_sender);

            // Helper to create SQLite pool with update hooks
            let create_sqlite_pool = |url: String, sender: SqliteHookSender| async move {
                // Parse URL to extract path and mode
                let connect_opts: SqliteConnectOptions = url
                    .parse()
                    .map_err(|e| DbSqlError::Construction(format!("invalid SQLite URL: {e}")))?;

                let pool = SqlitePoolOptions::new()
                    .max_connections(cfg.max_connections)
                    .min_connections(1)
                    .acquire_timeout(Duration::from_secs(8))
                    .idle_timeout(Duration::from_secs(300))
                    .max_lifetime(Duration::from_secs(1800))
                    .after_connect({
                        let sender = sender.clone();
                        move |conn, _meta| {
                            let sender = sender.clone();
                            Box::pin(async move {
                                let mut handle = conn
                                    .lock_handle()
                                    .await
                                    .map_err(|e| sqlx::Error::Protocol(format!("failed to lock handle: {e}")))?;

                                // Set up update hook to notify on chain_info changes
                                handle.set_update_hook(move |result| {
                                    if result.table == "chain_info" {
                                        sender.send(SqliteNotification::ChainInfoUpdated);
                                    }
                                });

                                Ok(())
                            })
                        }
                    })
                    .connect_with(connect_opts)
                    .await
                    .map_err(|e| DbSqlError::Construction(format!("failed to connect: {e}")))?;

                Ok::<_, DbSqlError>(SqlxSqliteConnector::from_sqlx_sqlite_pool(pool))
            };

            // Create index database with hooks
            let index_db = create_sqlite_pool(database_url.to_string(), hook_sender.clone()).await?;

            // Create logs database with hooks (if provided)
            let logs_db = if let Some(logs_url) = logs_database_url {
                Some(create_sqlite_pool(logs_url.to_string(), hook_sender).await?)
            } else {
                None
            };

            (index_db, logs_db, Some(manager))
        } else {
            // PostgreSQL: use standard SeaORM connection (uses LISTEN/NOTIFY instead)
            let db = Database::connect(create_connection_opts(database_url))
                .await
                .map_err(|e| DbSqlError::Construction(format!("failed to connect to index database: {e}")))?;

            let logs_db = if let Some(logs_url) = logs_database_url {
                Some(
                    Database::connect(create_connection_opts(logs_url))
                        .await
                        .map_err(|e| DbSqlError::Construction(format!("failed to connect to logs database: {e}")))?,
                )
            } else {
                None
            };

            (db, logs_db, None)
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
            sqlite_notification_manager,
            cfg,
        })
    }

    /// Create an in-memory SQLite database for testing.
    ///
    /// Uses SQLite's in-memory mode with shared cache for fast testing without requiring
    /// external database services. Uses dual in-memory databases (one for index, one for logs)
    /// to match production behavior.
    ///
    /// # Returns
    ///
    /// A `BlokliDb` instance connected to in-memory databases
    ///
    /// # Errors
    ///
    /// Returns `DbSqlError` if connection or initialization fails
    pub async fn new_in_memory() -> Result<Self> {
        use std::sync::atomic::{AtomicU64, Ordering};

        // Atomic counter for unique database names across parallel test runs
        static DB_COUNTER: AtomicU64 = AtomicU64::new(0);
        let id = DB_COUNTER.fetch_add(1, Ordering::SeqCst);

        // Use SQLite in-memory databases with shared cache for testing
        // The shared cache mode allows multiple connections in the pool to access
        // the same in-memory database, which is essential for:
        // 1. Data visibility across connections
        // 2. Update hooks firing when any connection modifies data
        // Unique names (using counter) ensure test isolation in parallel runs
        let index_url = format!("sqlite:file:index_{}?mode=memory&cache=shared", id);
        let logs_url = format!("sqlite:file:logs_{}?mode=memory&cache=shared", id);
        Self::new(&index_url, Some(&logs_url), Default::default()).await
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

    /// Get a reference to the SQLite notification manager.
    ///
    /// Returns `Some` for SQLite databases (including in-memory), `None` for PostgreSQL.
    /// Use this to subscribe to real-time table change notifications.
    ///
    /// # Example
    ///
    /// ```rust,ignore
    /// if let Some(manager) = db.sqlite_notification_manager() {
    ///     let mut rx = manager.subscribe();
    ///     while let Ok(notification) = rx.recv().await {
    ///         // Handle notification
    ///     }
    /// }
    /// ```
    pub fn sqlite_notification_manager(&self) -> Option<&SqliteNotificationManager> {
        self.sqlite_notification_manager.as_ref()
    }
}

impl BlokliDbAllOperations for BlokliDb {}

#[cfg(test)]
mod tests {
    use migration::MigratorTrait;

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
