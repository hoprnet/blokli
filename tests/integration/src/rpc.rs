use std::time::Duration;

use alloy::primitives::U256;
use anyhow::{Context, Result, anyhow};
use reqwest::Client;
use serde::{Deserialize, Serialize};
use serde_json::{Value, json};
use tokio::time::{Instant, sleep};
use tracing::info;

pub struct RpcClient {
    http: Client,
    url: String,
}

impl RpcClient {
    pub fn new(url: &str, timeout: Duration) -> Result<Self> {
        let http = Client::builder()
            .timeout(timeout)
            .build()
            .context("Failed to build RPC client")?;
        Ok(Self {
            http,
            url: url.to_string(),
        })
    }

    pub async fn chain_id(&self) -> Result<u64> {
        let value = self
            .call_raw("eth_chainId", Vec::new())
            .await?
            .context("eth_chainId returned no result")?;
        parse_hex_quantity(value.as_str().context("eth_chainId returned non-string result")?)
    }

    pub async fn transaction_count(&self, address: &str) -> Result<u64> {
        let value = self
            .call_raw("eth_getTransactionCount", vec![json!(address), json!("latest")])
            .await?
            .context("eth_getTransactionCount returned no result")?;
        parse_hex_quantity(
            value
                .as_str()
                .context("eth_getTransactionCount returned non-string result")?,
        )
    }

    pub async fn get_balance(&self, address: &str) -> Result<U256> {
        let value = self
            .call_raw("eth_getBalance", vec![json!(address), json!("latest")])
            .await?
            .context("eth_getBalance returned no result")?;
        parse_u256(value.as_str().context("eth_getBalance returned non-string result")?)
    }

    pub async fn wait_for_receipt(
        &self,
        tx_hash: &str,
        timeout: Duration,
        poll_interval: Duration,
    ) -> Result<TransactionReceipt> {
        let start = Instant::now();
        loop {
            if let Some(value) = self.call_raw("eth_getTransactionReceipt", vec![json!(tx_hash)]).await? {
                let receipt: RpcTransactionReceipt = serde_json::from_value(value)?;
                let block_number = receipt
                    .block_number
                    .as_deref()
                    .map(parse_hex_quantity)
                    .transpose()?
                    .unwrap_or(0);
                let success = match receipt.status.as_deref() {
                    Some("0x1") => true,
                    Some("0x0") => false,
                    Some(other) => {
                        return Err(anyhow!("Unexpected receipt status value: {}", other));
                    }
                    None => false,
                };

                info!(hash = %receipt.transaction_hash, success, "received transaction receipt");

                return Ok(TransactionReceipt {
                    transaction_hash: receipt.transaction_hash,
                    block_number,
                    success,
                });
            }

            if start.elapsed() >= timeout {
                break;
            }
            sleep(poll_interval).await;
        }

        Err(anyhow!("Timed out waiting for receipt of transaction {}", tx_hash))
    }

    async fn call_raw(&self, method: &str, params: Vec<Value>) -> Result<Option<Value>> {
        let request = JsonRpcRequest {
            jsonrpc: "2.0",
            method,
            params,
            id: 1,
        };

        let response = self
            .http
            .post(&self.url)
            .json(&request)
            .send()
            .await
            .context("Failed to call JSON-RPC")?;

        let payload: JsonRpcResponse = response
            .error_for_status()
            .context("JSON-RPC request returned error status")?
            .json()
            .await
            .context("Failed to parse JSON-RPC response")?;

        if let Some(error) = payload.error {
            return Err(anyhow!(
                "JSON-RPC call {} failed (code {}): {}",
                method,
                error.code,
                error.message
            ));
        }

        Ok(payload.result)
    }
}

#[derive(Debug)]
pub struct TransactionReceipt {
    pub transaction_hash: String,
    pub block_number: u64,
    pub success: bool,
}

#[derive(Serialize)]
struct JsonRpcRequest<'a> {
    jsonrpc: &'static str,
    method: &'a str,
    params: Vec<Value>,
    id: u64,
}

#[derive(Deserialize)]
struct JsonRpcResponse {
    result: Option<Value>,
    error: Option<JsonRpcError>,
}

#[derive(Debug, Deserialize)]
struct JsonRpcError {
    code: i64,
    message: String,
    #[serde(default)]
    data: Option<Value>,
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
struct RpcTransactionReceipt {
    status: Option<String>,
    block_number: Option<String>,
    transaction_hash: String,
}

fn parse_hex_quantity(value: &str) -> Result<u64> {
    let trimmed = value.trim_start_matches("0x");
    if trimmed.is_empty() {
        return Ok(0);
    }
    u64::from_str_radix(trimmed, 16).map_err(|e| anyhow!("Failed to parse hex quantity {value}: {e}"))
}

fn parse_u256(value: &str) -> Result<U256> {
    let trimmed = value.trim_start_matches("0x");
    if trimmed.is_empty() {
        return Ok(U256::ZERO);
    }

    let padded = if trimmed.len() % 2 == 0 {
        trimmed.to_string()
    } else {
        format!("0{trimmed}")
    };
    let bytes = hex::decode(padded)?;
    Ok(U256::from_be_slice(&bytes))
}
