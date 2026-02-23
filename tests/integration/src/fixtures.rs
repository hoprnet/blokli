use std::{
    str::FromStr,
    sync::{Arc, Mutex, Once},
    time::Duration,
};

use anyhow::{Context, Result};
use blokli_client::{
    BlokliClient, BlokliClientConfig,
    api::{AccountSelector, BlokliQueryClient, BlokliTransactionClient, SafeSelector, types::Safe},
};
use hopli_lib::{
    methods::transfer_or_mint_tokens,
    utils::{ContractInstances, a2h},
};
use hopr_bindings::{
    exports::alloy::{
        primitives::{Address, U256, keccak256},
        providers::{
            ProviderBuilder,
            fillers::{BlobGasFiller, CachedNonceManager, ChainIdFiller, GasFiller, NonceFiller},
        },
        signers::local::PrivateKeySigner,
    },
    hopr_token::HoprToken::HoprTokenInstance,
};
use hopr_chain_connector::{BasicPayloadGenerator, PayloadGenerator, SafePayloadGenerator};
use hopr_chain_types::{ContractAddresses, prelude::SignableTransaction};
use hopr_crypto_types::keypairs::{ChainKeypair, Keypair};
use hopr_internal_types::{Multiaddr, announcement::AnnouncementData};
use hopr_primitive_types::{
    prelude::{Address as HoprAddress, HoprBalance},
    traits::IntoEndian,
};
use libc::atexit;
use rand::seq::IndexedRandom;
use rstest::fixture;
use tokio::{sync::OnceCell, time::sleep};
use tracing::{debug, info, warn};

use crate::{
    anvil::AnvilAccount, config::TestConfig, constants::STACK_STARTUP_WAIT, docker::DockerEnvironment, rpc::RpcClient,
    transaction::TransactionBuilder as TestTransactionBuilder,
};

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

const DEFAULT_MAX_FEE_PER_GAS: u128 = 2_000_000_000;
const DEFAULT_MAX_PRIORITY_FEE_PER_GAS: u128 = 1_000_000_000;
const DEFAULT_GAS_LIMIT: u64 = 10_000_000;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Eip1559GasParameters {
    pub max_fee_per_gas: u128,
    pub max_priority_fee_per_gas: u128,
    pub gas_limit: u64,
}

impl Default for Eip1559GasParameters {
    fn default() -> Self {
        Self {
            max_fee_per_gas: DEFAULT_MAX_FEE_PER_GAS,
            max_priority_fee_per_gas: DEFAULT_MAX_PRIORITY_FEE_PER_GAS,
            gas_limit: DEFAULT_GAS_LIMIT,
        }
    }
}

// Accessor methods
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
}

// Transaction related helpers
impl IntegrationFixture {
    /// Builds a raw EIP-1559 transaction hex string.
    pub async fn build_raw_tx(
        &self,
        value: U256,
        sender: &AnvilAccount,
        recipient: &AnvilAccount,
        nonce: u64,
    ) -> Result<String> {
        self.build_raw_tx_with_gas(value, sender, recipient, nonce, None).await
    }

    pub async fn build_raw_tx_with_gas(
        &self,
        value: U256,
        sender: &AnvilAccount,
        recipient: &AnvilAccount,
        nonce: u64,
        gas: Option<Eip1559GasParameters>,
    ) -> Result<String> {
        let gas = self.resolve_eip1559_gas_parameters(gas).await;
        let tx_builder = TestTransactionBuilder::new(&sender.keypair)?;
        tx_builder
            .build_eip1559_transaction_hex(
                self.rpc().chain_id().await?,
                nonce,
                &recipient.address.to_string(),
                value,
                gas.max_fee_per_gas,
                gas.max_priority_fee_per_gas,
                gas.gas_limit,
            )
            .await
    }

    async fn resolve_eip1559_gas_parameters(&self, gas: Option<Eip1559GasParameters>) -> Eip1559GasParameters {
        if let Some(gas) = gas {
            return gas;
        }

        let chain_info = match self.client().query_chain_info().await {
            Ok(chain_info) => chain_info,
            Err(error) => {
                warn!(
                    %error,
                    "failed to fetch chainInfo gas parameters, using integration defaults"
                );
                return Eip1559GasParameters::default();
            }
        };

        let max_fee_per_gas = chain_info
            .max_fee_per_gas
            .as_deref()
            .and_then(|value| value.parse::<u128>().ok());
        let max_priority_fee_per_gas = chain_info
            .max_priority_fee_per_gas
            .as_deref()
            .and_then(|value| value.parse::<u128>().ok());

        match (max_fee_per_gas, max_priority_fee_per_gas) {
            (Some(max_fee_per_gas), Some(max_priority_fee_per_gas)) => Eip1559GasParameters {
                max_fee_per_gas,
                max_priority_fee_per_gas,
                ..Eip1559GasParameters::default()
            },
            _ => {
                warn!("chainInfo gas parameters unavailable, using integration defaults");
                Eip1559GasParameters::default()
            }
        }
    }

    /// Submits the signed transaction blindly.
    pub async fn submit_tx(&self, signed_bytes: &[u8]) -> Result<[u8; 32]> {
        let receipt = self
            .client()
            .submit_transaction(signed_bytes)
            .await
            .context("blokli client failed to submit transaction")?;
        Ok(receipt)
    }

    /// Submits the signed transaction and returns the tracking id.
    pub async fn submit_and_track_tx(&self, signed_bytes: &[u8]) -> Result<String> {
        let tx_id = self
            .client()
            .submit_and_track_transaction(signed_bytes)
            .await
            .context("blokli client failed to submit transaction")?;
        Ok(tx_id)
    }

    /// Submits the signed transaction and waits for the specified number of confirmations.
    pub async fn submit_and_confirm_tx(&self, signed_bytes: &[u8], confirmations: usize) -> Result<[u8; 32]> {
        let receipt = self
            .client()
            .submit_and_confirm_transaction(signed_bytes, confirmations)
            .await
            .context("blokli client failed to submit transaction")?;
        Ok(receipt)
    }
}

// Safe related helpers
impl IntegrationFixture {
    /// Deploys a safe for the specified owner.
    async fn deploy_safe(&self, owner: &AnvilAccount, amount: HoprBalance) -> Result<[u8; 32]> {
        let nonce = self.rpc().transaction_count(&owner.address).await?;

        let contract_addresses = self.contract_addresses();
        let payload = hopli_lib::payloads::edge_node_deploy_safe_module_and_maybe_include_node(
            contract_addresses.node_stake_factory,
            contract_addresses.token,
            contract_addresses.channels,
            U256::from(nonce),
            U256::from_be_bytes(amount.to_be_bytes()),
            vec![owner.to_alloy_address()],
            true,
        )?;

        let payload_bytes = payload
            .sign_and_encode_to_eip2718(nonce, self.rpc().chain_id().await?, None, &owner.keypair)
            .await?;

        self.submit_and_confirm_tx(&payload_bytes, self.config().tx_confirmations)
            .await
    }

    /// Deploys and returns a safe for the specified owner if not already deployed, otherwise retrieves the existing
    /// safe.
    pub async fn deploy_or_get_safe(&self, owner: &AnvilAccount, amount: HoprBalance) -> Result<Safe> {
        let maybe_safe = self
            .client()
            .query_safe(SafeSelector::ChainKey(owner.to_alloy_address().into()))
            .await?;

        match maybe_safe {
            Some(s) => Ok(s),
            None => {
                self.deploy_safe(owner, amount).await?;

                sleep(Duration::from_secs(5)).await;
                let safe = self
                    .client()
                    .query_safe(SafeSelector::ChainKey(owner.to_alloy_address().into()))
                    .await?
                    .expect("deployed safe for src not found");

                self.register_safe(owner, &safe.address).await?;

                Ok(safe)
            }
        }
    }

    pub async fn register_safe(&self, owner: &AnvilAccount, safe_address: &str) -> Result<[u8; 32]> {
        let nonce = self.rpc().transaction_count(&owner.address).await?;

        let payload_generator = BasicPayloadGenerator::new(owner.address, *self.contract_addresses());

        let payload = payload_generator.register_safe_by_node(HoprAddress::from_str(safe_address)?)?;

        let payload_bytes = payload
            .sign_and_encode_to_eip2718(nonce, self.rpc().chain_id().await?, None, &owner.keypair)
            .await?;

        self.submit_and_confirm_tx(&payload_bytes, self.config().tx_confirmations)
            .await
    }
}

// Account related helpers
impl IntegrationFixture {
    /// Announces the account using the specified safe module.
    pub async fn announce_account(&self, account: &AnvilAccount, module: &str) -> Result<[u8; 32]> {
        let nonce = self.rpc().transaction_count(&account.address).await?;

        let payload_generator = SafePayloadGenerator::new(
            &account.keypair,
            *self.contract_addresses(),
            HoprAddress::from_str(module)?,
        );
        let multiaddress: Multiaddr = "/ip4/127.0.0.1/udp/3001".parse().expect("multiaddress parsing failed");
        let binding_fee = "0.01 wxHOPR".parse().expect("failed parsing the binding fee");

        let payload = payload_generator.announce(
            AnnouncementData::new(account.keybinding(), Some(multiaddress))?,
            binding_fee,
        )?;

        let payload_bytes = payload
            .sign_and_encode_to_eip2718(nonce, self.rpc().chain_id().await?, None, &account.keypair)
            .await?;

        self.submit_and_confirm_tx(&payload_bytes, self.config().tx_confirmations)
            .await
    }

    /// Announces the account if not announced yet. If already announced, does nothing.
    pub async fn announce_or_get_account(&self, account: &AnvilAccount, module: &str) -> Result<()> {
        let maybe_account = self
            .client()
            .query_accounts(AccountSelector::Address(account.to_alloy_address().into()))
            .await?;

        match maybe_account.first() {
            Some(_) => Ok(()),
            None => {
                debug!("account not found, proceeding to announce");
                self.announce_account(account, module).await?;
                sleep(Duration::from_secs(10)).await;

                let src_account = self
                    .client()
                    .query_accounts(AccountSelector::Address(account.to_alloy_address().into()))
                    .await?;

                assert!(!src_account.is_empty(), "account not found after announcement");
                Ok(())
            }
        }
    }
}

// Ticket related helpers
impl IntegrationFixture {
    /// Updates the ticket's winning probability to `new_win_prob`.
    pub async fn update_winning_probability(
        &self,
        owner: &AnvilAccount,
        contract_address: Address,
        new_win_prob: f64,
    ) -> Result<[u8; 32]> {
        let nonce = self.rpc().transaction_count(&owner.address).await?;

        let payload = hopli_lib::payloads::set_winning_probability(contract_address, new_win_prob)?;

        let payload_bytes = payload
            .sign_and_encode_to_eip2718(nonce, self.rpc().chain_id().await?, None, &owner.keypair)
            .await?;

        self.submit_and_confirm_tx(&payload_bytes, self.config().tx_confirmations)
            .await
    }
}

// Token related helpers
impl IntegrationFixture {
    /// Approves the safe module to spend `amount` of wxHOPR on behalf of `owner`.
    pub async fn approve(&self, owner: &AnvilAccount, amount: HoprBalance, module: &str) -> Result<[u8; 32]> {
        let nonce = self.rpc().transaction_count(&owner.address).await?;
        let spender = HoprAddress::from_str(&self.contract_addresses().channels.to_string())
            .expect("Invalid spender address hex");

        let payload_generator = SafePayloadGenerator::new(
            &owner.keypair,
            *self.contract_addresses(),
            HoprAddress::from_str(module)?,
        );

        let payload = payload_generator.approve(spender, amount)?;

        let payload_bytes = payload
            .sign_and_encode_to_eip2718(nonce, self.rpc().chain_id().await?, None, &owner.keypair)
            .await?;

        self.submit_and_confirm_tx(&payload_bytes, self.config().tx_confirmations)
            .await
    }
}

impl IntegrationFixture {
    pub async fn deploy_safe_and_announce(&self, owner: &AnvilAccount, amount: HoprBalance) -> Result<Safe> {
        let safe = self.deploy_or_get_safe(owner, amount).await?;
        self.announce_or_get_account(owner, &safe.module_address).await?;
        Ok(safe)
    }
}

// Channel related helpers
impl IntegrationFixture {
    /// Opens a channel from `from` to `to` with the specified `amount`.
    pub async fn open_channel(
        &self,
        from: &AnvilAccount,
        to: &AnvilAccount,
        amount: HoprBalance,
        module: &str,
        nonce: Option<u64>,
    ) -> Result<[u8; 32]> {
        let nonce = self
            .rpc()
            .transaction_count(&from.address)
            .await?
            .max(nonce.unwrap_or(0));

        let payload_generator = SafePayloadGenerator::new(
            &from.keypair,
            *self.contract_addresses(),
            HoprAddress::from_str(module)?,
        );

        let payload = payload_generator.fund_channel(to.address, amount)?;

        let payload_bytes = payload
            .sign_and_encode_to_eip2718(nonce, self.rpc().chain_id().await?, None, &from.keypair)
            .await?;

        self.submit_and_confirm_tx(&payload_bytes, self.config().tx_confirmations)
            .await
    }

    /// Starts closing an outgoing channel from `from` to `to`.
    pub async fn initiate_outgoing_channel_closure(
        &self,
        from: &AnvilAccount,
        to: &AnvilAccount,
        module: &str,
    ) -> Result<[u8; 32]> {
        let nonce = self.rpc().transaction_count(&from.address).await?;

        let payload_generator = SafePayloadGenerator::new(
            &from.keypair,
            *self.contract_addresses(),
            HoprAddress::from_str(module)?,
        );

        let payload = payload_generator.initiate_outgoing_channel_closure(to.address)?;

        let payload_bytes = payload
            .sign_and_encode_to_eip2718(nonce, self.rpc().chain_id().await?, None, &from.keypair)
            .await?;

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

    let deployer: ChainKeypair = accounts[0].keypair.clone();
    let wallet = PrivateKeySigner::from_slice(deployer.secret().as_ref()).expect("failed to construct wallet");

    // Build default JSON RPC provider
    let provider = ProviderBuilder::new()
        .disable_recommended_fillers()
        .filler(ChainIdFiller::default())
        .filler(NonceFiller::new(CachedNonceManager::default()))
        .filler(GasFiller)
        .filler(BlobGasFiller::default())
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

    let all_addresses: Vec<Address> = accounts.iter().map(|acc| acc.to_alloy_address()).collect();

    let hopr_token = HoprTokenInstance::new(
        *contract_instances.token.address(),
        Arc::new(contract_instances.token.provider().clone()),
    );

    let total_transferred_amount = transfer_or_mint_tokens(
        hopr_token,
        all_addresses,
        vec![U256::from(100_000_000_000_000_000_000_000u128); accounts.len()],
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

    tokio::time::sleep(Duration::from_secs(15)).await;

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
