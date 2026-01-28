use anyhow::{Result, anyhow};
use blokli_client::api::{
    AccountSelector, BlokliQueryClient, BlokliSubscriptionClient, ChannelFilter, ChannelSelector, SafeSelector,
    types::ChannelStatus,
};
use blokli_integration_tests::{
    constants::{EPSILON, parsed_safe_balance, subscription_timeout},
    fixtures::{IntegrationFixture, integration_fixture as fixture},
};
use eventsource_client::{Client, ClientBuilder, SSE};
use futures::stream::StreamExt;
use futures_time::future::FutureExt as FutureTimeoutExt;
use hex::ToHex;
use hopr_crypto_types::{keypairs::Keypair, types::Hash};
use hopr_internal_types::channels::generate_channel_id;
use rstest::*;
use serde_json::json;
use serial_test::serial;
use uuid::Uuid;

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
    let expected_channel_id = Hash::from(expected_id).encode_hex::<String>();
    let client = fixture.client().clone();

    let src_safe = fixture.deploy_safe_and_announce(&src, parsed_safe_balance()).await?;
    fixture.deploy_safe_and_announce(&dst, parsed_safe_balance()).await?;

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

                let should_skip = !(expected_channel_id.to_lowercase() == retrieved_channel);
                futures::future::ready(should_skip)
            })
            .next()
            .timeout(subscription_timeout())
            .await
    });

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
    let account_address = account.address.to_string();

    let selector = AccountSelector::Address(*account.to_alloy_address().as_ref());
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

    let deployed_safe = fixture.deploy_safe_and_announce(account, parsed_safe_balance()).await?;

    let retrieved_account = handle
        .await??
        .ok_or_else(|| anyhow!("no update received from subscription"))??;

    // The retrieved account must have a matching address
    assert_eq!(retrieved_account.chain_key.to_lowercase(), account.address.to_string());

    // The retrieved account must have an offchain key
    assert_eq!(
        retrieved_account.packet_key.to_lowercase(),
        account.offchain_keypair().public().encode_hex::<String>()
    );

    // The retrieved account must have a matching Safe address (due to registration)
    assert_eq!(
        retrieved_account.safe_address.map(|a| a.to_lowercase()),
        Some(deployed_safe.address.to_lowercase())
    );

    // Deployed safe must have a matching owner address
    assert_eq!(deployed_safe.chain_key.to_lowercase(), account.address.to_string());

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

                let should_skip = !(expected_id.encode_hex::<String>() == retrieved_channel);
                futures::future::ready(should_skip)
            })
            .next()
            .timeout(subscription_timeout())
            .await
    });

    fixture
        .open_channel(&src, &dst, amount, &src_safe.module_address)
        .await?;

    let graph_entry = handle
        .await??
        .ok_or_else(|| anyhow!("no update received from subscription"))??;

    assert_eq!(graph_entry.channel.balance.0, amount.to_string());
    assert_eq!(graph_entry.source.chain_key, src.address.to_string());
    assert_eq!(graph_entry.destination.chain_key, dst.address.to_string());
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
        .update_winning_probability(
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
        .update_winning_probability(account, fixture.contract_addresses().winning_probability_oracle, 1.0)
        .await?;

    Ok(())
}

#[rstest]
#[test_log::test(tokio::test)]
#[serial]
/// subscribes to safe deployments, deploys a safe from an account that does not have a safe yet, and verifies that the
/// deployment is received through the subscription.
/// Also verifies that re-registering the same safe fails.
async fn subscribe_safe_deployments(#[future(awt)] fixture: IntegrationFixture) -> Result<()> {
    let account = loop {
        let [maybe_account] = fixture.sample_accounts::<1>();
        let safe = fixture
            .client()
            .query_safe(SafeSelector::ChainKey(maybe_account.to_alloy_address().into()))
            .await?;
        if safe.is_none() {
            break maybe_account;
        }
    };

    let account_address = account.address.to_string();
    let client = fixture.client().clone();

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

#[rstest]
#[test_log::test(tokio::test)]
#[serial]
async fn subscribe_keepalive_comments(#[future(awt)] fixture: IntegrationFixture) -> Result<()> {
    // Open an SSE subscription without emitting events.
    let query = json!({
        "query": format!(
            "subscription {{ transactionUpdated(id: \"{}\") {{ id status }} }}",
            Uuid::new_v4()
        )
    });
    let request_body = serde_json::to_string(&query).expect("Failed to serialize request");
    let url = fixture
        .config()
        .bloklid_url
        .join("graphql")
        .map_err(|e| anyhow!("failed to build GraphQL URL: {e}"))?;

    let client = ClientBuilder::for_url(url.as_str())
        .map_err(|e| anyhow!("failed to build SSE client: {e}"))?
        .connect_timeout(fixture.config().http_timeout)
        .header("Accept", "text/event-stream")
        .map_err(|e| anyhow!("failed to set Accept header: {e}"))?
        .header("Content-Type", "application/json")
        .map_err(|e| anyhow!("failed to set Content-Type header: {e}"))?
        .method("POST".into())
        .body(request_body)
        .build();

    let stream = client.stream();
    // Validate that the server emits keepalive comments on idle streams.
    let comment = stream
        .filter_map(|item| {
            futures::future::ready(match item {
                Ok(SSE::Comment(comment)) => Some(Ok(comment)),
                Ok(_) => None,
                Err(err) => Some(Err(anyhow!("SSE error: {err}"))),
            })
        })
        .next()
        .timeout(subscription_timeout())
        .await
        .map_err(|_| anyhow!("timed out waiting for keepalive comment"))?
        .ok_or_else(|| anyhow!("SSE stream closed before keepalive"))??;

    assert!(
        comment.contains("keep-alive"),
        "unexpected keepalive comment: {comment}"
    );

    Ok(())
}
