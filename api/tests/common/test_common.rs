//! Integration tests for the common test utilities
//!
//! These tests verify that the shared test setup functions work correctly
//! and provide the expected components for API integration testing.

use super::*;

/// Test that setup_test_environment creates all required components
#[test_log::test(tokio::test)]
async fn test_setup_environment_creates_all_components() -> anyhow::Result<()> {
    let ctx = setup_simple_test_environment().await?;

    // Verify all components are created
    assert!(!ctx.anvil.endpoint().is_empty(), "Anvil should be running");
    assert!(!ctx.test_accounts.is_empty(), "Test accounts should be created");
    assert_eq!(ctx.test_accounts.len(), 3, "Should create 3 test accounts by default");
    assert!(!ctx.contract_addrs.token.is_zero(), "Contract addresses should be configured");
    assert_eq!(ctx.chain_id, 31337, "Chain ID should be Anvil default");
    assert!(ctx.db.is_some(), "Database should be Some (always created for GraphQL schema)");

    Ok(())
}

/// Test that custom configuration options work correctly
#[test_log::test(tokio::test)]
async fn test_custom_configuration() -> anyhow::Result<()> {
    let config = TestEnvironmentConfig {
        expected_block_time: Duration::from_secs(2),
        num_test_accounts: 5,
        run_migrations: true,
    };

    let ctx = setup_test_environment(config).await?;

    // Verify custom configuration is applied
    assert_eq!(ctx.test_accounts.len(), 5, "Should create 5 test accounts");
    assert!(ctx.db.is_some(), "Database should be Some when run_migrations is true");

    Ok(())
}

/// Test that RPC operations are properly configured
#[test_log::test(tokio::test)]
async fn test_rpc_operations_configuration() -> anyhow::Result<()> {
    let ctx = setup_simple_test_environment().await?;

    // Verify contract addresses are configured
    assert!(!ctx.contract_addrs.token.is_zero(), "Token address should be set");
    assert!(!ctx.contract_addrs.channels.is_zero(), "Channels address should be set");

    Ok(())
}