use std::{collections::HashMap, time::Duration};

use anyhow::{Result, anyhow};
use blokli_client::api::{BlokliQueryClient, SafeSelector};
use blokli_integration_tests::{
    constants::parsed_safe_balance,
    fixtures::{IntegrationFixture, integration_fixture as fixture},
};
use rstest::*;
use serial_test::serial;

#[rstest]
#[test_log::test(tokio::test)]
#[serial]
/// Deploy a safe for each account and announce it simultaneously, and then open a channel between each pair of safes simultaneously. This test verifies that the system can handle multiple concurrent deployments and channel openings without issues.
async fn open_multiple_channels_simultaneously(#[future(awt)] fixture: IntegrationFixture) -> Result<()> {
    let accounts = fixture.accounts();

    // deploy a safe for each account and announce it, all simultaneously
    let deploy_and_announce_futs = accounts
        .iter()
        .map(|account| fixture.deploy_safe_and_announce(account, parsed_safe_balance()));
    let _ = futures::future::join_all(deploy_and_announce_futs)
        .await
        .into_iter()
        .collect::<Result<Vec<_>>>()?;

    tokio::time::sleep(Duration::from_secs(8)).await;

    // Verify that all safes has been deployed and announced simultaneously
    let safes_futures = accounts.iter().map(|account| {
        fixture
            .client()
            .query_safe(SafeSelector::ChainKey(account.to_alloy_address().into()))
    });

    let modules_by_chain_key = futures::future::try_join_all(safes_futures)
        .await?
        .iter()
        .flatten()
        .map(|safe| (safe.chain_key.clone(), safe.module_address.clone()))
        .collect::<HashMap<String, String>>();

    // Create all possible pairs of accounts and open channels between them simultaneously
    let pairs = accounts
        .iter()
        .zip(accounts.iter().cycle().skip(1))
        .take(accounts.len());

    let fixture_ref = &fixture;
    let open_channel_futs = pairs.map(|(src, dst)| {
        let src_chain_key = src.address.to_string();
        let module_address = modules_by_chain_key
            .get(&src_chain_key)
            .cloned()
            .ok_or_else(|| anyhow!("Missing safe for source chain key {src_chain_key}"));

        async move {
            let module_address = module_address?;
            fixture_ref
                .open_channel(src, dst, parsed_safe_balance(), module_address.as_str())
                .await
        }
    });

    let _ = futures::future::try_join_all(open_channel_futs).await?;

    Ok(())
}
