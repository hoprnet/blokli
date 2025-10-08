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
/// Manages a single PostgreSQL database connection for all data operations.
/// All tables (accounts, channels, logs, etc.) are stored in the same database
/// with the `public` schema.
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
    /// Create a new database connection to PostgreSQL.
    ///
    /// # Arguments
    ///
    /// * `database_url` - PostgreSQL connection URL (e.g., "postgresql://user:pass@localhost:5432/bloklid")
    /// * `cfg` - Database configuration including connection pool settings
    ///
    /// # Process
    ///
    /// 1. Validates configuration
    /// 2. Establishes connection pool to PostgreSQL
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
    /// - Cannot connect to PostgreSQL
    /// - Migrations fail to apply
    /// - Account initialization encounters errors
    pub async fn new(database_url: &str, cfg: BlokliDbConfig) -> Result<Self> {
        cfg.validate()
            .map_err(|e| DbSqlError::Construction(format!("failed configuration validation: {e}")))?;

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
            .map_err(|e| DbSqlError::Construction(format!("failed to connect to PostgreSQL: {e}")))?;

        // Run migrations for index tables
        MigratorIndex::up(&db, None)
            .await
            .map_err(|e| DbSqlError::Construction(format!("cannot apply index migrations: {e}")))?;

        // Run migrations for logs tables
        MigratorChainLogs::up(&db, None)
            .await
            .map_err(|e| DbSqlError::Construction(format!("cannot apply logs migrations: {e}")))?;

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

    /// Create an in-memory PostgreSQL database for testing.
    ///
    /// **Note**: This requires a running PostgreSQL instance.
    /// For true in-memory testing, use a test database that can be quickly created/dropped.
    ///
    /// # Returns
    ///
    /// A `BlokliDb` instance connected to a test database
    ///
    /// # Errors
    ///
    /// Returns `DbSqlError` if connection or initialization fails
    pub async fn new_in_memory() -> Result<Self> {
        // For PostgreSQL, we connect to a test database
        // This requires PostgreSQL to be running
        let database_url = "postgresql://bloklid:bloklid@localhost:5432/bloklid_test";
        Self::new(database_url, Default::default()).await
    }
}

impl BlokliDbAllOperations for BlokliDb {}

#[cfg(test)]
mod tests {
    use migration::{MigratorChainLogs, MigratorIndex, MigratorTrait};

    use crate::{BlokliDbGeneralModelOperations, TargetDb, db::BlokliDb};

    #[tokio::test]
    #[ignore] // Requires PostgreSQL running
    async fn test_basic_db_init() -> anyhow::Result<()> {
        let db = BlokliDb::new_in_memory().await?;

        // With PostgreSQL, both migrations are in the same database
        MigratorIndex::status(db.conn(TargetDb::Index)).await?;
        MigratorChainLogs::status(db.conn(TargetDb::Logs)).await?;

        Ok(())
    }
}
