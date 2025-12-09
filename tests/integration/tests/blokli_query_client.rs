use std::{str::FromStr, time::Duration};

use alloy::primitives::Address;
use anyhow::{Result, anyhow};
use blokli_client::api::{AccountSelector, BlokliQueryClient, BlokliSubscriptionClient};
use blokli_integration_tests::fixtures::{IntegrationFixture, integration_fixture as fixture};
use futures::stream::StreamExt;
use rstest::*;
use serial_test::serial;
use tokio::time::sleep;

#[rstest]
#[test_log::test(tokio::test)]
#[serial]
async fn count_accounts_matches_deployed_accounts(fixture: IntegrationFixture) -> Result<()> {
    Ok(())
}

#[rstest]
#[test_log::test(tokio::test)]
#[serial]
async fn count_accounts_with_filter(fixture: IntegrationFixture) -> Result<()> {
    Ok(())
}

#[rstest]
#[test_log::test(tokio::test)]
#[serial]
async fn query_accounts(fixture: IntegrationFixture) -> Result<()> {
    Ok(())
}

#[rstest]
#[test_log::test(tokio::test)]
#[serial]
async fn query_native_balance(fixture: IntegrationFixture) -> Result<()> {
    Ok(())
}

#[rstest]
#[test_log::test(tokio::test)]
#[serial]
async fn query_token_balance(fixture: IntegrationFixture) -> Result<()> {
    Ok(())
}

#[rstest]
#[test_log::test(tokio::test)]
#[serial]
async fn query_transaction_count(fixture: IntegrationFixture) -> Result<()> {
    Ok(())
}

#[rstest]
#[test_log::test(tokio::test)]
#[serial]
async fn query_safe_allowance(fixture: IntegrationFixture) -> Result<()> {
    Ok(())
}

#[rstest]
#[test_log::test(tokio::test)]
#[serial]
async fn query_safe(fixture: IntegrationFixture) -> Result<()> {
    Ok(())
}

#[rstest]
#[test_log::test(tokio::test)]
#[serial]
async fn count_channels(fixture: IntegrationFixture) -> Result<()> {
    Ok(())
}

#[rstest]
#[test_log::test(tokio::test)]
#[serial]
async fn query_channels(fixture: IntegrationFixture) -> Result<()> {
    Ok(())
}

#[rstest]
#[test_log::test(tokio::test)]
#[serial]
async fn query_transaction_status(fixture: IntegrationFixture) -> Result<()> {
    Ok(())
}

#[rstest]
#[test_log::test(tokio::test)]
#[serial]
async fn chain_ids_provided_by_blokli_matches_the_rpc(fixture: IntegrationFixture) -> Result<()> {
    sleep(Duration::from_secs(8)).await;

    let chain = fixture.client.query_chain_info().await?;
    let chain_id = fixture.rpc.chain_id().await?;

    assert_eq!(chain.chain_id as u64, chain_id);

    Ok(())
}

#[rstest]
#[test_log::test(tokio::test)]
#[serial]
async fn query_version_should_succeed(fixture: IntegrationFixture) -> Result<()> {
    sleep(Duration::from_secs(8)).await;

    fixture.client.query_version().await?;
    Ok(())
}

#[rstest]
#[test_log::test(tokio::test)]
#[serial]
async fn subscrive_account_by_private_key(fixture: IntegrationFixture) -> Result<()> {
    sleep(Duration::from_secs(8)).await;

    let [input] = fixture.sample_accounts::<1>();
    let address = Address::from_str(&input.address)?.into_array();

    let selector = AccountSelector::Address(address);
    let mut subscription = fixture.client.subscribe_accounts(Some(selector))?;

    let output = subscription
        .next()
        .await
        .ok_or_else(|| anyhow!("no update received from subscription"))??;

    assert_eq!(output.chain_key, input.address);

    Ok(())
}

#[rstest]
#[test_log::test(tokio::test)]
#[serial]
async fn query_health(fixture: IntegrationFixture) -> Result<()> {
    Ok(())
}
