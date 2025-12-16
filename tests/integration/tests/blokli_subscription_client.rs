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
use rand::Rng;
use rstest::*;
use serial_test::serial;
use tracing::info;

const SUBSCRIPTION_TIMEOUT_SECS: u64 = 60;

fn subscription_timeout() -> Duration {
    Duration::from_secs(SUBSCRIPTION_TIMEOUT_SECS)
}

#[rstest]
#[test_log::test(tokio::test)]
#[serial]
async fn subscribe_channels(#[future(awt)] fixture: IntegrationFixture) -> Result<()> {
    let [src, dst] = fixture.sample_accounts::<2>();
    let expected_id = generate_channel_id(&src.hopr_address(), &dst.hopr_address());
    let channel_selector = ChannelSelector {
        filter: Some(ChannelFilter::ChannelId(expected_id.into())),
        status: Some(ChannelStatus::Open),
    };
    let expected_channel_id = Hash::from(expected_id).to_hex();

    let subscription = fixture
        .client()
        .subscribe_channels(channel_selector)
        .expect("failed to create channel subscription");

    let subscription = subscription.fuse();
    let amount = "1 wei wxHOPR".parse().expect("failed to parse amount");

    info!("Opening channel between {} and {}", src.address, dst.address);

    fixture.open_channel(&src, &dst, amount).await?;

    subscription
        .skip_while(|entry| {
            let should_skip = entry
                .as_ref()
                .expect("failed to get subscription update")
                .concrete_channel_id
                != expected_channel_id;
            futures::future::ready(should_skip)
        })
        .next()
        .timeout(subscription_timeout())
        .await
        .map_err(|_| anyhow!("subscription update timed out"))?
        .ok_or_else(|| anyhow!("no update received from subscription"))??;

    Ok(())
}

#[rstest]
#[test_log::test(tokio::test)]
#[serial]
async fn subscribe_account_by_private_key(#[future(awt)] fixture: IntegrationFixture) -> Result<()> {
    let [input] = fixture.sample_accounts::<1>();

    let selector = AccountSelector::Address(*input.alloy_address().as_ref());
    let subscription = fixture
        .client()
        .subscribe_accounts(selector)
        .expect("failed to create account subscription")
        .fuse();

    let output = subscription
        .skip_while(|entry| {
            let should_skip = entry.as_ref().expect("failed to get subscription update").chain_key != input.address;
            futures::future::ready(should_skip)
        })
        .next()
        .timeout(subscription_timeout())
        .await
        .map_err(|_| anyhow!("subscription update timed out"))?
        .ok_or_else(|| anyhow!("no update received from subscription"))??;

    assert_eq!(output.chain_key, input.address);

    Ok(())
}

#[rstest]
#[test_log::test(tokio::test)]
#[serial]
async fn subscribe_graph(#[future(awt)] fixture: IntegrationFixture) -> Result<()> {
    let [src, dst] = fixture.sample_accounts::<2>();
    let expected_id = generate_channel_id(&src.hopr_address(), &dst.hopr_address());
    let subscription = fixture
        .client()
        .subscribe_graph()
        .expect("failed to create graph subscription")
        .fuse();

    let amount = "1 wei wxHOPR".parse().expect("failed to parse amount");
    let expected_channel_id = Hash::from(expected_id).to_hex();

    fixture.open_channel(&src, &dst, amount).await?;

    subscription
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
        .map_err(|_| anyhow!("subscription update timed out"))?
        .ok_or_else(|| anyhow!("no update received from subscription"))??;

    Ok(())
}

#[rstest]
#[test_log::test(tokio::test)]
#[serial]
#[ignore = "not ready"]
async fn subscribe_ticket_params(#[future(awt)] fixture: IntegrationFixture) -> Result<()> {
    let mut rng = rand::rng();
    let new_win_prob = 0.0001f64;
    let new_ticket_value: HoprBalance = format!("{} wxHOPR", rng.random::<f64>())
        .parse()
        .expect("failed to parse amount");

    let subscription = fixture
        .client()
        .subscribe_ticket_params()
        .expect("failed to create ticket params subscription");
    let subscription = subscription.fuse();

    // TODO: update the ticket price and win prob through anvil / hopli

    let output = subscription
        .skip_while(|entry| {
            let should_skip = entry
                .as_ref()
                .expect("failed to get subscription update")
                .min_ticket_winning_probability
                != new_win_prob;
            futures::future::ready(should_skip)
        })
        .next()
        .timeout(subscription_timeout())
        .await
        .map_err(|_| anyhow!("subscription update timed out"))?
        .ok_or_else(|| anyhow!("no update received from subscription"))??;

    // TODO: set the ticket price and win prob back to original values

    let decoded_ticket_value: HoprBalance = output
        .ticket_price
        .0
        .parse()
        .map_err(|_| anyhow!("failed to parse ticket value from subscription"))?;

    assert_eq!(output.min_ticket_winning_probability, new_win_prob);
    assert_eq!(decoded_ticket_value, new_ticket_value);

    Ok(())
}

#[rstest]
#[test_log::test(tokio::test)]
#[serial]
async fn subscribe_safe_deployments(#[future(awt)] fixture: IntegrationFixture) -> Result<()> {
    let [account] = fixture.sample_accounts::<1>();
    let mut subscription = fixture
        .client()
        .subscribe_safe_deployments()
        .expect("failed to create safe deployments subscription")
        .fuse();

    fixture.deploy_safe(account, 1_000).await?;

    subscription
        .next()
        .timeout(subscription_timeout())
        .await
        .map_err(|_| anyhow!("subscription update timed out"))?
        .ok_or_else(|| anyhow!("no update received from subscription"))??;

    Ok(())
}
