use sea_orm_migration::prelude::*;

#[derive(DeriveMigrationName)]
pub struct Migration;

#[async_trait::async_trait]
impl MigrationTrait for Migration {
    async fn up(&self, manager: &SchemaManager) -> Result<(), DbErr> {
        // Create Account table (must be created before Channel due to FK constraint)
        manager
            .create_table(
                Table::create()
                    .table(Account::Table)
                    .if_not_exists()
                    .col(
                        ColumnDef::new(Account::Id)
                            .integer()
                            .not_null()
                            .auto_increment()
                            .primary_key(),
                    )
                    .col(ColumnDef::new(Account::ChainKey).binary_len(20).not_null())
                    .col(ColumnDef::new(Account::PacketKey).string_len(64).not_null())
                    .col(ColumnDef::new(Account::SafeAddress).binary_len(20).null())
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

        // Create Channel table
        manager
            .create_table(
                Table::create()
                    .table(Channel::Table)
                    .if_not_exists()
                    .col(
                        ColumnDef::new(Channel::Id)
                            .integer()
                            .not_null()
                            .auto_increment()
                            .primary_key(),
                    )
                    .col(
                        ColumnDef::new(Channel::ConcreteChannelId)
                            .string_len(64)
                            .not_null()
                            .unique_key(),
                    )
                    .col(ColumnDef::new(Channel::Source).integer().not_null())
                    .col(ColumnDef::new(Channel::Destination).integer().not_null())
                    .col(ColumnDef::new(Channel::Balance).binary_len(12).not_null())
                    .col(ColumnDef::new(Channel::Status).tiny_unsigned().not_null())
                    .col(ColumnDef::new(Channel::Epoch).big_integer().not_null().default(1))
                    .col(ColumnDef::new(Channel::TicketIndex).big_integer().not_null().default(0))
                    .col(ColumnDef::new(Channel::ClosureTime).timestamp().null())
                    .col(
                        ColumnDef::new(Channel::CorruptedState)
                            .boolean()
                            .not_null()
                            .default(false),
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

        // Create Announcement table
        manager
            .create_table(
                Table::create()
                    .table(Announcement::Table)
                    .if_not_exists()
                    .col(
                        ColumnDef::new(Announcement::Id)
                            .integer()
                            .not_null()
                            .auto_increment()
                            .primary_key(),
                    )
                    .col(ColumnDef::new(Announcement::AccountId).integer().not_null())
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

        // Create NodeInfo table
        manager
            .create_table(
                Table::create()
                    .table(NodeInfo::Table)
                    .if_not_exists()
                    .col(
                        ColumnDef::new(NodeInfo::Id)
                            .integer()
                            .not_null()
                            .auto_increment()
                            .primary_key(),
                    )
                    .col(
                        ColumnDef::new(NodeInfo::SafeBalance)
                            .binary_len(12)
                            .not_null()
                            .default(vec![0u8; 12]),
                    )
                    .col(
                        ColumnDef::new(NodeInfo::SafeAllowance)
                            .binary_len(12)
                            .not_null()
                            .default(vec![0u8; 12]),
                    )
                    .col(ColumnDef::new(NodeInfo::SafeAddress).binary_len(20).null())
                    .col(ColumnDef::new(NodeInfo::ModuleAddress).binary_len(20).null())
                    .to_owned(),
            )
            .await?;

        // Create ChainInfo table
        manager
            .create_table(
                Table::create()
                    .table(ChainInfo::Table)
                    .if_not_exists()
                    .col(
                        ColumnDef::new(ChainInfo::Id)
                            .integer()
                            .not_null()
                            .auto_increment()
                            .primary_key(),
                    )
                    .col(
                        ColumnDef::new(ChainInfo::LastIndexedBlock)
                            .big_integer()
                            .not_null()
                            .default(0),
                    )
                    .col(ColumnDef::new(ChainInfo::TicketPrice).binary_len(12).null())
                    .col(ColumnDef::new(ChainInfo::ChannelsDST).binary_len(32).null())
                    .col(ColumnDef::new(ChainInfo::LedgerDST).binary_len(32).null())
                    .col(ColumnDef::new(ChainInfo::SafeRegistryDST).binary_len(32).null())
                    .col(
                        ColumnDef::new(ChainInfo::MinIncomingTicketWinProb)
                            .float()
                            .not_null()
                            .default(hopr_internal_types::protocol::DEFAULT_MINIMUM_INCOMING_TICKET_WIN_PROB),
                    )
                    .to_owned(),
            )
            .await
    }

    async fn down(&self, manager: &SchemaManager) -> Result<(), DbErr> {
        manager
            .drop_table(Table::drop().table(ChainInfo::Table).to_owned())
            .await?;
        manager
            .drop_table(Table::drop().table(NodeInfo::Table).to_owned())
            .await?;
        manager
            .drop_table(Table::drop().table(Announcement::Table).to_owned())
            .await?;
        // Drop Channel before Account due to FK constraint
        manager
            .drop_table(Table::drop().table(Channel::Table).to_owned())
            .await?;
        manager.drop_table(Table::drop().table(Account::Table).to_owned()).await
    }
}

#[derive(DeriveIden)]
enum Channel {
    Table,
    Id,
    ConcreteChannelId,
    Source,
    Destination,
    Balance,
    Status,
    Epoch,
    TicketIndex,
    ClosureTime,
    CorruptedState,
}

#[derive(DeriveIden)]
enum Account {
    Table,
    Id,
    ChainKey,
    PacketKey,
    SafeAddress,
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
enum NodeInfo {
    Table,
    Id,
    SafeBalance,
    SafeAllowance,
    SafeAddress,
    ModuleAddress,
}

#[derive(DeriveIden)]
enum ChainInfo {
    Table,
    Id,
    LastIndexedBlock,
    TicketPrice,
    ChannelsDST,
    LedgerDST,
    SafeRegistryDST,
    MinIncomingTicketWinProb,
    ChannelClosureGracePeriod,
}
