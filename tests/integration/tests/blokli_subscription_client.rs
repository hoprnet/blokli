use std::time::Duration;

use anyhow::{Result, anyhow};
use blokli_client::api::{
    AccountSelector, BlokliSubscriptionClient, ChannelFilter, ChannelSelector, types::ChannelStatus,
};
use blokli_integration_tests::fixtures::{IntegrationFixture, integration_fixture as fixture};
use futures::stream::StreamExt;
use hopr_crypto_types::types::Hash;
use hopr_internal_types::channels::generate_channel_id;
use hopr_primitive_types::traits::ToHex;
use rstest::*;
use serial_test::serial;
use tokio::time::timeout;

const SUBSCRIPTION_TIMEOUT: Duration = Duration::from_secs(5);

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

    let subscription = fixture.client().subscribe_channels(channel_selector)?;

    // the test should first subsribe, then open the channel, then receive the update
    let mut subscription = subscription.fuse();
    let amount = "1 wxHOPR".parse().expect("failed to parse amount");

    fixture.open_channel(&src, &dst, amount).await?;

    // wait for the subscription to update, until one of the channels returned matches the expected id
    timeout(SUBSCRIPTION_TIMEOUT, subscription.next())
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
    let mut subscription = fixture.client().subscribe_accounts(selector)?;

    let output = timeout(SUBSCRIPTION_TIMEOUT, subscription.next())
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
    let subscription = fixture.client().subscribe_graph()?;

    // the test should first subsribe, then open the channel, then receive the update
    let mut subscription = subscription.fuse();
    let amount = "1 wxHOPR".parse().expect("failed to parse amount");

    fixture.open_channel(&src, &dst, amount).await?;

    // wait for the subscription to update, until one of the channels returned matches the expected id
    timeout(SUBSCRIPTION_TIMEOUT, async {
        loop {
            match subscription.next().await {
                Some(update) => {
                    let entry = update?;
                    if entry.channel.concrete_channel_id == Hash::from(expected_id).to_hex() {
                        break Ok(());
                    }
                }
                None => break Err(anyhow!("subscription closed without updates")),
            }
        }
    })
    .await
    .map_err(|_| anyhow!("subscription update timed out"))??;

    Ok(())
}

#[rstest]
#[test_log::test(tokio::test)]
#[serial]
#[ignore = "not implemented"]
async fn subscribe_ticket_params(#[future(awt)] fixture: IntegrationFixture) -> Result<()> {
    Ok(())
}

#[rstest]
#[test_log::test(tokio::test)]
#[serial]
async fn subscribe_safe_deployments(#[future(awt)] fixture: IntegrationFixture) -> Result<()> {
    let [account] = fixture.sample_accounts::<1>();
    let mut subscription = fixture.client().subscribe_safe_deployments()?;

    fixture.deploy_safe(account, 1_000).await?;

    let safe = timeout(SUBSCRIPTION_TIMEOUT, async {
        loop {
            match subscription.next().await {
                Some(update) => {
                    let safe = update?;
                    if safe.chain_key == account.address {
                        break Ok(safe);
                    }
                }
                None => break Err(anyhow!("subscription closed without updates")),
            }
        }
    })
    .await
    .map_err(|_| anyhow!("subscription update timed out"))??;

    assert_eq!(safe.chain_key, account.address);
    assert!(safe.address.starts_with("0x"));
    assert!(safe.module_address.starts_with("0x"));

    Ok(())
}
