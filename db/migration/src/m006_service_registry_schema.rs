use sea_orm_migration::prelude::*;

/// Adds the `HoprServiceRegistry` schema: the permissionless registry of service types and of the
/// entries that nodes publish under them.
///
/// The layout follows the `X` / `X_state` / `X_current` pattern used by `account` and `channel`:
/// an identity table that never changes, an append-only state table keyed by the log position that
/// produced it, and a view projecting the newest state row per identity.
///
/// Two registry-wide values, the service type registration fee and the `NodeSafeRegistry` pointer,
/// live in the `service_registry_config` singleton rather than in `chain_info`, so that the table
/// every consumer already reads does not grow for values only the registry cares about.
///
/// The node of an entry is stored as a raw `node_address` and not as a foreign key into `account`.
/// The registry does not require a node to have announced itself, so an entry can legitimately
/// exist for a node with no `account` row; consumers join opportunistically, the way
/// `hopr_safe_redeemed_stats` does.
#[derive(DeriveMigrationName)]
pub struct Migration;

#[async_trait::async_trait]
impl MigrationTrait for Migration {
    async fn up(&self, manager: &SchemaManager) -> Result<(), DbErr> {
        let backend = manager.get_database_backend();

        // === service_type ===
        manager
            .create_table(
                Table::create()
                    .table(ServiceType::Table)
                    .if_not_exists()
                    .col(
                        ColumnDef::new(ServiceType::Id)
                            .big_integer()
                            .not_null()
                            .auto_increment()
                            .primary_key(),
                    )
                    .col(
                        ColumnDef::new(ServiceType::ServiceType)
                            .binary_len(32)
                            .not_null()
                            .unique_key(),
                    )
                    .to_owned(),
            )
            .await?;

        // === service_type_state ===
        manager
            .create_table(
                Table::create()
                    .table(ServiceTypeState::Table)
                    .if_not_exists()
                    .col(
                        ColumnDef::new(ServiceTypeState::Id)
                            .big_integer()
                            .not_null()
                            .auto_increment()
                            .primary_key(),
                    )
                    .col(ColumnDef::new(ServiceTypeState::ServiceTypeId).big_integer().not_null())
                    // NULL owner means the type was abandoned: the contract's zero-address sentinel.
                    .col(ColumnDef::new(ServiceTypeState::OwnerAddress).binary_len(20).null())
                    // NULL requirement means the type is open to any node.
                    .col(
                        ColumnDef::new(ServiceTypeState::RequirementAddress)
                            .binary_len(20)
                            .null(),
                    )
                    .col(
                        ColumnDef::new(ServiceTypeState::RegistrationBurn)
                            .binary_len(32)
                            .not_null(),
                    )
                    .col(ColumnDef::new(ServiceTypeState::UpdateBurn).binary_len(32).not_null())
                    .col(
                        ColumnDef::new(ServiceTypeState::PublishedBlock)
                            .big_integer()
                            .not_null(),
                    )
                    .col(
                        ColumnDef::new(ServiceTypeState::PublishedTxIndex)
                            .big_integer()
                            .not_null(),
                    )
                    .col(
                        ColumnDef::new(ServiceTypeState::PublishedLogIndex)
                            .big_integer()
                            .not_null(),
                    )
                    .foreign_key(
                        ForeignKey::create()
                            .name("fk_service_type_state_service_type_id")
                            .from(ServiceTypeState::Table, ServiceTypeState::ServiceTypeId)
                            .to(ServiceType::Table, ServiceType::Id)
                            .on_delete(ForeignKeyAction::Cascade)
                            .on_update(ForeignKeyAction::Cascade),
                    )
                    .to_owned(),
            )
            .await?;

        manager
            .create_index(
                Index::create()
                    .name("idx_service_type_state_unique_position")
                    .table(ServiceTypeState::Table)
                    .col(ServiceTypeState::ServiceTypeId)
                    .col(ServiceTypeState::PublishedBlock)
                    .col(ServiceTypeState::PublishedTxIndex)
                    .col(ServiceTypeState::PublishedLogIndex)
                    .unique()
                    .to_owned(),
            )
            .await?;

        manager
            .create_index(
                Index::create()
                    .name("idx_service_type_state_position")
                    .table(ServiceTypeState::Table)
                    .col(ServiceTypeState::ServiceTypeId)
                    .col((ServiceTypeState::PublishedBlock, IndexOrder::Desc))
                    .col((ServiceTypeState::PublishedTxIndex, IndexOrder::Desc))
                    .col((ServiceTypeState::PublishedLogIndex, IndexOrder::Desc))
                    .to_owned(),
            )
            .await?;

        // === service_entry ===
        manager
            .create_table(
                Table::create()
                    .table(ServiceEntry::Table)
                    .if_not_exists()
                    .col(
                        ColumnDef::new(ServiceEntry::Id)
                            .big_integer()
                            .not_null()
                            .auto_increment()
                            .primary_key(),
                    )
                    .col(ColumnDef::new(ServiceEntry::ServiceTypeId).big_integer().not_null())
                    .col(ColumnDef::new(ServiceEntry::NodeAddress).binary_len(20).not_null())
                    .foreign_key(
                        ForeignKey::create()
                            .name("fk_service_entry_service_type_id")
                            .from(ServiceEntry::Table, ServiceEntry::ServiceTypeId)
                            .to(ServiceType::Table, ServiceType::Id)
                            .on_delete(ForeignKeyAction::Cascade)
                            .on_update(ForeignKeyAction::Cascade),
                    )
                    .to_owned(),
            )
            .await?;

        manager
            .create_index(
                Index::create()
                    .name("idx_service_entry_unique_type_node")
                    .table(ServiceEntry::Table)
                    .col(ServiceEntry::ServiceTypeId)
                    .col(ServiceEntry::NodeAddress)
                    .unique()
                    .to_owned(),
            )
            .await?;

        manager
            .create_index(
                Index::create()
                    .if_not_exists()
                    .name("idx_service_entry_node_address")
                    .table(ServiceEntry::Table)
                    .col(ServiceEntry::NodeAddress)
                    .to_owned(),
            )
            .await?;

        // === service_entry_state ===
        manager
            .create_table(
                Table::create()
                    .table(ServiceEntryState::Table)
                    .if_not_exists()
                    .col(
                        ColumnDef::new(ServiceEntryState::Id)
                            .big_integer()
                            .not_null()
                            .auto_increment()
                            .primary_key(),
                    )
                    .col(
                        ColumnDef::new(ServiceEntryState::ServiceEntryId)
                            .big_integer()
                            .not_null(),
                    )
                    // The four entry columns below are NULL on a deregistration tombstone: the
                    // `Deregistered` event removes the entry on-chain and carries no entry data.
                    .col(ColumnDef::new(ServiceEntryState::SafeAddress).binary_len(20).null())
                    .col(
                        ColumnDef::new(ServiceEntryState::Metadata)
                            .binary_len(SERVICE_METADATA_MAX_LENGTH)
                            .null(),
                    )
                    .col(
                        ColumnDef::new(ServiceEntryState::RegisteredAt)
                            .timestamp_with_time_zone()
                            .null(),
                    )
                    .col(
                        ColumnDef::new(ServiceEntryState::UpdatedAt)
                            .timestamp_with_time_zone()
                            .null(),
                    )
                    .col(
                        ColumnDef::new(ServiceEntryState::Deregistered)
                            .boolean()
                            .not_null()
                            .default(false),
                    )
                    .col(
                        ColumnDef::new(ServiceEntryState::PublishedBlock)
                            .big_integer()
                            .not_null(),
                    )
                    .col(
                        ColumnDef::new(ServiceEntryState::PublishedTxIndex)
                            .big_integer()
                            .not_null(),
                    )
                    .col(
                        ColumnDef::new(ServiceEntryState::PublishedLogIndex)
                            .big_integer()
                            .not_null(),
                    )
                    .foreign_key(
                        ForeignKey::create()
                            .name("fk_service_entry_state_service_entry_id")
                            .from(ServiceEntryState::Table, ServiceEntryState::ServiceEntryId)
                            .to(ServiceEntry::Table, ServiceEntry::Id)
                            .on_delete(ForeignKeyAction::Cascade)
                            .on_update(ForeignKeyAction::Cascade),
                    )
                    .to_owned(),
            )
            .await?;

        manager
            .create_index(
                Index::create()
                    .name("idx_service_entry_state_unique_position")
                    .table(ServiceEntryState::Table)
                    .col(ServiceEntryState::ServiceEntryId)
                    .col(ServiceEntryState::PublishedBlock)
                    .col(ServiceEntryState::PublishedTxIndex)
                    .col(ServiceEntryState::PublishedLogIndex)
                    .unique()
                    .to_owned(),
            )
            .await?;

        manager
            .create_index(
                Index::create()
                    .name("idx_service_entry_state_position")
                    .table(ServiceEntryState::Table)
                    .col(ServiceEntryState::ServiceEntryId)
                    .col((ServiceEntryState::PublishedBlock, IndexOrder::Desc))
                    .col((ServiceEntryState::PublishedTxIndex, IndexOrder::Desc))
                    .col((ServiceEntryState::PublishedLogIndex, IndexOrder::Desc))
                    .to_owned(),
            )
            .await?;

        // === service_registry_config (singleton) ===
        manager
            .create_table(
                Table::create()
                    .table(ServiceRegistryConfig::Table)
                    .if_not_exists()
                    .col(
                        ColumnDef::new(ServiceRegistryConfig::Id)
                            .big_integer()
                            .not_null()
                            .primary_key(),
                    )
                    .col(
                        ColumnDef::new(ServiceRegistryConfig::TypeRegistrationFee)
                            .binary_len(32)
                            .not_null()
                            .default(vec![0u8; 32]),
                    )
                    // NULL until the registry reports its `NodeSafeRegistry` pointer.
                    .col(
                        ColumnDef::new(ServiceRegistryConfig::NodeSafeRegistry)
                            .binary_len(20)
                            .null(),
                    )
                    // Position of the newest config log applied, so that replaying an older log
                    // cannot overwrite a newer value.
                    .col(
                        ColumnDef::new(ServiceRegistryConfig::LastChangedBlock)
                            .big_integer()
                            .not_null()
                            .default(0),
                    )
                    .col(
                        ColumnDef::new(ServiceRegistryConfig::LastChangedTxIndex)
                            .big_integer()
                            .not_null()
                            .default(0),
                    )
                    .col(
                        ColumnDef::new(ServiceRegistryConfig::LastChangedLogIndex)
                            .big_integer()
                            .not_null()
                            .default(0),
                    )
                    .to_owned(),
            )
            .await?;

        // === views ===
        // The correlated `ORDER BY ... LIMIT 1` subquery is mandatory here: the
        // `ROW_NUMBER() OVER (PARTITION BY ...)` form is an optimization fence in PostgreSQL,
        // which is why m005 rewrote the m001 views away from it.
        let create_view = if backend == sea_orm::DatabaseBackend::Postgres {
            "CREATE OR REPLACE VIEW"
        } else {
            "CREATE VIEW IF NOT EXISTS"
        };

        manager
            .get_connection()
            .execute_unprepared(&format!(
                "{create_view} service_type_current AS
                SELECT
                    sts.id,
                    st.id AS service_type_id,
                    st.service_type,
                    sts.owner_address,
                    sts.requirement_address,
                    sts.registration_burn,
                    sts.update_burn,
                    sts.published_block,
                    sts.published_tx_index,
                    sts.published_log_index
                FROM service_type st
                JOIN service_type_state sts ON sts.service_type_id = st.id
                WHERE sts.id = (
                    SELECT s2.id FROM service_type_state s2
                    WHERE s2.service_type_id = st.id
                    ORDER BY s2.published_block DESC, s2.published_tx_index DESC, s2.published_log_index DESC
                    LIMIT 1
                )"
            ))
            .await?;

        manager
            .get_connection()
            .execute_unprepared(&format!(
                "{create_view} service_entry_current AS
                SELECT
                    ses.id,
                    se.id AS service_entry_id,
                    se.service_type_id,
                    st.service_type,
                    se.node_address,
                    ses.safe_address,
                    ses.metadata,
                    ses.registered_at,
                    ses.updated_at,
                    ses.deregistered,
                    ses.published_block,
                    ses.published_tx_index,
                    ses.published_log_index
                FROM service_entry se
                JOIN service_type st ON st.id = se.service_type_id
                JOIN service_entry_state ses ON ses.service_entry_id = se.id
                WHERE ses.id = (
                    SELECT s2.id FROM service_entry_state s2
                    WHERE s2.service_entry_id = se.id
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
            .execute_unprepared("DROP VIEW IF EXISTS service_entry_current")
            .await?;

        manager
            .get_connection()
            .execute_unprepared("DROP VIEW IF EXISTS service_type_current")
            .await?;

        manager
            .drop_table(Table::drop().table(ServiceRegistryConfig::Table).if_exists().to_owned())
            .await?;
        manager
            .drop_table(Table::drop().table(ServiceEntryState::Table).if_exists().to_owned())
            .await?;
        manager
            .drop_table(Table::drop().table(ServiceEntry::Table).if_exists().to_owned())
            .await?;
        manager
            .drop_table(Table::drop().table(ServiceTypeState::Table).if_exists().to_owned())
            .await?;
        manager
            .drop_table(Table::drop().table(ServiceType::Table).if_exists().to_owned())
            .await?;

        Ok(())
    }
}

/// Hard cap on the metadata of a single entry, mirroring `MAX_METADATA_LENGTH` in
/// `ServiceRegistry.sol` and `hopr_types::internal::service::ServiceMetadata::MAX_LENGTH`.
///
/// The contract is not upgradeable and applies the cap on every write path, so the value is
/// permanent.
const SERVICE_METADATA_MAX_LENGTH: u32 = 2048;

// The identity column carries the same name as its table, which is what the schema calls it.
#[allow(clippy::enum_variant_names)]
#[derive(DeriveIden)]
enum ServiceType {
    Table,
    Id,
    ServiceType,
}

#[derive(DeriveIden)]
enum ServiceTypeState {
    Table,
    Id,
    ServiceTypeId,
    OwnerAddress,
    RequirementAddress,
    RegistrationBurn,
    UpdateBurn,
    PublishedBlock,
    PublishedTxIndex,
    PublishedLogIndex,
}

#[derive(DeriveIden)]
enum ServiceEntry {
    Table,
    Id,
    ServiceTypeId,
    NodeAddress,
}

#[derive(DeriveIden)]
enum ServiceEntryState {
    Table,
    Id,
    ServiceEntryId,
    SafeAddress,
    Metadata,
    RegisteredAt,
    UpdatedAt,
    Deregistered,
    PublishedBlock,
    PublishedTxIndex,
    PublishedLogIndex,
}

#[derive(DeriveIden)]
enum ServiceRegistryConfig {
    Table,
    Id,
    TypeRegistrationFee,
    NodeSafeRegistry,
    LastChangedBlock,
    LastChangedTxIndex,
    LastChangedLogIndex,
}
