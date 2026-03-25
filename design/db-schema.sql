CREATE TABLE IF NOT EXISTS "account" ( "id" integer NOT NULL PRIMARY KEY AUTOINCREMENT, "chain_key" blob(20) NOT NULL, "packet_key" varchar(64) NOT NULL, "published_block" integer NOT NULL DEFAULT 0, "published_tx_index" integer NOT NULL DEFAULT 0, "published_log_index" integer NOT NULL DEFAULT 0 );
CREATE TABLE IF NOT EXISTS "channel" ( "id" integer NOT NULL PRIMARY KEY AUTOINCREMENT, "concrete_channel_id" varchar(64) NOT NULL UNIQUE, "source" integer NOT NULL, "destination" integer NOT NULL, FOREIGN KEY ("source") REFERENCES "account" ("id") ON DELETE CASCADE ON UPDATE CASCADE, FOREIGN KEY ("destination") REFERENCES "account" ("id") ON DELETE CASCADE ON UPDATE CASCADE );
CREATE TABLE IF NOT EXISTS "announcement" ( "id" integer NOT NULL PRIMARY KEY AUTOINCREMENT, "account_id" integer NOT NULL, "multiaddress" text NOT NULL, "published_block" integer NOT NULL DEFAULT 0, "published_tx_index" integer NOT NULL DEFAULT 0, "published_log_index" integer NOT NULL DEFAULT 0, FOREIGN KEY ("account_id") REFERENCES "account" ("id") ON DELETE CASCADE ON UPDATE CASCADE );
CREATE INDEX "idx_channel_destination" ON "channel" ("destination");
CREATE INDEX "idx_channel_source" ON "channel" ("source");
CREATE INDEX "idx_channel_source_destination" ON "channel" ("source", "destination");
CREATE INDEX "idx_account_chain_key" ON "account" ("chain_key");
CREATE INDEX "idx_account_packet_key" ON "account" ("packet_key");
CREATE UNIQUE INDEX "idx_account_chain_packet_key" ON "account" ("chain_key", "packet_key");
CREATE INDEX "idx_announcement_account_id" ON "announcement" ("account_id");
CREATE TABLE IF NOT EXISTS "log" ( "id" integer NOT NULL PRIMARY KEY AUTOINCREMENT, "tx_index" integer NOT NULL, "log_index" integer NOT NULL, "block_number" integer NOT NULL, "block_hash" blob(32) NOT NULL, "transaction_hash" blob(32) NOT NULL, "address" blob(20) NOT NULL, "topics" blob(1) NOT NULL, "data" blob(1) NOT NULL, "removed" boolean NOT NULL DEFAULT FALSE );
CREATE UNIQUE INDEX "idx_log_composite" ON "log" ("block_number", "tx_index", "log_index");
CREATE TABLE IF NOT EXISTS "log_status" ( "id" integer NOT NULL PRIMARY KEY AUTOINCREMENT, "log_id" integer NOT NULL, "tx_index" integer NOT NULL, "log_index" integer NOT NULL, "block_number" integer NOT NULL, "processed" boolean NOT NULL DEFAULT FALSE, "processed_at" datetime_text, "checksum" blob(32), FOREIGN KEY ("log_id") REFERENCES "log" ("id") ON DELETE CASCADE ON UPDATE CASCADE );
CREATE UNIQUE INDEX "idx_log_status_composite" ON "log_status" ("block_number", "tx_index", "log_index");
CREATE INDEX "idx_unprocessed_log_status" ON "log_status" ("processed", "block_number", "tx_index", "log_index");
CREATE INDEX "idx_log_status_block_number_processed" ON "log_status" ("block_number", "processed");
CREATE TABLE IF NOT EXISTS "hopr_balance" ( "id" integer NOT NULL PRIMARY KEY AUTOINCREMENT, "address" blob(20) NOT NULL UNIQUE, "balance" blob(12) NOT NULL DEFAULT x'000000000000000000000000', "last_changed_block" integer NOT NULL DEFAULT 0, "last_changed_tx_index" integer NOT NULL DEFAULT 0, "last_changed_log_index" integer NOT NULL DEFAULT 0 );
CREATE TABLE IF NOT EXISTS "native_balance" ( "id" integer NOT NULL PRIMARY KEY AUTOINCREMENT, "address" blob(20) NOT NULL UNIQUE, "balance" blob(12) NOT NULL DEFAULT x'000000000000000000000000', "last_changed_block" integer NOT NULL DEFAULT 0, "last_changed_tx_index" integer NOT NULL DEFAULT 0, "last_changed_log_index" integer NOT NULL DEFAULT 0 );
CREATE INDEX "idx_hopr_balance_last_changed_block" ON "hopr_balance" ("last_changed_block");
CREATE INDEX "idx_native_balance_last_changed_block" ON "native_balance" ("last_changed_block");
CREATE TABLE IF NOT EXISTS "account_state" ( "id" integer NOT NULL PRIMARY KEY AUTOINCREMENT, "account_id" integer NOT NULL, "safe_address" blob(20) NULL, "published_block" integer NOT NULL, "published_tx_index" integer NOT NULL, "published_log_index" integer NOT NULL, FOREIGN KEY ("account_id") REFERENCES "account" ("id") ON DELETE CASCADE ON UPDATE CASCADE );
CREATE UNIQUE INDEX "idx_account_state_unique_position" ON "account_state" ("account_id", "published_block", "published_tx_index", "published_log_index");
CREATE INDEX "idx_account_state_position" ON "account_state" ("account_id", "published_block" DESC, "published_tx_index" DESC, "published_log_index" DESC);
CREATE TABLE IF NOT EXISTS "channel_state" ( "id" integer NOT NULL PRIMARY KEY AUTOINCREMENT, "channel_id" integer NOT NULL, "balance" blob(12) NOT NULL, "status" smallint NOT NULL, "epoch" integer NOT NULL, "ticket_index" integer NOT NULL, "closure_time" timestamp_with_timezone_text NULL, "corrupted_state" boolean NOT NULL DEFAULT FALSE, "published_block" integer NOT NULL, "published_tx_index" integer NOT NULL, "published_log_index" integer NOT NULL, "reorg_correction" boolean NOT NULL DEFAULT FALSE, FOREIGN KEY ("channel_id") REFERENCES "channel" ("id") ON DELETE CASCADE ON UPDATE CASCADE );
CREATE UNIQUE INDEX "idx_channel_state_unique_position" ON "channel_state" ("channel_id", "published_block", "published_tx_index", "published_log_index");
CREATE INDEX "idx_channel_state_position" ON "channel_state" ("channel_id", "published_block" DESC, "published_tx_index" DESC, "published_log_index" DESC);
CREATE INDEX "idx_channel_state_status_position" ON "channel_state" ("status", "published_block" DESC, "published_tx_index" DESC, "published_log_index" DESC);
CREATE INDEX "idx_channel_state_status_channel_position" ON "channel_state" ("status", "channel_id", "published_block" DESC, "published_tx_index" DESC, "published_log_index" DESC);
CREATE INDEX "idx_announcement_position" ON "announcement" ("account_id", "published_block" DESC, "published_tx_index" DESC, "published_log_index" DESC);
CREATE TABLE IF NOT EXISTS "log_topic_info" ( "id" integer NOT NULL PRIMARY KEY AUTOINCREMENT, "address" blob(20) NOT NULL, "topic" blob(32) NOT NULL );
CREATE UNIQUE INDEX "idx_contract_log_topic" ON "log_topic_info" ("address", "topic");
CREATE TABLE IF NOT EXISTS "chain_info" ( "id" integer NOT NULL PRIMARY KEY, "last_indexed_block" integer NOT NULL DEFAULT 0, "last_indexed_tx_index" integer NULL, "last_indexed_log_index" integer NULL, "ticket_price" blob(12) NULL, "channels_dst" blob(32) NULL, "ledger_dst" blob(32) NULL, "safe_registry_dst" blob(32) NULL, "min_incoming_ticket_win_prob" float NOT NULL DEFAULT 1, "channel_closure_grace_period" integer NULL, "key_binding_fee" blob(12) NULL );
CREATE TABLE IF NOT EXISTS "schema_version" ( "id" integer NOT NULL PRIMARY KEY, "version" integer NOT NULL, "updated_at" timestamp_text NOT NULL DEFAULT CURRENT_TIMESTAMP );
CREATE TABLE IF NOT EXISTS "hopr_node_safe_registration" ( "id" integer NOT NULL PRIMARY KEY AUTOINCREMENT, "safe_address" blob(20) NOT NULL, "node_address" blob(20) NOT NULL UNIQUE, "registered_block" integer NOT NULL, "registered_tx_index" integer NOT NULL, "registered_log_index" integer NOT NULL );
CREATE UNIQUE INDEX "idx_hopr_node_safe_registration_binding" ON "hopr_node_safe_registration" ("safe_address", "node_address");
CREATE INDEX "idx_hopr_node_safe_registration_safe" ON "hopr_node_safe_registration" ("safe_address");
CREATE UNIQUE INDEX "idx_hopr_node_safe_registration_event" ON "hopr_node_safe_registration" ("registered_block", "registered_tx_index", "registered_log_index");
CREATE TABLE IF NOT EXISTS "hopr_safe_contract" ( "id" integer NOT NULL PRIMARY KEY AUTOINCREMENT, "address" blob(20) NOT NULL UNIQUE );
CREATE TABLE IF NOT EXISTS "hopr_safe_contract_state" ( "id" integer NOT NULL PRIMARY KEY AUTOINCREMENT, "hopr_safe_contract_id" integer NOT NULL, "module_address" blob(20) NOT NULL, "chain_key" blob(20) NOT NULL, "published_block" integer NOT NULL, "published_tx_index" integer NOT NULL, "published_log_index" integer NOT NULL, FOREIGN KEY ("hopr_safe_contract_id") REFERENCES "hopr_safe_contract" ("id") ON DELETE CASCADE ON UPDATE CASCADE );
CREATE UNIQUE INDEX "idx_safe_contract_state_unique_position" ON "hopr_safe_contract_state" ("hopr_safe_contract_id", "published_block", "published_tx_index", "published_log_index");
CREATE INDEX "idx_safe_contract_state_position" ON "hopr_safe_contract_state" ("hopr_safe_contract_id", "published_block" DESC, "published_tx_index" DESC, "published_log_index" DESC);
CREATE VIEW safe_contract_current AS
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
                )
/* safe_contract_current(safe_contract_id,address,module_address,chain_key,published_block,published_tx_index,published_log_index) */;
CREATE VIEW channel_current AS
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
                SELECT
                    cs.*,
                    ROW_NUMBER() OVER (
                        PARTITION BY cs.channel_id
                        ORDER BY cs.published_block DESC, cs.published_tx_index DESC, cs.published_log_index DESC
                    ) AS rn
                FROM channel_state cs
            ) s ON s.channel_id = c.id AND s.rn = 1
/* channel_current(id,channel_id,concrete_channel_id,source,destination,balance,status,epoch,ticket_index,closure_time,corrupted_state,published_block,published_tx_index,published_log_index,reorg_correction) */;
CREATE VIEW account_current AS
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
                SELECT
                    acs.*,
                    ROW_NUMBER() OVER (
                        PARTITION BY acs.account_id
                        ORDER BY acs.published_block DESC, acs.published_tx_index DESC, acs.published_log_index DESC
                    ) AS rn
                FROM account_state acs
            ) s ON s.account_id = a.id AND s.rn = 1
/* account_current(id,account_id,chain_key,packet_key,safe_address,published_block,published_tx_index,published_log_index) */;
CREATE TABLE IF NOT EXISTS "hopr_safe_redeemed_stats" ( "id" integer NOT NULL PRIMARY KEY AUTOINCREMENT, "safe_address" blob(20) NOT NULL, "node_address" blob(20) NOT NULL, "redeemed_amount" blob(32) NOT NULL DEFAULT x'0000000000000000000000000000000000000000000000000000000000000000', "redemption_count" integer NOT NULL DEFAULT 0, "last_redeemed_block" integer NOT NULL DEFAULT 0, "last_redeemed_tx_index" integer NOT NULL DEFAULT 0, "last_redeemed_log_index" integer NOT NULL DEFAULT 0 );
CREATE UNIQUE INDEX "idx_hopr_safe_redeemed_stats_safe_node_unique" ON "hopr_safe_redeemed_stats" ("safe_address", "node_address");
CREATE INDEX "idx_hopr_safe_redeemed_stats_safe" ON "hopr_safe_redeemed_stats" ("safe_address");
CREATE INDEX "idx_hopr_safe_redeemed_stats_node" ON "hopr_safe_redeemed_stats" ("node_address");
