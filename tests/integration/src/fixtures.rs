use std::{
    sync::{Arc, Mutex, Once},
    thread,
    time::Duration,
};

use alloy::{
    primitives::U256,
    providers::{
        ProviderBuilder,
        fillers::{BlobGasFiller, CachedNonceManager, ChainIdFiller, GasFiller, NonceFiller},
    },
    signers::local::PrivateKeySigner,
};
use anyhow::{Context, Result};
use blokli_client::{BlokliClient, BlokliClientConfig, api::BlokliTransactionClient};
use hex::FromHex;
use hopli_lib::utils::ContractInstances;
use hopr_chain_connector::{BasicPayloadGenerator, PayloadGenerator};
use hopr_chain_types::ContractAddresses;
use hopr_crypto_types::keypairs::{ChainKeypair, Keypair};
use hopr_internal_types::announcement::{AnnouncementData, KeyBinding};
use hopr_primitive_types::prelude::HoprBalance;
use libc::atexit;
use rand::seq::IndexedRandom;
use rstest::fixture;
use tokio::sync::OnceCell;
use tracing::{debug, info};

use crate::{
    anvil::AnvilAccount, config::TestConfig, docker::DockerEnvironment, rpc::RpcClient, transaction::TransactionBuilder,
};

const STACK_STARTUP_WAIT: Duration = Duration::from_secs(8);

#[derive(Clone)]
pub struct IntegrationFixture {
    inner: Arc<IntegrationFixtureInner>,
}

struct IntegrationFixtureInner {
    config: Arc<TestConfig>,
    accounts: Vec<AnvilAccount>,
    client: BlokliClient,
    rpc: RpcClient,
    docker: Mutex<Option<DockerEnvironment>>,
    contract_addrs: ContractAddresses,
}

impl IntegrationFixture {
    pub fn config(&self) -> &TestConfig {
        &self.inner.config
    }

    pub fn accounts(&self) -> &[AnvilAccount] {
        &self.inner.accounts
    }

    pub fn sample_accounts<const N: usize>(&self) -> [&AnvilAccount; N] {
        assert!(self.inner.accounts.len() >= N, "not enough accounts available");

        let selected = self.inner.accounts.choose_multiple(&mut rand::rng(), N);
        let mut iter = selected.into_iter();
        let result: [&AnvilAccount; N] = std::array::from_fn(|_| iter.next().unwrap());
        result
    }

    pub fn client(&self) -> &BlokliClient {
        &self.inner.client
    }

    pub fn rpc(&self) -> &RpcClient {
        &self.inner.rpc
    }

    pub fn contract_addresses(&self) -> &ContractAddresses {
        &self.inner.contract_addrs
    }

    pub fn transaction_builder(&self, private_key: &str) -> Result<TransactionBuilder> {
        TransactionBuilder::new(private_key)
    }

    pub async fn submit_tx(&self, raw_tx: &str) -> Result<[u8; 32]> {
        let signed_bytes =
            Vec::from_hex(raw_tx.trim_start_matches("0x")).context("failed to decode raw transaction payload")?;

        let receipt = self
            .client()
            .submit_transaction(&signed_bytes)
            .await
            .context("blokli client failed to submit transaction")?;

        let tx_hash = format!("0x{}", hex::encode(receipt));

        info!(
            hash = %tx_hash,
            "transaction submitted via blokli"
        );
        Ok(receipt)
    }

    pub async fn submit_and_track_tx(&self, raw_tx: &str) -> Result<String> {
        let signed_bytes =
            Vec::from_hex(raw_tx.trim_start_matches("0x")).context("failed to decode raw transaction payload")?;

        let tx_id = self
            .client()
            .submit_and_track_transaction(&signed_bytes)
            .await
            .context("blokli client failed to submit transaction")?;

        info!(tx_id, "transaction submitted and tracked via blokli");
        Ok(tx_id)
    }

    pub async fn submit_and_confirm_tx(&self, raw_tx: &str, confirmations: usize) -> Result<[u8; 32]> {
        let signed_bytes =
            Vec::from_hex(raw_tx.trim_start_matches("0x")).context("failed to decode raw transaction payload")?;

        let receipt = self
            .client()
            .submit_and_confirm_transaction(&signed_bytes, confirmations)
            .await
            .context("blokli client failed to submit transaction")?;
        let tx_hash = format!("0x{}", hex::encode(receipt));

        info!(
            hash = %tx_hash,
            "transaction submitted and confirmed via blokli"
        );
        Ok(receipt)
    }

    pub async fn deploy_safe(&self, owner: &AnvilAccount, amount: u64) -> Result<[u8; 32]> {
        let payload = self.safe_deployment_payload(owner, amount).await?;
        self.submit_and_confirm_tx(&hex::encode(&payload), 1).await
    }

    pub async fn announce_account(&self, account: &AnvilAccount) -> Result<()> {
        let payload_generator = BasicPayloadGenerator::new(account.hopr_address(), *self.contract_addresses());
        let keybinding = KeyBinding::new(account.hopr_address(), &account.offchain_key_pair());
        let binding_fee = HoprBalance::zero();
        let payload = payload_generator.announce(AnnouncementData::new(keybinding, None)?, binding_fee)?;

        let tx_input = payload
            .input
            .input
            .context("transaction input missing from announcement payload")?;
        let raw_tx = hex::encode(&tx_input);

        self.submit_and_confirm_tx(&raw_tx, 1).await.map(|_| ())
    }

    pub async fn open_channel(&self, from: &AnvilAccount, to: &AnvilAccount, amount: HoprBalance) -> Result<[u8; 32]> {
        let payload_generator = BasicPayloadGenerator::new(from.hopr_address(), *self.contract_addresses());

        let payload = payload_generator.fund_channel(to.hopr_address(), amount)?;
        let tx_input = payload
            .input
            .input
            .context("transaction input missing from fund channel payload")?;

        self.submit_and_confirm_tx(&hex::encode(&tx_input), 1).await
    }

    async fn safe_deployment_payload(&self, owner: &AnvilAccount, amount: u64) -> Result<Vec<u8>> {
        let nonce = self.rpc().transaction_count(owner.address.as_ref()).await?;

        let contract_addresses = self.contract_addresses();
        let payload = hopli_lib::payloads::edge_node_deploy_safe_module_and_maybe_include_node(
            contract_addresses.node_stake_factory,
            contract_addresses.token,
            contract_addresses.channels,
            U256::from(nonce),
            U256::from(amount),
            vec![owner.alloy_address()],
            true,
        )?;

        let tx_input = payload
            .input
            .input
            .context("transaction input missing from safe deployment payload")?;

        let payload_bytes: Vec<u8> = tx_input.into();
        Ok(payload_bytes)
    }

    fn teardown(&self) {
        self.inner.teardown();
    }
}

impl IntegrationFixtureInner {
    fn teardown(&self) {
        let mut docker_guard = self
            .docker
            .lock()
            .expect("integration docker environment mutex poisoned");

        if docker_guard.is_some() {
            info!("tearing down docker stack for integration tests");
        }

        docker_guard.take();
    }
}

pub async fn build_integration_fixture() -> Result<IntegrationFixture> {
    let config: Arc<TestConfig> = Arc::new(TestConfig::load()?);
    let mut docker = DockerEnvironment::new(config.clone());

    docker.ensure_image_available()?;
    docker.compose_up()?;
    info!(
        seconds = STACK_STARTUP_WAIT.as_secs(),
        "waiting for integration stack to stabilize"
    );
    thread::sleep(STACK_STARTUP_WAIT);
    let accounts = docker.fetch_anvil_accounts()?;

    debug!("setting up rpc client");
    let rpc = RpcClient::new(config.rpc_url.as_str(), config.http_timeout)?;

    debug!("setting up blokli client");
    let client = BlokliClient::new(config.bloklid_url.clone(), BlokliClientConfig::default());

    let deployer: ChainKeypair = accounts[0].chain_key_pair();
    let wallet = PrivateKeySigner::from_slice(deployer.secret().as_ref()).expect("failed to construct wallet");

    // Build default JSON RPC provider
    let provider = ProviderBuilder::new()
        .disable_recommended_fillers()
        .filler(ChainIdFiller::default())
        .filler(NonceFiller::new(CachedNonceManager::default()))
        .filler(GasFiller)
        .filler(BlobGasFiller)
        .wallet(wallet)
        .connect_http(config.rpc_url.clone());

    let contract_instances = ContractInstances::deploy_for_testing(provider, &deployer).await?;
    info!("deployed hopr contracts for testing");

    let contract_addresses = ContractAddresses {
        announcements: *contract_instances.announcements.address(),
        channels: *contract_instances.channels.address(),
        module_implementation: *contract_instances.module_implementation.address(),
        node_safe_migration: *contract_instances.node_safe_migration.address(),
        node_safe_registry: *contract_instances.safe_registry.address(),
        node_stake_factory: *contract_instances.stake_factory.address(),
        ticket_price_oracle: *contract_instances.price_oracle.address(),
        token: *contract_instances.token.address(),
        winning_probability_oracle: *contract_instances.win_prob_oracle.address(),
    };

    let fixture = IntegrationFixture {
        inner: Arc::new(IntegrationFixtureInner {
            config,
            accounts,
            client,
            rpc,
            docker: Mutex::new(Some(docker)),
            contract_addrs: contract_addresses,
        }),
    };

    register_shutdown_hook();

    Ok(fixture)
}

static SHARED_FIXTURE: OnceCell<IntegrationFixture> = OnceCell::const_new();
static SHUTDOWN_HOOK: Once = Once::new();

extern "C" fn teardown_shared_fixture() {
    if let Some(fixture) = SHARED_FIXTURE.get() {
        fixture.teardown();
    }
}

fn register_shutdown_hook() {
    SHUTDOWN_HOOK.call_once(|| unsafe {
        let result = atexit(teardown_shared_fixture);
        if result != 0 {
            panic!("failed to register integration fixture teardown hook");
        }
    });
}

#[fixture]
pub async fn integration_fixture() -> IntegrationFixture {
    SHARED_FIXTURE
        .get_or_try_init(|| async { build_integration_fixture().await })
        .await
        .expect("failed to set up integration fixture")
        .clone()
}
