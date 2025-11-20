//! Unified database notification abstraction for PostgreSQL and SQLite
//!
//! This module provides a unified interface for receiving database change notifications
//! across different database backends:
//! - PostgreSQL: Uses LISTEN/NOTIFY
//! - SQLite: Uses update hooks with async channels

use std::pin::Pin;
use std::time::Duration;

use anyhow::anyhow;
use async_stream::stream;
use futures::Stream;
use sea_orm::{DatabaseBackend, DatabaseConnection};
use sqlx::postgres::PgListener;
use tokio::time::sleep;
use tracing::error;

use crate::errors::ApiError;

/// Create a notification stream for ticket parameters updates
///
/// Returns a stream that yields `()` whenever the ticket_price or
/// min_incoming_ticket_win_prob fields in the chain_info table are updated.
///
/// # Backend-specific implementations
///
/// - **PostgreSQL**: Uses LISTEN on the `ticket_params_updated` channel. Notifications are sent by a database trigger
///   when chain_info is updated.
///
/// - **SQLite**: Falls back to polling with 1-second interval since SQLite update hooks require access to the
///   underlying sqlx connection which is not easily accessible through SeaORM's connection pool.
///
/// # Arguments
///
/// * `db` - Database connection (SeaORM)
///
/// # Returns
///
/// A stream that yields `()` on each notification
pub async fn create_ticket_params_notification_stream(
    db: &DatabaseConnection,
) -> Result<Pin<Box<dyn Stream<Item = ()> + Send>>, ApiError> {
    match db.get_database_backend() {
        DatabaseBackend::Postgres => create_postgres_notification_stream(db).await,
        DatabaseBackend::Sqlite => create_sqlite_notification_stream(db).await,
        backend => Err(ApiError::ConfigError(format!(
            "Unsupported database backend: {:?}",
            backend
        ))),
    }
}

/// Create PostgreSQL LISTEN/NOTIFY notification stream
#[cfg(feature = "runtime-tokio")]
async fn create_postgres_notification_stream(
    db: &DatabaseConnection,
) -> Result<Pin<Box<dyn Stream<Item = ()> + Send>>, ApiError> {
    // Ensure we're using PostgreSQL backend
    if db.get_database_backend() != DatabaseBackend::Postgres {
        return Err(ApiError::ConfigError(
            "PostgreSQL notification stream requires PostgreSQL backend".to_string(),
        ));
    }

    // Get the underlying SQLx pool (returns a reference)
    let sqlx_pool = db.get_postgres_connection_pool();

    // Create a listener for the ticket_params_updated channel
    let mut listener = PgListener::connect_with(sqlx_pool)
        .await
        .map_err(|e| ApiError::InternalError(anyhow!("Failed to create PgListener: {}", e)))?;

    listener
        .listen("ticket_params_updated")
        .await
        .map_err(|e| ApiError::InternalError(anyhow!("Failed to LISTEN to channel: {}", e)))?;

    // Convert the listener into a stream
    Ok(Box::pin(stream! {
        loop {
            match listener.recv().await {
                Ok(_notification) => {
                    // Notification received, yield unit to trigger subscription update
                    yield ();
                }
                Err(e) => {
                    // Log error but continue listening
                    error!("Error receiving PostgreSQL notification: {}", e);
                    // Brief delay before continuing to avoid tight error loop
                    sleep(Duration::from_millis(100)).await;
                }
            }
        }
    }))
}

/// Create SQLite polling notification stream
///
/// Since SQLite update hooks require raw sqlx connection access which is not
/// easily available through SeaORM's connection pool, we fall back to polling
/// for simplicity in tests and development environments.
async fn create_sqlite_notification_stream(
    _db: &DatabaseConnection,
) -> Result<Pin<Box<dyn Stream<Item = ()> + Send>>, ApiError> {
    Ok(Box::pin(stream! {
        loop {
            sleep(Duration::from_secs(1)).await;
            yield ();
        }
    }))
}

#[cfg(test)]
mod tests {
    use blokli_db::{BlokliDbGeneralModelOperations, TargetDb, db::BlokliDb};

    use super::*;

    #[tokio::test]
    async fn test_create_sqlite_notification_stream() {
        let db = BlokliDb::new_in_memory().await.unwrap();
        let result = create_ticket_params_notification_stream(db.conn(TargetDb::Index)).await;
        assert!(result.is_ok());
    }
}
