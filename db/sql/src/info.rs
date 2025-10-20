use async_trait::async_trait;
use blokli_db_api::info::{DomainSeparator, IndexerData};
use blokli_db_entity::{
    chain_info, node_info,
    prelude::{Account, Announcement, ChainInfo, Channel, NodeInfo},
};
use futures::TryFutureExt;
use hopr_crypto_types::prelude::Hash;
use hopr_internal_types::prelude::WinningProbability;
use hopr_primitive_types::prelude::{HoprBalance, IntoEndian};
use sea_orm::{
    ActiveModelBehavior, ActiveModelTrait, EntityOrSelect, EntityTrait, IntoActiveModel, PaginatorTrait, Set,
};
use tracing::trace;

use crate::{
    BlokliDbGeneralModelOperations, OptTx, SINGULAR_TABLE_FIXED_ID, TargetDb,
    db::BlokliDb,
    errors::{DbSqlError, DbSqlError::MissingFixedTableEntry, Result},
};

#[derive(Copy, Clone, Debug, Default, PartialEq, Eq)]
pub struct IndexerStateInfo {
    // the latest block number that has been indexed and persisted to the database
    pub latest_block_number: u32,
    pub latest_log_block_number: u32,
    pub latest_log_checksum: Hash,
}

/// Defines DB access API for various node information.
///
/// # Checksum computation
///
/// $H$ denotes Keccak256 hash function and $||$  byte string concatenation.
///
/// For a block $b_1$ containing logs $L_1, L_2, \ldots L_n$ corresponding to tx hashes $Tx_1, Tx_2, \ldots Tx_n$, a
/// block hash is computed as:
///
/// ```math
/// H_{b_1} = H(Tx_1 || Tx_2 || \ldots || Tx_n)
/// ```
/// Given $C_0 = H(0x00...0)$ , the checksum $C_{k+1}$ after processing block $b_{k+1}$ is given as follows:
///
/// ```math
/// C_{k+1} = H(C_k || H_{b_{k+1}})
/// ```
#[async_trait]
pub trait BlokliDbInfoOperations {
    /// Checks if the index is empty.
    ///
    /// # Returns
    ///
    /// A `Result` containing a boolean indicating whether the index is empty.
    async fn index_is_empty(&self) -> Result<bool>;

    /// Removes all data from all tables in the index database.
    ///
    /// # Returns
    ///
    /// A `Result` indicating the success or failure of the operation.
    async fn clear_index_db<'a>(&'a self, tx: OptTx<'a>) -> Result<()>;

    /// Gets node's Safe balance of HOPR tokens.
    async fn get_safe_hopr_balance<'a>(&'a self, tx: OptTx<'a>) -> Result<HoprBalance>;

    /// Sets node's Safe balance of HOPR tokens.
    async fn set_safe_hopr_balance<'a>(&'a self, tx: OptTx<'a>, new_balance: HoprBalance) -> Result<()>;

    /// Gets node's Safe allowance of HOPR tokens.
    async fn get_safe_hopr_allowance<'a>(&'a self, tx: OptTx<'a>) -> Result<HoprBalance>;

    /// Sets node's Safe allowance of HOPR tokens.
    async fn set_safe_hopr_allowance<'a>(&'a self, tx: OptTx<'a>, new_allowance: HoprBalance) -> Result<()>;

    /// Gets stored Indexer data.
    ///
    /// To update information stored in [IndexerData], use the individual setter methods,
    /// such as [`BlokliDbInfoOperations::set_domain_separator`]... etc.
    async fn get_indexer_data<'a>(&'a self, tx: OptTx<'a>) -> Result<IndexerData>;

    /// Sets a domain separator.
    ///
    /// To retrieve stored domain separator info, use [`BlokliDbInfoOperations::get_indexer_data`],
    async fn set_domain_separator<'a>(&'a self, tx: OptTx<'a>, dst_type: DomainSeparator, value: Hash) -> Result<()>;

    /// Sets the minimum required winning probability for incoming tickets.
    /// The value must be between zero and 1.
    async fn set_minimum_incoming_ticket_win_prob<'a>(
        &'a self,
        tx: OptTx<'a>,
        win_prob: WinningProbability,
    ) -> Result<()>;

    /// Updates the ticket price.
    /// To retrieve the stored ticket price, use [`BlokliDbInfoOperations::get_indexer_data`],
    async fn update_ticket_price<'a>(&'a self, tx: OptTx<'a>, price: HoprBalance) -> Result<()>;

    /// Gets the indexer state info.
    async fn get_indexer_state_info<'a>(&'a self, tx: OptTx<'a>) -> Result<IndexerStateInfo>;

    /// Updates the indexer state info.
    async fn set_indexer_state_info<'a>(&'a self, tx: OptTx<'a>, block_num: u32) -> Result<()>;
}

#[async_trait]
impl BlokliDbInfoOperations for BlokliDb {
    async fn index_is_empty(&self) -> Result<bool> {
        let c = self.conn(TargetDb::Index);

        // There is always at least the node's own AccountEntry
        if Account::find().select().count(c).await? > 1 {
            return Ok(false);
        }

        if Announcement::find().one(c).await?.is_some() {
            return Ok(false);
        }

        if Channel::find().one(c).await?.is_some() {
            return Ok(false);
        }

        Ok(true)
    }

    async fn clear_index_db<'a>(&'a self, tx: OptTx<'a>) -> Result<()> {
        self.nest_transaction(tx)
            .await?
            .perform(|tx| {
                Box::pin(async move {
                    Account::delete_many().exec(tx.as_ref()).await?;
                    Announcement::delete_many().exec(tx.as_ref()).await?;
                    Channel::delete_many().exec(tx.as_ref()).await?;
                    ChainInfo::delete_many().exec(tx.as_ref()).await?;
                    NodeInfo::delete_many().exec(tx.as_ref()).await?;

                    // Initial rows are needed in the ChainInfo and NodeInfo tables
                    // See the m20240226_000007_index_initial_seed.rs migration

                    let mut initial_row = chain_info::ActiveModel::new();
                    initial_row.id = Set(1);
                    ChainInfo::insert(initial_row).exec(tx.as_ref()).await?;

                    let mut initial_row = node_info::ActiveModel::new();
                    initial_row.id = Set(1);
                    NodeInfo::insert(initial_row).exec(tx.as_ref()).await?;

                    Ok::<(), DbSqlError>(())
                })
            })
            .await?;

        Ok(())
    }

    async fn get_safe_hopr_balance<'a>(&'a self, tx: OptTx<'a>) -> Result<HoprBalance> {
        self.nest_transaction(tx)
            .await?
            .perform(|tx| {
                Box::pin(async move {
                    node_info::Entity::find_by_id(SINGULAR_TABLE_FIXED_ID)
                        .one(tx.as_ref())
                        .await?
                        .ok_or(MissingFixedTableEntry("node_info".into()))
                        .map(|m| HoprBalance::from_be_bytes(m.safe_balance))
                })
            })
            .await
    }

    async fn set_safe_hopr_balance<'a>(&'a self, tx: OptTx<'a>, new_balance: HoprBalance) -> Result<()> {
        self.nest_transaction(tx)
            .await?
            .perform(|tx| {
                Box::pin(async move {
                    Ok::<_, DbSqlError>(
                        node_info::ActiveModel {
                            id: Set(SINGULAR_TABLE_FIXED_ID),
                            safe_balance: Set(new_balance.to_be_bytes().into()),
                            ..Default::default()
                        }
                        .update(tx.as_ref()) // DB is primed in the migration, so only update is needed
                        .await?,
                    )
                })
            })
            .await?;

        Ok(())
    }

    async fn get_safe_hopr_allowance<'a>(&'a self, tx: OptTx<'a>) -> Result<HoprBalance> {
        self.nest_transaction(tx)
            .await?
            .perform(|tx| {
                Box::pin(async move {
                    node_info::Entity::find_by_id(SINGULAR_TABLE_FIXED_ID)
                        .one(tx.as_ref())
                        .await?
                        .ok_or(MissingFixedTableEntry("node_info".into()))
                        .map(|m| HoprBalance::from_be_bytes(m.safe_allowance))
                })
            })
            .await
    }

    async fn set_safe_hopr_allowance<'a>(&'a self, tx: OptTx<'a>, new_allowance: HoprBalance) -> Result<()> {
        self.nest_transaction(tx)
            .await?
            .perform(|tx| {
                Box::pin(async move {
                    node_info::ActiveModel {
                        id: Set(SINGULAR_TABLE_FIXED_ID),
                        safe_allowance: Set(new_allowance.amount().to_be_bytes().to_vec()),
                        ..Default::default()
                    }
                    .update(tx.as_ref()) // DB is primed in the migration, so only update is needed
                    .await?;

                    Ok::<_, DbSqlError>(())
                })
            })
            .await
    }

    async fn get_indexer_data<'a>(&'a self, tx: OptTx<'a>) -> Result<IndexerData> {
        let myself = self.clone();
        Ok(myself
            .nest_transaction(tx)
            .and_then(|op| {
                op.perform(|tx| {
                    Box::pin(async move {
                        let model = chain_info::Entity::find_by_id(SINGULAR_TABLE_FIXED_ID)
                            .one(tx.as_ref())
                            .await?
                            .ok_or(MissingFixedTableEntry("chain_info".into()))?;

                        let ledger_dst = if let Some(b) = model.ledger_dst {
                            Some(Hash::try_from(b.as_ref())?)
                        } else {
                            None
                        };

                        let safe_registry_dst = if let Some(b) = model.safe_registry_dst {
                            Some(Hash::try_from(b.as_ref())?)
                        } else {
                            None
                        };

                        let channels_dst = if let Some(b) = model.channels_dst {
                            Some(Hash::try_from(b.as_ref())?)
                        } else {
                            None
                        };

                        Ok::<_, DbSqlError>(IndexerData {
                            ledger_dst,
                            safe_registry_dst,
                            channels_dst,
                            ticket_price: model.ticket_price.map(HoprBalance::from_be_bytes),
                            minimum_incoming_ticket_winning_prob: (model.min_incoming_ticket_win_prob as f64)
                                .try_into()?,
                        })
                    })
                })
            })
            .await?)
    }

    async fn set_domain_separator<'a>(&'a self, tx: OptTx<'a>, dst_type: DomainSeparator, value: Hash) -> Result<()> {
        self.nest_transaction(tx)
            .await?
            .perform(|tx| {
                Box::pin(async move {
                    let mut active_model = chain_info::ActiveModel {
                        id: Set(SINGULAR_TABLE_FIXED_ID),
                        ..Default::default()
                    };

                    match dst_type {
                        DomainSeparator::Ledger => {
                            active_model.ledger_dst = Set(Some(value.as_ref().into()));
                        }
                        DomainSeparator::SafeRegistry => {
                            active_model.safe_registry_dst = Set(Some(value.as_ref().into()));
                        }
                        DomainSeparator::Channel => {
                            active_model.channels_dst = Set(Some(value.as_ref().into()));
                        }
                    }

                    // DB is primed in the migration, so only update is needed
                    active_model.update(tx.as_ref()).await?;

                    Ok::<(), DbSqlError>(())
                })
            })
            .await?;

        Ok(())
    }

    async fn set_minimum_incoming_ticket_win_prob<'a>(
        &'a self,
        tx: OptTx<'a>,
        win_prob: WinningProbability,
    ) -> Result<()> {
        self.nest_transaction(tx)
            .await?
            .perform(|tx| {
                Box::pin(async move {
                    chain_info::ActiveModel {
                        id: Set(SINGULAR_TABLE_FIXED_ID),
                        min_incoming_ticket_win_prob: Set(win_prob.as_f64() as f32),
                        ..Default::default()
                    }
                    .update(tx.as_ref())
                    .await?;

                    Ok::<(), DbSqlError>(())
                })
            })
            .await?;

        Ok(())
    }

    async fn update_ticket_price<'a>(&'a self, tx: OptTx<'a>, price: HoprBalance) -> Result<()> {
        self.nest_transaction(tx)
            .await?
            .perform(|tx| {
                Box::pin(async move {
                    chain_info::ActiveModel {
                        id: Set(SINGULAR_TABLE_FIXED_ID),
                        ticket_price: Set(Some(price.to_be_bytes().into())),
                        ..Default::default()
                    }
                    .update(tx.as_ref())
                    .await?;

                    Ok::<(), DbSqlError>(())
                })
            })
            .await?;

        Ok(())
    }

    async fn get_indexer_state_info<'a>(&'a self, tx: OptTx<'a>) -> Result<IndexerStateInfo> {
        self.nest_transaction(tx)
            .await?
            .perform(|tx| {
                Box::pin(async move {
                    chain_info::Entity::find_by_id(SINGULAR_TABLE_FIXED_ID)
                        .one(tx.as_ref())
                        .await?
                        .ok_or(DbSqlError::MissingFixedTableEntry("chain_info".into()))
                        .map(|m| IndexerStateInfo {
                            latest_block_number: u64::from_be_bytes(m.last_indexed_block.as_slice().try_into().unwrap())
                                as u32,
                            ..Default::default()
                        })
                })
            })
            .await
    }

    async fn set_indexer_state_info<'a>(&'a self, tx: OptTx<'a>, block_num: u32) -> Result<()> {
        self.nest_transaction(tx)
            .await?
            .perform(|tx| {
                Box::pin(async move {
                    let model = chain_info::Entity::find_by_id(SINGULAR_TABLE_FIXED_ID)
                        .one(tx.as_ref())
                        .await?
                        .ok_or(MissingFixedTableEntry("chain_info".into()))?;

                    let current_last_indexed_block =
                        u64::from_be_bytes(model.last_indexed_block.as_slice().try_into().unwrap()) as u32;

                    let mut active_model = model.into_active_model();

                    trace!(
                        old_block = current_last_indexed_block,
                        new_block = block_num,
                        "update block"
                    );

                    active_model.last_indexed_block = Set((block_num as u64).to_be_bytes().to_vec());
                    active_model.update(tx.as_ref()).await?;

                    Ok::<_, DbSqlError>(())
                })
            })
            .await
    }
}

#[cfg(test)]
mod tests {
    use hex_literal::hex;
    use hopr_crypto_types::{keypairs::ChainKeypair, prelude::Keypair};
    use hopr_primitive_types::{balance::HoprBalance, prelude::Address};

    use crate::{db::BlokliDb, info::BlokliDbInfoOperations};

    lazy_static::lazy_static! {
        static ref ADDR_1: Address = Address::from(hex!("86fa27add61fafc955e2da17329bba9f31692fe7"));
        static ref ADDR_2: Address = Address::from(hex!("4c8bbd047c2130e702badb23b6b97a88b6562324"));
    }

    #[tokio::test]
    async fn test_set_get_balance() -> anyhow::Result<()> {
        let db = BlokliDb::new_in_memory().await?;

        assert_eq!(
            HoprBalance::zero(),
            db.get_safe_hopr_balance(None).await?,
            "balance must be 0"
        );

        let balance = HoprBalance::from(10_000);
        db.set_safe_hopr_balance(None, balance).await?;

        assert_eq!(
            balance,
            db.get_safe_hopr_balance(None).await?,
            "balance must be {balance}"
        );
        Ok(())
    }

    #[tokio::test]
    async fn test_set_get_allowance() -> anyhow::Result<()> {
        let db = BlokliDb::new_in_memory().await?;

        assert_eq!(
            HoprBalance::zero(),
            db.get_safe_hopr_allowance(None).await?,
            "balance must be 0"
        );

        let balance = HoprBalance::from(10_000);
        db.set_safe_hopr_allowance(None, balance).await?;

        assert_eq!(
            balance,
            db.get_safe_hopr_allowance(None).await?,
            "balance must be {balance}"
        );

        Ok(())
    }

    #[tokio::test]
    async fn test_set_get_indexer_data() -> anyhow::Result<()> {
        let db = BlokliDb::new_in_memory().await?;

        let data = db.get_indexer_data(None).await?;
        assert_eq!(data.ticket_price, None);

        let price = HoprBalance::from(10);
        db.update_ticket_price(None, price).await?;

        db.set_minimum_incoming_ticket_win_prob(None, 0.5.try_into()?).await?;

        let data = db.get_indexer_data(None).await?;

        assert_eq!(data.ticket_price, Some(price));
        assert_eq!(data.minimum_incoming_ticket_winning_prob, 0.5);
        Ok(())
    }
}
