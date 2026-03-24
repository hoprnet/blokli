use hopr_types::internal::prelude::DEFAULT_MINIMUM_INCOMING_TICKET_WIN_PROB;
use sea_orm_migration::prelude::*;

#[derive(DeriveMigrationName)]
pub struct Migration;

#[async_trait::async_trait]
impl MigrationTrait for Migration {
    async fn up(&self, manager: &SchemaManager) -> Result<(), DbErr> {
        let backend = manager.get_database_backend();

        // === account ===
        manager
            .create_table(
                Table::create()
                    .table(Account::Table)
                    .if_not_exists()
                    .col(
                        ColumnDef::new(Account::Id)
                            .big_integer()
                            .not_null()
                            .auto_increment()
                            .primary_key(),
                    )
                    .col(ColumnDef::new(Account::ChainKey).binary_len(20).not_null())
                    .col(ColumnDef::new(Account::PacketKey).string_len(64).not_null())
                    .col(
                        ColumnDef::new(Account::PublishedBlock)
                            .big_integer()
                            .not_null()
                            .default(0),
                    )
                    .col(
                        ColumnDef::new(Account::PublishedTxIndex)
                            .big_integer()
                            .not_null()
                            .default(0),
                    )
                    .col(
                        ColumnDef::new(Account::PublishedLogIndex)
                            .big_integer()
                            .not_null()
                            .default(0),
                    )
                    .to_owned(),
            )
            .await?;

        manager
            .create_index(
                Index::create()
                    .if_not_exists()
                    .name("idx_account_chain_key")
                    .table(Account::Table)
                    .col(Account::ChainKey)
                    .to_owned(),
            )
            .await?;

        manager
            .create_index(
                Index::create()
                    .if_not_exists()
                    .name("idx_account_packet_key")
                    .table(Account::Table)
                    .col(Account::PacketKey)
                    .to_owned(),
            )
            .await?;

        manager
            .create_index(
                Index::create()
                    .if_not_exists()
                    .name("idx_account_chain_packet_key")
                    .table(Account::Table)
                    .col(Account::ChainKey)
                    .col(Account::PacketKey)
                    .unique()
                    .to_owned(),
            )
            .await?;

        // === account_state ===
        manager
            .create_table(
                Table::create()
                    .table(AccountState::Table)
                    .if_not_exists()
                    .col(
                        ColumnDef::new(AccountState::Id)
                            .big_integer()
                            .not_null()
                            .auto_increment()
                            .primary_key(),
                    )
                    .col(ColumnDef::new(AccountState::AccountId).big_integer().not_null())
                    .col(ColumnDef::new(AccountState::SafeAddress).binary_len(20).null())
                    .col(ColumnDef::new(AccountState::PublishedBlock).big_integer().not_null())
                    .col(ColumnDef::new(AccountState::PublishedTxIndex).big_integer().not_null())
                    .col(ColumnDef::new(AccountState::PublishedLogIndex).big_integer().not_null())
                    .foreign_key(
                        ForeignKey::create()
                            .name("fk_account_state_account_id")
                            .from(AccountState::Table, AccountState::AccountId)
                            .to(Account::Table, Account::Id)
                            .on_delete(ForeignKeyAction::Cascade)
                            .on_update(ForeignKeyAction::Cascade),
                    )
                    .to_owned(),
            )
            .await?;

        manager
            .create_index(
                Index::create()
                    .name("idx_account_state_unique_position")
                    .table(AccountState::Table)
                    .col(AccountState::AccountId)
                    .col(AccountState::PublishedBlock)
                    .col(AccountState::PublishedTxIndex)
                    .col(AccountState::PublishedLogIndex)
                    .unique()
                    .to_owned(),
            )
            .await?;

        manager
            .create_index(
                Index::create()
                    .name("idx_account_state_position")
                    .table(AccountState::Table)
                    .col(AccountState::AccountId)
                    .col((AccountState::PublishedBlock, IndexOrder::Desc))
                    .col((AccountState::PublishedTxIndex, IndexOrder::Desc))
                    .col((AccountState::PublishedLogIndex, IndexOrder::Desc))
                    .to_owned(),
            )
            .await?;

        // === channel ===
        manager
            .create_table(
                Table::create()
                    .table(Channel::Table)
                    .if_not_exists()
                    .col(
                        ColumnDef::new(Channel::Id)
                            .big_integer()
                            .not_null()
                            .auto_increment()
                            .primary_key(),
                    )
                    .col(ColumnDef::new(Channel::Source).big_integer().not_null())
                    .col(ColumnDef::new(Channel::Destination).big_integer().not_null())
                    .col(
                        ColumnDef::new(Channel::ConcreteChannelId)
                            .string_len(64)
                            .not_null()
                            .unique_key(),
                    )
                    .foreign_key(
                        ForeignKey::create()
                            .name("fk_channel_source_account_id")
                            .from(Channel::Table, Channel::Source)
                            .to(Account::Table, Account::Id)
                            .on_delete(ForeignKeyAction::Cascade)
                            .on_update(ForeignKeyAction::Cascade),
                    )
                    .foreign_key(
                        ForeignKey::create()
                            .name("fk_channel_destination_account_id")
                            .from(Channel::Table, Channel::Destination)
                            .to(Account::Table, Account::Id)
                            .on_delete(ForeignKeyAction::Cascade)
                            .on_update(ForeignKeyAction::Cascade),
                    )
                    .to_owned(),
            )
            .await?;

        manager
            .create_index(
                Index::create()
                    .if_not_exists()
                    .name("idx_channel_source")
                    .table(Channel::Table)
                    .col(Channel::Source)
                    .to_owned(),
            )
            .await?;

        manager
            .create_index(
                Index::create()
                    .if_not_exists()
                    .name("idx_channel_destination")
                    .table(Channel::Table)
                    .col(Channel::Destination)
                    .to_owned(),
            )
            .await?;

        manager
            .create_index(
                Index::create()
                    .if_not_exists()
                    .name("idx_channel_source_destination")
                    .table(Channel::Table)
                    .col(Channel::Source)
                    .col(Channel::Destination)
                    .to_owned(),
            )
            .await?;

        // === channel_state ===
        manager
            .create_table(
                Table::create()
                    .table(ChannelState::Table)
                    .if_not_exists()
                    .col(
                        ColumnDef::new(ChannelState::Id)
                            .big_integer()
                            .not_null()
                            .auto_increment()
                            .primary_key(),
                    )
                    .col(ColumnDef::new(ChannelState::ChannelId).big_integer().not_null())
                    .col(ColumnDef::new(ChannelState::Balance).binary_len(12).not_null())
                    .col(ColumnDef::new(ChannelState::Status).small_integer().not_null())
                    .col(ColumnDef::new(ChannelState::Epoch).big_integer().not_null())
                    .col(ColumnDef::new(ChannelState::TicketIndex).big_integer().not_null())
                    .col(
                        ColumnDef::new(ChannelState::ClosureTime)
                            .timestamp_with_time_zone()
                            .null(),
                    )
                    .col(
                        ColumnDef::new(ChannelState::CorruptedState)
                            .boolean()
                            .not_null()
                            .default(false),
                    )
                    .col(
                        ColumnDef::new(ChannelState::ReorgCorrection)
                            .boolean()
                            .not_null()
                            .default(false),
                    )
                    .col(ColumnDef::new(ChannelState::PublishedBlock).big_integer().not_null())
                    .col(ColumnDef::new(ChannelState::PublishedTxIndex).big_integer().not_null())
                    .col(ColumnDef::new(ChannelState::PublishedLogIndex).big_integer().not_null())
                    .foreign_key(
                        ForeignKey::create()
                            .name("fk_channel_state_channel_id")
                            .from(ChannelState::Table, ChannelState::ChannelId)
                            .to(Channel::Table, Channel::Id)
                            .on_delete(ForeignKeyAction::Cascade)
                            .on_update(ForeignKeyAction::Cascade),
                    )
                    .to_owned(),
            )
            .await?;

        manager
            .create_index(
                Index::create()
                    .name("idx_channel_state_unique_position")
                    .table(ChannelState::Table)
                    .col(ChannelState::ChannelId)
                    .col(ChannelState::PublishedBlock)
                    .col(ChannelState::PublishedTxIndex)
                    .col(ChannelState::PublishedLogIndex)
                    .unique()
                    .to_owned(),
            )
            .await?;

        manager
            .create_index(
                Index::create()
                    .name("idx_channel_state_position")
                    .table(ChannelState::Table)
                    .col(ChannelState::ChannelId)
                    .col((ChannelState::PublishedBlock, IndexOrder::Desc))
                    .col((ChannelState::PublishedTxIndex, IndexOrder::Desc))
                    .col((ChannelState::PublishedLogIndex, IndexOrder::Desc))
                    .to_owned(),
            )
            .await?;

        manager
            .create_index(
                Index::create()
                    .name("idx_channel_state_status_position")
                    .table(ChannelState::Table)
                    .col(ChannelState::Status)
                    .col((ChannelState::PublishedBlock, IndexOrder::Desc))
                    .col((ChannelState::PublishedTxIndex, IndexOrder::Desc))
                    .col((ChannelState::PublishedLogIndex, IndexOrder::Desc))
                    .to_owned(),
            )
            .await?;

        manager
            .create_index(
                Index::create()
                    .name("idx_channel_state_status_channel_position")
                    .table(ChannelState::Table)
                    .col(ChannelState::Status)
                    .col(ChannelState::ChannelId)
                    .col((ChannelState::PublishedBlock, IndexOrder::Desc))
                    .col((ChannelState::PublishedTxIndex, IndexOrder::Desc))
                    .col((ChannelState::PublishedLogIndex, IndexOrder::Desc))
                    .to_owned(),
            )
            .await?;

        // === announcement ===
        manager
            .create_table(
                Table::create()
                    .table(Announcement::Table)
                    .if_not_exists()
                    .col(
                        ColumnDef::new(Announcement::Id)
                            .big_integer()
                            .not_null()
                            .auto_increment()
                            .primary_key(),
                    )
                    .col(ColumnDef::new(Announcement::AccountId).big_integer().not_null())
                    .col(ColumnDef::new(Announcement::Multiaddress).text().not_null())
                    .col(
                        ColumnDef::new(Announcement::PublishedBlock)
                            .big_integer()
                            .not_null()
                            .default(0),
                    )
                    .col(
                        ColumnDef::new(Announcement::PublishedTxIndex)
                            .big_integer()
                            .not_null()
                            .default(0),
                    )
                    .col(
                        ColumnDef::new(Announcement::PublishedLogIndex)
                            .big_integer()
                            .not_null()
                            .default(0),
                    )
                    .foreign_key(
                        ForeignKey::create()
                            .name("fk_announcement_account_id")
                            .from(Announcement::Table, Announcement::AccountId)
                            .to(Account::Table, Account::Id)
                            .on_delete(ForeignKeyAction::Cascade)
                            .on_update(ForeignKeyAction::Cascade),
                    )
                    .to_owned(),
            )
            .await?;

        manager
            .create_index(
                Index::create()
                    .if_not_exists()
                    .name("idx_announcement_account_id")
                    .table(Announcement::Table)
                    .col(Announcement::AccountId)
                    .to_owned(),
            )
            .await?;

        manager
            .create_index(
                Index::create()
                    .name("idx_announcement_position")
                    .table(Announcement::Table)
                    .col(Announcement::AccountId)
                    .col((Announcement::PublishedBlock, IndexOrder::Desc))
                    .col((Announcement::PublishedTxIndex, IndexOrder::Desc))
                    .col((Announcement::PublishedLogIndex, IndexOrder::Desc))
                    .to_owned(),
            )
            .await?;

        // === chain_info ===
        // No auto_increment: the application seeds and manages this singleton row directly.
        manager
            .create_table(
                Table::create()
                    .table(ChainInfo::Table)
                    .if_not_exists()
                    .col(ColumnDef::new(ChainInfo::Id).big_integer().not_null().primary_key())
                    .col(
                        ColumnDef::new(ChainInfo::LastIndexedBlock)
                            .big_integer()
                            .not_null()
                            .default(0),
                    )
                    .col(ColumnDef::new(ChainInfo::LastIndexedTxIndex).big_integer().null())
                    .col(ColumnDef::new(ChainInfo::LastIndexedLogIndex).big_integer().null())
                    .col(
                        ColumnDef::new(ChainInfo::ChannelClosureGracePeriod)
                            .big_integer()
                            .null(),
                    )
                    .col(ColumnDef::new(ChainInfo::KeyBindingFee).binary_len(12).null())
                    .col(ColumnDef::new(ChainInfo::TicketPrice).binary_len(12).null())
                    .col(ColumnDef::new(ChainInfo::ChannelsDST).binary_len(32).null())
                    .col(ColumnDef::new(ChainInfo::LedgerDST).binary_len(32).null())
                    .col(ColumnDef::new(ChainInfo::SafeRegistryDST).binary_len(32).null())
                    .col(
                        ColumnDef::new(ChainInfo::MinIncomingTicketWinProb)
                            .double()
                            .not_null()
                            .default(DEFAULT_MINIMUM_INCOMING_TICKET_WIN_PROB),
                    )
                    .to_owned(),
            )
            .await?;

        // === hopr_balance ===
        manager
            .create_table(
                Table::create()
                    .table(HoprBalance::Table)
                    .if_not_exists()
                    .col(
                        ColumnDef::new(HoprBalance::Id)
                            .big_integer()
                            .not_null()
                            .auto_increment()
                            .primary_key(),
                    )
                    .col(
                        ColumnDef::new(HoprBalance::Address)
                            .binary_len(20)
                            .not_null()
                            .unique_key(),
                    )
                    .col(
                        ColumnDef::new(HoprBalance::Balance)
                            .binary_len(12)
                            .not_null()
                            .default(vec![0u8; 12]),
                    )
                    .col(
                        ColumnDef::new(HoprBalance::LastChangedBlock)
                            .big_integer()
                            .not_null()
                            .default(0),
                    )
                    .col(
                        ColumnDef::new(HoprBalance::LastChangedTxIndex)
                            .big_integer()
                            .not_null()
                            .default(0),
                    )
                    .col(
                        ColumnDef::new(HoprBalance::LastChangedLogIndex)
                            .big_integer()
                            .not_null()
                            .default(0),
                    )
                    .to_owned(),
            )
            .await?;

        manager
            .create_index(
                Index::create()
                    .if_not_exists()
                    .name("idx_hopr_balance_last_changed_block")
                    .table(HoprBalance::Table)
                    .col(HoprBalance::LastChangedBlock)
                    .to_owned(),
            )
            .await?;

        // === native_balance ===
        manager
            .create_table(
                Table::create()
                    .table(NativeBalance::Table)
                    .if_not_exists()
                    .col(
                        ColumnDef::new(NativeBalance::Id)
                            .big_integer()
                            .not_null()
                            .auto_increment()
                            .primary_key(),
                    )
                    .col(
                        ColumnDef::new(NativeBalance::Address)
                            .binary_len(20)
                            .not_null()
                            .unique_key(),
                    )
                    .col(
                        ColumnDef::new(NativeBalance::Balance)
                            .binary_len(12)
                            .not_null()
                            .default(vec![0u8; 12]),
                    )
                    .col(
                        ColumnDef::new(NativeBalance::LastChangedBlock)
                            .big_integer()
                            .not_null()
                            .default(0),
                    )
                    .col(
                        ColumnDef::new(NativeBalance::LastChangedTxIndex)
                            .big_integer()
                            .not_null()
                            .default(0),
                    )
                    .col(
                        ColumnDef::new(NativeBalance::LastChangedLogIndex)
                            .big_integer()
                            .not_null()
                            .default(0),
                    )
                    .to_owned(),
            )
            .await?;

        manager
            .create_index(
                Index::create()
                    .if_not_exists()
                    .name("idx_native_balance_last_changed_block")
                    .table(NativeBalance::Table)
                    .col(NativeBalance::LastChangedBlock)
                    .to_owned(),
            )
            .await?;

        // === hopr_safe_contract (temporal) ===
        manager
            .create_table(
                Table::create()
                    .table(HoprSafeContract::Table)
                    .if_not_exists()
                    .col(
                        ColumnDef::new(HoprSafeContract::Id)
                            .big_integer()
                            .not_null()
                            .auto_increment()
                            .primary_key(),
                    )
                    .col(
                        ColumnDef::new(HoprSafeContract::Address)
                            .binary_len(20)
                            .not_null()
                            .unique_key(),
                    )
                    .to_owned(),
            )
            .await?;

        // === hopr_safe_contract_state ===
        manager
            .create_table(
                Table::create()
                    .table(HoprSafeContractState::Table)
                    .if_not_exists()
                    .col(
                        ColumnDef::new(HoprSafeContractState::Id)
                            .big_integer()
                            .not_null()
                            .auto_increment()
                            .primary_key(),
                    )
                    .col(
                        ColumnDef::new(HoprSafeContractState::HoprSafeContractId)
                            .big_integer()
                            .not_null(),
                    )
                    .col(
                        ColumnDef::new(HoprSafeContractState::ModuleAddress)
                            .binary_len(20)
                            .not_null(),
                    )
                    .col(
                        ColumnDef::new(HoprSafeContractState::ChainKey)
                            .binary_len(20)
                            .not_null(),
                    )
                    .col(
                        ColumnDef::new(HoprSafeContractState::PublishedBlock)
                            .big_integer()
                            .not_null(),
                    )
                    .col(
                        ColumnDef::new(HoprSafeContractState::PublishedTxIndex)
                            .big_integer()
                            .not_null(),
                    )
                    .col(
                        ColumnDef::new(HoprSafeContractState::PublishedLogIndex)
                            .big_integer()
                            .not_null(),
                    )
                    .foreign_key(
                        ForeignKey::create()
                            .from(HoprSafeContractState::Table, HoprSafeContractState::HoprSafeContractId)
                            .to(HoprSafeContract::Table, HoprSafeContract::Id)
                            .on_delete(ForeignKeyAction::Cascade)
                            .on_update(ForeignKeyAction::Cascade),
                    )
                    .to_owned(),
            )
            .await?;

        manager
            .create_index(
                Index::create()
                    .name("idx_safe_contract_state_unique_position")
                    .table(HoprSafeContractState::Table)
                    .col(HoprSafeContractState::HoprSafeContractId)
                    .col(HoprSafeContractState::PublishedBlock)
                    .col(HoprSafeContractState::PublishedTxIndex)
                    .col(HoprSafeContractState::PublishedLogIndex)
                    .unique()
                    .to_owned(),
            )
            .await?;

        manager
            .create_index(
                Index::create()
                    .name("idx_safe_contract_state_position")
                    .table(HoprSafeContractState::Table)
                    .col(HoprSafeContractState::HoprSafeContractId)
                    .col((HoprSafeContractState::PublishedBlock, IndexOrder::Desc))
                    .col((HoprSafeContractState::PublishedTxIndex, IndexOrder::Desc))
                    .col((HoprSafeContractState::PublishedLogIndex, IndexOrder::Desc))
                    .to_owned(),
            )
            .await?;

        // === hopr_node_safe_registration ===
        manager
            .create_table(
                Table::create()
                    .table(HoprNodeSafeRegistration::Table)
                    .if_not_exists()
                    .col(
                        ColumnDef::new(HoprNodeSafeRegistration::Id)
                            .big_integer()
                            .not_null()
                            .auto_increment()
                            .primary_key(),
                    )
                    .col(
                        ColumnDef::new(HoprNodeSafeRegistration::SafeAddress)
                            .binary_len(20)
                            .not_null(),
                    )
                    .col(
                        ColumnDef::new(HoprNodeSafeRegistration::NodeAddress)
                            .binary_len(20)
                            .not_null()
                            .unique_key(),
                    )
                    .col(
                        ColumnDef::new(HoprNodeSafeRegistration::RegisteredBlock)
                            .big_integer()
                            .not_null(),
                    )
                    .col(
                        ColumnDef::new(HoprNodeSafeRegistration::RegisteredTxIndex)
                            .big_integer()
                            .not_null(),
                    )
                    .col(
                        ColumnDef::new(HoprNodeSafeRegistration::RegisteredLogIndex)
                            .big_integer()
                            .not_null(),
                    )
                    .to_owned(),
            )
            .await?;

        manager
            .create_index(
                Index::create()
                    .name("idx_hopr_node_safe_registration_binding")
                    .table(HoprNodeSafeRegistration::Table)
                    .col(HoprNodeSafeRegistration::SafeAddress)
                    .col(HoprNodeSafeRegistration::NodeAddress)
                    .unique()
                    .to_owned(),
            )
            .await?;

        manager
            .create_index(
                Index::create()
                    .name("idx_hopr_node_safe_registration_safe")
                    .table(HoprNodeSafeRegistration::Table)
                    .col(HoprNodeSafeRegistration::SafeAddress)
                    .to_owned(),
            )
            .await?;

        manager
            .create_index(
                Index::create()
                    .name("idx_hopr_node_safe_registration_event")
                    .table(HoprNodeSafeRegistration::Table)
                    .col(HoprNodeSafeRegistration::RegisteredBlock)
                    .col(HoprNodeSafeRegistration::RegisteredTxIndex)
                    .col(HoprNodeSafeRegistration::RegisteredLogIndex)
                    .unique()
                    .to_owned(),
            )
            .await?;

        // === schema_version ===
        manager
            .create_table(
                Table::create()
                    .table(SchemaVersion::Table)
                    .if_not_exists()
                    .col(ColumnDef::new(SchemaVersion::Id).big_integer().not_null().primary_key())
                    .col(ColumnDef::new(SchemaVersion::Version).big_integer().not_null())
                    .col(
                        ColumnDef::new(SchemaVersion::UpdatedAt)
                            .timestamp()
                            .not_null()
                            .default(Expr::current_timestamp()),
                    )
                    .to_owned(),
            )
            .await?;

        manager
            .exec_stmt(
                Query::insert()
                    .into_table(SchemaVersion::Table)
                    .columns([SchemaVersion::Id, SchemaVersion::Version])
                    .values_panic([1.into(), 1.into()])
                    .to_owned(),
            )
            .await?;

        // === hopr_safe_redeemed_stats ===
        manager
            .create_table(
                Table::create()
                    .table(HoprSafeRedeemedStats::Table)
                    .if_not_exists()
                    .col(
                        ColumnDef::new(HoprSafeRedeemedStats::Id)
                            .big_integer()
                            .not_null()
                            .auto_increment()
                            .primary_key(),
                    )
                    .col(
                        ColumnDef::new(HoprSafeRedeemedStats::SafeAddress)
                            .binary_len(20)
                            .not_null(),
                    )
                    .col(
                        ColumnDef::new(HoprSafeRedeemedStats::NodeAddress)
                            .binary_len(20)
                            .not_null(),
                    )
                    .col(
                        ColumnDef::new(HoprSafeRedeemedStats::RedeemedAmount)
                            .binary_len(32)
                            .not_null()
                            .default(vec![0u8; 32]),
                    )
                    .col(
                        ColumnDef::new(HoprSafeRedeemedStats::RedemptionCount)
                            .big_integer()
                            .not_null()
                            .default(0),
                    )
                    .col(
                        ColumnDef::new(HoprSafeRedeemedStats::LastRedeemedBlock)
                            .big_integer()
                            .not_null()
                            .default(0),
                    )
                    .col(
                        ColumnDef::new(HoprSafeRedeemedStats::LastRedeemedTxIndex)
                            .big_integer()
                            .not_null()
                            .default(0),
                    )
                    .col(
                        ColumnDef::new(HoprSafeRedeemedStats::LastRedeemedLogIndex)
                            .big_integer()
                            .not_null()
                            .default(0),
                    )
                    .to_owned(),
            )
            .await?;

        manager
            .create_index(
                Index::create()
                    .name("idx_hopr_safe_redeemed_stats_safe_node_unique")
                    .table(HoprSafeRedeemedStats::Table)
                    .col(HoprSafeRedeemedStats::SafeAddress)
                    .col(HoprSafeRedeemedStats::NodeAddress)
                    .unique()
                    .to_owned(),
            )
            .await?;

        manager
            .create_index(
                Index::create()
                    .name("idx_hopr_safe_redeemed_stats_safe")
                    .table(HoprSafeRedeemedStats::Table)
                    .col(HoprSafeRedeemedStats::SafeAddress)
                    .to_owned(),
            )
            .await?;

        manager
            .create_index(
                Index::create()
                    .name("idx_hopr_safe_redeemed_stats_node")
                    .table(HoprSafeRedeemedStats::Table)
                    .col(HoprSafeRedeemedStats::NodeAddress)
                    .to_owned(),
            )
            .await?;

        if backend == sea_orm::DatabaseBackend::Postgres {
            manager
                .create_foreign_key(
                    ForeignKey::create()
                        .name("fk_hopr_safe_redeemed_stats_safe_address")
                        .from(HoprSafeRedeemedStats::Table, HoprSafeRedeemedStats::SafeAddress)
                        .to(HoprSafeContract::Table, HoprSafeContract::Address)
                        .on_delete(ForeignKeyAction::Cascade)
                        .on_update(ForeignKeyAction::Cascade)
                        .to_owned(),
                )
                .await?;
        }

        // === views ===
        // PostgreSQL: CREATE OR REPLACE VIEW <name> AS ...
        // SQLite:     CREATE VIEW IF NOT EXISTS <name> AS ...
        let create_view = if backend == sea_orm::DatabaseBackend::Postgres {
            "CREATE OR REPLACE VIEW"
        } else {
            "CREATE VIEW IF NOT EXISTS"
        };

        manager
            .get_connection()
            .execute_unprepared(&format!(
                "{create_view} channel_current AS
                SELECT
                    s.id,
                    c.id AS channel_id,
                    c.concrete_channel_id,
                    c.source,
                    c.destination,
                    s.balance,
                    s.status,
                    s.epoch,
                    s.ticket_index,
                    s.closure_time,
                    s.corrupted_state,
                    s.published_block,
                    s.published_tx_index,
                    s.published_log_index,
                    s.reorg_correction
                FROM channel c
                JOIN (
                    SELECT cs.*, ROW_NUMBER() OVER (
                        PARTITION BY cs.channel_id
                        ORDER BY cs.published_block DESC, cs.published_tx_index DESC, cs.published_log_index DESC
                    ) AS rn
                    FROM channel_state cs
                ) s ON s.channel_id = c.id AND s.rn = 1"
            ))
            .await?;

        manager
            .get_connection()
            .execute_unprepared(&format!(
                "{create_view} account_current AS
                SELECT
                    s.id,
                    a.id AS account_id,
                    a.chain_key,
                    a.packet_key,
                    s.safe_address,
                    s.published_block,
                    s.published_tx_index,
                    s.published_log_index
                FROM account a
                JOIN (
                    SELECT acs.*, ROW_NUMBER() OVER (
                        PARTITION BY acs.account_id
                        ORDER BY acs.published_block DESC, acs.published_tx_index DESC, acs.published_log_index DESC
                    ) AS rn
                    FROM account_state acs
                ) s ON s.account_id = a.id AND s.rn = 1"
            ))
            .await?;

        manager
            .get_connection()
            .execute_unprepared(&format!(
                "{create_view} safe_contract_current AS
                SELECT
                    sc.id AS safe_contract_id,
                    sc.address,
                    scs.module_address,
                    scs.chain_key,
                    scs.published_block,
                    scs.published_tx_index,
                    scs.published_log_index
                FROM hopr_safe_contract sc
                JOIN hopr_safe_contract_state scs ON scs.hopr_safe_contract_id = sc.id
                WHERE scs.id = (
                    SELECT s2.id FROM hopr_safe_contract_state s2
                    WHERE s2.hopr_safe_contract_id = sc.id
                    ORDER BY s2.published_block DESC, s2.published_tx_index DESC, s2.published_log_index DESC
                    LIMIT 1
                )"
            ))
            .await?;

        Ok(())
    }

    async fn down(&self, manager: &SchemaManager) -> Result<(), DbErr> {
        manager
            .get_connection()
            .execute_unprepared("DROP VIEW IF EXISTS safe_contract_current")
            .await?;

        manager
            .get_connection()
            .execute_unprepared("DROP VIEW IF EXISTS channel_current")
            .await?;

        manager
            .get_connection()
            .execute_unprepared("DROP VIEW IF EXISTS account_current")
            .await?;

        if manager.get_database_backend() == sea_orm::DatabaseBackend::Postgres {
            let _ = manager
                .drop_foreign_key(
                    ForeignKey::drop()
                        .name("fk_hopr_safe_redeemed_stats_safe_address")
                        .table(HoprSafeRedeemedStats::Table)
                        .to_owned(),
                )
                .await;
        }

        manager
            .drop_table(Table::drop().table(HoprSafeRedeemedStats::Table).if_exists().to_owned())
            .await?;
        manager
            .drop_table(Table::drop().table(SchemaVersion::Table).if_exists().to_owned())
            .await?;
        manager
            .drop_table(
                Table::drop()
                    .table(HoprNodeSafeRegistration::Table)
                    .if_exists()
                    .to_owned(),
            )
            .await?;
        manager
            .drop_table(Table::drop().table(HoprSafeContractState::Table).if_exists().to_owned())
            .await?;
        manager
            .drop_table(Table::drop().table(HoprSafeContract::Table).if_exists().to_owned())
            .await?;
        manager
            .drop_table(Table::drop().table(NativeBalance::Table).if_exists().to_owned())
            .await?;
        manager
            .drop_table(Table::drop().table(HoprBalance::Table).if_exists().to_owned())
            .await?;
        manager
            .drop_table(Table::drop().table(ChainInfo::Table).if_exists().to_owned())
            .await?;
        manager
            .drop_table(Table::drop().table(ChannelState::Table).if_exists().to_owned())
            .await?;
        manager
            .drop_table(Table::drop().table(AccountState::Table).if_exists().to_owned())
            .await?;
        manager
            .drop_table(Table::drop().table(Announcement::Table).if_exists().to_owned())
            .await?;
        manager
            .drop_table(Table::drop().table(Channel::Table).if_exists().to_owned())
            .await?;
        manager
            .drop_table(Table::drop().table(Account::Table).if_exists().to_owned())
            .await?;

        Ok(())
    }
}

#[derive(DeriveIden)]
enum Account {
    Table,
    Id,
    ChainKey,
    PacketKey,
    PublishedBlock,
    PublishedTxIndex,
    PublishedLogIndex,
}

#[derive(DeriveIden)]
enum AccountState {
    Table,
    Id,
    AccountId,
    SafeAddress,
    PublishedBlock,
    PublishedTxIndex,
    PublishedLogIndex,
}

#[derive(DeriveIden)]
enum Channel {
    Table,
    Id,
    Source,
    Destination,
    ConcreteChannelId,
}

#[derive(DeriveIden)]
enum ChannelState {
    Table,
    Id,
    ChannelId,
    Balance,
    Status,
    Epoch,
    TicketIndex,
    ClosureTime,
    CorruptedState,
    ReorgCorrection,
    PublishedBlock,
    PublishedTxIndex,
    PublishedLogIndex,
}

#[derive(DeriveIden)]
enum Announcement {
    Table,
    Id,
    AccountId,
    Multiaddress,
    PublishedBlock,
    PublishedTxIndex,
    PublishedLogIndex,
}

#[derive(DeriveIden)]
#[allow(dead_code)]
enum ChainInfo {
    Table,
    Id,
    LastIndexedBlock,
    LastIndexedTxIndex,
    LastIndexedLogIndex,
    ChannelClosureGracePeriod,
    KeyBindingFee,
    TicketPrice,
    ChannelsDST,
    LedgerDST,
    SafeRegistryDST,
    MinIncomingTicketWinProb,
}

#[derive(DeriveIden)]
enum HoprBalance {
    Table,
    Id,
    Address,
    Balance,
    LastChangedBlock,
    LastChangedTxIndex,
    LastChangedLogIndex,
}

#[derive(DeriveIden)]
enum NativeBalance {
    Table,
    Id,
    Address,
    Balance,
    LastChangedBlock,
    LastChangedTxIndex,
    LastChangedLogIndex,
}

#[derive(DeriveIden)]
enum HoprSafeContract {
    Table,
    Id,
    Address,
}

#[derive(DeriveIden)]
enum HoprSafeContractState {
    Table,
    Id,
    HoprSafeContractId,
    ModuleAddress,
    ChainKey,
    PublishedBlock,
    PublishedTxIndex,
    PublishedLogIndex,
}

#[derive(DeriveIden)]
enum HoprNodeSafeRegistration {
    Table,
    Id,
    SafeAddress,
    NodeAddress,
    RegisteredBlock,
    RegisteredTxIndex,
    RegisteredLogIndex,
}

#[derive(DeriveIden)]
enum SchemaVersion {
    Table,
    Id,
    Version,
    UpdatedAt,
}

#[derive(DeriveIden)]
enum HoprSafeRedeemedStats {
    Table,
    Id,
    SafeAddress,
    NodeAddress,
    RedeemedAmount,
    RedemptionCount,
    LastRedeemedBlock,
    LastRedeemedTxIndex,
    LastRedeemedLogIndex,
}
