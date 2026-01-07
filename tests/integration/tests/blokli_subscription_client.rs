use anyhow::{Result, anyhow};
use blokli_client::api::{
    AccountSelector, BlokliSubscriptionClient, ChannelFilter, ChannelSelector, types::ChannelStatus,
};
use blokli_integration_tests::{
    constants::{EPSILON, parsed_safe_balance, subscription_timeout},
    fixtures::{IntegrationFixture, integration_fixture as fixture},
};
use futures::stream::StreamExt;
use futures_time::future::FutureExt as FutureTimeoutExt;
use hopr_crypto_types::types::Hash;
use hopr_internal_types::channels::generate_channel_id;
use hopr_primitive_types::traits::ToHex;
use rstest::*;
use serial_test::serial;
use tracing::debug;

#[rstest]
#[test_log::test(tokio::test)]
#[serial]
/// subscribes to channel updates, deploys two safes from different EOAs, publishes announcements
/// using the safe modules, open a channel between the EOAs, and verifies that the channel update
/// is received through the subscription.
async fn subscribe_channels(#[future(awt)] fixture: IntegrationFixture) -> Result<()> {
    let [src, dst] = fixture.sample_accounts::<2>();
    let expected_id = generate_channel_id(&src.address, &dst.address);
    let channel_selector = ChannelSelector {
        filter: Some(ChannelFilter::ChannelId(expected_id.into())),
        status: Some(ChannelStatus::Open),
    };
    let amount = "1 wei wxHOPR".parse().expect("failed to parse amount");
    let expected_channel_id = Hash::from(expected_id).to_hex();
    let client = fixture.client().clone();

    let src_safe = fixture.deploy_safe_and_announce(&src, parsed_safe_balance()).await?;
    fixture.deploy_safe_and_announce(&dst, parsed_safe_balance()).await?;

    debug!("setting allowance");
    fixture.approve(&src, amount, &src_safe.module_address).await?;

    let handle = tokio::task::spawn(async move {
        client
            .subscribe_channels(channel_selector)
            .expect("failed to create safe deployments subscription")
            .skip_while(|entry| {
                let retrieved_channel = entry
                    .as_ref()
                    .expect("failed to get subscription update")
                    .concrete_channel_id
                    .to_lowercase();

                let should_skip = !expected_channel_id.to_lowercase().contains(&retrieved_channel);
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

    assert_eq!(channel.balance.0, amount.to_string());

    Ok(())
}

#[rstest]
#[test_log::test(tokio::test)]
#[serial]
/// subscribes to account updates by private key, deploys a safe from the account, publishes an
/// announcement using the safe module, and verifies that the account update is received through
/// the subscription.
async fn subscribe_account_by_private_key(#[future(awt)] fixture: IntegrationFixture) -> Result<()> {
    let [account] = fixture.sample_accounts::<1>();
    let account_address = account.to_string_address();

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
                    != account_address;
                futures::future::ready(should_skip)
            })
            .next()
            .timeout(subscription_timeout())
            .await
    });

    fixture.deploy_safe_and_announce(account, parsed_safe_balance()).await?;

    assert_eq!(
        handle
            .await??
            .ok_or_else(|| anyhow!("no update received from subscription"))??
            .chain_key
            .to_lowercase(),
        account.to_string_address()
    );

    Ok(())
}

#[rstest]
#[test_log::test(tokio::test)]
#[serial]
/// subscribes to graph updates, deploys two safes from different EOAs, publishes announcements
/// using the safe modules, opens a channel between the EOAs, and verifies that the channel update
/// and the source/destination are received through the subscription
async fn subscribe_graph(#[future(awt)] fixture: IntegrationFixture) -> Result<()> {
    let [src, dst] = fixture.sample_accounts::<2>();
    let expected_id = generate_channel_id(&src.address, &dst.address);

    let random_amount: u8 = rand::random::<u8>() % 100 + 1;
    let amount = format!("{random_amount} wei wxHOPR")
        .parse()
        .expect("failed to parse amount");
    let client = fixture.client().clone();

    let src_safe = fixture.deploy_safe_and_announce(&src, parsed_safe_balance()).await?;
    fixture.deploy_safe_and_announce(&dst, parsed_safe_balance()).await?;

    debug!("setting allowance");
    fixture.approve(&src, amount, &src_safe.module_address).await?;

    let handle = tokio::task::spawn(async move {
        client
            .subscribe_graph()
            .expect("failed to create safe deployments subscription")
            .skip_while(|entry| {
                let retrieved_channel = entry
                    .as_ref()
                    .expect("failed to get subscription update")
                    .channel
                    .concrete_channel_id
                    .to_lowercase();

                let should_skip = !expected_id.to_hex().to_lowercase().contains(&retrieved_channel);
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

    let graph_entry = handle
        .await??
        .ok_or_else(|| anyhow!("no update received from subscription"))??;

    assert_eq!(graph_entry.channel.balance.0, amount.to_string());
    assert_eq!(graph_entry.source.chain_key, src.to_string_address());
    assert_eq!(graph_entry.destination.chain_key, dst.to_string_address());
    assert_eq!(graph_entry.channel.status, ChannelStatus::Open);

    Ok(())
}

#[rstest]
#[test_log::test(tokio::test)]
#[serial]
/// subscribes to ticket parameter updates, updates the winning probability, and verifies that
/// the update is received through the subscription.
async fn subscribe_ticket_params(#[future(awt)] fixture: IntegrationFixture) -> Result<()> {
    let account = fixture.accounts().first().expect("no accounts in fixture");
    let client = fixture.client().clone();

    let new_win_prob = 0.00005f64;

    let mut stream = client
        .subscribe_ticket_params()
        .expect("failed to create ticket parameters subscription");

    fixture
        .update_winn_prob(
            account,
            fixture.contract_addresses().winning_probability_oracle,
            new_win_prob,
        )
        .await?;

    let output = stream
        .next()
        .timeout(subscription_timeout())
        .await?
        .ok_or_else(|| anyhow!("no update received from subscription"))??;

    assert!((output.min_ticket_winning_probability - new_win_prob).abs() < EPSILON);

    fixture
        .update_winn_prob(account, fixture.contract_addresses().winning_probability_oracle, 1.0)
        .await?;

    Ok(())
}

#[rstest]
#[test_log::test(tokio::test)]
#[serial]
/// subscribes to safe deployments, deploys a safe from an account, and verifies that the
/// deployment is received through the subscription.
/// Also verifies that re-registering the same safe fails.
async fn subscribe_safe_deployments(#[future(awt)] fixture: IntegrationFixture) -> Result<()> {
    let [account] = fixture.sample_accounts::<1>();
    let account_address = account.to_string_address();
    let client = fixture.client().clone();

    debug!("subscribing to safe deployments");
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
                    != account_address;
                futures::future::ready(should_skip)
            })
            .next()
            .timeout(subscription_timeout())
            .await
    });

    fixture.deploy_or_get_safe(account, parsed_safe_balance()).await?;

    let safe = handle
        .await??
        .ok_or_else(|| anyhow!("no update received from subscription"))??;

    // re-registering the same safe should fail
    assert!(fixture.register_safe(account, &safe.address).await.is_err());

    Ok(())
}
