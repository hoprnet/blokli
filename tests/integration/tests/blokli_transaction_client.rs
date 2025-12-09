use std::time::Duration;

use alloy::primitives::U256;
use anyhow::{Context, Result, anyhow};
use blokli_client::api::BlokliQueryClient;
use blokli_integration_tests::fixtures::{IntegrationFixture, integration_fixture as fixture};
use rstest::*;
use serial_test::serial;
use tokio::time::sleep;
use tracing::info;

#[rstest]
#[case("rpc")]
#[case("blokli")]
#[test_log::test(tokio::test)]
#[serial]
async fn send_raw_transaction(fixture: IntegrationFixture, #[case] client_type: &str) -> Result<()> {
    sleep(Duration::from_secs(8)).await;

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
        "rpc" => {
            info!("sending raw transaction through RPC client");
            fixture.rpc.execute_transaction(&raw_tx).await?;
        }
        "blokli" => {
            info!("sending raw transaction through Blokli client");
            fixture.execute_transaction(&raw_tx).await?;
        }
        other => {
            return Err(anyhow!("unknown client type: {}", other));
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
#[test_log::test(tokio::test)]
#[serial]
async fn chain_ids_provided_by_blokli_matches_the_rpc(fixture: IntegrationFixture) -> Result<()> {
    sleep(Duration::from_secs(8)).await;

    let chain = fixture.client.query_chain_info().await?;
    let chain_id = fixture.rpc.chain_id().await?;

    assert_eq!(chain.chain_id as u64, chain_id);

    Ok(())
}
