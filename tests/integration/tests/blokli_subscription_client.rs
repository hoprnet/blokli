use anyhow::{Result, anyhow};
use blokli_client::api::{
    AccountSelector, BlokliSubscriptionClient, ChannelFilter, ChannelSelector, types::ChannelStatus,
};
use blokli_integration_tests::fixtures::{IntegrationFixture, integration_fixture as fixture};
use futures::stream::StreamExt;
use futures_time::{future::FutureExt as FutureTimeoutExt, time::Duration};
use hopr_crypto_types::types::Hash;
use hopr_internal_types::channels::generate_channel_id;
use hopr_primitive_types::{prelude::HoprBalance, traits::ToHex};
use rstest::*;
use serial_test::serial;
use tracing::{debug, info};

const SUBSCRIPTION_TIMEOUT_SECS: u64 = 60;

fn subscription_timeout() -> Duration {
    Duration::from_secs(SUBSCRIPTION_TIMEOUT_SECS)
}

#[rstest]
#[test_log::test(tokio::test)]
#[serial]
async fn subscribe_channels(#[future(awt)] fixture: IntegrationFixture) -> Result<()> {
    // FIXME: tx reverts
    let [src, dst] = fixture.sample_accounts::<2>();
    let expected_id = generate_channel_id(&src.hopr_address(), &dst.hopr_address());
    let channel_selector = ChannelSelector {
        filter: Some(ChannelFilter::ChannelId(expected_id.into())),
        status: Some(ChannelStatus::Open),
    };
    let amount = "1 wei wxHOPR".parse().expect("failed to parse amount");
    let expected_channel_id = Hash::from(expected_id).to_hex();
    let client = fixture.client().clone();

    info!(src = %src.hopr_address(), dst = %dst.hopr_address(), "opening channel");

    let src_safe = fixture.deploy_safe_and_announce(&src, 500_000_000_000_000_000).await?;
    fixture.deploy_safe_and_announce(&dst, 500_000_000_000_000_000).await?;

    // Create the channel
    debug!("setting allowance");
    fixture.approve(&src, amount, &src_safe.module_address).await?;

    let handle = tokio::task::spawn(async move {
        client
            .subscribe_channels(channel_selector)
            .expect("failed to create safe deployments subscription")
            .skip_while(|entry| {
                let should_skip = entry
                    .as_ref()
                    .expect("failed to get subscription update")
                    .concrete_channel_id
                    .to_lowercase()
                    != expected_channel_id.to_lowercase();
                futures::future::ready(should_skip)
            })
            .next()
            .timeout(subscription_timeout())
            .await
    });

    debug!("opening channel");
    fixture
        .open_channel(&src, &dst, amount, &src_safe.module_address)
        .await?;

    let channel = handle
        .await??
        .ok_or_else(|| anyhow!("no update received from subscription"))??;

    info!(?channel, "received channel update");

    assert_eq!(channel.balance.0, amount.to_string());

    Ok(())
}

#[rstest]
#[test_log::test(tokio::test)]
#[serial]
async fn subscribe_account_by_private_key(#[future(awt)] fixture: IntegrationFixture) -> Result<()> {
    let [account] = fixture.sample_accounts::<1>();
    let account_address = account.address.clone();

    let selector = AccountSelector::Address(*account.alloy_address().as_ref());
    let client = fixture.client().clone();
    let handle = tokio::task::spawn(async move {
        client
            .subscribe_accounts(selector)
            .expect("failed to create account subscription")
            .skip_while(|entry| {
                let should_skip = entry
                    .as_ref()
                    .expect("failed to get subscription update")
                    .chain_key
                    .to_lowercase()
                    != account_address.to_lowercase();
                futures::future::ready(should_skip)
            })
            .next()
            .timeout(subscription_timeout())
            .await
    });

    fixture
        .deploy_safe_and_announce(account, 500_000_000_000_000_000)
        .await?;

    assert_eq!(
        handle
            .await??
            .ok_or_else(|| anyhow!("no update received from subscription"))??
            .chain_key
            .to_lowercase(),
        account.address.to_lowercase()
    );

    Ok(())
}

#[rstest]
#[test_log::test(tokio::test)]
#[serial]
async fn subscribe_graph(#[future(awt)] fixture: IntegrationFixture) -> Result<()> {
    // FIXME: tx reverts
    let [src, dst] = fixture.sample_accounts::<2>();
    let client = fixture.client().clone();
    let expected_id = generate_channel_id(&src.hopr_address(), &dst.hopr_address());

    let amount: HoprBalance = "1 wei wxHOPR".parse().expect("failed to parse amount");
    let expected_channel_id = Hash::from(expected_id).to_hex();

    let handle = tokio::task::spawn(async move {
        client
            .subscribe_graph()
            .expect("failed to create ticket parameters subscription")
            .skip_while(|entry| {
                let should_skip = entry
                    .as_ref()
                    .expect("failed to get subscription update")
                    .channel
                    .concrete_channel_id
                    != expected_channel_id;
                futures::future::ready(should_skip)
            })
            .next()
            .timeout(subscription_timeout())
            .await
    });

    // Deploy safes for both parties
    let src_safe = fixture.deploy_safe_and_announce(&src, 1_000).await?;
    fixture.deploy_safe_and_announce(&dst, 1_000).await?;

    // Create the channel
    fixture.approve(&src, amount, &src_safe.module_address).await?;

    fixture
        .open_channel(&src, &dst, amount, &src_safe.module_address)
        .await?;

    handle
        .await??
        .ok_or_else(|| anyhow!("no update received from subscription"))??;

    Ok(())
}

#[rstest]
#[test_log::test(tokio::test)]
#[serial]
async fn subscribe_ticket_params(#[future(awt)] fixture: IntegrationFixture) -> Result<()> {
    // FIXME: test timeouts
    let account = fixture.accounts().first().expect("no accounts in fixture");
    let client = fixture.client().clone();

    let new_win_prob = 0.00005f64;

    let handle = tokio::task::spawn(async move {
        client
            .subscribe_ticket_params()
            .expect("failed to create ticket parameters subscription")
            .skip_while(|entry| {
                let should_skip = (entry
                    .as_ref()
                    .expect("failed to get subscription update")
                    .min_ticket_winning_probability
                    - new_win_prob)
                    .abs()
                    > f64::EPSILON;
                futures::future::ready(should_skip)
            })
            .next()
            .timeout(subscription_timeout())
            .await
    });

    fixture
        .update_winn_prob(
            account,
            fixture.contract_addresses().winning_probability_oracle,
            new_win_prob,
        )
        .await?;

    let output = handle
        .await??
        .ok_or_else(|| anyhow!("no update received from subscription"))??;

    fixture
        .update_winn_prob(account, fixture.contract_addresses().winning_probability_oracle, 1.0)
        .await?;

    assert_eq!(output.min_ticket_winning_probability, new_win_prob);

    Ok(())
}

#[rstest]
#[test_log::test(tokio::test)]
#[serial]
async fn subscribe_safe_deployments(#[future(awt)] fixture: IntegrationFixture) -> Result<()> {
    let [account] = fixture.sample_accounts::<1>();
    let account_address = account.address.clone();
    let client = fixture.client().clone();

    info!("subscribing to safe deployments");
    let handle = tokio::task::spawn(async move {
        client
            .subscribe_safe_deployments()
            .expect("failed to create safe deployments subscription")
            .skip_while(|entry| {
                let should_skip = entry
                    .as_ref()
                    .expect("failed to get subscription update")
                    .chain_key
                    .to_lowercase()
                    != account_address.to_lowercase();
                futures::future::ready(should_skip)
            })
            .next()
            .timeout(subscription_timeout())
            .await
    });

    fixture.deploy_safe(account, 1_000).await?;

    let safe = handle
        .await??
        .ok_or_else(|| anyhow!("no update received from subscription"))??;

    assert_eq!(safe.chain_key.to_lowercase(), account.address.to_lowercase());

    Ok(())
}
