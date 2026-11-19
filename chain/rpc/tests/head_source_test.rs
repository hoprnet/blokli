//! Integration tests for the WebSocket head source against anvil.

use std::time::Duration;

use anyhow::Context;
use blokli_chain_rpc::{
    rpc::{RpcOperations, RpcOperationsConfig},
    tip::HeadSource,
};
use blokli_chain_types::utils::create_anvil;
use hopr_bindings::exports::alloy::{
    rpc::client::ClientBuilder,
    transports::{http::ReqwestTransport, layers::RetryBackoffLayer},
};
use tokio::time::sleep;

const TEST_BLOCK_TIME: Duration = Duration::from_secs(1);
const TEST_FINALITY: u32 = 2;
const TEST_TX_POLLING_INTERVAL: Duration = Duration::from_millis(10);

#[tokio::test]
async fn test_websocket_head_source_advances_confirmed_tip() -> anyhow::Result<()> {
    let _ = env_logger::builder().is_test(true).try_init();

    let anvil = create_anvil(Some(TEST_BLOCK_TIME));
    let transport_client = ReqwestTransport::new(anvil.endpoint_url());
    let rpc_client = ClientBuilder::default()
        .layer(RetryBackoffLayer::new(2, 100, 100))
        .transport(transport_client.clone(), transport_client.guess_local());

    let cfg = RpcOperationsConfig {
        chain_id: anvil.chain_id(),
        tx_polling_interval: TEST_TX_POLLING_INTERVAL,
        expected_block_time: TEST_BLOCK_TIME,
        finality: TEST_FINALITY,
        head_ws_url: Some(anvil.ws_endpoint_url()),
        ..RpcOperationsConfig::default()
    };

    let rpc = RpcOperations::new(rpc_client, transport_client.client().clone(), cfg, None)?;
    let tracker = rpc.tip_tracker();
    tracker.ensure_started();

    tokio::time::timeout(Duration::from_secs(20), async {
        loop {
            if tracker.current().is_some_and(|tip| tip.source == HeadSource::WebSocket) {
                break;
            }
            sleep(Duration::from_millis(100)).await;
        }
    })
    .await
    .context("tracker did not switch to the WebSocket head source")?;

    let initial_confirmed = tracker
        .current()
        .map(|tip| tip.confirmed)
        .context("tracker did not publish a tip")?;

    tokio::time::timeout(Duration::from_secs(20), async {
        loop {
            if tracker
                .current()
                .is_some_and(|tip| tip.confirmed > initial_confirmed && tip.source == HeadSource::WebSocket)
            {
                break;
            }
            sleep(Duration::from_millis(100)).await;
        }
    })
    .await
    .context("confirmed tip did not advance through the WebSocket head source")?;

    Ok(())
}
