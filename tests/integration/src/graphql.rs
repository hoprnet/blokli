use std::{sync::Arc, time::Duration};

use anyhow::{Context, Result, anyhow};
use reqwest::Client;
use serde::Deserialize;
use serde_json::{Value, json};
use tokio::time::{Instant, sleep};
use tracing::{info, warn};

use crate::TestConfig;

const SEND_TRANSACTION_SYNC_MUTATION: &str = r#"
mutation($input: TransactionInput!, $confirmations: Int) {
    sendTransactionSync(input: $input, confirmations: $confirmations) {
        __typename
        ... on Transaction {
            id
            status
            transactionHash
        }
        ... on RpcError {
            code
            message
        }
        ... on ContractNotAllowedError {
            code
            message
        }
        ... on FunctionNotAllowedError {
            code
            message
        }
        ... on TimeoutError {
            code
            message
        }
    }
}
"#;

pub struct GraphqlClient {
    http: Client,
    base_url: String,
    ready_timeout: Duration,
    ready_interval: Duration,
}

impl GraphqlClient {
    pub fn new(config: &Arc<TestConfig>) -> Result<Self> {
        let http = Client::builder()
            .timeout(config.http_timeout)
            .build()
            .context("Failed to build HTTP client for GraphQL")?;

        Ok(Self {
            http,
            base_url: config.bloklid_url.clone(),
            ready_timeout: config.ready_timeout,
            ready_interval: config.ready_poll_interval,
        })
    }

    pub async fn wait_until_ready(&self) -> Result<ReadyzResponse> {
        let start = Instant::now();
        loop {
            match self.readyz().await {
                Ok(resp) if resp.status == "ready" => {
                    info!("bloklid reported ready status");
                    return Ok(resp);
                }
                Ok(resp) => {
                    warn!(
                        status = ?resp.status,
                        db = ?resp.checks.database.status,
                        rpc = ?resp.checks.rpc.status,
                        indexer = ?resp.checks.indexer.status,
                        "bloklid not ready yet"
                    );
                }
                Err(_error) => {
                    warn!("readyz probe failed");
                }
            }

            if start.elapsed() >= self.ready_timeout {
                break;
            }
            sleep(self.ready_interval).await;
        }

        Err(anyhow!("bloklid did not become ready within {:?}", self.ready_timeout))
    }

    pub async fn readyz(&self) -> Result<ReadyzResponse> {
        let response = self
            .http
            .get(format!("{}/readyz", self.base_url))
            .send()
            .await
            .context("Failed to call /readyz")?;

        let payload = response
            .error_for_status()
            .context("Readyz endpoint returned error status")?
            .json::<ReadyzResponse>()
            .await
            .context("Failed to parse /readyz response")?;

        Ok(payload)
    }

    pub async fn send_transaction_sync(&self, raw_tx: &str, confirmations: u32) -> Result<TransactionSubmission> {
        let body = json!({
            "query": SEND_TRANSACTION_SYNC_MUTATION,
            "variables": {
                "input": {
                    "rawTransaction": raw_tx,
                },
                "confirmations": confirmations as i32,
            },
        });

        let response_text = self
            .http
            .post(format!("{}/graphql", self.base_url))
            .json(&body)
            .send()
            .await
            .context("Failed to call GraphQL endpoint")?
            .error_for_status()
            .context("GraphQL endpoint returned error status")?
            .text()
            .await
            .context("Failed to read GraphQL response body")?;

        let payload: Value = serde_json::from_str(&response_text)?;

        if let Some(errors) = payload.get("errors") {
            return Err(anyhow!("GraphQL returned errors: {errors:?}"));
        }

        let result = payload
            .get("data")
            .and_then(|data| data.get("sendTransactionSync"))
            .context("GraphQL response missing sendTransactionSync field")?;

        let typename = result.get("__typename").and_then(Value::as_str).unwrap_or("Unknown");
        if typename != "Transaction" {
            return Err(anyhow!(
                "Unexpected GraphQL result type: {} (payload: {})",
                typename,
                response_text
            ));
        }

        let id = result
            .get("id")
            .and_then(Value::as_str)
            .context("Transaction response missing id")?;
        let status = result
            .get("status")
            .and_then(Value::as_str)
            .context("Transaction response missing status")?;
        let transaction_hash = result
            .get("transactionHash")
            .and_then(Value::as_str)
            .context("Transaction response missing hash")?;

        info!(id = %id, status = %status, hash = %transaction_hash, "submitted transaction via GraphQL");

        Ok(TransactionSubmission {
            id: id.to_string(),
            status: status.to_string(),
            transaction_hash: transaction_hash.to_string(),
        })
    }
}

#[derive(Debug, Clone)]
pub struct TransactionSubmission {
    pub id: String,
    pub status: String,
    pub transaction_hash: String,
}

#[derive(Debug, Clone, Deserialize)]
pub struct ReadyzResponse {
    pub status: String,
    #[serde(default)]
    pub checks: ReadyChecks,
}

#[derive(Debug, Clone, Default, Deserialize)]
pub struct ReadyChecks {
    #[serde(default)]
    pub database: CheckStatus,
    #[serde(default)]
    pub rpc: RpcStatus,
    #[serde(default)]
    pub indexer: IndexerStatus,
}

#[derive(Debug, Clone, Default, Deserialize)]
pub struct CheckStatus {
    pub status: Option<String>,
    pub error: Option<String>,
}

#[derive(Debug, Clone, Default, Deserialize)]
pub struct RpcStatus {
    pub status: Option<String>,
    pub error: Option<String>,
    #[serde(rename = "block_number")]
    pub block_number: Option<u64>,
}

#[derive(Debug, Clone, Default, Deserialize)]
pub struct IndexerStatus {
    pub status: Option<String>,
    pub error: Option<String>,
    #[serde(rename = "last_indexed_block")]
    pub last_indexed_block: Option<u64>,
    pub lag: Option<u64>,
}
