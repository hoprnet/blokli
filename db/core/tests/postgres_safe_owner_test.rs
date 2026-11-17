//! Runs the Safe owner and threshold state operations against a real PostgreSQL.
//!
//! SQLite and PostgreSQL render placeholders differently (`?` against `$n`), so statements that pass every
//! SQLite test can still be rejected by PostgreSQL. These tests exercise each guarded statement there.
//!
//! They need a PostgreSQL server and are therefore ignored by default. Run them with
//! `just test-postgres-schema`, or point `BLOKLI_TEST_POSTGRES_URL` at a server whose user may create
//! databases and run `cargo test -p blokli-db --test postgres_safe_owner_test -- --ignored`.

use std::{
    env, process,
    sync::atomic::{AtomicUsize, Ordering},
};

use anyhow::{Context, Result};
use blokli_db::{
    BlokliDbGeneralModelOperations, TargetDb, db::BlokliDb, safe_contracts::BlokliDbSafeContractOperations,
    safe_history::BlokliDbSafeHistoryOperations,
};
use blokli_db_entity::{hopr_safe_owner_state, prelude::HoprSafeOwnerState};
use hopr_types::primitive::prelude::Address;
use sea_orm::{ColumnTrait, ConnectionTrait, Database, EntityTrait, QueryFilter};
use url::Url;

const POSTGRES_URL_VAR: &str = "BLOKLI_TEST_POSTGRES_URL";

static SCRATCH_COUNTER: AtomicUsize = AtomicUsize::new(0);

/// A scratch database on the server named by `BLOKLI_TEST_POSTGRES_URL`, with all migrations applied.
struct Scratch {
    admin_url: Url,
    name: String,
    db: BlokliDb,
}

impl Scratch {
    async fn create() -> Result<Self> {
        let admin_url: Url = env::var(POSTGRES_URL_VAR)
            .with_context(|| format!("{POSTGRES_URL_VAR} must point at a PostgreSQL server"))?
            .parse()?;
        let name = format!(
            "blokli_safe_owner_{}_{}",
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

    async fn drop_database(self) -> Result<()> {
        drop(self.db);
        let admin = Database::connect(self.admin_url.as_str()).await?;
        admin
            .execute_unprepared(&format!("DROP DATABASE IF EXISTS \"{}\" WITH (FORCE)", self.name))
            .await?;
        Ok(())
    }
}

fn address(byte: u8) -> Address {
    Address::from([byte; 20])
}

async fn add_safe(db: &BlokliDb, byte: u8) -> Result<Address> {
    let safe = address(byte);
    db.create_safe_contract(None, safe, address(0xa0), address(0xa1), 100, 0, 0)
        .await?;
    Ok(safe)
}

async fn owner_rows(db: &BlokliDb, safe: Address) -> Result<Vec<hopr_safe_owner_state::Model>> {
    let entry = db
        .get_safe_contract_by_address(None, safe)
        .await?
        .context("the Safe exists")?;
    Ok(HoprSafeOwnerState::find()
        .filter(hopr_safe_owner_state::Column::HoprSafeContractId.eq(entry.id))
        .all(db.conn(TargetDb::Index))
        .await?)
}

#[tokio::test]
#[ignore = "needs PostgreSQL, run with `just test-postgres-schema`"]
async fn owner_upsert_is_position_guarded_on_postgres() -> Result<()> {
    let scratch = Scratch::create().await?;
    let db = &scratch.db;
    let safe = add_safe(db, 1).await?;
    let owner = address(2);

    // Insert, then update by a newer position.
    db.upsert_safe_owner_state(None, safe, owner, true, 110, 1, 1).await?;
    assert_eq!(db.get_safe_owners(None, safe).await?, vec![owner]);
    db.upsert_safe_owner_state(None, safe, owner, false, 120, 0, 0).await?;
    assert!(db.get_safe_owners(None, safe).await?.is_empty());

    // Older and equal positions are no-ops (a replayed AddedOwner cannot revive the owner).
    db.upsert_safe_owner_state(None, safe, owner, true, 110, 1, 1).await?;
    db.upsert_safe_owner_state(None, safe, owner, true, 120, 0, 0).await?;
    assert!(db.get_safe_owners(None, safe).await?.is_empty());
    // Same block, later transaction index: newer.
    db.upsert_safe_owner_state(None, safe, owner, true, 120, 0, 1).await?;
    assert_eq!(db.get_safe_owners(None, safe).await?, vec![owner]);

    let rows = owner_rows(db, safe).await?;
    assert_eq!(rows.len(), 1, "one row per (safe, owner)");
    assert_eq!(rows[0].published_log_index, 1);

    scratch.drop_database().await
}

#[tokio::test]
#[ignore = "needs PostgreSQL, run with `just test-postgres-schema`"]
async fn threshold_upsert_is_position_guarded_on_postgres() -> Result<()> {
    let scratch = Scratch::create().await?;
    let db = &scratch.db;
    let safe = add_safe(db, 3).await?;

    db.upsert_safe_threshold_state(None, safe, "2".into(), 110, 0, 0)
        .await?;
    assert_eq!(db.get_safe_threshold(None, safe).await?.as_deref(), Some("2"));
    db.upsert_safe_threshold_state(None, safe, "5".into(), 105, 0, 0)
        .await?;
    db.upsert_safe_threshold_state(None, safe, "6".into(), 110, 0, 0)
        .await?;
    assert_eq!(db.get_safe_threshold(None, safe).await?.as_deref(), Some("2"));
    db.upsert_safe_threshold_state(None, safe, "3".into(), 111, 0, 0)
        .await?;
    assert_eq!(db.get_safe_threshold(None, safe).await?.as_deref(), Some("3"));

    scratch.drop_database().await
}

#[tokio::test]
#[ignore = "needs PostgreSQL, run with `just test-postgres-schema`"]
async fn owner_snapshot_flow_works_on_postgres() -> Result<()> {
    let scratch = Scratch::create().await?;
    let db = &scratch.db;
    let safe = add_safe(db, 4).await?;
    let (stale, kept, added) = (address(5), address(6), address(7));

    // A fresh database may already hold preseeded Safes, so only this Safe is looked at.
    assert!(db.get_safes_pending_owner_snapshot(None, 200).await?.contains(&safe));
    assert_eq!(db.get_safe_owners_synced_block(None, safe).await?, None);

    // A live event written before the snapshot, below and above the snapshot block.
    db.upsert_safe_owner_state(None, safe, stale, true, 150, 0, 0).await?;
    db.upsert_safe_owner_state(None, safe, added, true, 205, 0, 0).await?;
    db.upsert_safe_threshold_state(None, safe, "9".into(), 206, 0, 0)
        .await?;

    // Fresh snapshot: the stale owner is flagged as removed, the newer rows are kept.
    db.apply_safe_owner_snapshot(None, safe, vec![kept], "2".into(), 200)
        .await?;
    let mut expected = vec![kept, added];
    expected.sort();
    assert_eq!(db.get_safe_owners(None, safe).await?, expected);
    assert_eq!(db.get_safe_threshold(None, safe).await?.as_deref(), Some("9"));
    assert_eq!(db.get_safe_owners_synced_block(None, safe).await?, Some(200));
    let flagged = owner_rows(db, safe).await?;
    assert!(
        flagged
            .iter()
            .any(|row| row.owner_address == stale.as_ref() && !row.is_current_owner),
        "the owner missing from the snapshot is kept as is_current_owner = false"
    );

    // An older snapshot after a newer one is a no-op.
    db.apply_safe_owner_snapshot(None, safe, vec![stale, kept], "1".into(), 190)
        .await?;
    assert_eq!(db.get_safe_owners(None, safe).await?, expected);
    db.apply_safe_owner_snapshot(None, safe, vec![kept], "3".into(), 210)
        .await?;
    assert_eq!(db.get_safe_owners_synced_block(None, safe).await?, Some(210));

    // Pending listing follows the marker.
    assert!(!db.get_safes_pending_owner_snapshot(None, 210).await?.contains(&safe));
    assert!(db.get_safes_pending_owner_snapshot(None, 211).await?.contains(&safe));

    scratch.drop_database().await
}
