use std::{collections::HashMap, ops::Mul, sync::Arc, time::Duration};

use anyhow::{Result, anyhow};
use blokli_client::api::{BlokliQueryClient, SafeSelector};
use blokli_integration_tests::{
    constants::parsed_safe_balance,
    fixtures::{IntegrationFixture, integration_fixture as fixture},
};
use rstest::*;
use serial_test::serial;
use tokio::sync::Mutex;

#[rstest]
#[test_log::test(tokio::test)]
#[serial]
/// Deploy a safe for each account and announce it simultaneously, and then open a channel between each pair of safes
/// simultaneously. This test verifies that the system can handle multiple concurrent deployments and channel openings
/// without issues.
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

    // Create all possible source/destination pairs (excluding self)
    let pairs = accounts.iter().flat_map(|src| {
        accounts
            .iter()
            .filter(move |dst| dst.address != src.address)
            .map(move |dst| (src, dst))
    });

    let fixture_ref = &fixture;

    let approve_futs = accounts.iter().map(|account| {
        let fixture_ref = &fixture_ref;

        let src_chain_key: String = account.address.to_string();
        let module_address = modules_by_chain_key
            .get(&src_chain_key)
            .cloned()
            .unwrap_or_else(|| panic!("Missing safe for source chain key {src_chain_key}"));
        
        async move {
            fixture_ref
                .approve(
                    account, 
                    parsed_safe_balance().mul(accounts.len() - 1), 
                    module_address.as_str()).await
        }
    });

    let _ = futures::future::try_join_all(approve_futs).await?;

    // Get the tx counts for each accounts and store them as hashmap of chain_key to tx_count
    let nonce_futures = accounts.iter().map(|account| {
        let fixture_ref = fixture_ref;
        async move {
            let nonce = fixture_ref.rpc().transaction_count(&account.address).await?;
            Ok::<(String, u64), anyhow::Error>((account.address.to_string(), nonce))
        }
    });
    let nonces = futures::future::try_join_all(nonce_futures)
    .await?
    .into_iter()
    .collect::<HashMap<String, u64>>();

    let nonces_ref = Arc::new(Mutex::new(nonces));
    let open_channel_futs = pairs.map(|(src, dst)| {
        let fixture_ref = fixture_ref;
        let nonces_ref = Arc::clone(&nonces_ref);
        let src_chain_key = src.address.to_string();
        let module_address = modules_by_chain_key
            .get(&src_chain_key)
            .cloned()
            .ok_or_else(|| anyhow!("Missing safe for source chain key {src_chain_key}"));

        async move {
            let module_address = module_address?;
            let nonce = {
                let mut nonces_guard = nonces_ref.lock().await;
                nonces_guard.get_mut(&src_chain_key).map(|value| {
                    let nonce_value = *value;
                    *value = nonce_value + 1;
                    nonce_value
                })
            };
            fixture_ref
                .open_channel(src, dst, parsed_safe_balance(), module_address.as_str(), nonce)
                .await
        }
    });

    let _ = futures::future::try_join_all(open_channel_futs).await?;

    Ok(())
}
