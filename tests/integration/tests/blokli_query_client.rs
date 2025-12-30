use std::{str::FromStr, time::Duration};

use alloy::primitives::{Address, U256};
use anyhow::Result;
use blokli_client::api::{
    AccountSelector, BlokliQueryClient, ChannelFilter, ChannelSelector, SafeSelector, types::ChannelStatus,
};
use blokli_integration_tests::fixtures::{IntegrationFixture, integration_fixture as fixture};
use hopr_internal_types::channels::generate_channel_id;
use hopr_primitive_types::prelude::{HoprBalance, XDaiBalance};
use rstest::*;
use serial_test::serial;
use tokio::time::sleep;
use tracing::debug;

const INITIAL_SAFE_BALANCE: u64 = 500_000_000_000_000_000;

#[rstest]
#[test_log::test(tokio::test)]
#[serial]
/// deploy a safe, publish an announcement using the safe module, and verify that the account count
/// increases by one.
async fn count_accounts_matches_deployed_accounts(#[future(awt)] fixture: IntegrationFixture) -> Result<()> {
    let [account] = fixture.sample_accounts::<1>();

    let account_count = fixture
        .client()
        .count_accounts(AccountSelector::Address(account.hopr_address().into()))
        .await?;

    fixture.deploy_safe_and_announce(account, INITIAL_SAFE_BALANCE).await?;

    assert_eq!(
        fixture
            .client()
            .count_accounts(AccountSelector::Address(account.hopr_address().into()))
            .await?,
        account_count + 1
    );

    Ok(())
}

#[rstest]
#[test_log::test(tokio::test)]
#[serial]
/// deploy a safe, publish an announcement using the safe module, and verify that the account is
/// found upon querying.
async fn query_accounts(#[future(awt)] fixture: IntegrationFixture) -> Result<()> {
    let [account] = fixture.sample_accounts::<1>();

    fixture.deploy_safe_and_announce(account, INITIAL_SAFE_BALANCE).await?;

    let found_accounts = fixture
        .client()
        .query_accounts(AccountSelector::Address(account.hopr_address().into()))
        .await?;

    assert_eq!(found_accounts.len(), 1);
    assert_eq!(
        found_accounts[0].chain_key.to_lowercase(),
        account.address.to_lowercase()
    );

    Ok(())
}

#[rstest]
#[test_log::test(tokio::test)]
#[serial]
/// query the native balance of an EOA via blokli and via direct RPC, and verify that both match
/// and that the returned format is correct.
async fn query_native_balance(#[future(awt)] fixture: IntegrationFixture) -> Result<()> {
    let [account] = fixture.sample_accounts::<1>();

    let blokli_balance = fixture
        .client()
        .query_native_balance(account.alloy_address().as_ref())
        .await?;
    let rpc_balance = fixture.rpc().get_balance(account.address.as_ref()).await?;

    let parsed_blokli_balance =
        XDaiBalance::from_str(blokli_balance.balance.0.as_ref()).expect("failed to parse blokli balance");
    let parsed_rpc_balance =
        XDaiBalance::from_str(&format!("{rpc_balance} wei xDai")).expect("failed to parse rpc balance");

    assert_eq!(parsed_blokli_balance, parsed_rpc_balance);
    Ok(())
}

#[rstest]
#[test_log::test(tokio::test)]
#[serial]
/// query the token (wxHOPR) balance of an EOA via blokli and verify that the format is correct.
async fn query_token_balance_of_eoa(#[future(awt)] fixture: IntegrationFixture) -> Result<()> {
    let [account] = fixture.sample_accounts::<1>();

    let blokli_balance = fixture
        .client()
        .query_token_balance(account.alloy_address().as_ref())
        .await?;

    let _: HoprBalance = blokli_balance
        .balance
        .0
        .parse()
        .expect("failed to parse blokli token balance");

    Ok(())
}

#[rstest]
#[test_log::test(tokio::test)]
#[serial]
/// deploy a safe, query the token (wxHOPR) balance of the safe via blokli and verify that
/// format is correct.
async fn query_token_balance_of_safe(#[future(awt)] fixture: IntegrationFixture) -> Result<()> {
    let [account] = fixture.sample_accounts::<1>();

    fixture.deploy_safe(account, INITIAL_SAFE_BALANCE).await?;

    tokio::time::sleep(Duration::from_secs(8)).await; // dummy wait for the safe to be indexed

    let safe = fixture
        .client()
        .query_safe(SafeSelector::ChainKey(account.alloy_address().into()))
        .await?
        .expect("Safe not found");
    let safe_address = Address::from_str(&safe.address)?;

    let blokli_balance = fixture.client().query_token_balance(safe_address.as_ref()).await?;

    let _: HoprBalance = blokli_balance
        .balance
        .0
        .parse()
        .expect("failed to parse blokli token balance");

    Ok(())
}

#[rstest]
#[test_log::test(tokio::test)]
#[serial]
/// verifies that the transaction count of a given account increases by one after submitting a transaction.
async fn query_transaction_count(#[future(awt)] fixture: IntegrationFixture) -> Result<()> {
    let [sender, recipient] = fixture.sample_accounts::<2>();
    let tx_value = U256::from(1_000_000u64);
    let nonce = fixture.rpc().transaction_count(&sender.address).await?;

    let raw_tx = fixture.build_raw_tx(tx_value, sender, recipient, nonce).await?;

    let before_count = fixture
        .client()
        .query_transaction_count(sender.alloy_address().as_ref())
        .await?;

    fixture.rpc().execute_transaction(&raw_tx).await?;

    sleep(Duration::from_secs(8)).await;

    let after_count = fixture
        .client()
        .query_transaction_count(sender.alloy_address().as_ref())
        .await?;

    assert_eq!(after_count, before_count + 1);

    Ok(())
}

#[rstest]
#[test_log::test(tokio::test)]
#[serial]
/// deploys two safes from different EOAs, publish announcements using the safe modules, open a channel
/// between the EOAs, and verifies that the channel count increases by one
async fn count_and_query_channels(#[future(awt)] fixture: IntegrationFixture) -> Result<()> {
    let [src, dst] = fixture.sample_accounts::<2>();
    let expected_id = generate_channel_id(&src.hopr_address(), &dst.hopr_address());

    let channel_selector = ChannelSelector {
        filter: Some(ChannelFilter::ChannelId(expected_id.into())),
        status: None, /* fails otherwise as it's not allowed for now: "Channel status filtering during schema
                       * migration is not yet implemented" */
    };

    let before_count = fixture.client().count_channels(channel_selector.clone()).await?;
    let amount: HoprBalance = "1 wei wxHOPR".parse().expect("failed to parse amount");

    // Deploy safes for both parties
    debug!("deploying safes");
    let src_safe = fixture.deploy_safe_and_announce(&src, INITIAL_SAFE_BALANCE).await?;
    fixture.deploy_safe_and_announce(&dst, INITIAL_SAFE_BALANCE).await?;

    // Set allowance
    debug!("setting allowance");
    fixture.approve(&src, amount, &src_safe.module_address).await?;

    sleep(Duration::from_secs(8)).await;

    // Create the channel
    debug!("opening channel");
    fixture
        .open_channel(&src, &dst, amount, &src_safe.module_address)
        .await?;

    sleep(Duration::from_secs(8)).await;

    let after_count = fixture.client().count_channels(channel_selector.clone()).await?;

    assert_eq!(after_count, before_count + 1);

    debug!("close channel");
    fixture
        .initiate_outgoing_channel_closure(&src, &dst, &src_safe.module_address)
        .await?;
    sleep(Duration::from_secs(8)).await;

    let channels = fixture.client().query_channels(channel_selector).await?;

    // count channels that are in Open state
    let count = channels
        .iter()
        .filter(|channel| channel.status == ChannelStatus::Open)
        .count();
    assert_eq!(count, 0);

    Ok(())
}

#[rstest]
#[test_log::test(tokio::test)]
#[serial]
/// verifies that the chain ids provided by blokli and the RPC match.
async fn chain_ids_provided_by_blokli_matches_the_rpc(#[future(awt)] fixture: IntegrationFixture) -> Result<()> {
    let chain = fixture.client().query_chain_info().await?;
    let chain_id = fixture.rpc().chain_id().await?;

    assert_eq!(chain.chain_id as u64, chain_id);

    Ok(())
}

#[rstest]
#[test_log::test(tokio::test)]
#[serial]
/// verifies that the version string returned by blokli can be parsed as a semver.
async fn query_version_should_be_parsable(#[future(awt)] fixture: IntegrationFixture) -> Result<()> {
    let version = fixture.client().query_version().await?;
    let _parsed_version = semver::Version::parse(&version).expect("failed to parse version as semver");
    Ok(())
}

#[rstest]
#[test_log::test(tokio::test)]
#[serial]
/// verifies that the health status returned by blokli is "ok".
async fn query_health_should_be_ok(#[future(awt)] fixture: IntegrationFixture) -> Result<()> {
    assert_eq!(fixture.client().query_health().await?, "ok");
    Ok(())
}
