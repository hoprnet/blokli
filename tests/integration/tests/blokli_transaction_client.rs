use std::time::Duration;

use alloy::primitives::U256;
use anyhow::{Context, Result};
use blokli_client::api::{BlokliTransactionClient, types::TransactionStatus};
use blokli_integration_tests::{
    anvil::AnvilAccount,
    config::TestConfig,
    fixtures::{IntegrationFixture, integration_fixture as fixture},
    transaction::TransactionBuilder,
};
use rstest::*;
use serial_test::serial;
use tokio::time::sleep;
use tracing::info;

const TX_VALUE: u128 = 1_000_000_000_000; // 0.000001 ETH
enum ClientType {
    RPC,
    Blokli,
}

async fn build_raw_tx(
    value: U256,
    sender: &AnvilAccount,
    recipient: &AnvilAccount,
    nonce: u64,
    chain_id: u64,
    config: &TestConfig,
) -> Result<String> {
    let tx_builder = TransactionBuilder::new(&sender.private_key)?;

    tx_builder
        .build_eip1559_transaction_hex(
            chain_id,
            nonce,
            &recipient.address,
            value,
            config.max_fee_per_gas,
            config.max_priority_fee_per_gas,
            config.gas_limit,
        )
        .await
}

#[rstest]
#[case(ClientType::RPC)]
#[case(ClientType::Blokli)]
#[test_log::test(tokio::test)]
#[serial]
async fn submit_transaction(#[future(awt)] fixture: IntegrationFixture, #[case] client_type: ClientType) -> Result<()> {
    let [sender, recipient] = fixture.sample_accounts::<2>();
    let tx_value = U256::from(TX_VALUE);
    let chain_id = fixture.rpc().chain_id().await?;
    let nonce = fixture.rpc().transaction_count(&sender.address).await?;

    let raw_tx = build_raw_tx(tx_value, sender, recipient, nonce, chain_id, fixture.config()).await?;
    info!(%raw_tx, "built raw transaction hex");

    let initial_balance = fixture.rpc().get_balance(&recipient.address).await?;

    match client_type {
        ClientType::RPC => {
            info!("sending raw transaction through RPC client");
            fixture.rpc().execute_transaction(&raw_tx).await?;
        }
        ClientType::Blokli => {
            info!("sending raw transaction through Blokli client");
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
#[test_log::test(tokio::test)]
#[serial]
async fn submit_and_track_transaction(#[future(awt)] fixture: IntegrationFixture) -> Result<()> {
    let [sender, recipient] = fixture.sample_accounts::<2>();
    let tx_value = U256::from(TX_VALUE);
    let chain_id = fixture.rpc().chain_id().await?;
    let nonce = fixture.rpc().transaction_count(&sender.address).await?;

    let raw_tx = build_raw_tx(tx_value, sender, recipient, nonce, chain_id, fixture.config()).await?;
    info!(%raw_tx, "built raw transaction hex");

    let initial_balance = fixture.rpc().get_balance(&recipient.address).await?;

    info!("sending raw transaction through Blokli client");
    let txid = fixture.submit_and_track_tx(&raw_tx).await?;
    info!(%txid, "transaction submitted and tracked");

    // Poll transaction status until confirmed
    loop {
        match fixture
            .client()
            .track_transaction(txid.clone(), Duration::from_secs(5))
            .await
        {
            Ok(s) => {
                if s.status == TransactionStatus::Confirmed {
                    break;
                }
            }
            _ => continue,
        }
        sleep(Duration::from_millis(50)).await;
    }

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
    let chain_id = fixture.rpc().chain_id().await?;
    let nonce = fixture.rpc().transaction_count(&sender.address).await?;

    let raw_tx = build_raw_tx(tx_value, sender, recipient, nonce, chain_id, fixture.config()).await?;
    info!(%raw_tx, "built raw transaction hex");

    let initial_balance = fixture.rpc().get_balance(&recipient.address).await?;

    info!("sending raw transaction through Blokli client");
    fixture.submit_and_confirm_tx(&raw_tx, 2).await?;

    sleep(Duration::from_secs(8)).await; // TODO: replace with actual block time

    let final_balance = fixture.rpc().get_balance(&recipient.address).await?;
    let delta = final_balance
        .checked_sub(initial_balance)
        .context("recipient balance decreased unexpectedly")?;
    assert_eq!(delta, tx_value);
    Ok(())
}
