use std::time::Duration;

use alloy::primitives::U256;
use anyhow::{Context, Result};
use blokli_client::api::{BlokliQueryClient, BlokliTransactionClient, types::TransactionStatus};
use blokli_integration_tests::fixtures::{IntegrationFixture, integration_fixture as fixture};
use rstest::*;
use serial_test::serial;
use tokio::time::sleep;

const TX_VALUE: u128 = 1_000_000_000_000; // 0.000001 ETH
enum ClientType {
    RPC,
    Blokli,
}

// TODO: test a withdrawal transaction with too much value

#[rstest]
#[case(ClientType::RPC)]
#[case(ClientType::Blokli)]
#[test_log::test(tokio::test)]
#[serial]
async fn submit_transaction(#[future(awt)] fixture: IntegrationFixture, #[case] client_type: ClientType) -> Result<()> {
    let [sender, recipient] = fixture.sample_accounts::<2>();
    let tx_value = U256::from(TX_VALUE);
    let nonce = fixture.rpc().transaction_count(&sender.address).await?;

    let raw_tx = fixture.build_raw_tx(tx_value, sender, recipient, nonce).await?;

    let initial_balance = fixture.rpc().get_balance(&recipient.address).await?;

    match client_type {
        ClientType::RPC => {
            fixture.rpc().execute_transaction(&raw_tx).await?;
        }
        ClientType::Blokli => {
            fixture.submit_tx(&raw_tx).await?;
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
async fn submit_transaction_with_incorrect_payload(
    #[future(awt)] fixture: IntegrationFixture,
    #[case] client_type: ClientType,
) -> Result<()> {
    let [sender, recipient] = fixture.sample_accounts::<2>();
    let tx_value = U256::MAX; // definitely too much value
    let nonce = fixture.rpc().transaction_count(&sender.address).await?;

    let mut raw_tx = fixture.build_raw_tx(tx_value, sender, recipient, nonce).await?;
    raw_tx.replace_range(10..14, "dead");

    let res = match client_type {
        ClientType::RPC => fixture.rpc().execute_transaction(&raw_tx).await,
        ClientType::Blokli => fixture.submit_tx(&raw_tx).await,
    };
    assert!(res.is_err(), "transaction with incorrect payload should fail");

    Ok(())
}

#[rstest]
#[case(ClientType::RPC)]
#[case(ClientType::Blokli)]
#[test_log::test(tokio::test)]
#[serial]
async fn submit_transaction_with_too_much_value(
    #[future(awt)] fixture: IntegrationFixture,
    #[case] client_type: ClientType,
) -> Result<()> {
    let [sender, recipient] = fixture.sample_accounts::<2>();
    let tx_value = U256::from(TX_VALUE);
    let nonce = fixture.rpc().transaction_count(&sender.address).await?;

    let raw_tx = fixture.build_raw_tx(tx_value, sender, recipient, nonce).await?;

    let res = match client_type {
        ClientType::RPC => fixture.rpc().execute_transaction(&raw_tx).await,
        ClientType::Blokli => fixture.submit_tx(&raw_tx).await,
    };
    assert!(res.is_err(), "transaction with incorrect payload should fail");

    Ok(())
}

#[rstest]
#[test_log::test(tokio::test)]
#[serial]
async fn submit_and_track_transaction(#[future(awt)] fixture: IntegrationFixture) -> Result<()> {
    let [sender, recipient] = fixture.sample_accounts::<2>();
    let tx_value = U256::from(TX_VALUE);
    let nonce = fixture.rpc().transaction_count(&sender.address).await?;

    let raw_tx = fixture.build_raw_tx(tx_value, sender, recipient, nonce).await?;
    let initial_balance = fixture.rpc().get_balance(&recipient.address).await?;
    let txid = fixture.submit_and_track_tx(&raw_tx).await?;

    let res = fixture
        .client()
        .track_transaction(txid.clone(), Duration::from_secs(5))
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
async fn submit_and_confirm_transaction(#[future(awt)] fixture: IntegrationFixture) -> Result<()> {
    let [sender, recipient] = fixture.sample_accounts::<2>();
    let tx_value = U256::from(TX_VALUE);
    let nonce = fixture.rpc().transaction_count(&sender.address).await?;

    let raw_tx = fixture.build_raw_tx(tx_value, sender, recipient, nonce).await?;

    let initial_balance = fixture.rpc().get_balance(&recipient.address).await?;

    let block_number = fixture.client().query_chain_info().await?.block_number;
    fixture.submit_and_confirm_tx(&raw_tx, 2).await?;

    assert!(fixture.client().query_chain_info().await?.block_number >= block_number + 2);

    let final_balance = fixture.rpc().get_balance(&recipient.address).await?;
    let delta = final_balance
        .checked_sub(initial_balance)
        .context("recipient balance decreased unexpectedly")?;
    assert_eq!(delta, tx_value);
    Ok(())
}
