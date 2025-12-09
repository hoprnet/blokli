mod config;
mod docker;
mod graphql;
mod rpc;
mod tx;
mod util;

use std::sync::Arc;

use alloy::primitives::U256;
use anyhow::{Context, Result, anyhow};
pub use config::TestConfig;
use docker::DockerEnvironment;
use graphql::GraphqlClient;
use rpc::RpcClient;
use tracing::{info, warn};
use tx::TransactionBuilder;

use crate::graphql::ReadyzResponse;

static TRACING: std::sync::Once = std::sync::Once::new();

pub fn init_tracing() {
    TRACING.call_once(|| {
        let _ = tracing_subscriber::fmt()
            .with_env_filter(tracing_subscriber::EnvFilter::try_from_default_env().unwrap_or_else(|_| "info".into()))
            .try_init();
    });
}

pub async fn run_blokli_transaction_test() -> Result<()> {
    init_tracing();

    let config = Arc::new(TestConfig::load()?);
    let mut docker = DockerEnvironment::new(config.clone());
    docker.ensure_image_available()?;
    docker.compose_up()?;
    let accounts = docker.fetch_anvil_accounts()?;

    let graphql_client = GraphqlClient::new(&config)?;
    let ready = graphql_client.wait_until_ready().await?;
    log_ready_state(&ready);

    let rpc_client = RpcClient::new(&config.rpc_url, config.http_timeout)?;
    let chain_id = rpc_client.chain_id().await?;
    info!(chain_id, "connected to RPC endpoint");

    let tx_builder = TransactionBuilder::new(&accounts.sender_private_key)?;
    let sender_address = tx_builder.sender_address();
    info!(sender_address = %sender_address, "detected sender address");
    let nonce = rpc_client.transaction_count(&sender_address).await?;
    info!(nonce, "updated sender nonce");

    let recipient = accounts.recipient_address.clone();
    let initial_balance = rpc_client.get_balance(&recipient).await?;
    info!(recipient = %recipient, balance=?initial_balance, "recipient initial balance retrieved");

    let raw_tx = tx_builder
        .build_legacy_transaction_hex(
            chain_id,
            nonce,
            &recipient,
            U256::from(config.tx_value_wei),
            config.gas_price,
            config.gas_limit,
        )
        .await?;
    let payload_bytes = raw_tx.len().saturating_sub(2) / 2;
    info!(payload_bytes, "signed transaction payload");

    let submission = graphql_client
        .send_transaction_sync(&raw_tx, config.tx_confirmations)
        .await?;
    if submission.status != "CONFIRMED" {
        warn!(
            status = %submission.status,
            "graphQL sendTransactionSync returned unexpected status",
        );
    }

    let receipt = rpc_client
        .wait_for_receipt(
            &submission.transaction_hash,
            config.receipt_timeout,
            config.receipt_poll_interval,
        )
        .await?;
    if !receipt.success {
        return Err(anyhow!("transaction {} reverted on-chain", receipt.transaction_hash));
    }
    info!(
        hash = %receipt.transaction_hash,
        block = receipt.block_number,
        "transaction confirmed on-chain",
    );

    let final_balance = rpc_client.get_balance(&recipient).await?;
    let delta = final_balance
        .checked_sub(initial_balance)
        .context("recipient balance decreased unexpectedly")?;
    let expected_delta = U256::from(config.tx_value_wei);
    if delta != expected_delta {
        return Err(anyhow!(
            "recipient balance delta mismatch. expected={}, actual={}",
            expected_delta,
            delta
        ));
    }
    info!(recipient = %recipient, delta = ?delta, "recipient balance increased as expected");

    Ok(())
}

fn log_ready_state(ready: &ReadyzResponse) {
    info!(
        ready = ?ready.status,
        db = ?ready.checks.database.status,
        rpc = ?ready.checks.rpc.status,
        indexer = ?ready.checks.indexer.status,
        "status report"
    );
    if let Some(block_number) = ready.checks.rpc.block_number {
        info!(block = block_number, "rpc head block");
    }
    if let Some(block) = ready.checks.indexer.last_indexed_block {
        info!(block = block, "indexer last block");
    }
}
