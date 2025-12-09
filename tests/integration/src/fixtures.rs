use std::sync::Arc;

use anyhow::{Context, Result};
use blokli_client::{BlokliClient, BlokliClientConfig, api::BlokliTransactionClient};
use hex::FromHex;
use rand::seq::IndexedRandom;
use rstest::fixture;
use tracing::info;

use crate::{
    config::TestConfig,
    docker::{AnvilAccount, DockerEnvironment},
    rpc::RpcClient,
    transaction::TransactionBuilder,
};

pub struct IntegrationFixture {
    config: Arc<TestConfig>,
    accounts: Vec<AnvilAccount>,
    pub client: BlokliClient,
    pub rpc: RpcClient,
    #[allow(dead_code)]
    docker: DockerEnvironment,
}

impl IntegrationFixture {
    pub fn config(&self) -> &TestConfig {
        &self.config
    }

    pub fn accounts(&self) -> &[AnvilAccount] {
        &self.accounts
    }

    pub fn sample_accounts<const N: usize>(&self) -> [&AnvilAccount; N] {
        assert!(self.accounts.len() >= N, "not enough accounts available");

        let selected = self.accounts.choose_multiple(&mut rand::rng(), N);
        let mut iter = selected.into_iter();
        let result: [&AnvilAccount; N] = std::array::from_fn(|_| iter.next().unwrap());
        result
    }

    pub fn transaction_builder(&self, private_key: &str) -> Result<TransactionBuilder> {
        TransactionBuilder::new(private_key)
    }

    pub async fn execute_transaction(&self, raw_tx: &str) -> Result<[u8; 32]> {
        let signed_bytes =
            Vec::from_hex(raw_tx.trim_start_matches("0x")).context("failed to decode raw transaction payload")?;

        let receipt = self
            .client
            .submit_transaction(&signed_bytes)
            .await
            .context("blokli client failed to submit transaction")?;

        let tx_hash = format!("0x{}", hex::encode(receipt));

        info!(hash = %tx_hash, confirmations = self.config.tx_confirmations, "transaction confirmed via blokli");
        Ok(receipt)
    }
}

pub fn build_integration_fixture() -> Result<IntegrationFixture> {
    let config = Arc::new(TestConfig::load()?);
    let mut docker = DockerEnvironment::new(config.clone());

    docker.ensure_image_available()?;
    docker.compose_up()?;
    let accounts = docker.fetch_anvil_accounts()?;

    let rpc = RpcClient::new(config.rpc_url.as_str(), config.http_timeout)?;
    let client = BlokliClient::new(config.bloklid_url.clone(), BlokliClientConfig::default());

    Ok(IntegrationFixture {
        config,
        accounts,
        client,
        rpc,
        docker,
    })
}

#[fixture]
pub fn integration_fixture() -> IntegrationFixture {
    build_integration_fixture().expect("failed to set up integration fixture")
}
