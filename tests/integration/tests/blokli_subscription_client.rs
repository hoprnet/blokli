use std::time::Duration;

use anyhow::{Result, anyhow};
use blokli_client::api::{
    AccountSelector, BlokliQueryClient, BlokliSubscriptionClient, ChannelFilter, ChannelSelector, SafeSelector,
    types::{ChannelStatus, TransactionStatus},
};
use blokli_integration_tests::{
    constants::{EPSILON, parsed_safe_balance, subscription_timeout},
    fixtures::{IntegrationFixture, integration_fixture as fixture},
};
use eventsource_client::{Client, ClientBuilder, SSE};
use futures::stream::StreamExt;
use futures_time::future::FutureExt as FutureTimeoutExt;
use hex::{FromHex, ToHex};
use hopr_bindings::exports::alloy::primitives::U256;
use hopr_crypto_types::{keypairs::Keypair, types::Hash};
use hopr_internal_types::channels::generate_channel_id;
use hopr_primitive_types::prelude::HoprBalance;
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
        .open_channel(&src, &dst, amount, &src_safe.module_address, None)
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
        .open_channel(&src, &dst, amount, &src_safe.module_address, None)
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
/// Subscribes to graph updates after a channel is already open, then triggers a balance increase
/// and a channel closure. Asserts each event sequentially interleaved with the actions that
/// trigger them, proving that events arrive at the correct time relative to their triggers.
async fn subscribe_graph_channel_update_on_closure(#[future(awt)] fixture: IntegrationFixture) -> Result<()> {
    let [src, dst] = fixture.sample_accounts::<2>();
    let expected_id = generate_channel_id(&src.address, &dst.address);
    let expected_channel_id = Hash::from(expected_id).encode_hex::<String>();
    let initial_amount = "1 wei wxHOPR".parse().expect("failed to parse amount");
    let fund_amount = "2 wei wxHOPR".parse().expect("failed to parse amount");
    let total_amount: HoprBalance = "3 wei wxHOPR".parse().expect("failed to parse amount");

    // Setup: deploy safes, announce, approve, and open channel with initial balance
    let src_safe = fixture.deploy_safe_and_announce(&src, parsed_safe_balance()).await?;
    fixture.deploy_safe_and_announce(&dst, parsed_safe_balance()).await?;
    fixture.approve(&src, initial_amount, &src_safe.module_address).await?;
    fixture
        .open_channel(&src, &dst, initial_amount, &src_safe.module_address, None)
        .await?;

    // Wait for indexing so the channel is in the database
    tokio::time::sleep(Duration::from_secs(8)).await;

    // Subscribe after the channel exists - consume the stream directly in this task
    let client = fixture.client().clone();
    let mut stream = client.subscribe_graph().expect("failed to create graph subscription");

    // Phase 1: receive and assert the initial Open state BEFORE triggering any updates
    let phase1 = loop {
        let entry = stream
            .next()
            .timeout(subscription_timeout())
            .await?
            .ok_or_else(|| anyhow!("stream ended before receiving initial channel state"))??;
        if entry.channel.concrete_channel_id.to_lowercase() == expected_channel_id.to_lowercase() {
            break entry;
        }
    };
    assert_eq!(phase1.channel.status, ChannelStatus::Open);
    assert_eq!(phase1.channel.balance.0, initial_amount.to_string());
    assert_eq!(phase1.source.chain_key, src.address.to_string());
    assert_eq!(phase1.destination.chain_key, dst.address.to_string());

    // Fund the channel with additional tokens - triggers ChannelBalanceIncreased -> ChannelUpdated.
    // This runs AFTER the initial state was received and asserted above.
    fixture.approve(&src, fund_amount, &src_safe.module_address).await?;
    fixture
        .open_channel(&src, &dst, fund_amount, &src_safe.module_address, None)
        .await?;

    // Receive balance increase event - can only arrive after the fund action above
    let balance_update = loop {
        let entry = stream
            .next()
            .timeout(subscription_timeout())
            .await?
            .ok_or_else(|| anyhow!("stream ended before receiving balance update"))??;
        if entry.channel.concrete_channel_id.to_lowercase() == expected_channel_id.to_lowercase() {
            break entry;
        }
    };
    assert_eq!(balance_update.channel.status, ChannelStatus::Open);
    assert_eq!(balance_update.channel.balance.0, total_amount.to_string());

    // Trigger channel closure - runs AFTER the balance update was received and asserted above
    fixture
        .initiate_outgoing_channel_closure(&src, &dst, &src_safe.module_address)
        .await?;

    // Receive closure event - can only arrive after the closure action above
    let closure = loop {
        let entry = stream
            .next()
            .timeout(subscription_timeout())
            .await?
            .ok_or_else(|| anyhow!("stream ended before receiving closure event"))??;
        if entry.channel.concrete_channel_id.to_lowercase() == expected_channel_id.to_lowercase() {
            break entry;
        }
    };
    assert_eq!(closure.channel.status, ChannelStatus::PendingToClose);
    assert_eq!(
        closure.channel.concrete_channel_id.to_lowercase(),
        expected_channel_id.to_lowercase()
    );

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

#[rstest]
#[test_log::test(tokio::test)]
#[serial]
/// Verifies that channelUpdated subscription does not emit duplicate events when subscribing
/// to a channel that already exists in the database (tests the 2-phase subscription pattern).
async fn subscribe_channels_no_duplicate_initial_state(#[future(awt)] fixture: IntegrationFixture) -> Result<()> {
    // 1. Setup accounts and deploy safes
    let [src, dst] = fixture.sample_accounts::<2>();
    let expected_id = generate_channel_id(&src.address, &dst.address);
    let src_safe = fixture.deploy_safe_and_announce(&src, parsed_safe_balance()).await?;
    fixture.deploy_safe_and_announce(&dst, parsed_safe_balance()).await?;

    // 2. Open channel BEFORE subscribing
    let amount = "100 wei wxHOPR".parse().expect("failed to parse amount");
    let expected_channel_id = Hash::from(expected_id).encode_hex::<String>();

    fixture.approve(&src, amount, &src_safe.module_address).await?;
    fixture
        .open_channel(&src, &dst, amount, &src_safe.module_address, None)
        .await?;

    // 3. Wait for indexing to complete
    tokio::time::sleep(Duration::from_secs(2)).await;

    // 4. NOW subscribe (channel already exists in DB)
    let client = fixture.client().clone();
    let channel_selector = ChannelSelector {
        filter: Some(ChannelFilter::ChannelId(expected_id.into())),
        status: Some(ChannelStatus::Open),
    };

    let handle = tokio::task::spawn(async move {
        client
            .subscribe_channels(channel_selector)
            .expect("failed to create subscription")
            .take(1) // Should only get 1 emission (initial state)
            .next()
            .timeout(subscription_timeout())
            .await
    });

    // 5. No additional actions - just wait for initial emission
    let result = handle.await??;

    // 6. Assert we got exactly 1 emission
    let received_channel = result.ok_or_else(|| anyhow!("Should receive initial state"))?;
    assert_eq!(
        received_channel?.concrete_channel_id.to_lowercase(),
        expected_channel_id.to_lowercase()
    );

    Ok(())
}

#[rstest]
#[test_log::test(tokio::test)]
#[serial]
/// Verifies that accountUpdated subscription does not emit duplicate events when subscribing
/// to an account that already exists in the database (tests the 2-phase subscription pattern).
async fn subscribe_account_no_duplicate_initial_state(#[future(awt)] fixture: IntegrationFixture) -> Result<()> {
    // 1. Deploy safe and announce BEFORE subscribing
    let [account] = fixture.sample_accounts::<1>();
    let account_address = account.address.to_string();

    fixture
        .deploy_safe_and_announce(&account, parsed_safe_balance())
        .await?;

    // 2. Wait for indexing
    tokio::time::sleep(Duration::from_secs(2)).await;

    // 3. Subscribe AFTER account already announced
    let client = fixture.client().clone();
    let selector = AccountSelector::Address(*account.to_alloy_address().as_ref());

    let handle = tokio::task::spawn(async move {
        client
            .subscribe_accounts(selector)
            .expect("failed to create subscription")
            .take(1) // Should only get 1 emission
            .next()
            .timeout(subscription_timeout())
            .await
    });

    // 4. Collect result
    let result = handle.await??;

    // 5. Assert single emission
    let account_data = result.ok_or_else(|| anyhow!("Should receive initial account state"))?;
    assert_eq!(account_data?.chain_key.to_lowercase(), account_address.to_lowercase());

    Ok(())
}

#[rstest]
#[test_log::test(tokio::test)]
#[serial]
/// Subscribes to transaction status updates, submits a transaction and verifies that
/// the subscription receives status updates (SUBMITTED -> CONFIRMED).
async fn subscribe_transaction_status_updates(#[future(awt)] fixture: IntegrationFixture) -> Result<()> {
    // 1. Build and submit transaction
    let [sender, recipient] = fixture.sample_accounts::<2>();
    let tx_value = U256::from(1_000_000u128); // 0.000000000001 ETH
    let nonce = fixture.rpc().transaction_count(&sender.address).await?;

    let raw_tx = fixture.build_raw_tx(tx_value, &sender, &recipient, nonce).await?;
    let signed_bytes =
        Vec::from_hex(raw_tx.trim_start_matches("0x")).map_err(|e| anyhow!("failed to decode raw transaction: {e}"))?;

    // 2. Submit and get tracking ID
    let tx_id = fixture.submit_and_track_tx(&signed_bytes).await?;

    // 3. Spawn subscription task using raw GraphQL query (since there's no client method yet)
    let query = json!({
        "query": format!(
            "subscription {{ transactionUpdated(id: \"{}\") {{ id status submittedAt transactionHash }} }}",
            tx_id
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

    let mut stream = client.stream().filter_map(|item| {
        futures::future::ready(match item {
            Ok(SSE::Event(event)) if event.event_type == "next" => {
                serde_json::from_str::<serde_json::Value>(&event.data).ok()
            }
            _ => None,
        })
    });

    // 4. Collect first two status updates
    let mut statuses = Vec::new();
    for _ in 0..2 {
        if let Some(update) = stream
            .next()
            .timeout(subscription_timeout())
            .await
            .map_err(|_| anyhow!("timed out waiting for transaction update"))?
        {
            if let Some(status_str) = update
                .get("data")
                .and_then(|d| d.get("transactionUpdated"))
                .and_then(|tx| tx.get("status"))
                .and_then(|s| s.as_str())
            {
                let status = match status_str {
                    "SUBMITTED" => TransactionStatus::Submitted,
                    "PENDING" => TransactionStatus::Pending,
                    "CONFIRMED" => TransactionStatus::Confirmed,
                    _ => continue,
                };
                statuses.push(status);
                if status == TransactionStatus::Confirmed {
                    break;
                }
            }
        }
    }

    // 5. Assert we received SUBMITTED and CONFIRMED statuses
    assert!(!statuses.is_empty(), "Should receive at least one status update");
    assert!(
        statuses.contains(&TransactionStatus::Submitted) || statuses.contains(&TransactionStatus::Pending),
        "Should receive SUBMITTED or PENDING status"
    );
    assert!(
        statuses.contains(&TransactionStatus::Confirmed),
        "Should receive CONFIRMED status"
    );

    Ok(())
}
