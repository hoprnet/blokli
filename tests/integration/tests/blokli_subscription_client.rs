use std::time::{Duration, Instant};

use anyhow::{Result, anyhow};
use blokli_client::api::{
    AccountSelector, BlokliQueryClient, BlokliSubscriptionClient, ChannelFilter, ChannelSelector, SafeSelector,
    types::{ChannelStatus, TransactionStatus},
};
use blokli_integration_tests::{
    constants::{EPSILON, parsed_safe_balance, subscription_timeout},
    fixtures::{IntegrationFixture, integration_fixture as fixture, poll_until},
};
use eventsource_client::{Client, ClientBuilder, SSE};
use futures::stream::StreamExt;
use futures_time::{future::FutureExt as FutureTimeoutExt, time::Duration as FuturesDuration};
use hex::{FromHex, ToHex};
use hopr_bindings::exports::alloy::primitives::U256;
use hopr_types::{
    crypto::{keypairs::Keypair, types::Hash},
    internal::channels::generate_channel_id,
    primitive::prelude::HoprBalance,
};
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
        ..Default::default()
    };
    let amount = "1 wei wxHOPR".parse().expect("failed to parse amount");
    let expected_channel_id = Hash::from(expected_id).encode_hex::<String>();
    let client = fixture.client().clone();

    let (src_safe, _dst_safe) = tokio::try_join!(
        fixture.deploy_safe_and_announce(&src, parsed_safe_balance()),
        fixture.deploy_safe_and_announce(&dst, parsed_safe_balance()),
    )?;

    fixture.approve(src, amount, &src_safe.module_address).await?;

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

                let should_skip = expected_channel_id.to_lowercase() != retrieved_channel;
                futures::future::ready(should_skip)
            })
            .next()
            .timeout(subscription_timeout())
            .await
    });

    fixture
        .open_channel(src, dst, amount, &src_safe.module_address, None)
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

    let (src_safe, _dst_safe) = tokio::try_join!(
        fixture.deploy_safe_and_announce(&src, parsed_safe_balance()),
        fixture.deploy_safe_and_announce(&dst, parsed_safe_balance()),
    )?;

    fixture.approve(src, amount, &src_safe.module_address).await?;

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

                let should_skip = expected_id.encode_hex::<String>() != retrieved_channel;
                futures::future::ready(should_skip)
            })
            .next()
            .timeout(subscription_timeout())
            .await
    });

    fixture
        .open_channel(src, dst, amount, &src_safe.module_address, None)
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
    let (src_safe, _dst_safe) = tokio::try_join!(
        fixture.deploy_safe_and_announce(&src, parsed_safe_balance()),
        fixture.deploy_safe_and_announce(&dst, parsed_safe_balance()),
    )?;
    fixture.approve(&src, initial_amount, &src_safe.module_address).await?;
    fixture
        .open_channel(src, dst, initial_amount, &src_safe.module_address, None)
        .await?;

    // Wait for the channel to be indexed before subscribing
    let channel_selector = ChannelSelector {
        filter: Some(ChannelFilter::ChannelId(expected_id.into())),
        status: Some(ChannelStatus::Open),
        ..Default::default()
    };
    let poll_client = fixture.client().clone();
    let poll_selector = channel_selector.clone();
    poll_until(
        "channel indexed for graph subscription",
        Duration::from_secs(30),
        Duration::from_millis(500),
        || {
            let client = poll_client.clone();
            let selector = poll_selector.clone();
            async move {
                let count = client.count_channels(selector).await?;
                Ok(if count >= 1 { Some(count) } else { None })
            }
        },
    )
    .await?;

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
    fixture.approve(src, fund_amount, &src_safe.module_address).await?;
    fixture
        .open_channel(src, dst, fund_amount, &src_safe.module_address, None)
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
        .initiate_outgoing_channel_closure(src, dst, &src_safe.module_address)
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

    let new_win_prob = 0.005f64;
    let quiet_period = Duration::from_secs(5);

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


    // wait for the latest ticket param update
    let deadline = Instant::now() + Duration::from_secs(subscription_timeout().as_secs());
    let mut latest_output = None;

    while Instant::now() < deadline {
        let wait_time = quiet_period.min(deadline.saturating_duration_since(Instant::now()));

        match tokio::time::timeout(wait_time, stream.next()).await {
            Ok(Some(Ok(output))) => {
                tracing::info!("Received ticket parameters subscription event: {:?}", output);
                latest_output = Some(output);
            }
            Ok(Some(Err(error))) => return Err(error.into()),
            Ok(None) => break,
            Err(_) => break,
        }
    }

    let output = latest_output.ok_or_else(|| anyhow!("no ticket parameters received from subscription"))?;

    tracing::info!(
        "Received ticket parameters update: {:?}, expected: {:?}",
        output,
        new_win_prob
    );

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

    assert!(
        safe.owners
            .iter()
            .any(|owner| owner.eq_ignore_ascii_case(&account.address.to_string())),
        "safe deployment subscription should include the deploying account in owners"
    );

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
        .bloklid_url()
        .join("graphql")
        .map_err(|e| anyhow!("failed to build GraphQL URL: {e}"))?;

    let reqwest_client = reqwest::Client::builder()
        .connect_timeout(fixture.config().http_timeout)
        .build()
        .map_err(|e| anyhow!("failed to build reqwest client: {e}"))?;
    let transport = blokli_client::ReqwestTransport::new(reqwest_client);

    let client = ClientBuilder::for_url(url.as_str())
        .map_err(|e| anyhow!("failed to build SSE client: {e}"))?
        .header("Accept", "text/event-stream")
        .map_err(|e| anyhow!("failed to set Accept header: {e}"))?
        .header("Content-Type", "application/json")
        .map_err(|e| anyhow!("failed to set Content-Type header: {e}"))?
        .method("POST".into())
        .body(request_body)
        .build_with_transport(transport);

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
    let (src_safe, _dst_safe) = tokio::try_join!(
        fixture.deploy_safe_and_announce(&src, parsed_safe_balance()),
        fixture.deploy_safe_and_announce(&dst, parsed_safe_balance()),
    )?;

    // 2. Open channel BEFORE subscribing
    let amount = "100 wei wxHOPR".parse().expect("failed to parse amount");
    let expected_channel_id = Hash::from(expected_id).encode_hex::<String>();

    fixture.approve(src, amount, &src_safe.module_address).await?;
    fixture
        .open_channel(src, dst, amount, &src_safe.module_address, None)
        .await?;

    // 3. Wait for the channel to be indexed
    let channel_selector = ChannelSelector {
        filter: Some(ChannelFilter::ChannelId(expected_id.into())),
        status: Some(ChannelStatus::Open),
        ..Default::default()
    };
    let poll_client = fixture.client().clone();
    let poll_selector = channel_selector.clone();
    poll_until(
        "channel indexed for no-duplicate test",
        Duration::from_secs(30),
        Duration::from_millis(500),
        || {
            let client = poll_client.clone();
            let selector = poll_selector.clone();
            async move {
                let count = client.count_channels(selector).await?;
                Ok(if count >= 1 { Some(count) } else { None })
            }
        },
    )
    .await?;

    // 4. NOW subscribe (channel already exists in DB)
    let client = fixture.client().clone();

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

    fixture.deploy_safe_and_announce(account, parsed_safe_balance()).await?;

    // 2. Wait for the account to be indexed
    let poll_client = fixture.client().clone();
    let poll_selector = AccountSelector::Address(*account.to_alloy_address().as_ref());
    poll_until(
        "account indexed for no-duplicate test",
        Duration::from_secs(30),
        Duration::from_millis(500),
        || {
            let client = poll_client.clone();
            let selector = poll_selector.clone();
            async move {
                let accounts = client.query_accounts(selector).await?;
                Ok(if accounts.is_empty() { None } else { Some(()) })
            }
        },
    )
    .await?;

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

    let raw_tx = fixture.build_raw_tx(tx_value, sender, recipient, nonce).await?;
    let signed_bytes =
        Vec::from_hex(raw_tx.trim_start_matches("0x")).map_err(|e| anyhow!("failed to decode raw transaction: {e}"))?;

    // 2. Submit and get tracking ID
    let tx_id = fixture.submit_and_track_tx(&signed_bytes).await?;

    // 3. Subscribe to transaction status updates via typed client
    let mut stream = fixture.client().subscribe_track_transaction(tx_id)?;

    // 4. Collect status updates until Confirmed or timeout
    let mut statuses = Vec::new();
    let start = Instant::now();
    let overall_timeout: Duration = subscription_timeout().into();
    loop {
        let elapsed = start.elapsed();
        if elapsed >= overall_timeout {
            break;
        }
        let remaining = FuturesDuration::from(overall_timeout - elapsed);

        match stream.next().timeout(remaining).await {
            Ok(Some(Ok(tx))) => {
                statuses.push(tx.status);
                if tx.status == TransactionStatus::Confirmed {
                    break;
                }
            }
            Ok(Some(Err(e))) => {
                panic!("Subscription error: {e:?}");
            }
            Ok(None) => break,
            Err(_) => break,
        }
    }

    // 5. Assert we received status updates including CONFIRMED
    assert!(!statuses.is_empty(), "Should receive at least one status update");
    assert!(
        statuses.contains(&TransactionStatus::Confirmed),
        "Should receive CONFIRMED status"
    );

    Ok(())
}
