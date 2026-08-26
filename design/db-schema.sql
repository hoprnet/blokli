CREATE TABLE "account" (
    "id" integer NOT NULL PRIMARY KEY AUTOINCREMENT,
    "chain_key" blob (20) NOT NULL,
    "packet_key" varchar(64) NOT NULL,
    "published_block" integer NOT NULL DEFAULT 0,
    "published_tx_index" integer NOT NULL DEFAULT 0,
    "published_log_index" integer NOT NULL DEFAULT 0
);

CREATE TABLE "account_state" (
    "id" integer NOT NULL PRIMARY KEY AUTOINCREMENT,
    "account_id" integer NOT NULL,
    "safe_address" blob (20) NULL,
    "published_block" integer NOT NULL,
    "published_tx_index" integer NOT NULL,
    "published_log_index" integer NOT NULL,
    FOREIGN KEY ("account_id") REFERENCES "account" ("id") ON DELETE CASCADE ON UPDATE CASCADE
);

CREATE TABLE "announcement" (
    "id" integer NOT NULL PRIMARY KEY AUTOINCREMENT,
    "account_id" integer NOT NULL,
    "multiaddress" text NOT NULL,
    "published_block" integer NOT NULL DEFAULT 0,
    "published_tx_index" integer NOT NULL DEFAULT 0,
    "published_log_index" integer NOT NULL DEFAULT 0,
    FOREIGN KEY ("account_id") REFERENCES "account" ("id") ON DELETE CASCADE ON UPDATE CASCADE
);

CREATE TABLE "chain_info" (
    "id" integer NOT NULL PRIMARY KEY,
    "last_indexed_block" integer NOT NULL DEFAULT 0,
    "last_indexed_tx_index" integer NULL,
    "last_indexed_log_index" integer NULL,
    "ticket_price" blob (12) NULL,
    "channels_dst" blob (32) NULL,
    "ledger_dst" blob (32) NULL,
    "safe_registry_dst" blob (32) NULL,
    "min_incoming_ticket_win_prob" double NOT NULL DEFAULT 1,
    "channel_closure_grace_period" integer NULL,
    "key_binding_fee" blob (12) NULL
);

CREATE TABLE "channel" (
    "id" integer NOT NULL PRIMARY KEY AUTOINCREMENT,
    "concrete_channel_id" varchar(64) NOT NULL UNIQUE,
    "source" integer NOT NULL,
    "destination" integer NOT NULL,
    FOREIGN KEY ("source") REFERENCES "account" ("id") ON DELETE CASCADE ON UPDATE CASCADE,
    FOREIGN KEY ("destination") REFERENCES "account" ("id") ON DELETE CASCADE ON UPDATE CASCADE
);

CREATE TABLE "channel_state" (
    "id" integer NOT NULL PRIMARY KEY AUTOINCREMENT,
    "channel_id" integer NOT NULL,
    "balance" blob (12) NOT NULL,
    "status" smallint NOT NULL,
    "epoch" integer NOT NULL,
    "ticket_index" integer NOT NULL,
    "closure_time" timestamp_with_timezone_text NULL,
    "corrupted_state" boolean NOT NULL DEFAULT FALSE,
    "published_block" integer NOT NULL,
    "published_tx_index" integer NOT NULL,
    "published_log_index" integer NOT NULL,
    "reorg_correction" boolean NOT NULL DEFAULT FALSE,
    FOREIGN KEY ("channel_id") REFERENCES "channel" ("id") ON DELETE CASCADE ON UPDATE CASCADE
);

CREATE TABLE "curvy_committed_note" (
    "id" integer PRIMARY KEY AUTOINCREMENT,
    "batch_index" blob (32) NOT NULL,
    "note_id" blob (32) NOT NULL,
    "event_item_index" integer NOT NULL,
    "chain_tx_hash" blob (32) NOT NULL,
    "block_hash" blob (32) NOT NULL,
    "published_block" integer NOT NULL,
    "published_tx_index" integer NOT NULL,
    "published_log_index" integer NOT NULL,
    "leaf_index" integer NOT NULL,
    CONSTRAINT "idx_curvy_committed_note_unique_position" UNIQUE ("published_block", "published_tx_index", "published_log_index", "event_item_index"),
    CONSTRAINT "idx_curvy_committed_note_leaf_index" UNIQUE ("leaf_index")
);

CREATE TABLE "curvy_committed_nullifier" (
    "id" integer PRIMARY KEY AUTOINCREMENT,
    "batch_index" blob (32) NOT NULL,
    "nullifier" blob (32) NOT NULL,
    "event_item_index" integer NOT NULL,
    "chain_tx_hash" blob (32) NOT NULL,
    "block_hash" blob (32) NOT NULL,
    "published_block" integer NOT NULL,
    "published_tx_index" integer NOT NULL,
    "published_log_index" integer NOT NULL,
    "nullifier_index" integer NOT NULL,
    CONSTRAINT "idx_curvy_committed_nullifier_unique_position" UNIQUE ("published_block", "published_tx_index", "published_log_index", "event_item_index"),
    CONSTRAINT "idx_curvy_committed_nullifier_index" UNIQUE ("nullifier_index")
);

CREATE TABLE "curvy_pending_note" (
    "id" integer PRIMARY KEY AUTOINCREMENT,
    "note_id" blob (32) NOT NULL,
    "ephemeral_key_x" blob (32) NOT NULL,
    "ephemeral_key_y" blob (32) NOT NULL,
    "view_tag" integer NOT NULL,
    "token_id" blob (32) NOT NULL,
    "amount" blob (32) NOT NULL,
    "is_plaintext" boolean NOT NULL,
    "event_item_index" integer NOT NULL,
    "chain_tx_hash" blob (32) NOT NULL,
    "block_hash" blob (32) NOT NULL,
    "published_block" integer NOT NULL,
    "published_tx_index" integer NOT NULL,
    "published_log_index" integer NOT NULL,
    CONSTRAINT "idx_curvy_pending_note_unique_position" UNIQUE ("published_block", "published_tx_index", "published_log_index", "event_item_index")
);

CREATE TABLE "curvy_shard_root" (
    "id" integer PRIMARY KEY AUTOINCREMENT,
    "tree_version" integer NOT NULL,
    "shard_height" integer NOT NULL,
    "shard_index" integer NOT NULL,
    "root" blob (32) NOT NULL,
    "block_hash" blob (32) NOT NULL,
    "chain_tx_hash" blob (32) NOT NULL,
    "completion_block" integer NOT NULL,
    "completion_tx_index" integer NOT NULL,
    "completion_log_index" integer NOT NULL,
    "completion_event_item_index" integer NOT NULL,
    CONSTRAINT "idx_curvy_shard_root_geometry_index" UNIQUE ("tree_version", "shard_height", "shard_index")
);

CREATE TABLE "curvy_sync_checkpoint" (
    "id" integer PRIMARY KEY AUTOINCREMENT,
    "block_number" integer NOT NULL,
    "block_hash" blob (32) NOT NULL UNIQUE,
    "aggregator_address" blob (20) NOT NULL,
    "tree_version" integer NOT NULL,
    "tree_depth" integer NOT NULL,
    "shard_height" integer NOT NULL,
    "leaf_count" integer NOT NULL,
    "nullifier_count" integer NOT NULL,
    "shard_count" integer NOT NULL,
    "root" blob (32) NOT NULL,
    "frontier_snapshot" blob NOT NULL,
    CONSTRAINT "idx_curvy_sync_checkpoint_block" UNIQUE ("block_number")
);

CREATE TABLE "hopr_balance" (
    "id" integer NOT NULL PRIMARY KEY AUTOINCREMENT,
    "address" blob (20) NOT NULL UNIQUE,
    "balance" blob (12) NOT NULL DEFAULT x'000000000000000000000000',
    "last_changed_block" integer NOT NULL DEFAULT 0,
    "last_changed_tx_index" integer NOT NULL DEFAULT 0,
    "last_changed_log_index" integer NOT NULL DEFAULT 0
);

CREATE TABLE "hopr_node_safe_registration" (
    "id" integer NOT NULL PRIMARY KEY AUTOINCREMENT,
    "safe_address" blob (20) NOT NULL,
    "node_address" blob (20) NOT NULL UNIQUE,
    "registered_block" integer NOT NULL,
    "registered_tx_index" integer NOT NULL,
    "registered_log_index" integer NOT NULL
);

CREATE TABLE "hopr_safe_contract" (
    "id" integer NOT NULL PRIMARY KEY AUTOINCREMENT,
    "address" blob (20) NOT NULL UNIQUE
);

CREATE TABLE "hopr_safe_contract_state" (
    "id" integer NOT NULL PRIMARY KEY AUTOINCREMENT,
    "hopr_safe_contract_id" integer NOT NULL,
    "module_address" blob (20) NOT NULL,
    "chain_key" blob (20) NOT NULL,
    "published_block" integer NOT NULL,
    "published_tx_index" integer NOT NULL,
    "published_log_index" integer NOT NULL,
    FOREIGN KEY ("hopr_safe_contract_id") REFERENCES "hopr_safe_contract" ("id") ON DELETE CASCADE ON UPDATE CASCADE
);

CREATE TABLE "hopr_safe_event" (
    "id" integer NOT NULL PRIMARY KEY AUTOINCREMENT,
    "hopr_safe_contract_id" integer NOT NULL,
    "event_kind" varchar NOT NULL,
    "chain_tx_hash" blob (32) NOT NULL,
    "published_block" integer NOT NULL,
    "published_tx_index" integer NOT NULL,
    "published_log_index" integer NOT NULL,
    FOREIGN KEY ("hopr_safe_contract_id") REFERENCES "hopr_safe_contract" ("id") ON DELETE CASCADE ON UPDATE CASCADE
);

CREATE TABLE "hopr_safe_execution_event" (
    "hopr_safe_event_id" integer NOT NULL PRIMARY KEY,
    "safe_tx_hash" blob (32) NOT NULL,
    "payment" varchar NOT NULL,
    FOREIGN KEY ("hopr_safe_event_id") REFERENCES "hopr_safe_event" ("id") ON DELETE CASCADE ON UPDATE CASCADE
);

CREATE TABLE "hopr_safe_owner_change_event" (
    "hopr_safe_event_id" integer NOT NULL PRIMARY KEY,
    "owner_address" blob (20) NOT NULL,
    FOREIGN KEY ("hopr_safe_event_id") REFERENCES "hopr_safe_event" ("id") ON DELETE CASCADE ON UPDATE CASCADE
);

CREATE TABLE "hopr_safe_owner_state" (
    "id" integer NOT NULL PRIMARY KEY AUTOINCREMENT,
    "hopr_safe_contract_id" integer NOT NULL,
    "owner_address" blob (20) NOT NULL,
    "is_current_owner" boolean NOT NULL,
    "published_block" integer NOT NULL,
    "published_tx_index" integer NOT NULL,
    "published_log_index" integer NOT NULL,
    FOREIGN KEY ("hopr_safe_contract_id") REFERENCES "hopr_safe_contract" ("id") ON DELETE CASCADE ON UPDATE CASCADE
);

CREATE TABLE "hopr_safe_redeemed_stats" (
    "id" integer NOT NULL PRIMARY KEY AUTOINCREMENT,
    "safe_address" blob (20) NOT NULL,
    "node_address" blob (20) NOT NULL,
    "redeemed_amount" blob (32) NOT NULL DEFAULT x'0000000000000000000000000000000000000000000000000000000000000000',
    "redemption_count" integer NOT NULL DEFAULT 0,
    "last_redeemed_block" integer NOT NULL DEFAULT 0,
    "last_redeemed_tx_index" integer NOT NULL DEFAULT 0,
    "last_redeemed_log_index" integer NOT NULL DEFAULT 0,
    "rejected_amount" blob (32) NOT NULL DEFAULT x'0000000000000000000000000000000000000000000000000000000000000000',
    "rejection_count" integer NOT NULL DEFAULT 0
);

CREATE TABLE "hopr_safe_setup_event" (
    "hopr_safe_event_id" integer NOT NULL PRIMARY KEY,
    "initiator_address" blob (20),
    "threshold" varchar NOT NULL,
    FOREIGN KEY ("hopr_safe_event_id") REFERENCES "hopr_safe_event" ("id") ON DELETE CASCADE ON UPDATE CASCADE
);

CREATE TABLE "hopr_safe_setup_owner" (
    "id" integer NOT NULL PRIMARY KEY AUTOINCREMENT,
    "hopr_safe_event_id" integer NOT NULL,
    "owner_position" integer NOT NULL,
    "owner_address" blob (20) NOT NULL,
    FOREIGN KEY ("hopr_safe_event_id") REFERENCES "hopr_safe_setup_event" ("hopr_safe_event_id") ON DELETE CASCADE ON UPDATE CASCADE
);

CREATE TABLE "hopr_safe_threshold_change_event" (
    "hopr_safe_event_id" integer NOT NULL PRIMARY KEY,
    "threshold" varchar NOT NULL,
    FOREIGN KEY ("hopr_safe_event_id") REFERENCES "hopr_safe_event" ("id") ON DELETE CASCADE ON UPDATE CASCADE
);

CREATE TABLE "hopr_safe_threshold_state" (
    "id" integer NOT NULL PRIMARY KEY AUTOINCREMENT,
    "hopr_safe_contract_id" integer NOT NULL,
    "threshold" varchar NOT NULL,
    "published_block" integer NOT NULL,
    "published_tx_index" integer NOT NULL,
    "published_log_index" integer NOT NULL,
    FOREIGN KEY ("hopr_safe_contract_id") REFERENCES "hopr_safe_contract" ("id") ON DELETE CASCADE ON UPDATE CASCADE
);

CREATE TABLE "log" (
    "id" integer NOT NULL PRIMARY KEY AUTOINCREMENT,
    "tx_index" integer NOT NULL,
    "log_index" integer NOT NULL,
    "block_number" integer NOT NULL,
    "block_hash" blob (32) NOT NULL,
    "transaction_hash" blob (32) NOT NULL,
    "address" blob (20) NOT NULL,
    "topics" blob (1) NOT NULL,
    "data" blob (1) NOT NULL,
    "removed" boolean NOT NULL DEFAULT FALSE
);

CREATE TABLE "log_status" (
    "id" integer NOT NULL PRIMARY KEY AUTOINCREMENT,
    "log_id" integer NOT NULL,
    "tx_index" integer NOT NULL,
    "log_index" integer NOT NULL,
    "block_number" integer NOT NULL,
    "processed" boolean NOT NULL DEFAULT FALSE,
    "processed_at" datetime_text,
    "checksum" blob (32),
    FOREIGN KEY ("log_id") REFERENCES "log" ("id") ON DELETE CASCADE ON UPDATE CASCADE
);

CREATE TABLE "log_topic_info" (
    "id" integer NOT NULL PRIMARY KEY AUTOINCREMENT,
    "address" blob (20) NOT NULL,
    "topic" blob (32) NOT NULL
);

CREATE TABLE "native_balance" (
    "id" integer NOT NULL PRIMARY KEY AUTOINCREMENT,
    "address" blob (20) NOT NULL UNIQUE,
    "balance" blob (12) NOT NULL DEFAULT x'000000000000000000000000',
    "last_changed_block" integer NOT NULL DEFAULT 0,
    "last_changed_tx_index" integer NOT NULL DEFAULT 0,
    "last_changed_log_index" integer NOT NULL DEFAULT 0
);

CREATE TABLE "schema_version" (
    "id" integer NOT NULL PRIMARY KEY,
    "version" text NOT NULL,
    "updated_at" timestamp_text NOT NULL DEFAULT CURRENT_TIMESTAMP
);

CREATE TABLE "service_entry" (
    "id" integer NOT NULL PRIMARY KEY AUTOINCREMENT,
    "service_type_id" integer NOT NULL,
    "node_address" blob (20) NOT NULL,
    FOREIGN KEY ("service_type_id") REFERENCES "service_type" ("id") ON DELETE CASCADE ON UPDATE CASCADE
);

CREATE TABLE "service_entry_state" (
    "id" integer NOT NULL PRIMARY KEY AUTOINCREMENT,
    "service_entry_id" integer NOT NULL,
    "safe_address" blob (20) NULL,
    "metadata" blob (2048) NULL,
    "registered_at" timestamp_with_timezone_text NULL,
    "updated_at" timestamp_with_timezone_text NULL,
    "deregistered" boolean NOT NULL DEFAULT FALSE,
    "published_block" integer NOT NULL,
    "published_tx_index" integer NOT NULL,
    "published_log_index" integer NOT NULL,
    FOREIGN KEY ("service_entry_id") REFERENCES "service_entry" ("id") ON DELETE CASCADE ON UPDATE CASCADE
);

CREATE TABLE "service_registry_config" (
    "id" integer NOT NULL PRIMARY KEY,
    "type_registration_fee" blob (32) NOT NULL DEFAULT x'0000000000000000000000000000000000000000000000000000000000000000',
    "node_safe_registry" blob (20) NULL,
    "last_changed_block" integer NOT NULL DEFAULT 0,
    "last_changed_tx_index" integer NOT NULL DEFAULT 0,
    "last_changed_log_index" integer NOT NULL DEFAULT 0
);

CREATE TABLE "service_type" (
    "id" integer NOT NULL PRIMARY KEY AUTOINCREMENT,
    "service_type" blob (32) NOT NULL UNIQUE
);

CREATE TABLE "service_type_state" (
    "id" integer NOT NULL PRIMARY KEY AUTOINCREMENT,
    "service_type_id" integer NOT NULL,
    "owner_address" blob (20) NULL,
    "requirement_address" blob (20) NULL,
    "registration_burn" blob (32) NOT NULL,
    "update_burn" blob (32) NOT NULL,
    "published_block" integer NOT NULL,
    "published_tx_index" integer NOT NULL,
    "published_log_index" integer NOT NULL,
    FOREIGN KEY ("service_type_id") REFERENCES "service_type" ("id") ON DELETE CASCADE ON UPDATE CASCADE
);

CREATE VIEW account_current AS
SELECT
    acs.id,
    a.id AS account_id,
    a.chain_key,
    a.packet_key,
    acs.safe_address,
    acs.published_block,
    acs.published_tx_index,
    acs.published_log_index
FROM
    account a
    JOIN account_state acs ON acs.account_id = a.id
WHERE
    acs.id = (
        SELECT
            s2.id
        FROM
            account_state s2
        WHERE
            s2.account_id = a.id
        ORDER BY
            s2.published_block DESC,
            s2.published_tx_index DESC,
            s2.published_log_index DESC
        LIMIT 1);

CREATE VIEW channel_current AS
SELECT
    cs.id,
    c.id AS channel_id,
    c.concrete_channel_id,
    c.source,
    c.destination,
    cs.balance,
    cs.status,
    cs.epoch,
    cs.ticket_index,
    cs.closure_time,
    cs.corrupted_state,
    cs.published_block,
    cs.published_tx_index,
    cs.published_log_index,
    cs.reorg_correction
FROM
    channel c
    JOIN channel_state cs ON cs.channel_id = c.id
WHERE
    cs.id = (
        SELECT
            s2.id
        FROM
            channel_state s2
        WHERE
            s2.channel_id = c.id
        ORDER BY
            s2.published_block DESC,
            s2.published_tx_index DESC,
            s2.published_log_index DESC
        LIMIT 1);

CREATE VIEW safe_contract_current AS
SELECT
    sc.id AS safe_contract_id,
    sc.address,
    scs.module_address,
    scs.chain_key,
    scs.published_block,
    scs.published_tx_index,
    scs.published_log_index
FROM
    hopr_safe_contract sc
    JOIN hopr_safe_contract_state scs ON scs.hopr_safe_contract_id = sc.id
WHERE
    scs.id = (
        SELECT
            s2.id
        FROM
            hopr_safe_contract_state s2
        WHERE
            s2.hopr_safe_contract_id = sc.id
        ORDER BY
            s2.published_block DESC,
            s2.published_tx_index DESC,
            s2.published_log_index DESC
        LIMIT 1);

CREATE VIEW safe_owner_current AS
SELECT
    sc.id AS safe_contract_id,
    sc.address AS safe_address,
    sos.owner_address,
    sos.published_block,
    sos.published_tx_index,
    sos.published_log_index
FROM
    hopr_safe_contract sc
    JOIN hopr_safe_owner_state sos ON sos.hopr_safe_contract_id = sc.id
WHERE
    sos.is_current_owner = TRUE
    AND sos.id = (
        SELECT
            s2.id
        FROM
            hopr_safe_owner_state s2
        WHERE
            s2.hopr_safe_contract_id = sc.id
            AND s2.owner_address = sos.owner_address
        ORDER BY
            s2.published_block DESC,
            s2.published_tx_index DESC,
            s2.published_log_index DESC
        LIMIT 1);

CREATE VIEW safe_threshold_current AS
SELECT
    sc.id AS safe_contract_id,
    sc.address AS safe_address,
    sts.threshold,
    sts.published_block,
    sts.published_tx_index,
    sts.published_log_index
FROM
    hopr_safe_contract sc
    JOIN hopr_safe_threshold_state sts ON sts.hopr_safe_contract_id = sc.id
WHERE
    sts.id = (
        SELECT
            s2.id
        FROM
            hopr_safe_threshold_state s2
        WHERE
            s2.hopr_safe_contract_id = sc.id
        ORDER BY
            s2.published_block DESC,
            s2.published_tx_index DESC,
            s2.published_log_index DESC
        LIMIT 1);

CREATE VIEW service_entry_current AS
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
FROM
    service_entry se
    JOIN service_type st ON st.id = se.service_type_id
    JOIN service_entry_state ses ON ses.service_entry_id = se.id
WHERE
    ses.id = (
        SELECT
            s2.id
        FROM
            service_entry_state s2
        WHERE
            s2.service_entry_id = se.id
        ORDER BY
            s2.published_block DESC,
            s2.published_tx_index DESC,
            s2.published_log_index DESC
        LIMIT 1);

CREATE VIEW service_type_current AS
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
FROM
    service_type st
    JOIN service_type_state sts ON sts.service_type_id = st.id
WHERE
    sts.id = (
        SELECT
            s2.id
        FROM
            service_type_state s2
        WHERE
            s2.service_type_id = st.id
        ORDER BY
            s2.published_block DESC,
            s2.published_tx_index DESC,
            s2.published_log_index DESC
        LIMIT 1);

CREATE INDEX "idx_account_chain_key" ON "account" ("chain_key");

CREATE UNIQUE INDEX "idx_account_chain_packet_key" ON "account" ("chain_key", "packet_key");

CREATE INDEX "idx_account_packet_key" ON "account" ("packet_key");

CREATE INDEX "idx_account_state_position" ON "account_state" ("account_id", "published_block" DESC, "published_tx_index" DESC, "published_log_index" DESC);

CREATE UNIQUE INDEX "idx_account_state_unique_position" ON "account_state" ("account_id", "published_block", "published_tx_index", "published_log_index");

CREATE INDEX "idx_announcement_account_id" ON "announcement" ("account_id");

CREATE INDEX "idx_announcement_position" ON "announcement" ("account_id", "published_block" DESC, "published_tx_index" DESC, "published_log_index" DESC);

CREATE INDEX "idx_channel_destination" ON "channel" ("destination");

CREATE INDEX "idx_channel_source" ON "channel" ("source");

CREATE INDEX "idx_channel_source_destination" ON "channel" ("source", "destination");

CREATE INDEX "idx_channel_state_position" ON "channel_state" ("channel_id", "published_block" DESC, "published_tx_index" DESC, "published_log_index" DESC);

CREATE INDEX "idx_channel_state_status_channel_position" ON "channel_state" ("status", "channel_id", "published_block" DESC, "published_tx_index" DESC, "published_log_index" DESC);

CREATE INDEX "idx_channel_state_status_position" ON "channel_state" ("status", "published_block" DESC, "published_tx_index" DESC, "published_log_index" DESC);

CREATE UNIQUE INDEX "idx_channel_state_unique_position" ON "channel_state" ("channel_id", "published_block", "published_tx_index", "published_log_index");

CREATE UNIQUE INDEX "idx_contract_log_topic" ON "log_topic_info" ("address", "topic");

CREATE INDEX "idx_curvy_committed_note_note_id" ON "curvy_committed_note" ("note_id");

CREATE INDEX "idx_curvy_pending_note_note_id" ON "curvy_pending_note" ("note_id");

CREATE INDEX "idx_hopr_balance_last_changed_block" ON "hopr_balance" ("last_changed_block");

CREATE UNIQUE INDEX "idx_hopr_node_safe_registration_binding" ON "hopr_node_safe_registration" ("safe_address", "node_address");

CREATE UNIQUE INDEX "idx_hopr_node_safe_registration_event" ON "hopr_node_safe_registration" ("registered_block", "registered_tx_index", "registered_log_index");

CREATE INDEX "idx_hopr_node_safe_registration_safe" ON "hopr_node_safe_registration" ("safe_address");

CREATE INDEX "idx_hopr_safe_redeemed_stats_node" ON "hopr_safe_redeemed_stats" ("node_address");

CREATE INDEX "idx_hopr_safe_redeemed_stats_safe" ON "hopr_safe_redeemed_stats" ("safe_address");

CREATE UNIQUE INDEX "idx_hopr_safe_redeemed_stats_safe_node_unique" ON "hopr_safe_redeemed_stats" ("safe_address", "node_address");

CREATE UNIQUE INDEX "idx_log_composite" ON "log" ("block_number", "tx_index", "log_index");

CREATE INDEX "idx_log_status_block_number_processed" ON "log_status" ("block_number", "processed");

CREATE UNIQUE INDEX "idx_log_status_composite" ON "log_status" ("block_number", "tx_index", "log_index");

CREATE INDEX "idx_native_balance_last_changed_block" ON "native_balance" ("last_changed_block");

CREATE INDEX "idx_safe_contract_state_position" ON "hopr_safe_contract_state" ("hopr_safe_contract_id", "published_block" DESC, "published_tx_index" DESC, "published_log_index" DESC);

CREATE UNIQUE INDEX "idx_safe_contract_state_unique_position" ON "hopr_safe_contract_state" ("hopr_safe_contract_id", "published_block", "published_tx_index", "published_log_index");

CREATE INDEX "idx_safe_event_chain_tx_hash" ON "hopr_safe_event" ("chain_tx_hash");

CREATE INDEX "idx_safe_event_safe_position" ON "hopr_safe_event" ("hopr_safe_contract_id", "published_block" DESC, "published_tx_index" DESC, "published_log_index" DESC);

CREATE UNIQUE INDEX "idx_safe_event_unique_position" ON "hopr_safe_event" ("hopr_safe_contract_id", "published_block", "published_tx_index", "published_log_index");

CREATE INDEX "idx_safe_execution_safe_tx_hash" ON "hopr_safe_execution_event" ("safe_tx_hash");

CREATE INDEX "idx_safe_owner_state_current_lookup" ON "hopr_safe_owner_state" ("hopr_safe_contract_id", "owner_address", "published_block" DESC, "published_tx_index" DESC, "published_log_index" DESC);

CREATE INDEX "idx_safe_owner_state_owner_lookup" ON "hopr_safe_owner_state" ("owner_address", "published_block" DESC, "published_tx_index" DESC, "published_log_index" DESC);

CREATE UNIQUE INDEX "idx_safe_owner_state_unique_position" ON "hopr_safe_owner_state" ("hopr_safe_contract_id", "owner_address", "published_block", "published_tx_index", "published_log_index");

CREATE INDEX "idx_safe_setup_owner_event_lookup" ON "hopr_safe_setup_owner" ("hopr_safe_event_id", "owner_position");

CREATE UNIQUE INDEX "idx_safe_setup_owner_unique_position" ON "hopr_safe_setup_owner" ("hopr_safe_event_id", "owner_position");

CREATE INDEX "idx_safe_threshold_state_current_lookup" ON "hopr_safe_threshold_state" ("hopr_safe_contract_id", "published_block" DESC, "published_tx_index" DESC, "published_log_index" DESC);

CREATE UNIQUE INDEX "idx_safe_threshold_state_unique_position" ON "hopr_safe_threshold_state" ("hopr_safe_contract_id", "published_block", "published_tx_index", "published_log_index");

CREATE INDEX "idx_service_entry_node_address" ON "service_entry" ("node_address");

CREATE INDEX "idx_service_entry_state_position" ON "service_entry_state" ("service_entry_id", "published_block" DESC, "published_tx_index" DESC, "published_log_index" DESC);

CREATE UNIQUE INDEX "idx_service_entry_state_unique_position" ON "service_entry_state" ("service_entry_id", "published_block", "published_tx_index", "published_log_index");

CREATE UNIQUE INDEX "idx_service_entry_unique_type_node" ON "service_entry" ("service_type_id", "node_address");

CREATE INDEX "idx_service_type_state_position" ON "service_type_state" ("service_type_id", "published_block" DESC, "published_tx_index" DESC, "published_log_index" DESC);

CREATE UNIQUE INDEX "idx_service_type_state_unique_position" ON "service_type_state" ("service_type_id", "published_block", "published_tx_index", "published_log_index");

CREATE INDEX "idx_unprocessed_log_status" ON "log_status" ("processed", "block_number", "tx_index", "log_index");

