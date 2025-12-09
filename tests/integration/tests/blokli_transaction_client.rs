use std::time::Duration;

use alloy::primitives::U256;
use anyhow::{Context, Result, anyhow};
use blokli_integration_tests::fixtures::{IntegrationFixture, integration_fixture as fixture};
use rstest::*;
use serial_test::serial;
use tokio::time::sleep;
use tracing::info;

// TODO: This has to be moved to the fixture creation eventually
const START_SLEEP_DURATION: Duration = Duration::from_secs(8);

enum ClientType {
    RPC,
    Blokli,
}

#[rstest]
#[case(ClientType::RPC)]
#[case(ClientType::Blokli)]
#[test_log::test(tokio::test)]
#[serial]
async fn send_submit_transaction(fixture: IntegrationFixture, #[case] client_type: ClientType) -> Result<()> {
    sleep(START_SLEEP_DURATION).await;

    let tx_value = U256::from(1_000_000_000_000_000u128); // 0.001 ETH

    let [sender, recipient] = fixture.sample_accounts::<2>();
    let tx_builder = fixture.transaction_builder(&sender.private_key)?;

    let chain_id = fixture.rpc.chain_id().await?;
    let nonce = fixture.rpc.transaction_count(&sender.address).await?;

    let initial_balance = fixture.rpc.get_balance(&recipient.address).await?;

    let raw_tx = tx_builder
        .build_legacy_transaction_hex(
            chain_id,
            nonce,
            &recipient.address,
            tx_value,
            fixture.config().gas_price,
            fixture.config().gas_limit,
        )
        .await?;
    info!(%raw_tx, "built raw transaction hex");

    match client_type {
        ClientType::RPC => {
            info!("sending raw transaction through RPC client");
            fixture.rpc.execute_transaction(&raw_tx).await?;
        }
        ClientType::Blokli => {
            info!("sending raw transaction through Blokli client");
            fixture.execute_transaction(&raw_tx).await?;
        }
    }

    sleep(Duration::from_secs(8)).await; // TODO: replace with actual block time

    let final_balance = fixture.rpc.get_balance(&recipient.address).await?;
    let delta = final_balance
        .checked_sub(initial_balance)
        .context("recipient balance decreased unexpectedly")?;
    if delta != tx_value {
        return Err(anyhow!(
            "recipient balance delta mismatch. expected={}, actual={}",
            tx_value,
            delta
        ));
    }
    Ok(())
}

#[rstest]
#[case(ClientType::RPC)]
#[case(ClientType::Blokli)]
#[test_log::test(tokio::test)]
#[serial]
async fn submit_and_track_transaction(fixture: IntegrationFixture, #[case] client_type: ClientType) -> Result<()> {
    Ok(())
}

#[rstest]
#[case(ClientType::RPC)]
#[case(ClientType::Blokli)]
#[test_log::test(tokio::test)]
#[serial]
async fn submit_and_confirm_transaction(fixture: IntegrationFixture, #[case] client_type: ClientType) -> Result<()> {
    Ok(())
}

#[rstest]
#[case(ClientType::RPC)]
#[case(ClientType::Blokli)]
#[test_log::test(tokio::test)]
#[serial]
async fn track_transaction(fixture: IntegrationFixture, #[case] client_type: ClientType) -> Result<()> {
    Ok(())
}
