//! Scratch PostgreSQL databases for the ignored tests that need a real server (`just test-postgres-schema`).
//!
//! The server is named by `BLOKLI_TEST_POSTGRES_URL`; its user must be allowed to create databases.

use std::{
    env, process,
    sync::atomic::{AtomicUsize, Ordering},
};

use anyhow::Context;
use blokli_db::db::BlokliDb;
use sea_orm::{ConnectionTrait, Database};
use url::Url;

const POSTGRES_URL_VAR: &str = "BLOKLI_TEST_POSTGRES_URL";

static SCRATCH_COUNTER: AtomicUsize = AtomicUsize::new(0);

/// A migrated scratch database that is dropped by [`ScratchPostgres::drop_database`].
pub(crate) struct ScratchPostgres {
    admin_url: Url,
    name: String,
    pub(crate) db: BlokliDb,
}

impl ScratchPostgres {
    pub(crate) async fn create() -> anyhow::Result<Self> {
        let admin_url: Url = env::var(POSTGRES_URL_VAR)
            .with_context(|| format!("{POSTGRES_URL_VAR} must point at a PostgreSQL server"))?
            .parse()?;
        let name = format!(
            "blokli_indexer_{}_{}",
            process::id(),
            SCRATCH_COUNTER.fetch_add(1, Ordering::SeqCst)
        );
        let admin = Database::connect(admin_url.as_str()).await?;
        admin.execute_unprepared(&format!("CREATE DATABASE \"{name}\"")).await?;
        admin.close().await?;

        let mut url = admin_url.clone();
        url.set_path(&name);
        let db = BlokliDb::new(url.as_str(), None, Default::default())
            .await
            .context("database construction (all migrations) failed on PostgreSQL")?;
        db.ensure_singletons().await?;
        Ok(Self { admin_url, name, db })
    }

    pub(crate) async fn drop_database(self) -> anyhow::Result<()> {
        drop(self.db);
        let admin = Database::connect(self.admin_url.as_str()).await?;
        admin
            .execute_unprepared(&format!("DROP DATABASE IF EXISTS \"{}\" WITH (FORCE)", self.name))
            .await?;
        Ok(())
    }
}
