use std::{
    sync::{Arc, Mutex, Once},
    time::Duration,
};

use alloy::{
    eips::Encodable2718,
    network::TransactionBuilder,
    primitives::{Address, U256, keccak256},
    providers::{
        ProviderBuilder,
        fillers::{BlobGasFiller, CachedNonceManager, ChainIdFiller, GasFiller, NonceFiller},
    },
    signers::local::PrivateKeySigner,
};
use anyhow::{Context, Result};
use blokli_client::{BlokliClient, BlokliClientConfig, api::BlokliTransactionClient};
use hopli_lib::{
    methods::transfer_or_mint_tokens,
    utils::{ContractInstances, a2h},
};
use hopr_bindings::hopr_token::HoprToken::HoprTokenInstance;
use hopr_chain_connector::{BasicPayloadGenerator, PayloadGenerator};
use hopr_chain_types::ContractAddresses;
use hopr_crypto_types::keypairs::{ChainKeypair, Keypair};
use hopr_internal_types::announcement::{AnnouncementData, KeyBinding};
use hopr_primitive_types::prelude::HoprBalance;
use libc::atexit;
use rand::seq::IndexedRandom;
use rstest::fixture;
use tokio::sync::OnceCell;
use tracing::info;

use crate::{
    anvil::AnvilAccount, config::TestConfig, docker::DockerEnvironment, rpc::RpcClient,
    transaction::TransactionBuilder as TestTransactionBuilder,
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

    pub async fn build_raw_tx(
        &self,
        value: U256,
        sender: &AnvilAccount,
        recipient: &AnvilAccount,
        nonce: u64,
    ) -> Result<String> {
        let tx_builder = TestTransactionBuilder::new(&sender.private_key)?;
        tx_builder
            .build_eip1559_transaction_hex(
                self.rpc().chain_id().await?,
                nonce,
                &recipient.address,
                value,
                self.config().max_fee_per_gas,
                self.config().max_priority_fee_per_gas,
                self.config().gas_limit,
            )
            .await
    }

    pub async fn submit_tx(&self, signed_bytes: &[u8]) -> Result<[u8; 32]> {
        let receipt = self
            .client()
            .submit_transaction(signed_bytes)
            .await
            .context("blokli client failed to submit transaction")?;
        Ok(receipt)
    }

    pub async fn submit_and_track_tx(&self, signed_bytes: &[u8]) -> Result<String> {
        let tx_id = self
            .client()
            .submit_and_track_transaction(signed_bytes)
            .await
            .context("blokli client failed to submit transaction")?;
        Ok(tx_id)
    }

    pub async fn submit_and_confirm_tx(&self, signed_bytes: &[u8], confirmations: usize) -> Result<[u8; 32]> {
        let receipt = self
            .client()
            .submit_and_confirm_transaction(signed_bytes, confirmations)
            .await
            .context("blokli client failed to submit transaction")?;
        Ok(receipt)
    }

    pub async fn update_winn_prob(
        &self,
        owner: &AnvilAccount,
        contract_address: Address,
        new_win_prob: f64,
    ) -> Result<[u8; 32]> {
        let nonce = self.rpc().transaction_count(owner.address.as_ref()).await?;

        let payload = hopli_lib::payloads::set_winning_probability(contract_address, new_win_prob)?
            .gas_limit(self.config().gas_limit)
            .max_fee_per_gas(self.config().max_fee_per_gas)
            .max_priority_fee_per_gas(self.config().max_priority_fee_per_gas)
            .with_chain_id(self.rpc().chain_id().await?)
            .nonce(nonce);

        let payload_bytes = payload.build(&owner.as_wallet()).await?.encoded_2718();

        self.submit_and_confirm_tx(&payload_bytes, self.config().tx_confirmations)
            .await
    }

    pub async fn deploy_safe(&self, owner: &AnvilAccount, amount: u64) -> Result<[u8; 32]> {
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
        )?
        .gas_limit(self.config().gas_limit)
        .max_fee_per_gas(self.config().max_fee_per_gas)
        .max_priority_fee_per_gas(self.config().max_priority_fee_per_gas)
        .with_chain_id(self.rpc().chain_id().await?)
        .nonce(nonce);

        let payload_bytes = payload.build(&owner.as_wallet()).await?.encoded_2718();

        self.submit_and_confirm_tx(&payload_bytes, self.config().tx_confirmations)
            .await
    }

    pub async fn announce_account(&self, account: &AnvilAccount) -> Result<[u8; 32]> {
        let nonce = self.rpc().transaction_count(account.address.as_ref()).await?;

        let payload_generator = BasicPayloadGenerator::new(account.hopr_address(), *self.contract_addresses());
        let keybinding = KeyBinding::new(account.hopr_address(), &account.offchain_key_pair());
        let binding_fee = HoprBalance::zero();
        let payload = payload_generator
            .announce(AnnouncementData::new(keybinding, None)?, binding_fee)?
            .gas_limit(self.config().gas_limit)
            .max_fee_per_gas(self.config().max_fee_per_gas)
            .max_priority_fee_per_gas(self.config().max_priority_fee_per_gas)
            .with_chain_id(self.rpc().chain_id().await?)
            .nonce(nonce);

        let payload_bytes = payload.build(&account.as_wallet()).await?.encoded_2718();

        self.submit_and_confirm_tx(&payload_bytes, self.config().tx_confirmations)
            .await
    }

    pub async fn open_channel(&self, from: &AnvilAccount, to: &AnvilAccount, amount: HoprBalance) -> Result<[u8; 32]> {
        let nonce = self.rpc().transaction_count(from.address.as_ref()).await?;

        let payload_generator = BasicPayloadGenerator::new(from.hopr_address(), *self.contract_addresses()); // Could work. Could fail. SafePayloadGenerator maybe

        let payload = payload_generator
            .fund_channel(to.hopr_address(), amount)?
            .gas_limit(self.config().gas_limit)
            .max_fee_per_gas(self.config().max_fee_per_gas)
            .max_priority_fee_per_gas(self.config().max_priority_fee_per_gas)
            .with_chain_id(self.rpc().chain_id().await?)
            .nonce(nonce);

        let payload_bytes = payload.build(&from.as_wallet()).await?.encoded_2718();

        self.submit_and_confirm_tx(&payload_bytes, self.config().tx_confirmations)
            .await
    }

    pub async fn initiate_outgoing_channel_closure(&self, from: &AnvilAccount, to: &AnvilAccount) -> Result<[u8; 32]> {
        let nonce = self.rpc().transaction_count(from.address.as_ref()).await?;

        let payload_generator = BasicPayloadGenerator::new(from.hopr_address(), *self.contract_addresses()); // Could work. Could fail. SafePayloadGenerator maybe

        let payload = payload_generator
            .initiate_outgoing_channel_closure(to.hopr_address())?
            .gas_limit(self.config().gas_limit)
            .max_fee_per_gas(self.config().max_fee_per_gas)
            .max_priority_fee_per_gas(self.config().max_priority_fee_per_gas)
            .with_chain_id(self.rpc().chain_id().await?)
            .nonce(nonce);

        let payload_bytes = payload.build(&from.as_wallet()).await?.encoded_2718();

        self.submit_and_confirm_tx(&payload_bytes, self.config().tx_confirmations)
            .await
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
    tokio::time::sleep(STACK_STARTUP_WAIT).await;
    let accounts = docker.fetch_anvil_accounts()?;

    let rpc = RpcClient::new(config.rpc_url.as_str(), config.http_timeout)?;

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

    let contract_instances = ContractInstances::deploy_for_testing(provider, &deployer)
        .await
        .expect("failed to deploy hopr contracts for testing");

    info!("deployed hopr contracts for testing");

    let contract_addresses = contract_instances.get_contract_addresses();

    info!("deployed contract addresses: {:?}", contract_addresses);

    // Mint HOPR tokens
    let encoded_minter_role = keccak256(b"MINTER_ROLE");
    contract_instances
        .token
        .grantRole(encoded_minter_role, a2h(deployer.public().to_address()))
        .send()
        .await?
        .watch()
        .await?;

    let all_addresses: Vec<Address> = accounts.iter().map(|acc| acc.alloy_address()).collect();

    let hopr_token = HoprTokenInstance::new(
        *contract_instances.token.address(),
        Arc::new(contract_instances.token.provider().clone()),
    );

    let total_transferred_amount = transfer_or_mint_tokens(
        hopr_token,
        all_addresses,
        vec![U256::from(1_000_000_000_000u64); accounts.len()],
    )
    .await?;

    info!(
        total=?total_transferred_amount,
        "minted and distributed HOPR tokens to test accounts",
    );

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

    tokio::time::sleep(Duration::from_secs(2)).await;

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
