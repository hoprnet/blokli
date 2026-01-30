# Blokli Architecture

## Document Scope

This document describes the conceptual architecture, component responsibilities, and data flows for Blokli. It intentionally avoids code-level details and configuration snippets. For schema references, see `design/target-api-schema.graphql` and `design/target-db-schema.mmd`.

## Table of Contents

- System Overview
- High-Level Component Diagram
- Core Components
- User Flows
- Data Flow Architecture
- Deployment Architectures
- Performance Characteristics
- Security Considerations
- Error Handling Strategy
- Future Architecture Considerations
- Design Principles and Patterns
- Conclusion

## System Overview

Blokli is an on-chain indexer and operations provider for HOPR smart contracts. The system consists of two main components that can run together or separately:

1. **bloklid**: A daemon that indexes blockchain events and provides transaction submission capabilities
2. **blokli-api**: A GraphQL API server that exposes indexed data and transaction operations

The architecture follows an event-driven model with clear separation between data ingestion (indexer), storage (database), and exposure (GraphQL API).

## High-Level Component Diagram

```text
┌─────────────────────────────────────────────────────────────────┐
│                           bloklid                                │
│  ┌────────────────────────────────────────────────────────────┐ │
│  │                    Configuration Layer                      │ │
│  │   - TOML config loading & validation                       │ │
│  │   - Signal handling (SIGHUP reload, SIGTERM shutdown)      │ │
│  └────────────────────────────────────────────────────────────┘ │
│                               │                                  │
│      ┌───────────────────────┴──────────────────────┐          │
│      ▼                                               ▼          │
│  ┌──────────────────────┐                  ┌──────────────────┐ │
│  │   BlokliChain        │                  │   API Server     │ │
│  │   - Indexer          │                  │   (optional)     │ │
│  │   - TxExecutor       │                  │                  │ │
│  │   - TxMonitor        │                  └──────────────────┘ │
│  └──────────────────────┘                                       │
└─────────────────────────────────────────────────────────────────┘
                    │
                    ▼
        ┌─────────────────────┐
        │   Blockchain RPC    │
        │   Ethereum/Gnosis   │
        └─────────────────────┘
```

## Core Components

### 1. bloklid Daemon

**Responsibilities**:

- Configuration management with TOML-based file loading, environment variable overrides, and validation
- Signal handling for graceful shutdown and config reload
- Orchestration of BlokliChain and API server components
- Process lifecycle management

**Configuration Layer**:
The configuration layer aggregates settings from multiple sources (Env > File > Defaults). It supports canonical environment variables (e.g. `DATABASE_URL`, `PGHOST`) alongside application-specific variables (`BLOKLI_HOST`, `BLOKLI_INDEXER_FAST_SYNC`) for flexible deployment in containerized environments.

**Key Features**:

- SIGHUP signal support for configuration reload without restart
- SIGINT/SIGTERM signal handling for graceful component shutdown
- Dual operation modes: standalone indexer or with embedded API server
- Validates configuration against blockchain network parameters at startup

**Lifecycle Flow**:

```text
Startup → Load Config → Validate Network → Initialize Database
    → Create BlokliChain → Start Processes → Signal Loop → Graceful Shutdown
```

### 2. BlokliChain - Core Chain Operations

**Responsibilities**:

- Coordinates all blockchain interactions
- Manages indexer, transaction executor, and transaction monitor
- Provides unified interface to chain operations

**Architecture**:

```text
BlokliChain
├── Indexer (reads blockchain events)
│   ├── Block/Log fetching from RPC
│   ├── Fast sync via snapshots
│   └── Event processing pipeline
├── TransactionExecutor (writes to blockchain)
│   ├── Raw transaction validation
│   ├── Transaction submission (3 modes)
│   └── Confirmation tracking
└── TransactionMonitor (monitors pending txs)
    ├── Background monitoring loop
    ├── Status updates
    └── Timeout handling
```

**Process Types**:

- **Indexer Process**: Continuous blockchain event ingestion with finality awareness
- **TransactionMonitor Process**: Background transaction confirmation tracking and status updates

### 3. Indexer - Blockchain Event Processing

**Responsibilities**:

- Fetch blocks and logs from Ethereum/Gnosis chain via RPC
- Process and decode HOPR contract events
- Store indexed data in database
- Emit events to subscribers via IndexerState
- Handle blockchain reorganizations

**Event Processing Pipeline**:

```text
RPC Endpoint
     │
     ▼
┌────────────────────┐
│  Block Fetcher     │  ← Streams blocks with logs using filter sets
│  (FilterSet)       │     Configured with contract addresses & topics
└────────────────────┘
     │
     ▼
┌────────────────────┐
│  Fast Sync?        │  ← Optional snapshot download/import
│  (Snapshot Mgr)    │     Accelerates initial synchronization
└────────────────────┘
     │
     ▼
┌────────────────────┐
│  ContractEvent     │  ← Routes events by contract address
│  Handlers          │     Decodes & processes typed events
└────────────────────┘
     │
     ├── Announcements (multiaddress updates)
     ├── Channels (open/close/balance updates)
     ├── Token (HOPR token transfers/approvals)
     ├── SafeRegistry (Safe address linking)
     └── Oracles (ticket price, win probability)
     │
     ▼
┌────────────────────┐
│  Database Storage  │  ← Persistent state with transaction safety
│  (BlokliDb)        │     Event sourcing pattern with version history
└────────────────────┘
     │
     ▼
┌────────────────────┐
│  IndexerState      │  ← Publishes updates to subscribers
│  (Event Bus)       │     Coordinated with watermark synchronization
└────────────────────┘
```

**Key Features**:

- **Fast Sync**: Downloads pre-built logs database snapshots for quick initial sync, reducing time-to-ready from hours to minutes
- **Finality Awareness**: Only processes blocks after configured confirmation count to avoid reorg complications
- **Reorg Handling**: Detects blockchain reorganizations by tracking block hashes and marks affected logs as removed
- **Watermark Tracking**: Maintains precise last processed position using (block, tx_index, log_index) triplet for exact resume capability
- **Dual Database Support**: Separates logs (high write volume) from indexed state (frequent reads) to reduce contention

**Contract Event Types**:

- **HoprAnnouncements**: Node multiaddress announcements for network discovery
- **HoprChannels**: Payment channel lifecycle events (open, fund, close)
- **HoprToken**: wxHOPR token transfers and approvals for balance tracking
- **HoprNodeSafeRegistry**: Safe contract address registration linking accounts to multisig wallets
- **HoprNodeStakeFactory**: Safe contract deployment events with module and owner tracking
- **HoprTicketPriceOracle**: Network-wide ticket price updates
- **HoprWinningProbabilityOracle**: Network-wide winning probability updates

### 4. Database Layer

**Database Schema Architecture**:

The target database schema reference lives in `design/target-db-schema.mmd` and should remain consistent with the implemented entities and migrations.

The database implements an event sourcing pattern with temporal versioning:

**Core Tables**:

- **account**: Immutable node identities (keyid, chain_key, packet_key)
- **account_state**: Version history of account states (Safe linking) with position tracking
- **announcement**: Multiaddress announcements with position tracking
- **channel**: Immutable payment channel identities linking source/destination accounts
- **channel_state**: Version history of channel states (balance, status, epoch) with position tracking
- **hopr_safe_contract**: Safe contract deployments with module addresses and owner chain keys
- **hopr_balance**: Current wxHOPR token balances indexed by address
- **native_balance**: Current native token balances indexed by address
- **chain_info**: Singleton table tracking indexer metadata (watermark, network parameters)

**Logs Tables** (for fast sync and reorg handling):

- **log**: Raw blockchain logs with full event data
- **log_status**: Processing status and checksums for each log
- **log_topic_info**: Event signature tracking for filter generation

**Database Architecture Modes**:

The system supports two database configurations:

**Single Database (PostgreSQL)**:
All tables reside in a single PostgreSQL database. This provides simpler management, better transaction support across tables, and MVCC-based concurrency control for high read/write throughput.

**Dual Database (SQLite)**:
Splits into Index DB (accounts, channels, balances) and Logs DB (raw blockchain logs). The Logs DB can be atomically replaced during fast sync, and separation reduces write lock contention in SQLite's locking model.

**Key Design Patterns**:

- **Event Sourcing**: State tables maintain complete version history with position-based ordering
- **Temporal Queries**: Query state at any historical point using position coordinates
- **Optimistic Locking**: Position-based unique constraints prevent duplicate event processing
- **Binary Storage**: Large integers stored as binary to preserve full precision across language boundaries
- **Watermark Pattern**: Tracks exact last processed position for reliable resume after restarts

**SeaORM Integration**:
The database layer uses SeaORM for type-safe database access with auto-generated entity models, migration management, and connection pooling. The trait-based design allows transaction-agnostic APIs.

### 5. GraphQL API Server

**Responsibilities**:

- Expose indexed blockchain data via GraphQL queries
- Support real-time updates via Server-Sent Events (SSE) subscriptions
- Handle transaction submission in three distinct modes
- Provide health checks and optional GraphQL Playground
- Maintain SSE subscriptions with periodic keep-alive events to prevent idle timeouts

**API Architecture**:

```text
┌─────────────────────────────────────────────────┐
│              Axum HTTP Server                    │
│  ┌─────────────────────────────────────────┐   │
│  │  Middleware Stack                        │   │
│  │  - CORS (configurable origins)           │   │
│  │  - Zstandard Compression (>1KB, non-SSE) │   │
│  │  - Tracing (HTTP request logging)        │   │
│  └─────────────────────────────────────────┘   │
│                    │                             │
│        ┌──────────┴──────────┐                 │
│        ▼                      ▼                 │
│  ┌──────────┐         ┌──────────────┐         │
│  │  Query   │         │ Subscription │         │
│  │  Root    │         │  Root (SSE)  │         │
│  └──────────┘         └──────────────┘         │
│        │                      │                 │
│        ▼                      ▼                 │
│  ┌─────────────────────────────────┐           │
│  │    async-graphql Schema         │           │
│  │  - QueryRoot                    │           │
│  │  - MutationRoot                 │           │
│  │  - SubscriptionRoot             │           │
│  └─────────────────────────────────┘           │
└─────────────────────────────────────────────────┘
```

**GraphQL Schema Structure**:

The target schema reference lives in `design/target-api-schema.graphql` and should remain consistent with the implemented API surface.

The schema is organized into three root types following GraphQL best practices:

**Query Operations**:

- Account queries with required filtering to prevent excessive data exposure
- Channel queries with identity-based filters
- Safe contract queries by address or chain key (owner)
- Balance queries by address (works for any Ethereum address)
- Chain information and network parameters
- Transaction status queries by UUID
- Health check and version endpoints

**Mutation Operations**:
Three transaction submission modes with different guarantees:

- Fire-and-forget: Returns transaction hash immediately, no tracking
- Async: Returns UUID for status tracking, background monitoring
- Sync: Waits for blockchain confirmations before returning

**Subscription Operations** (via SSE):

- Account updates: Real-time changes to account balances and Safe linking
- Channel updates: Real-time changes to payment channel states
- Safe deployments: Real-time notifications when new Safe contracts are deployed
- Network topology: Opened channel graph updates for routing decisions
- Transaction updates: Status changes for submitted transactions

**Error Handling**:
Uses GraphQL union types to return domain-specific error types (InvalidAddressError, ContractNotAllowedError, etc.) alongside success types, providing structured error responses with codes and context.

**Compression Strategy**:
Applies Zstandard compression selectively:

- Only for responses exceeding 1KB to avoid overhead on small payloads
- Excludes Server-Sent Events to preserve real-time streaming characteristics
- Uses balanced compression level to optimize CPU usage vs. bandwidth savings

### 6. RPC Operations Layer

**Responsibilities**:

- Abstract Ethereum RPC operations behind trait interfaces
- Implement rate limiting and retry logic with exponential backoff
- Provide block and log fetching optimized for indexer needs
- Execute smart contract calls for reading on-chain state
- Broadcast raw transactions to blockchain network

**Key Traits**:

- **HoprIndexerRpcOperations**: Indexer-specific operations including log streaming, block fetching with configurable ranges, filter management, and transaction sender retrieval for chain key extraction
- **HoprRpcOperations**: General HOPR contract operations including balance queries, allowance checks, and network parameter reads

**Transaction Sender Retrieval**:
The `get_transaction_sender()` operation retrieves the sender address (chain key) from a transaction hash. This is used during Safe deployment tracking to identify the owner's chain key. The operation is cryptographically secure because transaction signatures are verified by Ethereum consensus—only successfully mined transactions can be queried, and the sender address is recovered from the transaction's ECDSA signature by the blockchain itself.

**Configuration Parameters**:
The RPC layer is configured with network-specific parameters including chain ID, contract addresses, expected block time for polling intervals, transaction polling configuration, finality depth, and maximum block range for batch fetching.

**Retry Strategy**:
Implements exponential backoff with jitter for transient failures, distinguishes between retryable errors (network, rate limit) and non-retryable errors (invalid transaction), and respects rate limits through request throttling.

### 7. Transaction Submission System

**Components**:

**TransactionValidator**:
Validates raw transaction format and structure, checks target contract against allowlist, and validates function selectors against permitted operations. Prevents submission of malicious or unintended transactions.

**TransactionExecutor**:
Provides three submission modes with different guarantees:

1. **Fire-and-forget Mode**: Submits transaction and returns hash immediately. No tracking or confirmation monitoring. Lowest latency, suitable for non-critical operations.

2. **Async Mode**: Submits transaction, stores record with generated UUID, returns immediately. Background monitor tracks confirmation. Suitable for most applications needing status tracking.

3. **Sync Mode**: Submits transaction and waits for specified number of confirmations before returning. Highest certainty, suitable for critical operations requiring immediate confirmation.

**TransactionStore**:
In-memory storage using concurrent hash map (DashMap) for thread-safe access. Tracks transaction records with UUID identifiers, maintains status lifecycle, and provides query interface for GraphQL resolvers.

**TransactionMonitor**:
Background process continuously monitoring pending transactions. Queries blockchain for transaction receipts, updates transaction status based on confirmation depth, handles timeouts and reverted transactions, and publishes status updates to event bus for subscriptions.

**Transaction Lifecycle States**:

```text
PENDING → Initial state after validation
SUBMITTED → Transaction sent to blockchain
CONFIRMED → Transaction included and confirmed
REVERTED → Transaction executed but reverted on-chain
TIMEOUT → Transaction not mined within timeout window
VALIDATION_FAILED → Pre-submission validation failed
SUBMISSION_FAILED → RPC submission failed
```

### 8. IndexerState - Event Coordination

**Responsibilities**:

- Coordinate block processing with GraphQL subscription setup
- Prevent race conditions through watermark synchronization
- Broadcast events to multiple subscribers efficiently
- Signal shutdown to all components

**Architecture Components**:

- **event_bus**: Broadcast channel distributing IndexerEvent messages to all active subscriptions
- **shutdown_signal**: Broadcast channel for coordinating graceful shutdown
- **watermark_lock**: Read-write lock synchronizing event emission with subscription setup

**Synchronization Pattern**:

The IndexerState implements a critical synchronization mechanism preventing race conditions in the subscription model:

**Block Processing Phase** (write lock):
The indexer acquires write lock before processing any block. While holding lock, it processes all events in the block, updates database with new state, and broadcasts events to event_bus. The write lock ensures no subscriptions can be created during this window.

**Subscription Setup Phase** (read lock):
When a GraphQL subscription starts, it acquires read lock. While holding lock, it reads current watermark from database and subscribes to event_bus. The read lock allows multiple concurrent subscriptions but blocks block processing, guaranteeing no events are emitted between reading watermark and subscribing.

**Guarantee**: No events can be missed between reading the watermark and subscribing to the event bus, ensuring complete event delivery to all subscriptions.

**Event Types**:
The event bus carries four event types: AccountUpdated (with account keyid), ChannelUpdated (with channel id), BalanceUpdated (with address bytes), and SafeDeployed (with Safe contract address). Subscribers filter events based on their query parameters.

## User Flows

### Flow 1: Query Account Information

This flow demonstrates a synchronous database query for account data with related entities:

```text
Client sends GraphQL query request
    ↓
API validates filter parameters (requires at least one identity filter)
    ↓
API validates address format if chain_key provided
    ↓
Database executes optimized join query:
    - Fetch account identity from account table
    - Join latest account_state for Safe address
    - Join hopr_balance and native_balance tables
    - Join announcement table for multiaddresses
    ↓
API assembles GraphQL Account response
    ↓
Client receives structured account data with all related entities
```

**Query Optimization Strategy**: The account query uses batch loading pattern with four separate database queries to avoid N+1 problems while maintaining flexibility. Temporal join fetches only the latest state using position-based ordering.

**Required Filters**: To prevent exposing entire account database, at least one identity filter (keyid, packet_key, or chain_key) must be provided. This protects against excessive data exposure.

### Flow 2: Subscribe to Channel Updates

This flow demonstrates the two-phase subscription model with watermark synchronization:

```text
Client sends subscription request with Accept: text/event-stream header
    ↓
API recognizes subscription request (starts with "subscription" keyword)
    ↓
PHASE 1: Historical Snapshot
    ↓
API acquires IndexerState watermark read lock
    ↓
API reads current watermark position from chain_info table
    ↓
API subscribes to IndexerState event bus (while holding lock)
    ↓
API releases watermark lock
    ↓
API queries all matching channels at watermark position
    ↓
For each historical channel:
    - Load source and destination account details
    - Stream as SSE event to client
    ↓
PHASE 2: Real-time Updates
    ↓
API listens to event bus for ChannelUpdated events
    ↓
On each event:
    - Query latest channel state from database
    - Load source and destination account details
    - Stream as SSE event to client
    ↓
Process continues until client disconnects or shutdown signal
```

**Two-Phase Guarantee**: Phase 1 delivers all historical data at watermark. Phase 2 delivers all changes after watermark. The watermark synchronization prevents gaps or duplicates between phases.

**SSE Streaming**: Server-Sent Events provide one-way streaming from server to client over HTTP. Each event is a JSON-encoded GraphQL response. Periodic keep-alive events keep idle connections from timing out while the stream stays open.

### Flow 3: Submit Transaction (Async Mode)

This flow demonstrates asynchronous transaction submission with background monitoring:

```text
Client sends mutation with raw signed transaction (hex encoded)
    ↓
API decodes hex transaction to bytes
    ↓
TransactionValidator validates transaction:
    - Decode transaction structure
    - Verify transaction format
    - Check target contract against allowlist
    - Validate function selector against permitted operations
    ↓
TransactionExecutor async submission:
    - Generate UUID for transaction
    - Create TransactionRecord with PENDING status
    - Store record in TransactionStore
    - Submit raw transaction to blockchain RPC
    - Update record status to SUBMITTED
    - Return UUID to client
    ↓
Client receives transaction UUID and SUBMITTED status
    ↓
[Background: TransactionMonitor Process]
    ↓
Monitor polls TransactionStore for SUBMITTED transactions
    ↓
For each transaction:
    - Query blockchain for transaction receipt
    - Check confirmation depth
    - Update status: CONFIRMED, REVERTED, or keep SUBMITTED
    - Check timeout threshold
    - Broadcast status update to event bus
    ↓
Subscribed clients receive transactionUpdated events via SSE
```

**Async Benefits**: Client receives immediate response without waiting for blockchain confirmation. Transaction monitoring happens in background. Status can be queried or streamed via subscription.

**Monitoring Strategy**: Background loop polls blockchain at configured interval (typically matching block time). Tracks confirmation depth by comparing block numbers. Handles reverted transactions by checking receipt status.

### Flow 4: Real-time Network Topology

This flow demonstrates streaming network topology for routing decisions:

```text
Client subscribes to openedChannelGraphUpdated
    ↓
API executes watermark synchronization (acquire read lock)
    ↓
PHASE 1: Emit Historical Network Topology
    ↓
Query all channels with status = OPEN at watermark
    ↓
For each open channel:
    - Load source account details
    - Load destination account details
    - Create OpenedChannelsGraphEntry (directed edge)
    - Stream as SSE event
    ↓
PHASE 2: Stream Topology Changes
    ↓
Listen to event bus for ChannelUpdated events
    ↓
On each event:
    - Query latest channel state
    - If channel status is OPEN:
        * Load source and destination accounts
        * Create OpenedChannelsGraphEntry
        * Stream as SSE event
    - If channel status is CLOSED or PENDINGTOCLOSE:
        * Do not emit (client removes edge)
    ↓
Client accumulates entries to build directed graph
```

**Graph Structure**: Each entry represents one directed edge (source → destination). Bidirectional channels require two separate entries (A→B and B→A). Clients must accumulate entries to build complete topology.

**Update Semantics**: Entry emitted when channel opens or its properties change (balance, status). Entry no longer emitted when channel closes. Client is responsible for graph maintenance logic.

## Data Flow Architecture

### Indexer → Database → API Flow

This diagram illustrates the complete data flow from blockchain to clients:

```text
Blockchain RPC
      │
      │ RPC calls: eth_getLogs, eth_getBlockByNumber
      │ Returns: blocks with transaction logs
      │
      ▼
┌─────────────────┐
│  Block Indexer  │
│  - Fetch blocks │  Streams blocks respecting finality depth
│  - Decode logs  │  Applies filter sets for HOPR contracts
└─────────────────┘
      │
      │ SerializableLog (contract address, topics, data)
      │
      ▼
┌──────────────────────┐
│ ContractEventHandlers│
│ - Route by address   │  Dispatches to contract-specific handlers
│ - Decode events      │  Uses Alloy to decode log data to typed events
└──────────────────────┘
      │
      │ Typed events (Announcement, ChannelOpened, Transfer, etc.)
      │
      ▼
┌──────────────────────┐
│  Database Operations │
│  - BEGIN transaction │  Atomic update with position tracking
│  - INSERT/UPDATE     │  Applies event to appropriate state tables
│  - COMMIT            │  Persists all changes atomically
└──────────────────────┘
      │
      │ Database updated with new state
      │
      ▼
┌──────────────────────┐
│   IndexerState       │
│ - Broadcast events   │  Publishes to all active subscriptions
└──────────────────────┘
      │
      │ IndexerEvent (AccountUpdated, ChannelUpdated, BalanceUpdated, SafeDeployed)
      │
      ▼
┌──────────────────────┐
│ GraphQL Subscribers  │
│ - Receive events     │  Filters events by subscription parameters
│ - Query fresh data   │  Fetches updated state from database
│ - Send via SSE       │  Streams formatted response to client
└──────────────────────┘
      │
      │ Server-Sent Events (text/event-stream)
      │
      ▼
   Clients
```

**Key Characteristics**:

- **Atomic Updates**: Database transaction ensures consistency between related tables
- **Event Sourcing**: Historical state preserved through version history
- **Position Tracking**: Every state change tagged with (block, tx_index, log_index)
- **Fan-out Pattern**: Single database update triggers multiple subscription notifications

### Transaction Submission Flow

This diagram illustrates outbound transaction flow from clients to blockchain:

```text
Client
  │
  │ Raw signed transaction (hex encoded)
  │ GraphQL mutation: sendTransaction/Async/Sync
  │
  ▼
┌──────────────────────┐
│ GraphQL Mutation     │
│ - Parse input        │  Validates GraphQL request structure
│ - Decode hex         │  Converts hex string to byte array
└──────────────────────┘
  │
  ▼
┌──────────────────────┐
│ TransactionValidator │
│ - Decode transaction │  Parses RLP-encoded transaction structure
│ - Verify allowlist   │  Checks target contract against whitelist
│ - Check selector     │  Validates function call is permitted
└──────────────────────┘
  │
  │ Validation passed
  │
  ▼
┌──────────────────────┐
│ TransactionExecutor  │
│ - Store (if tracked) │  Creates record in TransactionStore (async/sync)
│ - Submit to RPC      │  Calls eth_sendRawTransaction
└──────────────────────┘
  │
  │ eth_sendRawTransaction call
  │
  ▼
┌──────────────────────┐
│   RPC Client         │
│ - Rate limiting      │  Respects configured requests per second
│ - Retry logic        │  Handles transient failures with backoff
│ - Error handling     │  Distinguishes retryable vs permanent errors
└──────────────────────┘
  │
  │ JSON-RPC over HTTP/HTTPS
  │
  ▼
Blockchain Network
  │
  │ Transaction enters mempool
  │ Mined in block
  │ Confirmations accumulate
  │
  ▼
┌──────────────────────┐
│ TransactionMonitor   │
│ - Poll receipts      │  Background loop queries transaction status
│ - Update status      │  Updates record as confirmations arrive
│ - Emit events        │  Broadcasts status changes to subscribers
│ - Handle timeouts    │  Marks transactions that don't mine in time
└──────────────────────┘
  │
  │ Status updates via event bus
  │
  ▼
Subscribed Clients
```

**Security Layers**:

1. **Format Validation**: Ensures transaction is properly structured RLP
2. **Allowlist Check**: Only permits transactions to whitelisted HOPR contracts
3. **Selector Validation**: Only permits calls to approved function selectors
4. **Rate Limiting**: Prevents flooding RPC endpoint or blockchain network

### Safe Contract Deployment Tracking

This flow demonstrates how Safe contract deployments are indexed with module and owner information:

```text
Blockchain emits NewHoprNodeStakeModuleForSafe event
    ↓
Indexer receives log from RPC provider
    ↓
StakeFactory handler processes event:
    - Extracts Safe address from event
    - Extracts module address from event
    - Calls get_transaction_sender(tx_hash) via RPC
    ↓
RPC provider returns transaction sender (chain_key/owner)
    - Sender extracted from tx.inner.signer()
    - Cryptographically verified by Ethereum consensus
    ↓
Database operations:
    - Check if Safe already exists (idempotency)
    - Insert into hopr_safe_contract table:
        * Safe address
        * Module address
        * Chain key (owner)
        * Event position (block, tx_index, log_index)
    ↓
Publish SafeDeployed event to IndexerState event bus
    ↓
GraphQL safeDeployed subscription receives event:
    - Queries database for full Safe details
    - Streams to subscribed clients via SSE
```

**Query Access Patterns**:

Clients can query Safe contracts through three methods:

1. **By Safe Address**: `safe(address: "0x...")` - Direct lookup by Safe contract address
2. **By Chain Key**: `safeByChainKey(chainKey: "0x...")` - Find Safe by owner's address
3. **List All**: `safes()` - Retrieve all indexed Safe contracts

**Idempotency Guarantee**:

The unique constraint on `(deployed_block, deployed_tx_index, deployed_log_index)` ensures that processing the same event multiple times (due to retries or reorgs) will not create duplicate Safe entries.

### Subscription Event Bus

Blokli provides real-time GraphQL subscriptions using an in-memory event bus architecture. All subscriptions use the same unified mechanism for consistency and simplicity.

**Event Bus Architecture**:

```text
Indexer Handler
    ↓
Process blockchain log
    ↓
Update database (transaction committed)
    ↓
Publish IndexerEvent to event bus
    ↓
IndexerState broadcasts to all subscribers
    ↓
GraphQL subscriptions receive event
    ↓
Filter and emit to matching clients via SSE
```

**Event Types**:

Indexer handlers publish structured events containing complete data to avoid N+1 database queries:

| Event Type                | Published When                        | Payload                       | Subscribers                 |
| ------------------------- | ------------------------------------- | ----------------------------- | --------------------------- |
| `AccountUpdated`          | Account state changes                 | Full Account object           | `accountUpdated`            |
| `ChannelUpdated`          | Channel opened/closed/balance changes | Full Channel object           | `openedChannelGraphUpdated` |
| `KeyBindingFeeUpdated`    | Protocol fee parameter changes        | Fee amount (TokenValueString) | `keyBindingFeeUpdated`      |
| `SafeDeployed`            | New safe contract deployed            | Safe address                  | `safeDeployed`              |
| `TicketParametersUpdated` | Ticket price/probability changes      | Full TicketParameters object  | `ticketParametersUpdated`   |

**Two-Phase Subscription Pattern**:

All event bus subscriptions follow a consistent pattern to prevent data loss:

```text
Client subscribes
    ↓
Phase 1: Capture watermark and subscribe to event bus
    - Read current IndexerState position (watermark)
    - Subscribe to event channel (buffered)
    - Subscribe to shutdown channel (reorg detection)
    ↓
Phase 2: Emit historical snapshot
    - Query database for current state at watermark
    - Emit initial values to client
    ↓
Phase 3: Stream real-time updates
    - Receive events from event bus
    - Filter for relevant events
    - Deduplicate (compare to last emitted value)
    - Emit updates to client via SSE
```

**Synchronization Guarantee**:

The IndexerState uses an RwLock to ensure no events are missed between the snapshot and subscription:

1. Acquire read lock
2. Capture watermark (last indexed block/tx/log)
3. Subscribe to event channels
4. Release read lock
5. Events published during this time are buffered in the channel

**Overflow Handling**:

Event channels use `async-broadcast` with bounded capacity. Slow subscribers that can't keep up will receive `RecvError::Lagged(n)` and skip old messages. This prevents fast producers from being blocked by slow consumers.

**Reorg Safety**:

When a blockchain reorganization is detected:

1. IndexerState publishes shutdown signal
2. All active subscriptions receive the signal
3. Subscriptions terminate gracefully
4. Clients must reconnect after reorg completes

**Scalability Benefits**:

- **In-memory pub/sub**: No database overhead for notifications
- **Complete event data**: No N+1 queries in subscriptions
- **Consistent pattern**: Same two-phase approach across all subscriptions
- **Testable**: Easy to mock events for testing
- **Database agnostic**: Works identically with PostgreSQL and SQLite

**Implementation Details**:

- Event Bus: `chain/indexer/src/state.rs::IndexerState`
- Event Types: `chain/indexer/src/state.rs::IndexerEvent`
- Subscriptions: `api/src/subscription.rs`
- Watermark Helper: `api/src/subscription.rs::capture_watermark_synchronized`

## Deployment Architectures

### Architecture 1: Unified Deployment

Single process running both indexer and API server:

```text
┌─────────────────────────────────────┐
│           bloklid Process            │
│  ┌──────────────┐  ┌──────────────┐ │
│  │   Indexer    │  │  API Server  │ │
│  │              │  │  (embedded)  │ │
│  │  - RPC calls │  │  - Queries   │ │
│  │  - Events    │  │  - Mutations │ │
│  │  - Storage   │  │  - Subscr.   │ │
│  └──────────────┘  └──────────────┘ │
└─────────────────────────────────────┘
           │                  │
           │                  │
           ▼                  ▼
    ┌──────────────┐   ┌──────────┐
    │  Blockchain  │   │  Clients │
    │  RPC         │   │  (HTTP)  │
    └──────────────┘   └──────────┘
           │
           ▼
    ┌──────────────┐
    │  PostgreSQL  │
    │  Database    │
    └──────────────┘
```

**Use Cases**:

- Development and testing environments
- Small-scale production deployments
- Single-server infrastructure
- Simplified operations and monitoring

**Advantages**:

- Single process to manage and monitor
- Shared database connection pool
- Minimal operational complexity
- Direct event bus communication

**Considerations**:

- API and indexer share CPU resources
- Cannot scale components independently
- Single point of failure

### Architecture 2: Separated Deployment

Independent processes for indexer and API with horizontal scaling capability:

```text
┌─────────────────┐        ┌─────────────────┐
│  bloklid        │        │  blokli-api     │ (multiple instances)
│  (indexer only) │        │  (standalone)   │
│                 │        │                 │
│  - RPC polling  │        │  - Queries      │
│  - Event proc.  │        │  - Mutations    │
│  - DB writes    │        │  - Subscr.      │
└─────────────────┘        └─────────────────┘
         │                          │
         │                          │
         ▼                          │
┌─────────────────┐                │
│  Blockchain RPC │                │
└─────────────────┘                │
         │                          │
         │                          │
         ▼                          ▼
┌──────────────────────────────────────┐
│         PostgreSQL Database          │
│  - Index data (read/write)           │
│  - Logs data (write: indexer)        │
│  - Logs data (read: both)            │
└──────────────────────────────────────┘
         │                          │
         └──────────────────────────┘
                   │
                   ▼
              ┌──────────┐
              │  Clients │
              └──────────┘
```

**Use Cases**:

- Production environments requiring high availability
- Scaling API separately from indexer
- Load balancing across multiple API instances
- Independent component updates and restarts

**Advantages**:

- Horizontal scaling of API servers
- Indexer continues during API maintenance
- Independent resource allocation
- Better fault isolation

**Architecture Notes**:

- Indexer has exclusive write access to index tables
- API servers are read-only consumers
- All servers can read logs tables
- Database connection pooling per process
- Load balancer distributes client requests across API instances

**Scaling Considerations**:

- API instances are stateless and can scale horizontally
- Indexer is singleton due to sequential event processing requirement
- Database becomes potential bottleneck at high scale
- Consider read replicas for query scaling

### Architecture 3: Dual SQLite Development

Lightweight deployment using SQLite with separated databases:

```text
┌─────────────────────────────────────┐
│           bloklid Process            │
│  ┌──────────────┐  ┌──────────────┐ │
│  │   Indexer    │  │  API Server  │ │
│  └──────────────┘  └──────────────┘ │
└─────────────────────────────────────┘
           │                  │
           ▼                  ▼
    ┌────────────────────────────┐
    │  SQLite Index DB           │
    │  - accounts, channels      │
    │  - balances, chain_info    │
    │  - account_state, etc.     │
    └────────────────────────────┘
           │
           ▼
    ┌────────────────────────────┐
    │  SQLite Logs DB            │
    │  - log, log_status         │
    │  - log_topic_info          │
    │  - Raw blockchain data     │
    │  - Snapshot replaceable    │
    └────────────────────────────┘
```

**Use Cases**:

- Local development environments
- Testing and integration environments
- Embedded deployments with limited resources
- Quick prototyping and experimentation

**Advantages**:

- No external database server required
- Logs DB can be atomically replaced from snapshot
- Fast sync by downloading pre-built logs database
- Reduced write lock contention through separation
- Simple backup and restore (file copy)

**Architectural Benefits**:

- Logs DB is write-heavy (append-only during normal operation)
- Index DB has mixed read/write pattern
- Separation prevents lock contention in SQLite's locking model
- Logs DB can be regenerated from blockchain if corrupted
- Index DB contains derived state and is more critical

**Limitations**:

- No concurrent writers (indexer must be singleton)
- Limited scalability compared to PostgreSQL
- No built-in replication or high availability
- File I/O can become bottleneck on slow storage

## Performance Characteristics

### Indexer Throughput

**Performance Factors**:

- RPC endpoint rate limits and response latency
- Database write throughput and transaction commit time
- Event handler processing complexity per event type
- Configured finality depth (confirmation blocks required)

**Optimization Strategies**:

- **Fast Sync via Snapshots**: Reduces initial sync from hours to minutes by importing pre-indexed logs database
- **Batch Log Fetching**: Configurable block range size balances memory usage with RPC call overhead
- **Dual Database Mode**: Separates high-volume log writes from indexed state reads (SQLite)
- **Async Event Processing**: Pipeline stages run concurrently where possible
- **Position-based Deduplication**: Prevents reprocessing same events after restart

**Typical Performance**:
Initial sync speed depends heavily on RPC endpoint performance. Fast sync can reduce multi-hour syncs to under 30 minutes. Real-time indexing typically processes blocks within seconds of finality threshold.

### GraphQL API Performance

**Query Optimization**:

- Required filters prevent full table scans and accidental exposure of entire dataset
- Batch loading pattern for related entities reduces N+1 query problems
- Temporal queries leverage indexed position columns for efficient lookups
- Account aggregation uses optimized join strategy with four separate queries
- Database connection pooling amortizes connection overhead

**Subscription Efficiency**:

- Broadcast event bus provides efficient one-to-many distribution
- Watermark synchronization prevents redundant historical queries
- SSE keeps connections lightweight compared to WebSocket overhead
- Zstandard compression reduces bandwidth for large payloads
- Subscribers filter events client-side reducing database queries

**Compression Strategy**:
Selective compression balances bandwidth savings with latency:

- Only responses exceeding 1KB threshold to avoid overhead on small payloads
- Excludes SSE responses to preserve real-time characteristics
- Balanced compression level optimizes CPU usage vs. compression ratio
- Significantly reduces bandwidth for large result sets (accounts with many announcements)

**Scalability Characteristics**:
API servers are stateless and horizontally scalable. Subscription memory usage grows linearly with active subscriber count. Database becomes bottleneck at high read volume (consider read replicas).

### Database Scaling

**PostgreSQL Characteristics**:

- Single database simplifies management and backup procedures
- MVCC provides excellent read/write concurrency without blocking
- Connection pooling shares expensive database connections
- Query planner optimizes complex joins automatically
- Supports read replicas for scaling query workload
- Better suited for production deployments with high concurrency

**SQLite Dual Database Characteristics**:

- Reduces lock contention by separating write-heavy logs from read-heavy index
- Logs DB can be atomically replaced during fast sync operation
- Suitable for embedded deployments with limited resources
- Simple backup through file system copy operations
- No network overhead for database access
- Limited to single-writer concurrency model

**Scaling Strategies**:

- Index relevant columns for common query patterns
- Use read replicas for query scaling (PostgreSQL)
- Partition historical data if query performance degrades
- Consider time-series database for raw logs at massive scale
- Implement caching layer for frequently accessed data

## Security Considerations

### Transaction Submission Security

**Validation Layers**:

The transaction submission system implements defense-in-depth with multiple validation layers:

1. **Format Validation**: Verifies transaction is properly RLP-encoded with valid structure
2. **Contract Allowlist**: Only permits transactions targeting pre-approved HOPR contract addresses
3. **Function Selector Validation**: Validates that function call is among permitted operations
4. **Signature Validation**: Confirms transaction is properly signed (prevents malleability)

**Transaction Modes and Security Tradeoffs**:

- **Fire-and-forget**: No tracking means no status visibility; suitable for non-critical operations
- **Async**: Background monitoring provides visibility but requires trusted monitoring process
- **Sync**: Waits for confirmations providing highest certainty at cost of latency

**Rate Limiting Considerations**:
Current implementation lacks rate limiting per client. Future enhancement should implement:

- Per-IP rate limiting for transaction submission
- Per-address rate limiting to prevent griefing
- Global rate limiting to protect RPC endpoint

### API Security

**Current Protections**:

- **CORS Configuration**: Configurable allowed origins prevent unauthorized cross-origin requests
- **Required Filters**: Query endpoints require identity filters preventing full dataset exposure
- **Input Validation**: All user-provided data validated before database access
- **Address Validation**: Ethereum addresses validated for proper format and checksum

**Attack Surface Considerations**:

- GraphQL introspection enabled by default (should disable in production)
- No query complexity limits (vulnerable to resource exhaustion)
- No query depth limits (vulnerable to nested query attacks)
- No authentication/authorization mechanism (all data publicly accessible)

**Future Security Enhancements**:

- Implement authentication layer (JWT, OAuth, API keys)
- Add authorization rules for sensitive operations
- Implement GraphQL query complexity analysis
- Add query depth limits
- Implement per-client rate limiting
- Consider query cost budgets per time window
- Add request logging and anomaly detection

### Database Security

**Data Integrity Protections**:

- **Reorg Handling**: Detects blockchain reorganizations and marks affected logs
- **Transaction Safety**: All state changes wrapped in database transactions
- **Position Constraints**: Unique constraints prevent duplicate event processing
- **Atomic Snapshot Import**: Logs database replacement is atomic operation

**Precision Preservation**:
Large integers stored as binary to preserve full precision across language boundaries. GraphQL uses String type for UInt64 values to avoid JavaScript Number precision loss.

**Backup and Recovery**:
Event sourcing pattern enables point-in-time recovery. Complete state can be reconstructed from logs table. Regular database backups essential for disaster recovery.

## Error Handling Strategy

### Indexer Error Handling

**Recoverable Errors**:
These errors trigger retry logic without halting the indexer:

- RPC connection failures trigger exponential backoff retry
- Temporary database errors cause transaction rollback and retry
- Rate limit errors pause and retry after configured interval
- Log parsing errors skip event and log warning without halting

**Non-recoverable Errors**:
These errors halt the indexer requiring operator intervention:

- Invalid chain configuration indicates misconfiguration requiring fix
- Database schema mismatch indicates migration needed
- Corrupted state beyond repair requires investigation
- Missing required contract addresses prevents operation

**Error Recovery Strategy**:
Indexer maintains watermark position enabling exact resume after restart. Skipped events due to parsing errors are logged for later analysis. Database transactions ensure partial processing never corrupts state.

### API Error Handling

**GraphQL Error Pattern**:
Uses union types for expected errors providing structured error responses:

- Domain-specific error types (InvalidAddressError, ContractNotAllowedError)
- Consistent error code strings for programmatic handling
- Human-readable error messages for debugging
- Contextual information (invalid address value, contract address, etc.)

**Error Response Structure**:
GraphQL union types allow returning either success type or specific error type. Clients use GraphQL fragments to handle different response types. Errors include code field for programmatic handling and message field for display.

**Unexpected Error Handling**:
Internal errors return generic GraphQL error without exposing implementation details. Errors are logged server-side with full stack trace for debugging. Client receives safe error message without sensitive information.

## Future Architecture Considerations

### Scalability Roadmap

**Indexer Enhancements**:

- **Parallel Event Processing**: Process events from different contracts concurrently
- **Sharded Database**: Partition historical data by time period for query performance
- **Read Replicas**: Separate read and write workloads at database level
- **Multi-chain Support**: Index multiple blockchain networks simultaneously

**API Enhancements**:

- **Query Caching**: Implement Redis caching layer for frequently accessed data
- **DataLoader Pattern**: Batch and cache database queries within single request
- **Persisted Queries**: Pre-register queries reducing parsing overhead
- **CDN Integration**: Cache static GraphQL schema and playground assets

### Feature Extension Roadmap

**Indexer Features**:

- **Multi-chain Indexing**: Support multiple blockchain networks with separate configurations
- **Advanced Reorg Handling**: Handle deeper reorganizations with state rollback
- **Event Replay**: Debug tool to replay historical events for testing
- **Snapshot Generation**: Create logs database snapshots for distribution

**API Features**:

- **Pagination**: Implement cursor-based pagination for large result sets
- **Advanced Filtering**: Support date ranges, complex conditions, sorting
- **Aggregation Queries**: Provide statistics, summaries, and analytical queries
- **WebSocket Subscriptions**: Alternative to SSE for bidirectional communication
- **Batch Queries**: Process multiple operations in single request

### Monitoring and Observability

**Metrics** (Prometheus integration):
Key metrics for operational visibility:

- Indexer current block height and sync progress percentage
- Event processing counters by contract type
- API request rate, latency distribution, and error rate
- Database query performance and connection pool utilization
- Subscription count and event fan-out rate

**Distributed Tracing**:
Implement distributed tracing to track requests across components:

- Request ID propagation from API through database queries
- Trace indexer pipeline from RPC fetch through event broadcast
- Identify performance bottlenecks in complex operations
- Correlate errors across component boundaries

**Structured Logging**:
Enhanced logging for operational insights:

- JSON-formatted logs for machine parsing
- Consistent log levels across components (trace, debug, info, warn, error)
- Error aggregation and alerting
- Query performance logging with execution plans

**Health Checks**:
Kubernetes-compatible health endpoints for operational monitoring:

- `/healthz` - Liveness probe: minimal check that process is alive, returns version and status
- `/readyz` - Readiness probe: comprehensive check for service readiness with:
  - Database connectivity (queries chain_info table)
  - RPC endpoint availability (fetches current block number)
  - Indexer lag calculation (blocks behind chain head, configurable threshold default: 10 blocks)

Health check configuration:

- `max_indexer_lag` - Maximum allowed lag before readiness fails
- `timeout_ms` - Timeout for health check operations

### Finality Handling in Readiness Checks

The readiness check automatically accounts for blockchain finality to prevent false-positive "ready" states:

**Block Number Finality Adjustment**:
RPC block heights are treated as confirmed by subtracting the configured finality depth. For example, with a finality depth of 8, a chain head at block 1000 yields a confirmed height of 992, so only blocks with 8+ confirmations count as confirmed.

**Readiness Calculation**:
Readiness compares the confirmed height with the indexer watermark. If the lag in confirmed blocks is within `max_indexer_lag`, readiness passes.

**Effective Threshold**:
Because readiness uses confirmed height (not chain head), the indexer can be up to `max_indexer_lag + finality` behind the tip while still reporting ready.

**Example (Gnosis Chain)**:

- Configuration: `max_indexer_lag=10`, `finality=8`
- Latest RPC block: 1000
- Confirmed RPC block: 992
- Indexed block: 982
- Calculated lag: 10 blocks
- Result: READY

The indexer is actually 18 blocks behind the RPC chain head (1000 - 982), but only 10 blocks behind confirmed blocks—within the acceptable threshold.

**Configuration Flow**:
Finality is defined by the selected network, propagated through runtime configuration into the RPC layer, and reused by the readiness checks.

## Design Principles and Patterns

### Event Sourcing

The database implements event sourcing pattern where state changes are stored as immutable events with position tracking. Benefits include complete audit trail, point-in-time queries, and ability to reconstruct state from events.

### Watermark Pattern

The system maintains a watermark (block, tx_index, log_index) representing the last fully processed position. This enables exact resume after restart, provides synchronization point for subscriptions, and prevents duplicate processing.

### Two-Phase Subscription

Subscriptions use two-phase model: Phase 1 delivers historical snapshot at watermark, Phase 2 streams real-time updates. Watermark synchronization ensures no gaps or duplicates between phases.

### Separation of Concerns

Clear boundaries between components:

- Indexer responsible only for reading blockchain and storing events
- Database layer handles persistence and query optimization
- API layer handles client interaction and GraphQL schema
- Transaction system handles submission and monitoring

### Fail-Safe Design

System designed to fail safely:

- Indexer checkpoints watermark before processing each block
- Database transactions ensure atomic state updates
- Validation layers prevent invalid transactions from reaching blockchain
- Subscription setup synchronized to prevent missed events

## Conclusion

Blokli architecture is designed with these priorities:

**Reliability**:

- Event sourcing preserves complete history
- Reorg handling maintains consistency with blockchain
- Transaction safety through database ACID properties
- Watermark-based resume capability

**Performance**:

- Fast sync reduces initial synchronization time
- Dual databases reduce contention
- Efficient query patterns minimize database load
- Selective compression optimizes bandwidth

**Flexibility**:

- Standalone or embedded API deployment modes
- PostgreSQL or SQLite database backends
- Three transaction submission modes for different needs
- Configurable parameters for different environments

**Real-time Capability**:

- SSE subscriptions for live updates
- Event bus fan-out for efficient distribution
- Watermark synchronization prevents race conditions
- Background transaction monitoring

**Maintainability**:

- Clear component boundaries
- Trait-based abstractions
- Comprehensive error handling
- Type-safe database access

This architecture supports development environments (single process, SQLite) through production deployments (separated components, PostgreSQL) while maintaining consistent behavior and programming model across all configurations.
