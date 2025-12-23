use anyhow::{Result, anyhow};
use blokli_client::api::{
    AccountSelector, BlokliSubscriptionClient, ChannelFilter, ChannelSelector, types::ChannelStatus,
};
use blokli_integration_tests::fixtures::{IntegrationFixture, integration_fixture as fixture};
use futures::stream::StreamExt;
use futures_time::{future::FutureExt as FutureTimeoutExt, time::Duration};
use hopr_crypto_types::types::Hash;
use hopr_internal_types::channels::generate_channel_id;
use hopr_primitive_types::traits::ToHex;
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
    let amount = "1 wei wxHOPR".parse().expect("failed to parse amount");
    let expected_channel_id = Hash::from(expected_id).to_hex();
    let client = fixture.client().clone();

    let handle = tokio::task::spawn(async move {
        client
            .subscribe_channels(channel_selector)
            .expect("failed to create safe deployments subscription")
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
    });

    fixture.open_channel(&src, &dst, amount).await?;

    handle
        .await??
        .ok_or_else(|| anyhow!("no update received from subscription"))??;

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
                let should_skip =
                    entry.as_ref().expect("failed to get subscription update").chain_key != account_address;
                futures::future::ready(should_skip)
            })
            .next()
            .timeout(subscription_timeout())
            .await
    });

    fixture.announce_account(account).await?;

    assert_eq!(
        handle
            .await??
            .ok_or_else(|| anyhow!("no update received from subscription"))??
            .chain_key,
        account.address
    );

    Ok(())
}

#[rstest]
#[test_log::test(tokio::test)]
#[serial]
async fn subscribe_graph(#[future(awt)] fixture: IntegrationFixture) -> Result<()> {
    let [src, dst] = fixture.sample_accounts::<2>();
    let client = fixture.client().clone();
    let expected_id = generate_channel_id(&src.hopr_address(), &dst.hopr_address());

    let amount = "1 wei wxHOPR".parse().expect("failed to parse amount");
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

    fixture.open_channel(&src, &dst, amount).await?;

    handle
        .await??
        .ok_or_else(|| anyhow!("no update received from subscription"))??;

    Ok(())
}

#[rstest]
#[test_log::test(tokio::test)]
#[serial]
async fn subscribe_ticket_params(#[future(awt)] fixture: IntegrationFixture) -> Result<()> {
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
            // .skip_while(|entry| {
            //     let should_skip = entry
            //         .as_ref()
            //         .expect("failed to get subscription update")
            //         .chain_key != account_address;
            //     futures::future::ready(should_skip)
            // })
            .next()
            .timeout(subscription_timeout())
            .await
    });

    info!(owner=account_address, "deploying safe");
    fixture.deploy_safe(account, 1_000).await?;

    handle
        .await??
        .ok_or_else(|| anyhow!("no update received from subscription"))??;

    Ok(())
}
