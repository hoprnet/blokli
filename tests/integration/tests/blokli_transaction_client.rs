use std::str::FromStr;
use std::time::Duration;

use anyhow::{Context, Result};
use blokli_client::api::{BlokliQueryClient, BlokliTransactionClient, types::TransactionStatus};
use blokli_integration_tests::constants::parsed_safe_balance;
use blokli_integration_tests::fixtures::{IntegrationFixture, integration_fixture as fixture};
use hex::FromHex;
use hopr_bindings::exports::alloy::primitives::U256;
use hopr_chain_connector::{PayloadGenerator, SafePayloadGenerator};
use hopr_chain_types::prelude::SignableTransaction;
use hopr_primitive_types::prelude::Address as HoprAddress;
use rstest::*;
use serial_test::serial;
use tokio::time::sleep;

const TX_VALUE: u128 = 1_000_000; // 0.000000000001 ETH
enum ClientType {
    RPC,
    Blokli,
}

#[rstest]
#[case(ClientType::RPC)]
#[case(ClientType::Blokli)]
#[test_log::test(tokio::test)]
#[serial]
/// Test that submitting a transaction with a valid payload, via blokli and via direct RPC, goes through.
/// The payload is a simple token transfer to not test any HOPR-specific logic, but rather only the
/// transaction submission flow.
async fn submit_transaction(#[future(awt)] fixture: IntegrationFixture, #[case] client_type: ClientType) -> Result<()> {
    let [sender, recipient] = fixture.sample_accounts::<2>();
    let tx_value = U256::from(TX_VALUE);
    let nonce = fixture.rpc().transaction_count(&sender.address).await?;

    let raw_tx = fixture.build_raw_tx(tx_value, sender, recipient, nonce).await?;
    let signed_bytes =
        Vec::from_hex(raw_tx.trim_start_matches("0x")).context("failed to decode raw transaction payload")?;

    let initial_balance = fixture.rpc().get_balance(&recipient.address).await?;

    match client_type {
        ClientType::RPC => {
            fixture.rpc().execute_transaction(&raw_tx).await?;
        }
        ClientType::Blokli => {
            fixture.submit_tx(&signed_bytes).await?;
        }
    }

    sleep(Duration::from_secs(8)).await; // TODO: replace with actual block time

    let final_balance = fixture.rpc().get_balance(&recipient.address).await?;
    let delta = final_balance
        .checked_sub(initial_balance)
        .context("recipient balance decreased unexpectedly")?;

    assert_eq!(delta, tx_value);

    Ok(())
}

#[rstest]
#[case(ClientType::RPC)]
#[case(ClientType::Blokli)]
#[test_log::test(tokio::test)]
#[serial]
/// Test that submitting a transaction with an incorrect payload, via blokli and via direct RPC, fails.
/// The payload is generated as a valid payload but then tampered to make it invalid.
async fn submit_transaction_with_incorrect_payload(
    #[future(awt)] fixture: IntegrationFixture,
    #[case] client_type: ClientType,
) -> Result<()> {
    let [sender, recipient] = fixture.sample_accounts::<2>();
    let tx_value = U256::from(TX_VALUE);
    let nonce = fixture.rpc().transaction_count(&sender.address).await?;

    let mut raw_tx = fixture.build_raw_tx(tx_value, sender, recipient, nonce).await?;
    raw_tx.replace_range(10..14, "dead");
    let signed_bytes =
        Vec::from_hex(raw_tx.trim_start_matches("0x")).context("failed to decode raw transaction payload")?;

    let res = match client_type {
        ClientType::RPC => fixture.rpc().execute_transaction(&raw_tx).await,
        ClientType::Blokli => fixture.submit_tx(&signed_bytes).await,
    };
    assert!(res.is_err(), "transaction with incorrect payload should fail");

    Ok(())
}

#[rstest]
#[case(ClientType::RPC)]
#[case(ClientType::Blokli)]
#[test_log::test(tokio::test)]
#[serial]
/// Test that submitting a transaction with too much value, via blokli and via direct RPC, fails.
/// The payload used is per say valid, except that the value to transfer exceeds the source's balance.
async fn submit_transaction_with_too_much_value(
    #[future(awt)] fixture: IntegrationFixture,
    #[case] client_type: ClientType,
) -> Result<()> {
    let [sender, recipient] = fixture.sample_accounts::<2>();
    let tx_value = U256::MAX; // definitely too much value
    let nonce = fixture.rpc().transaction_count(&sender.address).await?;

    let raw_tx = fixture.build_raw_tx(tx_value, sender, recipient, nonce).await?;
    let signed_bytes =
        Vec::from_hex(raw_tx.trim_start_matches("0x")).context("failed to decode raw transaction payload")?;

    let res = match client_type {
        ClientType::RPC => fixture.rpc().execute_transaction(&raw_tx).await,
        ClientType::Blokli => fixture.submit_tx(&signed_bytes).await,
    };
    assert!(res.is_err(), "transaction with incorrect payload should fail");

    Ok(())
}

#[rstest]
#[test_log::test(tokio::test)]
#[serial]
/// Test that submitting a transaction with a valid payload via blokli goes through, and the tracking
/// id provided by blokli can be used to track the transaction status until confirmation.
async fn submit_and_track_transaction(#[future(awt)] fixture: IntegrationFixture) -> Result<()> {
    let [sender, recipient] = fixture.sample_accounts::<2>();
    let tx_value = U256::from(TX_VALUE);
    let nonce = fixture.rpc().transaction_count(&sender.address).await?;

    let raw_tx = fixture.build_raw_tx(tx_value, sender, recipient, nonce).await?;
    let initial_balance = fixture.rpc().get_balance(&recipient.address).await?;
    let signed_bytes =
        Vec::from_hex(raw_tx.trim_start_matches("0x")).context("failed to decode raw transaction payload")?;

    let txid = fixture.submit_and_track_tx(&signed_bytes).await?;

    let res = fixture
        .client()
        .track_transaction(txid.clone(), Duration::from_secs(30))
        .await?;
    assert_eq!(res.status, TransactionStatus::Confirmed);

    let final_balance = fixture.rpc().get_balance(&recipient.address).await?;
    let delta = final_balance
        .checked_sub(initial_balance)
        .context("recipient balance decreased unexpectedly")?;

    assert_eq!(delta, tx_value);

    Ok(())
}

#[rstest]
#[test_log::test(tokio::test)]
#[serial]
/// Test that submitting and confirming a transaction (1 block finality by default) with a valid
/// payload via blokli goes through.
async fn submit_and_confirm_transaction(#[future(awt)] fixture: IntegrationFixture) -> Result<()> {
    let [sender, recipient] = fixture.sample_accounts::<2>();
    let tx_value = U256::from(TX_VALUE);
    let nonce = fixture.rpc().transaction_count(&sender.address).await?;
    let confirmations = fixture.config().tx_confirmations;

    let raw_tx = fixture.build_raw_tx(tx_value, sender, recipient, nonce).await?;
    let signed_bytes =
        Vec::from_hex(raw_tx.trim_start_matches("0x")).context("failed to decode raw transaction payload")?;
    let initial_balance = fixture.rpc().get_balance(&recipient.address).await?;

    let block_number = fixture.client().query_chain_info().await?.block_number;
    fixture.submit_and_confirm_tx(&signed_bytes, confirmations).await?;

    assert!(fixture.client().query_chain_info().await?.block_number >= block_number + (confirmations as i32));

    let final_balance = fixture.rpc().get_balance(&recipient.address).await?;
    let delta = final_balance
        .checked_sub(initial_balance)
        .context("recipient balance decreased unexpectedly")?;
    assert_eq!(delta, tx_value);
    Ok(())
}

#[rstest]
#[test_log::test(tokio::test)]
#[serial]
/// Test that a Safe module transaction (via execTransactionFromModule) that succeeds internally
/// is correctly detected and enriched with safe_execution data.
async fn test_safe_module_transaction_execution_success(
    #[future(awt)] fixture: IntegrationFixture,
) -> Result<()> {
    let [owner] = fixture.sample_accounts::<1>();
    let safe = fixture.deploy_safe_and_announce(owner, parsed_safe_balance()).await?;

    // Build an approve transaction via the Safe module.
    // This is a module transaction (goes through execTransactionFromModule), not a direct
    // Safe execTransaction call.
    let nonce = fixture.rpc().transaction_count(&owner.address).await?;
    let spender = HoprAddress::from_str(&fixture.contract_addresses().channels.to_string())?;
    let amount = "0.01 wxHOPR".parse().expect("failed to parse amount");

    let payload_generator = SafePayloadGenerator::new(
        &owner.keypair,
        *fixture.contract_addresses(),
        HoprAddress::from_str(&safe.module_address)?,
    );
    let payload = payload_generator.approve(spender, amount)?;
    let payload_bytes = payload
        .sign_and_encode_to_eip2718(nonce, fixture.rpc().chain_id().await?, None, &owner.keypair)
        .await?;

    let txid = fixture.submit_and_track_tx(&payload_bytes).await?;

    let res = fixture
        .client()
        .track_transaction(txid, Duration::from_secs(60))
        .await?;

    assert_eq!(res.status, TransactionStatus::Confirmed);
    let safe_exec = res
        .safe_execution
        .expect("safe_execution should be populated for module transactions");
    assert!(safe_exec.success, "inner Safe execution should have succeeded");

    Ok(())
}

#[rstest]
#[test_log::test(tokio::test)]
#[serial]
/// Test that a Safe module transaction that fails internally (inner revert) is correctly
/// detected with safe_execution.success = false while the outer transaction still confirms.
async fn test_safe_module_transaction_execution_failure(
    #[future(awt)] fixture: IntegrationFixture,
) -> Result<()> {
    let [owner, counterparty] = fixture.sample_accounts::<2>();
    let safe = fixture.deploy_safe_and_announce(owner, parsed_safe_balance()).await?;

    // Build a channel closure transaction for a non-existent channel.
    // The inner call will revert because there's no open channel between these parties.
    let nonce = fixture.rpc().transaction_count(&owner.address).await?;

    let payload_generator = SafePayloadGenerator::new(
        &owner.keypair,
        *fixture.contract_addresses(),
        HoprAddress::from_str(&safe.module_address)?,
    );
    let payload = payload_generator.initiate_outgoing_channel_closure(counterparty.address)?;
    let payload_bytes = payload
        .sign_and_encode_to_eip2718(nonce, fixture.rpc().chain_id().await?, None, &owner.keypair)
        .await?;

    let txid = fixture.submit_and_track_tx(&payload_bytes).await?;

    let res = fixture
        .client()
        .track_transaction(txid, Duration::from_secs(60))
        .await?;

    // The outer transaction confirms (the module call itself went through),
    // but the internal Safe execution should have failed.
    assert_eq!(res.status, TransactionStatus::Confirmed);
    let safe_exec = res
        .safe_execution
        .expect("safe_execution should be populated for module transactions");
    assert!(
        !safe_exec.success,
        "inner Safe execution should have failed for non-existent channel"
    );

    Ok(())
}

#[rstest]
#[test_log::test(tokio::test)]
#[serial]
/// Test that a plain ETH transfer (not targeting a Safe or module) has no safe_execution
/// enrichment.
async fn test_plain_transaction_no_safe_enrichment(#[future(awt)] fixture: IntegrationFixture) -> Result<()> {
    let [sender, recipient] = fixture.sample_accounts::<2>();
    let tx_value = U256::from(TX_VALUE);
    let nonce = fixture.rpc().transaction_count(&sender.address).await?;

    let raw_tx = fixture.build_raw_tx(tx_value, sender, recipient, nonce).await?;
    let signed_bytes =
        Vec::from_hex(raw_tx.trim_start_matches("0x")).context("failed to decode raw transaction payload")?;

    let txid = fixture.submit_and_track_tx(&signed_bytes).await?;

    let res = fixture
        .client()
        .track_transaction(txid, Duration::from_secs(30))
        .await?;

    assert_eq!(res.status, TransactionStatus::Confirmed);
    assert!(
        res.safe_execution.is_none(),
        "plain transfers should not have safe_execution enrichment"
    );

    Ok(())
}
