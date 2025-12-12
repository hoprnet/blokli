use std::str::FromStr;

use anyhow::Result;
use blokli_client::api::{
    AccountSelector, BlokliQueryClient, ChannelFilter, ChannelSelector, SafeSelector, types::ChannelStatus,
};
use blokli_integration_tests::fixtures::{IntegrationFixture, integration_fixture as fixture};
use hopr_internal_types::channels::generate_channel_id;
use hopr_primitive_types::prelude::XDaiBalance;
use rstest::*;
use serial_test::serial;
use tracing::info;

#[rstest]
#[test_log::test(tokio::test)]
#[serial]
async fn count_accounts_matches_deployed_accounts(#[future(awt)] fixture: IntegrationFixture) -> Result<()> {
    let [account] = fixture.sample_accounts::<1>();

    let account_count = fixture
        .client()
        .count_accounts(AccountSelector::Address(account.hopr_address().into()))
        .await?;
    assert_eq!(account_count, 0);

    fixture.announce_account(account).await?;

    let account_count = fixture
        .client()
        .count_accounts(AccountSelector::Address(account.hopr_address().into()))
        .await?;
    assert_eq!(account_count, 1);

    Ok(())
}

#[rstest]
#[test_log::test(tokio::test)]
#[serial]
async fn query_accounts(#[future(awt)] fixture: IntegrationFixture) -> Result<()> {
    let [account] = fixture.sample_accounts::<1>();
    fixture.announce_account(account).await?;

    let found_accounts = fixture
        .client()
        .query_accounts(AccountSelector::Address(account.hopr_address().into()))
        .await?;

    assert_eq!(found_accounts.len(), 1);
    assert_eq!(found_accounts[0].chain_key, account.address);

    Ok(())
}

#[rstest]
#[test_log::test(tokio::test)]
#[serial]
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
async fn query_token_balance(#[future(awt)] fixture: IntegrationFixture) -> Result<()> {
    let [account] = fixture.sample_accounts::<1>();
    fixture.deploy_safe(account, 1_000).await?;

    let blokli_balance = fixture
        .client()
        .query_token_balance(account.alloy_address().as_ref())
        .await?;
    info!("Blokli token balance: {:?}", blokli_balance);

    Ok(())
}

#[rstest]
#[test_log::test(tokio::test)]
#[serial]
async fn query_transaction_count(#[future(awt)] fixture: IntegrationFixture) -> Result<()> {
    let [account] = fixture.sample_accounts::<1>();
    let before_count = fixture
        .client()
        .query_transaction_count(account.alloy_address().as_ref())
        .await?;

    fixture.announce_account(account).await?;

    let after_count = fixture
        .client()
        .query_transaction_count(account.alloy_address().as_ref())
        .await?;

    assert_eq!(after_count, before_count + 1);

    Ok(())
}

#[rstest]
#[test_log::test(tokio::test)]
#[serial]
async fn query_safe_allowance_should_be_returned_after_deployment(
    #[future(awt)] fixture: IntegrationFixture,
) -> Result<()> {
    let [account] = fixture.sample_accounts::<1>();
    fixture.deploy_safe(account, 1_000).await?;

    let safe_selector = SafeSelector::ChainKey(*account.alloy_address().as_ref());
    fixture
        .client()
        .query_safe(safe_selector)
        .await?
        .expect("Safe not found");

    Ok(())
}

#[rstest]
#[test_log::test(tokio::test)]
#[serial]
async fn count_and_query_channels(#[future(awt)] fixture: IntegrationFixture) -> Result<()> {
    let channel_selector = ChannelSelector {
        filter: None,
        status: Some(ChannelStatus::Open),
    };

    let before_count = fixture.client().count_channels(channel_selector.clone()).await?;

    let [src, dst] = fixture.sample_accounts::<2>();
    let amount = "1 wxHOPR".parse().expect("failed to parse amount");
    fixture.open_channel(&src, &dst, amount).await?;

    let after_count = fixture.client().count_channels(channel_selector.clone()).await?;

    assert_eq!(after_count, before_count + 1);

    let expected_id = generate_channel_id(&src.hopr_address(), &dst.hopr_address());
    let channel_selector = ChannelSelector {
        filter: Some(ChannelFilter::ChannelId(expected_id.into())),
        status: Some(ChannelStatus::Open),
    };
    let channels = fixture.client().query_channels(channel_selector).await?;

    assert_eq!(channels.len(), 1);

    Ok(())
}

#[rstest]
#[test_log::test(tokio::test)]
#[serial]
async fn chain_ids_provided_by_blokli_matches_the_rpc(#[future(awt)] fixture: IntegrationFixture) -> Result<()> {
    let chain = fixture.client().query_chain_info().await?;
    let chain_id = fixture.rpc().chain_id().await?;

    assert_eq!(chain.chain_id as u64, chain_id);

    Ok(())
}

#[rstest]
#[test_log::test(tokio::test)]
#[serial]
async fn query_version_should_be_parsable(#[future(awt)] fixture: IntegrationFixture) -> Result<()> {
    let version = fixture.client().query_version().await?;
    let _parsed_version = semver::Version::parse(&version).expect("failed to parse version as semver");
    Ok(())
}

#[rstest]
#[test_log::test(tokio::test)]
#[serial]
async fn query_health_should_be_ok(#[future(awt)] fixture: IntegrationFixture) -> Result<()> {
    assert_eq!(fixture.client().query_health().await?, "ok");
    Ok(())
}
