// Allow casts for block numbers stored as i64, converted to u32 (checked elsewhere)
#![allow(clippy::cast_sign_loss, clippy::cast_possible_truncation)]

use async_trait::async_trait;
use blokli_db_entity::{
    account, account_state, announcement,
    prelude::{Account, AccountState, Announcement},
};
use futures::TryFutureExt;
use hopr_crypto_types::prelude::OffchainPublicKey;
use hopr_internal_types::{account::AccountType, prelude::AccountEntry};
use hopr_primitive_types::{errors::GeneralError, prelude::Address, traits::ToHex};
use multiaddr::Multiaddr;
use sea_orm::{
    ActiveModelTrait, ColumnTrait, DbErr, EntityTrait, IntoActiveModel, ModelTrait, QueryFilter, QueryOrder, Set,
    sea_query::Expr,
};
use sea_query::{Condition, OnConflict};
use tracing::{info, instrument};

use crate::{
    BlokliDbGeneralModelOperations, OptTx,
    db::BlokliDb,
    errors::{DbSqlError, DbSqlError::MissingAccount, Result},
};

/// A type that can represent both [chain public key](Address) and [packet public key](OffchainPublicKey).
#[allow(clippy::large_enum_variant)] // TODO: use CompactOffchainPublicKey
#[derive(Copy, Clone, Debug, PartialEq, Eq)]
pub enum ChainOrPacketKey {
    /// Represents [chain public key](Address).
    ChainKey(Address),
    /// Represents [packet public key](OffchainPublicKey).
    PacketKey(OffchainPublicKey),
}

impl From<Address> for ChainOrPacketKey {
    fn from(value: Address) -> Self {
        Self::ChainKey(value)
    }
}

impl From<OffchainPublicKey> for ChainOrPacketKey {
    fn from(value: OffchainPublicKey) -> Self {
        Self::PacketKey(value)
    }
}

impl TryFrom<ChainOrPacketKey> for OffchainPublicKey {
    type Error = GeneralError;

    fn try_from(value: ChainOrPacketKey) -> std::result::Result<Self, Self::Error> {
        match value {
            ChainOrPacketKey::ChainKey(_) => Err(GeneralError::InvalidInput),
            ChainOrPacketKey::PacketKey(k) => Ok(k),
        }
    }
}

impl TryFrom<ChainOrPacketKey> for Address {
    type Error = GeneralError;

    fn try_from(value: ChainOrPacketKey) -> std::result::Result<Self, Self::Error> {
        match value {
            ChainOrPacketKey::ChainKey(k) => Ok(k),
            ChainOrPacketKey::PacketKey(_) => Err(GeneralError::InvalidInput),
        }
    }
}

impl From<ChainOrPacketKey> for Condition {
    fn from(key: ChainOrPacketKey) -> Condition {
        match key {
            ChainOrPacketKey::ChainKey(chain_key) => account::Column::ChainKey.eq(chain_key.as_ref().to_vec()).into(),
            ChainOrPacketKey::PacketKey(packet_key) => account::Column::PacketKey.eq(hex::encode(packet_key)).into(),
        }
    }
}

/// Defines DB API for accessing HOPR accounts and corresponding on-chain announcements.
///
/// Accounts store the Chain and Packet key information, so as the
/// routable network information if the account has been announced as well.
#[async_trait]
pub trait BlokliDbAccountOperations {
    /// Inserts or updates an account atomically with temporal versioning.
    ///
    /// If the account doesn't exist, creates the account identity and initial state.
    /// If the account exists, adds a new state record with the given temporal coordinates.
    ///
    /// # Arguments
    ///
    /// * `tx` - Optional database transaction
    /// * `chain_key` - Chain address (20 bytes)
    /// * `packet_key` - Off-chain public key
    /// * `safe_address` - Optional safe address for the account state
    /// * `block` - Block number where this state was published
    /// * `tx_index` - Transaction index within the block
    /// * `log_index` - Log index within the transaction
    ///
    /// # Temporal Database Semantics
    ///
    /// This function follows temporal database principles:
    /// - Account identity is created once and never modified
    /// - Each state change creates a new immutable record in account_state table
    /// - State records are identified by (account_id, block, tx_index, log_index)
    /// - Historical states are preserved and can be queried
    #[allow(clippy::too_many_arguments)]
    async fn upsert_account<'a>(
        &'a self,
        tx: OptTx<'a>,
        key_id: u32,
        chain_key: Address,
        packet_key: OffchainPublicKey,
        safe_address: Option<Address>,
        block: u32,
        tx_index: u32,
        log_index: u32,
    ) -> Result<()>;

    /// Retrieves the account entry using a Packet key or Chain key (returns latest state).
    async fn get_account<'a, T>(&'a self, tx: OptTx<'a>, key: T) -> Result<Option<AccountEntry>>
    where
        T: Into<ChainOrPacketKey> + Send + Sync;

    /// Retrieves entries of accounts with routable address announcements (if `public_only` is `true`)
    /// or about all accounts without routeable address announcements (if `public_only` is `false`).
    async fn get_accounts<'a>(&'a self, tx: OptTx<'a>, public_only: bool) -> Result<Vec<AccountEntry>>;

    /// Retrieves account state at a specific block height.
    ///
    /// Returns the most recent state that was published at or before the given block.
    /// If no state exists at or before the block, returns None.
    ///
    /// # Arguments
    ///
    /// * `tx` - Optional database transaction
    /// * `key` - Chain or packet key to identify the account
    /// * `block` - Block number to query state at
    async fn get_account_state_at_block<'a, T>(
        &'a self,
        tx: OptTx<'a>,
        key: T,
        block: u32,
    ) -> Result<Option<AccountEntry>>
    where
        T: Into<ChainOrPacketKey> + Send + Sync;

    /// Retrieves complete state history for an account.
    ///
    /// Returns all state records for the account ordered chronologically by
    /// (block, tx_index, log_index) from earliest to latest.
    ///
    /// # Arguments
    ///
    /// * `tx` - Optional database transaction
    /// * `key` - Chain or packet key to identify the account
    async fn get_account_history<'a, T>(&'a self, tx: OptTx<'a>, key: T) -> Result<Vec<AccountEntry>>
    where
        T: Into<ChainOrPacketKey> + Send + Sync;

    /// Inserts a new account entry to the database.
    /// Fails if such an entry already exists.
    ///
    /// **DEPRECATED**: Use `upsert_account` instead for temporal database semantics.
    async fn insert_account<'a>(&'a self, tx: OptTx<'a>, account: AccountEntry) -> Result<()>;

    /// Inserts a routable address announcement linked to a specific entry.
    ///
    /// If an account matching the given `key` (chain or off-chain key) does not exist, an
    /// error is returned.
    /// If such `multiaddr` has been already announced for the given account `key`, only
    /// the `published_at` will be updated on that announcement.
    async fn insert_announcement<'a, T>(
        &'a self,
        tx: OptTx<'a>,
        key: T,
        multiaddr: Multiaddr,
        published_at: u32,
    ) -> Result<AccountEntry>
    where
        T: Into<ChainOrPacketKey> + Send + Sync;

    /// Deletes all address announcements for the given account.
    async fn delete_all_announcements<'a, T>(&'a self, tx: OptTx<'a>, key: T) -> Result<()>
    where
        T: Into<ChainOrPacketKey> + Send + Sync;

    /// Deletes account with the given `key` (chain or off-chain).
    ///
    /// **DEPRECATED**: This violates temporal database principles. Accounts should never be deleted.
    async fn delete_account<'a, T>(&'a self, tx: OptTx<'a>, key: T) -> Result<()>
    where
        T: Into<ChainOrPacketKey> + Send + Sync;

    /// Translates the given Chain or Packet key to its counterpart.
    ///
    /// If `Address` is given as `key`, the result will contain `OffchainPublicKey` if present.
    /// If `OffchainPublicKey` is given as `key`, the result will contain `Address` if present.
    async fn translate_key<'a, T>(&'a self, tx: OptTx<'a>, key: T) -> Result<Option<ChainOrPacketKey>>
    where
        T: Into<ChainOrPacketKey> + Send + Sync;
}

// NOTE: this function currently assumes `announcements` are sorted from latest to earliest
pub(crate) fn model_to_account_entry(
    account: account::Model,
    account_state: Option<blokli_db_entity::account_state::Model>,
    announcements: Vec<announcement::Model>,
) -> Result<AccountEntry> {
    // Currently, we always take only the most recent announcement
    let announcement = announcements.first();

    // Convert Vec<u8> (20 bytes) to Address
    let chain_addr = Address::try_from(account.chain_key.as_slice())?;

    // Extract safe_address from account_state if present
    let safe_address = account_state.and_then(|state| {
        state
            .safe_address
            .map(|addr_bytes| Address::try_from(addr_bytes.as_slice()))
            .transpose()
            .ok()
            .flatten()
    });

    Ok(AccountEntry {
        public_key: OffchainPublicKey::from_hex(&account.packet_key)?,
        chain_addr,
        entry_type: match announcement {
            None => AccountType::NotAnnounced,
            Some(a) => AccountType::Announced(vec![a.multiaddress.parse().map_err(|_| DbSqlError::DecodingError)?]),
        },
        safe_address,
        key_id: (account.id as u32).into(),
    })
}

#[async_trait]
impl BlokliDbAccountOperations for BlokliDb {
    async fn upsert_account<'a>(
        &'a self,
        tx: OptTx<'a>,
        key_id: u32,
        chain_key: Address,
        packet_key: OffchainPublicKey,
        safe_address: Option<Address>,
        block: u32,
        tx_index: u32,
        log_index: u32,
    ) -> Result<()> {
        // Convert u32 to i64 for database storage
        let block_i64 = block as i64;
        let tx_index_i64 = tx_index as i64;
        let log_index_i64 = log_index as i64;

        self.nest_transaction(tx)
            .await?
            .perform(|tx| {
                Box::pin(async move {
                    // Step 1: Insert or find account identity record
                    let account_id = if let Some(existing_account) = account::Entity::find()
                        .filter(account::Column::ChainKey.eq(chain_key.as_ref().to_vec()))
                        .one(tx.as_ref())
                        .await?
                    {
                        // Account exists - we're adding a new state
                        existing_account.id
                    } else {
                        // Account doesn't exist - create identity + initial state
                        let account_model = account::ActiveModel {
                            id: Set(key_id as i64),
                            chain_key: Set(chain_key.as_ref().to_vec()),
                            packet_key: Set(hex::encode(packet_key.as_ref())),
                            published_block: Set(block_i64),
                            published_tx_index: Set(tx_index_i64),
                            published_log_index: Set(log_index_i64),
                        };
                        let inserted = account_model.insert(tx.as_ref()).await?;
                        inserted.id
                    };

                    // Step 2: Insert account_state record with state information
                    // This creates a new immutable state record (never updates existing records)
                    let state_model = account_state::ActiveModel {
                        account_id: Set(account_id),
                        safe_address: Set(safe_address.map(|a| a.as_ref().to_vec())),
                        published_block: Set(block_i64),
                        published_tx_index: Set(tx_index_i64),
                        published_log_index: Set(log_index_i64),
                        ..Default::default()
                    };

                    AccountState::insert(state_model).exec(tx.as_ref()).await?;

                    Ok::<_, DbSqlError>(())
                })
            })
            .await?;

        Ok(())
    }

    async fn get_account<'a, T: Into<ChainOrPacketKey> + Send + Sync>(
        &'a self,
        tx: OptTx<'a>,
        key: T,
    ) -> Result<Option<AccountEntry>> {
        let cpk = key.into();
        self.nest_transaction(tx)
            .await?
            .perform(|tx| {
                Box::pin(async move {
                    // Step 1: Find account by key
                    let account_model = match Account::find().filter(cpk).one(tx.as_ref()).await? {
                        Some(a) => a,
                        None => return Ok(None),
                    };

                    // Step 2: Get the most recent account_state (latest by block, tx_index, log_index)
                    let state_model = AccountState::find()
                        .filter(account_state::Column::AccountId.eq(account_model.id))
                        .order_by_desc(account_state::Column::PublishedBlock)
                        .order_by_desc(account_state::Column::PublishedTxIndex)
                        .order_by_desc(account_state::Column::PublishedLogIndex)
                        .one(tx.as_ref())
                        .await?;

                    // Step 3: Get announcements for account entry
                    let announcements = Announcement::find()
                        .filter(announcement::Column::AccountId.eq(account_model.id))
                        .order_by_desc(announcement::Column::PublishedBlock)
                        .all(tx.as_ref())
                        .await?;

                    // Step 4: Convert to AccountEntry
                    Ok::<_, DbSqlError>(Some(model_to_account_entry(account_model, state_model, announcements)?))
                })
            })
            .await
    }

    async fn get_accounts<'a>(&'a self, tx: OptTx<'a>, public_only: bool) -> Result<Vec<AccountEntry>> {
        self.nest_transaction(tx)
            .await?
            .perform(|tx| {
                Box::pin(async move {
                    let accounts = Account::find()
                        .find_with_related(Announcement)
                        .filter(if public_only {
                            announcement::Column::Multiaddress.ne("")
                        } else {
                            Expr::value(1)
                        })
                        .order_by_desc(announcement::Column::PublishedBlock)
                        .all(tx.as_ref())
                        .await?;

                    let mut entries = Vec::new();
                    for (account_model, announcements) in accounts {
                        // Get the most recent account_state for each account
                        let state_model = AccountState::find()
                            .filter(account_state::Column::AccountId.eq(account_model.id))
                            .order_by_desc(account_state::Column::PublishedBlock)
                            .order_by_desc(account_state::Column::PublishedTxIndex)
                            .order_by_desc(account_state::Column::PublishedLogIndex)
                            .one(tx.as_ref())
                            .await?;

                        entries.push(model_to_account_entry(account_model, state_model, announcements)?);
                    }
                    Ok::<_, DbSqlError>(entries)
                })
            })
            .await
    }

    async fn insert_account<'a>(&'a self, tx: OptTx<'a>, account: AccountEntry) -> Result<()> {
        let myself = self.clone();
        self.nest_transaction(tx)
            .await?
            .perform(|tx| {
                Box::pin(async move {
                    match account::Entity::insert(account::ActiveModel {
                        chain_key: Set(account.chain_addr.as_ref().to_vec()),
                        packet_key: Set(hex::encode(account.public_key)),
                        published_block: Set(0), // Default since AccountEntry no longer tracks this
                        ..Default::default()
                    })
                    .on_conflict(
                        OnConflict::columns([account::Column::ChainKey, account::Column::PacketKey])
                            .do_nothing()
                            .to_owned(),
                    )
                    .exec(tx.as_ref())
                    .await
                    {
                        // Proceed if succeeded or already exists
                        res @ Ok(_) | res @ Err(DbErr::RecordNotInserted) => {
                            // Update key-id binding only if the account was inserted successfully
                            // (= not re-announced)
                            if res.is_ok() {
                                // FIXME: update key id binding
                                // tracing::warn!(?account, %error, "keybinding not updated")
                            }

                            if let AccountType::Announced(multiaddrs) = &account.entry_type {
                                // Insert announcements for all multiaddrs
                                for multiaddr in multiaddrs {
                                    myself
                                        .insert_announcement(
                                            Some(tx),
                                            account.chain_addr,
                                            multiaddr.clone(),
                                            0, // Default since AccountEntry no longer tracks block number
                                        )
                                        .await?;
                                }
                            }

                            Ok::<(), DbSqlError>(())
                        }
                        Err(e) => Err(e.into()),
                    }
                })
            })
            .await
    }

    async fn insert_announcement<'a, T>(
        &'a self,
        tx: OptTx<'a>,
        key: T,
        multiaddr: Multiaddr,
        published_at: u32,
    ) -> Result<AccountEntry>
    where
        T: Into<ChainOrPacketKey> + Send + Sync,
    {
        let cpk = key.into();
        self.nest_transaction(tx)
            .await?
            .perform(|tx| {
                Box::pin(async move {
                    let (existing_account, mut existing_announcements) = account::Entity::find()
                        .find_with_related(announcement::Entity)
                        .filter(cpk)
                        .order_by_desc(announcement::Column::PublishedBlock)
                        .all(tx.as_ref())
                        .await?
                        .pop()
                        .ok_or(MissingAccount)?;

                    if let Some((index, _)) = existing_announcements
                        .iter()
                        .enumerate()
                        .find(|(_, announcement)| announcement.multiaddress == multiaddr.to_string())
                    {
                        let mut existing_announcement = existing_announcements.remove(index).into_active_model();
                        existing_announcement.published_block = Set(published_at as i64);
                        let updated_announcement = existing_announcement.update(tx.as_ref()).await?;

                        // To maintain the sort order, insert at the original location
                        existing_announcements.insert(index, updated_announcement);
                    } else {
                        let new_announcement = announcement::ActiveModel {
                            account_id: Set(existing_account.id),
                            multiaddress: Set(multiaddr.to_string()),
                            published_block: Set(published_at as i64),
                            ..Default::default()
                        }
                        .insert(tx.as_ref())
                        .await?;

                        // Assuming this is the latest announcement, so prepend it
                        existing_announcements.insert(0, new_announcement);
                    }

                    // Get the most recent account_state for the account
                    let state_model = AccountState::find()
                        .filter(account_state::Column::AccountId.eq(existing_account.id))
                        .order_by_desc(account_state::Column::PublishedBlock)
                        .order_by_desc(account_state::Column::PublishedTxIndex)
                        .order_by_desc(account_state::Column::PublishedLogIndex)
                        .one(tx.as_ref())
                        .await?;

                    model_to_account_entry(existing_account, state_model, existing_announcements)
                })
            })
            .await
    }

    async fn delete_all_announcements<'a, T>(&'a self, tx: OptTx<'a>, key: T) -> Result<()>
    where
        T: Into<ChainOrPacketKey> + Send + Sync,
    {
        let cpk = key.into();
        self.nest_transaction(tx)
            .await?
            .perform(|tx| {
                Box::pin(async move {
                    // First find the account
                    let account_opt = Account::find().filter(cpk).one(tx.as_ref()).await?;

                    if let Some(account_model) = account_opt {
                        // Find and delete all related announcements
                        let to_delete: Vec<i64> = Announcement::find()
                            .filter(announcement::Column::AccountId.eq(account_model.id))
                            .all(tx.as_ref())
                            .await?
                            .into_iter()
                            .map(|x: announcement::Model| x.id)
                            .collect();

                        if !to_delete.is_empty() {
                            announcement::Entity::delete_many()
                                .filter(announcement::Column::Id.is_in(to_delete))
                                .exec(tx.as_ref())
                                .await?;
                        }

                        Ok::<_, DbSqlError>(())
                    } else {
                        Err(MissingAccount)
                    }
                })
            })
            .await
    }

    async fn delete_account<'a, T>(&'a self, tx: OptTx<'a>, key: T) -> Result<()>
    where
        T: Into<ChainOrPacketKey> + Send + Sync,
    {
        let _myself = self.clone();
        let cpk = key.into();
        self.nest_transaction(tx)
            .await?
            .perform(|tx| {
                Box::pin(async move {
                    if let Some(entry) = account::Entity::find().filter(cpk).one(tx.as_ref()).await? {
                        let _account_entry = model_to_account_entry(entry.clone(), None, vec![])?;
                        entry.delete(tx.as_ref()).await?;

                        Ok::<_, DbSqlError>(())
                    } else {
                        Err(MissingAccount)
                    }
                })
            })
            .await
    }

    #[instrument(level = "trace", skip_all, err)]
    async fn translate_key<'a, T: Into<ChainOrPacketKey> + Send + Sync>(
        &'a self,
        tx: OptTx<'a>,
        key: T,
    ) -> Result<Option<ChainOrPacketKey>> {
        Ok(match key.into() {
            ChainOrPacketKey::ChainKey(chain_key) => self
                .nest_transaction(tx)
                .and_then(|op| {
                    op.perform(|tx| {
                        Box::pin(async move {
                            let maybe_model = Account::find()
                                .filter(account::Column::ChainKey.eq(chain_key.as_ref().to_vec()))
                                .one(tx.as_ref())
                                .await?;
                            info!(?maybe_model, "found model for chain key");
                            if let Some(m) = maybe_model {
                                Ok(Some(OffchainPublicKey::from_hex(&m.packet_key)?))
                            } else {
                                Ok(None)
                            }
                        })
                    })
                })
                .await?
                .map(ChainOrPacketKey::PacketKey),
            ChainOrPacketKey::PacketKey(packet_key) => self
                .nest_transaction(tx)
                .and_then(|op| {
                    op.perform(|tx| {
                        Box::pin(async move {
                            let maybe_model = Account::find()
                                .filter(account::Column::PacketKey.eq(hex::encode(packet_key.as_ref())))
                                .one(tx.as_ref())
                                .await?;
                            if let Some(m) = maybe_model {
                                // Convert Vec<u8> (20 bytes) to Address
                                Ok(Some(Address::try_from(m.chain_key.as_slice())?))
                            } else {
                                Ok(None)
                            }
                        })
                    })
                })
                .await?
                .map(ChainOrPacketKey::ChainKey),
        })
    }

    async fn get_account_state_at_block<'a, T: Into<ChainOrPacketKey> + Send + Sync>(
        &'a self,
        tx: OptTx<'a>,
        key: T,
        block: u32,
    ) -> Result<Option<AccountEntry>> {
        let cpk = key.into();
        let block_i64 = block as i64;

        self.nest_transaction(tx)
            .await?
            .perform(|tx| {
                Box::pin(async move {
                    // Step 1: Find account by key
                    let account_model = match Account::find().filter(cpk).one(tx.as_ref()).await? {
                        Some(a) => a,
                        None => return Ok(None),
                    };

                    // Step 2: Get the most recent account_state at or before the given block
                    let state_model = AccountState::find()
                        .filter(account_state::Column::AccountId.eq(account_model.id))
                        .filter(account_state::Column::PublishedBlock.lte(block_i64))
                        .order_by_desc(account_state::Column::PublishedBlock)
                        .order_by_desc(account_state::Column::PublishedTxIndex)
                        .order_by_desc(account_state::Column::PublishedLogIndex)
                        .one(tx.as_ref())
                        .await?;

                    // If no state exists at or before this block, return None
                    if state_model.is_none() {
                        return Ok(None);
                    }

                    // Step 3: Get announcements for account entry
                    let announcements = Announcement::find()
                        .filter(announcement::Column::AccountId.eq(account_model.id))
                        .order_by_desc(announcement::Column::PublishedBlock)
                        .all(tx.as_ref())
                        .await?;

                    // Step 4: Convert to AccountEntry
                    Ok::<_, DbSqlError>(Some(model_to_account_entry(account_model, state_model, announcements)?))
                })
            })
            .await
    }

    async fn get_account_history<'a, T: Into<ChainOrPacketKey> + Send + Sync>(
        &'a self,
        tx: OptTx<'a>,
        key: T,
    ) -> Result<Vec<AccountEntry>> {
        let cpk = key.into();

        self.nest_transaction(tx)
            .await?
            .perform(|tx| {
                Box::pin(async move {
                    // Step 1: Find account by key
                    let account_model = match Account::find().filter(cpk).one(tx.as_ref()).await? {
                        Some(a) => a,
                        None => return Ok(vec![]),
                    };

                    // Step 2: Get all account_state records ordered chronologically
                    let state_models = AccountState::find()
                        .filter(account_state::Column::AccountId.eq(account_model.id))
                        .order_by_asc(account_state::Column::PublishedBlock)
                        .order_by_asc(account_state::Column::PublishedTxIndex)
                        .order_by_asc(account_state::Column::PublishedLogIndex)
                        .all(tx.as_ref())
                        .await?;

                    // Step 3: Get announcements once
                    let announcements = Announcement::find()
                        .filter(announcement::Column::AccountId.eq(account_model.id))
                        .order_by_desc(announcement::Column::PublishedBlock)
                        .all(tx.as_ref())
                        .await?;

                    // Step 4: Convert each state to AccountEntry
                    // Note: We use the same announcements for all states
                    // In a temporal design, we'd want to filter announcements by state's block
                    let mut history = Vec::new();
                    for state_model in state_models {
                        let entry =
                            model_to_account_entry(account_model.clone(), Some(state_model), announcements.clone())?;
                        history.push(entry);
                    }

                    Ok::<_, DbSqlError>(history)
                })
            })
            .await
    }
}

#[cfg(test)]
mod tests {
    use anyhow::Context;
    use hopr_crypto_types::prelude::{ChainKeypair, Keypair, OffchainKeypair};
    use hopr_internal_types::prelude::AccountType;

    use super::*;
    use crate::{
        BlokliDbGeneralModelOperations,
        errors::{DbSqlError, DbSqlError::DecodingError},
    };

    #[tokio::test]
    async fn test_insert_account_announcement() -> anyhow::Result<()> {
        let db = BlokliDb::new_in_memory().await?;

        let chain_1 = ChainKeypair::random().public().to_address();
        let packet_1 = *OffchainKeypair::random().public();

        db.insert_account(
            None,
            AccountEntry {
                public_key: packet_1,
                chain_addr: chain_1,
                entry_type: AccountType::NotAnnounced,
                safe_address: None,
                key_id: 0.into(),
            },
        )
        .await?;

        let acc = db.get_account(None, chain_1).await?.expect("should contain account");
        assert_eq!(packet_1, acc.public_key, "pub keys must match");
        assert_eq!(AccountType::NotAnnounced, acc.entry_type.clone());

        let maddr: Multiaddr = "/ip4/1.2.3.4/tcp/8000".parse()?;
        let block = 100;

        let db_acc = db.insert_announcement(None, chain_1, maddr.clone(), block).await?;

        let acc = db.get_account(None, chain_1).await?.context("should contain account")?;
        assert_eq!(Some(&maddr), acc.get_multiaddrs().first(), "multiaddress must match");
        assert_eq!(acc, db_acc);

        let block = 200;
        let db_acc = db.insert_announcement(None, chain_1, maddr.clone(), block).await?;

        let acc = db.get_account(None, chain_1).await?.expect("should contain account");
        assert_eq!(Some(&maddr), acc.get_multiaddrs().first(), "multiaddress must match");
        assert_eq!(acc, db_acc);

        let maddr: Multiaddr = "/dns4/useful.domain/tcp/56".parse()?;
        let block = 300;
        let db_acc = db.insert_announcement(None, chain_1, maddr.clone(), block).await?;

        let acc = db.get_account(None, chain_1).await?.expect("should contain account");
        assert_eq!(Some(&maddr), acc.get_multiaddrs().first(), "multiaddress must match");
        assert_eq!(acc, db_acc);

        Ok(())
    }

    #[tokio::test]
    async fn test_should_allow_reannouncement() -> anyhow::Result<()> {
        let db = BlokliDb::new_in_memory().await?;

        let chain_1 = ChainKeypair::random().public().to_address();
        let packet_1 = *OffchainKeypair::random().public();

        db.insert_account(
            None,
            AccountEntry {
                public_key: packet_1,
                chain_addr: chain_1,
                entry_type: AccountType::NotAnnounced,
                safe_address: None,
                key_id: 0.into(),
            },
        )
        .await?;

        db.insert_announcement(None, chain_1, "/ip4/1.2.3.4/tcp/8000".parse()?, 100)
            .await?;

        let ae = db.get_account(None, chain_1).await?.ok_or(MissingAccount)?;

        assert_eq!(
            "/ip4/1.2.3.4/tcp/8000",
            ae.get_multiaddrs().first().expect("has multiaddress").to_string()
        );

        db.insert_announcement(None, chain_1, "/ip4/1.2.3.4/tcp/8001".parse()?, 110)
            .await?;

        let ae = db.get_account(None, chain_1).await?.ok_or(MissingAccount)?;

        assert_eq!(
            "/ip4/1.2.3.4/tcp/8001",
            ae.get_multiaddrs().first().expect("has multiaddress").to_string()
        );

        Ok(())
    }

    #[tokio::test]
    async fn test_should_not_insert_account_announcement_to_nonexisting_account() -> anyhow::Result<()> {
        let db = BlokliDb::new_in_memory().await?;

        let chain_1 = ChainKeypair::random().public().to_address();

        let maddr: Multiaddr = "/ip4/1.2.3.4/tcp/8000".parse()?;
        let block = 100;

        let r = db.insert_announcement(None, chain_1, maddr.clone(), block).await;
        assert!(
            matches!(r, Err(MissingAccount)),
            "should not insert announcement to non-existing account"
        );

        Ok(())
    }

    #[tokio::test]
    async fn test_should_allow_duplicate_announcement_per_different_accounts() -> anyhow::Result<()> {
        let db = BlokliDb::new_in_memory().await?;

        let chain_1 = ChainKeypair::random().public().to_address();
        let packet_1 = *OffchainKeypair::random().public();

        db.insert_account(
            None,
            AccountEntry {
                public_key: packet_1,
                chain_addr: chain_1,
                entry_type: AccountType::NotAnnounced,
                safe_address: None,
                key_id: 0.into(),
            },
        )
        .await?;

        let chain_2 = ChainKeypair::random().public().to_address();
        let packet_2 = *OffchainKeypair::random().public();

        db.insert_account(
            None,
            AccountEntry {
                public_key: packet_2,
                chain_addr: chain_2,
                entry_type: AccountType::NotAnnounced,
                safe_address: None,
                key_id: 0.into(),
            },
        )
        .await?;

        let maddr: Multiaddr = "/ip4/1.2.3.4/tcp/8000".parse()?;
        let block = 100;

        let db_acc_1 = db.insert_announcement(None, chain_1, maddr.clone(), block).await?;
        let db_acc_2 = db.insert_announcement(None, chain_2, maddr.clone(), block).await?;

        let acc = db.get_account(None, chain_1).await?.expect("should contain account");
        assert_eq!(Some(&maddr), acc.get_multiaddrs().first(), "multiaddress must match");
        assert_eq!(acc, db_acc_1);

        let acc = db.get_account(None, chain_2).await?.expect("should contain account");
        assert_eq!(Some(&maddr), acc.get_multiaddrs().first(), "multiaddress must match");
        assert_eq!(acc, db_acc_2);

        Ok(())
    }

    #[tokio::test]
    async fn test_delete_account() -> anyhow::Result<()> {
        let db = BlokliDb::new_in_memory().await?;

        let packet_1 = *OffchainKeypair::random().public();
        let chain_1 = ChainKeypair::random().public().to_address();
        db.insert_account(
            None,
            AccountEntry {
                public_key: packet_1,
                chain_addr: chain_1,
                entry_type: AccountType::Announced(vec!["/ip4/1.2.3.4/tcp/1234".parse()?]),
                safe_address: None,
                key_id: 0.into(),
            },
        )
        .await?;

        assert!(db.get_account(None, chain_1).await?.is_some());

        db.delete_account(None, chain_1).await?;

        assert!(db.get_account(None, chain_1).await?.is_none());

        Ok(())
    }

    #[tokio::test]
    async fn test_should_fail_to_delete_nonexistent_account() -> anyhow::Result<()> {
        let db = BlokliDb::new_in_memory().await?;

        let chain_1 = ChainKeypair::random().public().to_address();

        let r = db.delete_account(None, chain_1).await;
        assert!(
            matches!(r, Err(MissingAccount)),
            "should not delete non-existing account"
        );

        Ok(())
    }

    #[tokio::test]
    async fn test_should_not_fail_on_duplicate_account_insert() -> anyhow::Result<()> {
        let db = BlokliDb::new_in_memory().await?;

        let chain_1 = ChainKeypair::random().public().to_address();
        let packet_1 = *OffchainKeypair::random().public();

        db.insert_account(
            None,
            AccountEntry {
                public_key: packet_1,
                chain_addr: chain_1,
                entry_type: AccountType::NotAnnounced,
                safe_address: None,
                key_id: 1.into(),
            },
        )
        .await?;

        db.insert_account(
            None,
            AccountEntry {
                public_key: packet_1,
                chain_addr: chain_1,
                entry_type: AccountType::NotAnnounced,
                safe_address: None,
                key_id: 2.into(),
            },
        )
        .await?;

        Ok(())
    }

    #[tokio::test]
    async fn test_delete_announcements() -> anyhow::Result<()> {
        let db = BlokliDb::new_in_memory().await?;

        let packet_1 = *OffchainKeypair::random().public();
        let chain_1 = ChainKeypair::random().public().to_address();
        let entry = AccountEntry {
            public_key: packet_1,
            chain_addr: chain_1,
            entry_type: AccountType::Announced(vec!["/ip4/1.2.3.4/tcp/1234".parse()?]),
            safe_address: None,
            key_id: 1.into(),
        };

        db.insert_account(None, entry.clone()).await?;

        // Get the actual account with the correct key_id
        let entry_with_key_id = db.get_account(None, chain_1).await?.expect("account should exist");
        assert_eq!(packet_1, entry_with_key_id.public_key);
        assert_eq!(chain_1, entry_with_key_id.chain_addr);
        assert_eq!(
            AccountType::Announced(vec!["/ip4/1.2.3.4/tcp/1234".parse()?]),
            entry_with_key_id.entry_type
        );
        assert_eq!(entry_with_key_id.key_id, entry.key_id);
        db.delete_all_announcements(None, chain_1).await?;

        // After deleting announcements, the entry should still have the same key_id but NotAnnounced type
        let entry_after_delete = db.get_account(None, chain_1).await?.expect("account should exist");
        assert_eq!(packet_1, entry_after_delete.public_key);
        assert_eq!(chain_1, entry_after_delete.chain_addr);
        assert_eq!(AccountType::NotAnnounced, entry_after_delete.entry_type);
        assert_eq!(entry_with_key_id.key_id, entry_after_delete.key_id);

        Ok(())
    }

    #[tokio::test]
    async fn test_should_fail_to_delete_nonexistent_account_announcements() -> anyhow::Result<()> {
        let db = BlokliDb::new_in_memory().await?;

        let chain_1 = ChainKeypair::random().public().to_address();

        let r = db.delete_all_announcements(None, chain_1).await;
        assert!(
            matches!(r, Err(MissingAccount)),
            "should not delete non-existing account"
        );

        Ok(())
    }

    #[tokio::test]
    async fn test_translate_key() -> anyhow::Result<()> {
        let db = BlokliDb::new_in_memory().await?;

        let chain_1 = ChainKeypair::random().public().to_address();
        let packet_1 = *OffchainKeypair::random().public();

        let chain_2 = ChainKeypair::random().public().to_address();
        let packet_2 = *OffchainKeypair::random().public();

        let db_clone = db.clone();
        db.begin_transaction()
            .await?
            .perform(|tx| {
                Box::pin(async move {
                    db_clone
                        .insert_account(
                            tx.into(),
                            AccountEntry {
                                public_key: packet_1,
                                chain_addr: chain_1,
                                entry_type: AccountType::NotAnnounced,
                                safe_address: None,
                                key_id: 2.into(),
                            },
                        )
                        .await?;
                    db_clone
                        .insert_account(
                            tx.into(),
                            AccountEntry {
                                public_key: packet_2,
                                chain_addr: chain_2,
                                entry_type: AccountType::NotAnnounced,
                                safe_address: None,
                                key_id: 3.into(),
                            },
                        )
                        .await?;
                    Ok::<(), DbSqlError>(())
                })
            })
            .await?;

        let a: Address = db
            .translate_key(None, packet_1)
            .await?
            .context("must contain first key")?
            .try_into()?;

        let b: OffchainPublicKey = db
            .translate_key(None, chain_2)
            .await?
            .context("must contain second key")?
            .try_into()?;

        assert_eq!(chain_1, a, "chain keys must match");
        assert_eq!(packet_2, b, "chain keys must match");

        Ok(())
    }

    #[tokio::test]
    async fn test_get_accounts() -> anyhow::Result<()> {
        let db = BlokliDb::new_in_memory().await?;

        let chain_1 = ChainKeypair::random().public().to_address();
        let chain_2 = ChainKeypair::random().public().to_address();
        let chain_3 = ChainKeypair::random().public().to_address();

        let db_clone = db.clone();
        db.begin_transaction()
            .await?
            .perform(|tx| {
                Box::pin(async move {
                    db_clone
                        .insert_account(
                            Some(tx),
                            AccountEntry {
                                public_key: *OffchainKeypair::random().public(),
                                chain_addr: chain_1,
                                entry_type: AccountType::NotAnnounced,
                                safe_address: None,
                                key_id: 2.into(),
                            },
                        )
                        .await?;
                    db_clone
                        .insert_account(
                            Some(tx),
                            AccountEntry {
                                public_key: *OffchainKeypair::random().public(),
                                chain_addr: chain_2,
                                entry_type: AccountType::Announced(vec![
                                    "/ip4/10.10.10.10/tcp/1234".parse().map_err(|_| DecodingError)?,
                                ]),
                                safe_address: None,
                                key_id: 3.into(),
                            },
                        )
                        .await?;
                    db_clone
                        .insert_account(
                            Some(tx),
                            AccountEntry {
                                public_key: *OffchainKeypair::random().public(),
                                chain_addr: chain_3,
                                entry_type: AccountType::NotAnnounced,
                                safe_address: None,
                                key_id: 4.into(),
                            },
                        )
                        .await?;

                    db_clone
                        .insert_announcement(
                            Some(tx),
                            chain_3,
                            "/ip4/1.2.3.4/tcp/1234".parse().map_err(|_| DecodingError)?,
                            12,
                        )
                        .await?;
                    db_clone
                        .insert_announcement(
                            Some(tx),
                            chain_3,
                            "/ip4/8.8.1.1/tcp/1234".parse().map_err(|_| DecodingError)?,
                            15,
                        )
                        .await?;
                    db_clone
                        .insert_announcement(
                            Some(tx),
                            chain_3,
                            "/ip4/1.2.3.0/tcp/234".parse().map_err(|_| DecodingError)?,
                            14,
                        )
                        .await
                })
            })
            .await?;

        let all_accounts = db.get_accounts(None, false).await?;
        let public_only = db.get_accounts(None, true).await?;

        assert_eq!(3, all_accounts.len());

        assert_eq!(2, public_only.len());
        let acc_1 = public_only
            .iter()
            .find(|a| a.chain_addr.eq(&chain_2))
            .expect("should contain 1");

        let acc_2 = public_only
            .iter()
            .find(|a| a.chain_addr.eq(&chain_3))
            .expect("should contain 2");

        assert_eq!(
            "/ip4/10.10.10.10/tcp/1234",
            acc_1
                .get_multiaddrs()
                .first()
                .expect("should have a multiaddress")
                .to_string()
        );
        assert_eq!(
            "/ip4/8.8.1.1/tcp/1234",
            acc_2
                .get_multiaddrs()
                .first()
                .expect("should have a multiaddress")
                .to_string()
        );

        Ok(())
    }

    // ============================================================================
    // TDD Tests for Temporal Database Operations
    // ============================================================================

    #[tokio::test]
    async fn test_upsert_account_creates_new_with_initial_state() -> anyhow::Result<()> {
        let db = BlokliDb::new_in_memory().await?;

        let chain_key = ChainKeypair::random().public().to_address();
        let packet_key = *OffchainKeypair::random().public();
        let safe_addr = Address::from(hopr_crypto_random::random_bytes());

        // Create account with upsert at block 100
        db.upsert_account(None, 1, chain_key, packet_key, Some(safe_addr), 100, 0, 0)
            .await?;

        // Verify account can be retrieved with latest state
        let retrieved = db.get_account(None, chain_key).await?.expect("account should exist");

        assert_eq!(chain_key, retrieved.chain_addr);
        assert_eq!(packet_key, retrieved.public_key);

        Ok(())
    }

    #[tokio::test]
    async fn test_upsert_account_adds_state_to_existing() -> anyhow::Result<()> {
        let db = BlokliDb::new_in_memory().await?;

        let chain_key = ChainKeypair::random().public().to_address();
        let packet_key = *OffchainKeypair::random().public();
        let safe_addr_1 = Address::from(hopr_crypto_random::random_bytes());
        let safe_addr_2 = Address::from(hopr_crypto_random::random_bytes());

        // Create account with initial state at block 100
        db.upsert_account(None, 1, chain_key, packet_key, Some(safe_addr_1), 100, 0, 0)
            .await?;

        // Update account with new safe address at block 200
        db.upsert_account(None, 1, chain_key, packet_key, Some(safe_addr_2), 200, 0, 0)
            .await?;

        // Verify latest state has new safe address
        let latest = db.get_account(None, chain_key).await?.expect("account should exist");
        assert_eq!(chain_key, latest.chain_addr);
        assert_eq!(packet_key, latest.public_key);

        // Verify old state still exists at block 100 (via temporal query)
        let historical = db
            .get_account_state_at_block(None, chain_key, 150)
            .await?
            .expect("historical state should exist");
        assert_eq!(chain_key, historical.chain_addr);

        // Verify full history contains both states
        let history = db.get_account_history(None, chain_key).await?;
        assert_eq!(2, history.len(), "should have 2 state records");

        Ok(())
    }

    #[tokio::test]
    async fn test_upsert_account_safe_address_changes() -> anyhow::Result<()> {
        let db = BlokliDb::new_in_memory().await?;

        let chain_key = ChainKeypair::random().public().to_address();
        let packet_key = *OffchainKeypair::random().public();

        // State 1: No safe address at block 100
        db.upsert_account(None, 1, chain_key, packet_key, None, 100, 0, 0)
            .await?;

        // State 2: Add safe address at block 200
        let safe_addr = Address::from(hopr_crypto_random::random_bytes());
        db.upsert_account(None, 1, chain_key, packet_key, Some(safe_addr), 200, 0, 0)
            .await?;

        // State 3: Change safe address at block 300
        let safe_addr_2 = Address::from(hopr_crypto_random::random_bytes());
        db.upsert_account(None, 1, chain_key, packet_key, Some(safe_addr_2), 300, 0, 0)
            .await?;

        // Verify full history has all 3 states
        let history = db.get_account_history(None, chain_key).await?;
        assert_eq!(3, history.len(), "should have 3 state records");

        Ok(())
    }

    #[tokio::test]
    async fn test_get_account_state_at_block() -> anyhow::Result<()> {
        let db = BlokliDb::new_in_memory().await?;

        let chain_key = ChainKeypair::random().public().to_address();
        let packet_key = *OffchainKeypair::random().public();

        // Create multiple states at different blocks
        db.upsert_account(None, 1, chain_key, packet_key, None, 100, 0, 0)
            .await?;

        let safe_addr_1 = Address::from(hopr_crypto_random::random_bytes());
        db.upsert_account(None, 1, chain_key, packet_key, Some(safe_addr_1), 200, 0, 0)
            .await?;

        let safe_addr_2 = Address::from(hopr_crypto_random::random_bytes());
        db.upsert_account(None, 1, chain_key, packet_key, Some(safe_addr_2), 300, 0, 0)
            .await?;

        // Query state at various blocks
        let at_50 = db.get_account_state_at_block(None, chain_key, 50).await?;
        assert!(at_50.is_none(), "no state should exist before block 100");

        let at_100 = db
            .get_account_state_at_block(None, chain_key, 100)
            .await?
            .expect("state should exist at block 100");
        assert_eq!(chain_key, at_100.chain_addr);

        let at_150 = db
            .get_account_state_at_block(None, chain_key, 150)
            .await?
            .expect("state should exist at block 150");
        assert_eq!(chain_key, at_150.chain_addr);

        let at_250 = db
            .get_account_state_at_block(None, chain_key, 250)
            .await?
            .expect("state should exist at block 250");
        assert_eq!(chain_key, at_250.chain_addr);

        Ok(())
    }

    #[tokio::test]
    async fn test_get_account_history() -> anyhow::Result<()> {
        let db = BlokliDb::new_in_memory().await?;

        let chain_key = ChainKeypair::random().public().to_address();
        let packet_key = *OffchainKeypair::random().public();

        // Create 5 state changes across different blocks
        for i in 0..5 {
            let safe_addr = if i % 2 == 0 {
                Some(Address::from(hopr_crypto_random::random_bytes()))
            } else {
                None
            };
            db.upsert_account(None, 1, chain_key, packet_key, safe_addr, (i + 1) * 100, 0, 0)
                .await?;
        }

        // Get full history
        let history = db.get_account_history(None, chain_key).await?;

        assert_eq!(5, history.len(), "should have 5 state records");

        // Verify chronological ordering - all should have same chain/packet keys
        for state in &history {
            assert_eq!(chain_key, state.chain_addr);
            assert_eq!(packet_key, state.public_key);
        }

        Ok(())
    }

    #[tokio::test]
    async fn test_get_account_returns_latest_state() -> anyhow::Result<()> {
        let db = BlokliDb::new_in_memory().await?;

        let chain_key = ChainKeypair::random().public().to_address();
        let packet_key = *OffchainKeypair::random().public();

        // Create multiple states
        db.upsert_account(None, 1, chain_key, packet_key, None, 100, 0, 0)
            .await?;
        db.upsert_account(None, 1, chain_key, packet_key, None, 200, 0, 0)
            .await?;
        let latest_safe = Address::from(hopr_crypto_random::random_bytes());
        db.upsert_account(None, 1, chain_key, packet_key, Some(latest_safe), 300, 0, 0)
            .await?;

        // get_account should return the latest state
        let account = db.get_account(None, chain_key).await?.expect("account should exist");
        assert_eq!(chain_key, account.chain_addr);
        assert_eq!(packet_key, account.public_key);

        Ok(())
    }

    #[tokio::test]
    async fn test_translate_key_still_works() -> anyhow::Result<()> {
        let db = BlokliDb::new_in_memory().await?;

        let chain_key = ChainKeypair::random().public().to_address();
        let packet_key = *OffchainKeypair::random().public();

        // Create account with upsert
        db.upsert_account(None, 1, chain_key, packet_key, None, 100, 0, 0)
            .await?;

        // Verify translate_key works in both directions
        let translated_packet: OffchainPublicKey = db
            .translate_key(None, chain_key)
            .await?
            .context("should find packet key")?
            .try_into()?;
        assert_eq!(packet_key, translated_packet);

        let translated_chain: Address = db
            .translate_key(None, packet_key)
            .await?
            .context("should find chain key")?
            .try_into()?;
        assert_eq!(chain_key, translated_chain);

        Ok(())
    }

    #[tokio::test]
    async fn test_concurrent_account_upserts() -> anyhow::Result<()> {
        let db = BlokliDb::new_in_memory().await?;

        let chain_key = ChainKeypair::random().public().to_address();
        let packet_key = *OffchainKeypair::random().public();

        // Create initial account
        db.upsert_account(None, 1, chain_key, packet_key, None, 100, 0, 0)
            .await?;

        // Spawn multiple concurrent updates
        let mut handles = vec![];
        for i in 0..10 {
            let db_clone = db.clone();
            let handle = tokio::spawn(async move {
                let safe = if i % 2 == 0 {
                    Some(Address::from(hopr_crypto_random::random_bytes()))
                } else {
                    None
                };
                db_clone
                    .upsert_account(None, 1, chain_key, packet_key, safe, 200 + i, 0, 0)
                    .await
            });
            handles.push(handle);
        }

        // Wait for all updates to complete
        for handle in handles {
            handle.await??;
        }

        // Verify all states were persisted (no lost updates)
        let history = db.get_account_history(None, chain_key).await?;

        assert_eq!(
            11,
            history.len(),
            "should have 11 state records (1 initial + 10 updates)"
        );

        Ok(())
    }

    #[tokio::test]
    async fn test_account_state_isolation() -> anyhow::Result<()> {
        let db = BlokliDb::new_in_memory().await?;

        let chain_1 = ChainKeypair::random().public().to_address();
        let packet_1 = *OffchainKeypair::random().public();

        let chain_2 = ChainKeypair::random().public().to_address();
        let packet_2 = *OffchainKeypair::random().public();

        // Create two accounts with interleaved positions
        db.upsert_account(None, 1, chain_1, packet_1, None, 100, 5, 2).await?;
        db.upsert_account(None, 2, chain_2, packet_2, None, 100, 7, 0).await?;
        db.upsert_account(None, 1, chain_1, packet_1, None, 101, 3, 1).await?;
        db.upsert_account(None, 2, chain_2, packet_2, None, 101, 4, 2).await?;

        // Query account 1 history - should not include account 2's states
        let history_1 = db.get_account_history(None, chain_1).await?;
        assert_eq!(history_1.len(), 2, "account 1 should have 2 states");
        assert!(
            history_1.iter().all(|s| s.chain_addr == chain_1),
            "all states should belong to account 1"
        );

        // Query account 2 history - should not include account 1's states
        let history_2 = db.get_account_history(None, chain_2).await?;
        assert_eq!(history_2.len(), 2, "account 2 should have 2 states");
        assert!(
            history_2.iter().all(|s| s.chain_addr == chain_2),
            "all states should belong to account 2"
        );

        Ok(())
    }

    #[tokio::test]
    async fn test_safe_address_and_key_id_retrieval() -> anyhow::Result<()> {
        let db = BlokliDb::new_in_memory().await?;

        let chain_key = ChainKeypair::random().public().to_address();
        let packet_key = *OffchainKeypair::random().public();
        let safe_addr_1 = Address::from(hopr_crypto_random::random_bytes());
        let safe_addr_2 = Address::from(hopr_crypto_random::random_bytes());

        // State 1: Create account with first safe address at block 100
        db.upsert_account(None, 1, chain_key, packet_key, Some(safe_addr_1), 100, 0, 0)
            .await?;

        // Verify safe_address is correctly retrieved
        let account = db.get_account(None, chain_key).await?.expect("account should exist");
        assert_eq!(
            Some(safe_addr_1),
            account.safe_address,
            "safe_address should match first state"
        );
        assert_ne!(account.key_id, 0.into(), "key_id should be non-zero");

        // State 2: Update account with new safe address at block 200
        db.upsert_account(None, 1, chain_key, packet_key, Some(safe_addr_2), 200, 0, 0)
            .await?;

        // Verify latest state has updated safe_address
        let updated = db.get_account(None, chain_key).await?.expect("account should exist");
        assert_eq!(
            Some(safe_addr_2),
            updated.safe_address,
            "safe_address should match latest state"
        );
        assert_eq!(
            account.key_id, updated.key_id,
            "key_id should remain constant across state updates"
        );

        // Verify historical state at block 100 still has first safe address
        let historical = db
            .get_account_state_at_block(None, chain_key, 150)
            .await?
            .expect("historical state should exist");
        assert_eq!(
            Some(safe_addr_1),
            historical.safe_address,
            "historical safe_address should match block 100 state"
        );
        assert_eq!(
            account.key_id, historical.key_id,
            "key_id should be consistent in historical state"
        );

        // Verify account history contains different safe addresses
        let history = db.get_account_history(None, chain_key).await?;
        assert_eq!(2, history.len(), "should have 2 state records");
        assert_eq!(
            Some(safe_addr_1),
            history[0].safe_address,
            "first historical state should have first safe address"
        );
        assert_eq!(
            Some(safe_addr_2),
            history[1].safe_address,
            "second historical state should have second safe address"
        );
        assert!(
            history.iter().all(|s| s.key_id == account.key_id),
            "all states should have consistent key_id"
        );

        // State 3: Create account without safe address
        let chain_key_no_safe = ChainKeypair::random().public().to_address();
        let packet_key_no_safe = *OffchainKeypair::random().public();
        db.upsert_account(None, 2, chain_key_no_safe, packet_key_no_safe, None, 100, 0, 0)
            .await?;

        let no_safe = db
            .get_account(None, chain_key_no_safe)
            .await?
            .expect("account should exist");
        assert_eq!(None, no_safe.safe_address, "safe_address should be None when not set");
        assert_ne!(
            no_safe.key_id,
            0.into(),
            "key_id should still be set even without safe_address"
        );

        Ok(())
    }
}
