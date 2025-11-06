# Integration Test Refactoring Plan

## Overview

This document outlines the refactoring plan for the integration tests in `indexer_integration_test.rs`, which were disabled due to API changes in the indexer architecture.

## Why Tests Were Disabled

The integration tests access private APIs and internal implementation details that have evolved significantly:

1. **Constructor Signature Changes**: `ContractEventHandlers::new()` now requires an `IndexerState` parameter
2. **Event Publishing Model Changed**: The indexer no longer returns event types from `process_log_event()`; events are now published via `IndexerState` event bus
3. **Private API Access**: Tests directly instantiate and call internal handler methods that should be tested through public interfaces

## Current State

- **Status**: All integration tests wrapped in `#[cfg(disabled_pending_refactor)]` module
- **Files Affected**: `chain/indexer/tests/indexer_integration_test.rs` (~1200 lines)
- **Number of Tests**: ~15 integration tests covering various blockchain event scenarios

## Refactoring Approach: Option B (Recommended)

**Approach**: Refactor tests to use public APIs and verify behavior through the IndexerState event bus, mirroring the successful pattern used in handlers.rs unit tests.

**Estimated Effort**: 10-15 hours

### Phase 1: Infrastructure Setup (2-3 hours)

#### 1.1 Create Test Fixtures Module

Create a new file `chain/indexer/tests/fixtures.rs` with reusable test infrastructure:

```rust
use blokli_chain_indexer::{ContractEventHandlers, IndexerState};
use blokli_chain_indexer::state::IndexerEvent;
use blokli_db_api::BlokliDbAllOperations;
use blokli_chain_rpc::HoprIndexerRpcOperations;
use async_broadcast;

pub struct TestHarness<T, Db> {
    pub handlers: ContractEventHandlers<T, Db>,
    pub indexer_state: IndexerState,
    pub event_receiver: async_broadcast::Receiver<IndexerEvent>,
}

impl<T: HoprIndexerRpcOperations + Clone + Send + 'static, Db: BlokliDbAllOperations + Clone>
    TestHarness<T, Db>
{
    pub fn new(rpc_operations: T, db: Db, contract_addresses: ContractAddresses) -> Self {
        let indexer_state = IndexerState::default();
        let event_receiver = indexer_state.subscribe_to_events();

        let handlers = ContractEventHandlers::new(
            contract_addresses,
            db,
            rpc_operations,
            indexer_state.clone(),
        );

        Self {
            handlers,
            indexer_state,
            event_receiver,
        }
    }

    pub fn try_recv_event(&mut self) -> Option<IndexerEvent> {
        self.event_receiver.try_recv().ok()
    }

    pub async fn recv_event(&mut self) -> IndexerEvent {
        self.event_receiver.recv().await.expect("Failed to receive event")
    }
}
```

#### 1.2 Update Test Module Structure

Reorganize `indexer_integration_test.rs`:

```rust
mod fixtures;

use fixtures::TestHarness;

// Common setup helpers
async fn setup_test_environment() -> TestEnvironment {
    // Start anvil, deploy contracts, setup database
}

// Event verification helpers
fn assert_account_updated_event(event: &IndexerEvent, expected_address: Address) {
    // Common assertion logic
}

fn assert_channel_updated_event(event: &IndexerEvent, expected_channel_id: &[u8; 32]) {
    // Common assertion logic
}
```

### Phase 2: Refactor Individual Tests (6-9 hours)

For each test, follow this pattern:

#### 2.1 Test Structure Template

```rust
#[tokio::test]
async fn test_event_scenario() {
    // 1. Setup: Environment, contracts, database
    let env = setup_test_environment().await;

    // 2. Create test harness with event capture
    let mut harness = TestHarness::new(
        env.rpc_operations,
        env.db.clone(),
        env.contract_addresses,
    );

    // 3. Execute: Trigger blockchain events via Indexer public API
    let indexer = Indexer::new(
        env.rpc,
        harness.handlers,
        env.db,
        IndexerConfig::default(),
        async_channel::unbounded().0,
        harness.indexer_state.clone(),
    );

    indexer.index_block_range(start_block, end_block).await.unwrap();

    // 4. Verify: Check published events
    let event = harness.recv_event().await;
    assert_account_updated_event(&event, expected_address);

    // 5. Verify: Check database state
    let db_state = env.db.get_account(expected_address).await.unwrap();
    assert_eq!(db_state.balance, expected_balance);
}
```

#### 2.2 Tests to Refactor (in priority order)

1. **High Priority** (Core functionality):
   - `test_safe_registered` - Safe registration event handling
   - `test_channel_opened` - Channel opening event handling
   - `test_channel_closed` - Channel closing event handling
   - `test_ticket_redeemed` - Ticket redemption event handling

2. **Medium Priority** (Important features):
   - `test_node_announcement` - Node announcement handling
   - `test_balance_update` - Balance update tracking
   - `test_allowance_update` - Allowance update tracking
   - `test_channel_balance_increased` - Channel balance modifications

3. **Low Priority** (Edge cases):
   - `test_multiple_events_in_block` - Multiple events per block
   - `test_reorg_handling` - Blockchain reorganization handling
   - Remaining integration scenarios

### Phase 3: Remove Private API Dependencies (2-3 hours)

#### 3.1 Identify Private API Usage

Current tests may access:
- Internal handler methods directly
- Private database query methods
- Internal state not exposed through public APIs

#### 3.2 Replace with Public API Calls

For each private API usage:

```rust
// BEFORE: Direct handler call (private API)
handlers.handle_token_approval(&log_entry).await?;

// AFTER: Trigger via Indexer public API
indexer.index_block_range(block_num, block_num).await?;
```

#### 3.3 Add Public APIs if Needed

If tests require functionality not available through public APIs:
1. Evaluate if the test is actually testing internal implementation details
2. If legitimate need, add public methods to `Indexer` or `IndexerState`
3. Document new public APIs

### Phase 4: Validation & Documentation (1-2 hours)

#### 4.1 Verify Test Coverage

- Run `cargo test --package blokli-chain-indexer --test indexer_integration_test`
- Verify all tests pass
- Check test coverage hasn't decreased

#### 4.2 Update Documentation

- Remove `#[cfg(disabled_pending_refactor)]` attribute
- Update test module documentation
- Document test harness usage
- Add examples for common test patterns

#### 4.3 Remove This Document

Once refactoring is complete and tests are re-enabled, this document can be deleted.

## Alternative Approach: Option A (Not Recommended)

**Approach**: Update tests minimally to compile with new API signatures

**Effort**: 2-3 hours

**Why Not Recommended**:
- Tests would still access private APIs
- Brittle tests that break with internal refactoring
- Misses opportunity to improve test quality
- Doesn't align with testing best practices

## Success Criteria

- [ ] All integration tests re-enabled (remove `#[cfg(disabled_pending_refactor)]`)
- [ ] All tests pass: `cargo test --package blokli-chain-indexer`
- [ ] Tests use only public APIs
- [ ] Event verification via IndexerState event bus
- [ ] Test harness infrastructure documented
- [ ] No direct access to internal handler methods
- [ ] Test coverage maintained or improved
- [ ] CI/CD pipeline passes

## Resources

- **Reference Implementation**: See `chain/indexer/src/handlers.rs` test module for examples of event-based verification
- **Event Bus API**: `IndexerState::subscribe_to_events()` and `IndexerEvent` enum
- **Test Helpers**: `init_handlers_with_events()` and `try_recv_event()` in handlers.rs

## Questions & Decisions

### Q: Should we test handlers directly or through the Indexer?

**Decision**: Test through the Indexer public API when possible. This:
- Tests realistic usage patterns
- Reduces brittleness from internal changes
- Validates integration between components

### Q: What if we need to test error handling in handlers?

**Decision**: Add targeted unit tests in handlers.rs for error cases. Integration tests should focus on happy paths and common failure scenarios.

### Q: Should we keep the existing test structure or reorganize?

**Decision**: Reorganize tests by feature area (channels, safe registry, tickets) rather than by event type. This improves maintainability and makes it easier to find related tests.

## Timeline

**Total Estimated Time**: 10-15 hours

- Week 1 (4-5 hours): Phase 1 & Phase 2 (High Priority tests)
- Week 2 (4-5 hours): Phase 2 (Medium & Low Priority tests)
- Week 3 (2-3 hours): Phase 3 & Phase 4

Can be completed incrementally with regular commits after each test is refactored.
