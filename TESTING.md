# Testing Guide

This document provides comprehensive guidance for testing the Blokli indexer and API, with special focus on temporal queries, blockchain reorganization handling, and edge cases.

## Table of Contents

- [Running Tests](#running-tests)
- [Test Organization](#test-organization)
- [Writing New Tests](#writing-new-tests)
- [Continuous Integration](#continuous-integration)
- [Troubleshooting](#troubleshooting)
- [Additional Resources](#additional-resources)

## Running Tests

### Quick Test Commands

```bash
# Run all tests (requires nix develop shell)
just test

# Run tests for a specific package
just test-package blokli-db

# Run with debug output (single-threaded, shows println!)
just test-debug

# Run specific test by name
cargo test test_get_channel_state_at -F runtime-tokio -- --nocapture

# Run integration tests
just test-indexer
```

### Post-Test Workflow

Always run the quick check after making changes:

```bash
just quick  # Runs fmt, clippy, and check
```

## Test Organization

### Unit Tests

Unit tests are co-located with the code they test using `#[cfg(test)]` modules:

### Integration Tests

Integration tests are in `bloklid/tests/`:

- **`indexer_startup_test.rs`**: End-to-end indexer tests
  - Indexer startup and shutdown
  - Fast sync mode
  - Start block configuration
  - Mock RPC integration

## Writing New Tests

### Test Structure

Follow this structure for new tests:

```rust
#[cfg(test)]
mod tests {
    use super::*;
    use blokli_db::db::BlokliDb;
    use blokli_db_entity::codegen::{channel, channel_state};
    use sea_orm::{ActiveValue, EntityTrait, ActiveModelTrait};

    #[tokio::test]
    async fn test_your_feature() -> anyhow::Result<()> {
        // 1. Setup: Create in-memory database
        let db = BlokliDb::new_in_memory().await?;

        // 2. Arrange: Create test data
        let test_data = create_test_data(&db).await?;

        // 3. Act: Perform operation under test
        let result = function_under_test(&db, test_data).await?;

        // 4. Assert: Verify results
        assert_eq!(result.expected_field, expected_value);

        Ok(())
    }
}
```

### Best Practices

1. **Use `anyhow::Result<()>`** for test return types
2. **Prefer `new_in_memory()`** for unit tests (faster, isolated)
3. **Use descriptive test names** that explain what is being tested
4. **Test both success and error paths**
5. **Clean up resources** (though in-memory DBs are automatically cleaned up)
6. **Use `#[tokio::test]`** for async tests
7. **Add `-- --nocapture`** to see `println!` output during debugging
8. **Document complex test scenarios** with comments

### Common Patterns

#### Testing with Mock Data

```rust
#[tokio::test]
async fn test_with_mock_data() -> anyhow::Result<()> {
    let db = BlokliDb::new_in_memory().await?;

    // Create mock blockchain position
    let position = BlockPosition {
        block: 1000,
        tx_index: 5,
        log_index: 3,
    };

    // Insert test state
    let state = channel_state::ActiveModel {
        channel_id: ActiveValue::Set(channel_id),
        balance: ActiveValue::Set(vec![1, 2, 3, 4, 5, 6, 7, 8, 9, 10, 11, 12]),
        published_block: ActiveValue::Set(position.block as i64),
        published_tx_index: ActiveValue::Set(position.tx_index as i64),
        published_log_index: ActiveValue::Set(position.log_index as i64),
        // ... other fields
        ..Default::default()
    };

    state.insert(db.conn(TargetDb::Index)).await?;

    Ok(())
}
```

#### Testing Error Conditions

```rust
#[tokio::test]
async fn test_error_handling() -> anyhow::Result<()> {
    let db = BlokliDb::new_in_memory().await?;

    // Test with invalid ID
    let result = get_channel_state_at(
        &db.conn(TargetDb::Index),
        -1,  // Invalid channel ID
        BlockPosition { block: 1000, tx_index: 0, log_index: 0 }
    ).await?;

    // Should return None for non-existent channel
    assert!(result.is_none());

    Ok(())
}
```

## Continuous Integration

Tests run automatically in CI pipelines. Ensure all tests pass before submitting PRs:

```bash
# Run full test suite
just test

# Run formatting and linting
just quick

# Generate documentation
just doc
```

## Troubleshooting

### Tests Failing Locally

1. Ensure you're in the nix develop shell: `nix develop`
2. Clean build artifacts: `cargo clean`
3. Rebuild: `just build`
4. Run tests with output: `just test-debug`

### Slow Tests

- Use `new_in_memory()` instead of file-based databases
- Reduce dataset sizes in performance tests
- Run specific tests instead of full suite

### Flaky Tests

- Check for race conditions in concurrent operations
- Ensure proper cleanup between test runs
- Use deterministic random data (seeded RNG)

## Additional Resources

- [SeaORM Testing Guide](https://www.sea-ql.org/SeaORM/docs/write-test/testing/)
- [Tokio Testing Documentation](https://tokio.rs/tokio/topics/testing)
- Design documents in `/design` directory
- Module documentation in source files
