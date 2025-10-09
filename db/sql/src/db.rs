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
/// Manages a database connection (PostgreSQL or SQLite) for all data operations.
/// All tables (accounts, channels, logs, etc.) are stored in the same database.
///
/// Supports database snapshot imports for fast synchronization via
/// [`BlokliDbGeneralModelOperations::import_logs_db`].
#[derive(Debug, Clone)]
pub struct BlokliDb {
    pub(crate) db: sea_orm::DatabaseConnection,

    #[allow(dead_code)]
    pub(crate) cfg: BlokliDbConfig,
}

impl BlokliDb {
    /// Create a new database connection (PostgreSQL or SQLite).
    ///
    /// # Arguments
    ///
    /// * `database_url` - Database connection URL:
    ///   - PostgreSQL: "postgresql://user:pass@localhost:5432/bloklid"
    ///   - SQLite: "sqlite:///path/to/db.sqlite?mode=rwc" or "sqlite://:memory:?mode=rwc"
    /// * `cfg` - Database configuration including connection pool settings
    ///
    /// # Process
    ///
    /// 1. Validates configuration
    /// 2. Establishes connection pool to database
    /// 3. Runs migrations for both index and logs tables
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
    pub async fn new(database_url: &str, cfg: BlokliDbConfig) -> Result<Self> {
        cfg.validate()
            .map_err(|e| DbSqlError::Construction(format!("failed configuration validation: {e}")))?;

        // For SQLite, ensure parent directory exists
        if database_url.starts_with("sqlite://") && !database_url.contains(":memory:") {
            // Extract file path from SQLite URL
            let path_part = database_url
                .strip_prefix("sqlite://")
                .and_then(|s| s.split('?').next())
                .ok_or_else(|| DbSqlError::Construction("invalid SQLite URL format".to_string()))?;

            if let Some(parent) = std::path::Path::new(path_part).parent() {
                std::fs::create_dir_all(parent)
                    .map_err(|e| DbSqlError::Construction(format!("failed to create database directory: {e}")))?;
            }
        }

        // Configure connection options
        let mut opt = ConnectOptions::new(database_url.to_string());
        opt.max_connections(cfg.max_connections)
            .min_connections(1)
            .connect_timeout(Duration::from_secs(8))
            .idle_timeout(Duration::from_secs(300))
            .max_lifetime(Duration::from_secs(1800))
            .sqlx_logging(cfg.log_slow_queries.as_secs() > 0)
            .sqlx_logging_level(LevelFilter::Warn);

        // Establish database connection
        let db = Database::connect(opt)
            .await
            .map_err(|e| DbSqlError::Construction(format!("failed to connect to database: {e}")))?;

        // Apply migrations based on database backend
        // SQLite (single file): use unified Migrator with all migrations
        // PostgreSQL: use separate MigratorIndex and MigratorChainLogs
        let is_sqlite = database_url.starts_with("sqlite://");

        if is_sqlite {
            // For SQLite, use the unified Migrator that includes all migrations
            use migration::Migrator;
            Migrator::up(&db, None)
                .await
                .map_err(|e| DbSqlError::Construction(format!("cannot apply migrations: {e}")))?;
        } else {
            // For PostgreSQL, use separate migrators
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

        Ok(Self { db, cfg })
    }

    /// Create an in-memory SQLite database for testing.
    ///
    /// Uses SQLite's in-memory mode for fast testing without requiring external database services.
    ///
    /// # Returns
    ///
    /// A `BlokliDb` instance connected to an in-memory database
    ///
    /// # Errors
    ///
    /// Returns `DbSqlError` if connection or initialization fails
    pub async fn new_in_memory() -> Result<Self> {
        // Use SQLite in-memory database for testing
        let database_url = "sqlite://:memory:?mode=rwc";
        Self::new(database_url, Default::default()).await
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
