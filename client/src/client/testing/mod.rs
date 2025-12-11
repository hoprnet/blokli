use std::{ops::Div, sync::Arc, time::Duration};

use async_broadcast::TrySendError;
use futures::{Stream, StreamExt};
use indexmap::IndexMap;

use crate::{
    api::{types::*, *},
    errors::{BlokliClientError, ErrorKind, TrackingErrorKind},
};

fn serialize_as_empty_map<K, V, S>(_: &IndexMap<K, V>, serializer: S) -> std::result::Result<S::Ok, S::Error>
where
    K: serde::Serialize,
    V: serde::Serialize,
    S: serde::Serializer,
{
    serde::Serialize::serialize(&IndexMap::<K, V>::new(), serializer)
}

/// Represents a state for [`BlokliTestClient`].
#[derive(Clone, Debug, serde::Serialize, serde::Deserialize)]
pub struct BlokliTestState {
    /// Contains KeyID -> Account
    pub accounts: IndexMap<u32, Account>,
    /// Contains native balances for addresses.
    pub native_balances: IndexMap<String, NativeBalance>,
    /// Contains token balances for addresses.
    pub token_balances: IndexMap<String, HoprBalance>,
    /// Contains safe allowances for addresses.
    pub safe_allowances: IndexMap<String, SafeHoprAllowance>,
    /// Contains deployed Safes for addresses.
    pub deployed_safes: IndexMap<String, Safe>,
    /// Contains transaction counts for addresses.
    pub tx_counts: IndexMap<String, u64>,
    /// Contains ChannelId -> Channel.
    pub channels: IndexMap<String, Channel>,
    /// Contains chain info.
    pub chain_info: ChainInfo,
    /// Version of the Blokli server.
    pub version: String,
    /// Health of the Blokli server.
    pub health: String,
    /// Active transactions.
    ///
    /// This field is transient and not serialized.
    // Always serialize as empty, because the data are non-deterministic and do not make sense to compare.
    #[serde(serialize_with = "serialize_as_empty_map")]
    pub active_txs: IndexMap<TxId, Transaction>,
}

impl PartialEq for BlokliTestState {
    fn eq(&self, other: &Self) -> bool {
        // Skip active_txs because they are non-deterministic.
        self.accounts == other.accounts
            && self.deployed_safes == other.deployed_safes
            && self.native_balances == other.native_balances
            && self.token_balances == other.token_balances
            && self.safe_allowances == other.safe_allowances
            && self.tx_counts == other.tx_counts
            && self.channels == other.channels
            && self.chain_info == other.chain_info
            && self.version == other.version
            && self.health == other.health
    }
}

impl Default for BlokliTestState {
    fn default() -> Self {
        Self {
            accounts: Default::default(),
            native_balances: Default::default(),
            token_balances: Default::default(),
            safe_allowances: Default::default(),
            deployed_safes: Default::default(),
            tx_counts: Default::default(),
            channels: Default::default(),
            chain_info: ChainInfo {
                channel_closure_grace_period: Uint64("300".into()),
                channel_dst: Some("0000000000000000000000000000000000000000000000000000000000000000".into()),
                block_number: 1,
                chain_id: 100,
                ledger_dst: Some("0000000000000000000000000000000000000000000000000000000000000000".into()),
                min_ticket_winning_probability: 1.0,
                key_binding_fee: TokenValueString("0.01 wxHOPR".into()),
                safe_registry_dst: Some("0000000000000000000000000000000000000000000000000000000000000000".into()),
                ticket_price: TokenValueString("1 wxHOPR".into()),
                network: "rotsee".into(),
                contract_addresses: ContractAddressMap(
                    r#"
                {
                    "announcements": "0xf1c143B1bA20C7606d56aA2FA94502D25744b982",
                    "channels": "0x77C9414043d27fdC98A6A2d73fc77b9b383092a7",
                    "module_implementation": "0x32863c4974fBb6253E338a0cb70C382DCeD2eFCb",
                    "node_safe_registry": "0x4F7C7dE3BA2B29ED8B2448dF2213cA43f94E45c0",
                    "node_stake_factory": "0x791d190b2c95397F4BcE7bD8032FD67dCEA7a5F2",
                    "node_safe_migration": "0x0000000000000000000000000000000000000000",
                    "token": "0xD4fdec44DB9D44B8f2b6d529620f9C0C7066A2c1",
                    "ticket_price_oracle": "0x442df1d946303fB088C9377eefdaeA84146DA0A6",
                    "winning_probability_oracle": "0xC15675d4CCa538D91a91a8D3EcFBB8499C3B0471"
                }"#
                    .into(),
                ),
            },
            version: "1".to_string(),
            health: "OK".to_string(),
            active_txs: Default::default(),
        }
    }
}

impl BlokliTestState {
    /// Convenience method to return a reference to an [`Account`] with a given [`ChainAddress`].
    pub fn get_account(&self, chain_key: &ChainAddress) -> Option<&Account> {
        self.accounts
            .values()
            .find(|acc| acc.chain_key == hex::encode(chain_key))
    }

    /// Convenience method to return a mutable reference to an [`Account`] with a given [`ChainAddress`].
    pub fn get_account_mut(&mut self, chain_key: &ChainAddress) -> Option<&mut Account> {
        self.accounts
            .values_mut()
            .find(|acc| acc.chain_key == hex::encode(chain_key))
    }

    /// Convenience method to return a reference to a [`Channel`] with a given [` ChannelId `].
    pub fn get_channel_by_id(&self, channel_id: &ChannelId) -> Option<&Channel> {
        self.channels.get(&hex::encode(channel_id))
    }

    /// Convenience method to return a mutable reference to a [`Channel`] with a given [` ChannelId `].
    pub fn get_channel_by_id_mut(&mut self, channel_id: &ChannelId) -> Option<&mut Channel> {
        self.channels.get_mut(&hex::encode(channel_id))
    }

    /// Convenience method to return a reference to Safe balance corresponding to the given [`ChainAddress`] of the
    /// [`Account`].
    pub fn get_account_safe_token_balance(&self, chain_key: &ChainAddress) -> Option<&HoprBalance> {
        let account = self.get_account(chain_key)?;
        self.token_balances.get(account.safe_address.as_ref()?)
    }

    /// Convenience method to return a mutable reference to Safe balance corresponding to the given [`ChainAddress`] of
    /// the [`Account`].
    pub fn get_account_safe_token_balance_mut(&mut self, chain_key: &ChainAddress) -> Option<&mut HoprBalance> {
        let account = self.get_account(chain_key).and_then(|a| a.safe_address.clone())?;
        self.token_balances.get_mut(&account)
    }

    /// Convenience method to return a reference to Safe allowance corresponding to the given [`ChainAddress`] of the
    /// [`Account`].
    pub fn get_account_safe_allowance(&self, chain_key: &ChainAddress) -> Option<&SafeHoprAllowance> {
        let account = self.get_account(chain_key)?;
        self.safe_allowances.get(account.safe_address.as_ref()?)
    }

    /// Convenience method to return a mutable reference to Safe allowance corresponding to the given [`ChainAddress`]
    /// of the [`Account`].
    pub fn get_account_safe_allowance_mut(&mut self, chain_key: &ChainAddress) -> Option<&mut SafeHoprAllowance> {
        let account = self.get_account(chain_key).and_then(|a| a.safe_address.clone())?;
        self.safe_allowances.get_mut(&account)
    }

    /// Convenience method to return a reference to an [`Safe`] with the given owner's [`ChainAddress`].
    pub fn get_safe_by_owner(&self, owner: &ChainAddress) -> Option<&Safe> {
        self.deployed_safes
            .values()
            .find(|safe| safe.chain_key == hex::encode(owner))
    }

    /// Convenience method to return a mutable reference to an [`Safe`] with the given owner's [`ChainAddress`].
    pub fn get_safe_by_owner_mut(&mut self, owner: &ChainAddress) -> Option<&mut Safe> {
        self.deployed_safes
            .values_mut()
            .find(|safe| safe.chain_key == hex::encode(owner))
    }
}

/// Mutator for the [`BlokliTestState`] based on signed transactions.
pub trait BlokliTestStateMutator {
    /// Updates the state given the signed transaction.
    ///
    /// [`BlokliTestClient`] makes several consistency checks on the updates.
    /// For example, all mutations that remove anything from the state are not allowed.
    ///
    /// For arbitrary state updates via the client, see [`BlokliTestClient::update_state`].
    fn update_state(&self, signed_tx: &[u8], state: &mut BlokliTestState) -> Result<()>;
}

/// No-op state mutator.
///
/// Useful for static tests.
#[derive(Clone, Debug, Default)]
pub struct NopStateMutator;

impl BlokliTestStateMutator for NopStateMutator {
    fn update_state(&self, _: &[u8], _: &mut BlokliTestState) -> Result<()> {
        Ok(())
    }
}

type AccountEvents = (
    async_broadcast::Sender<Account>,
    async_broadcast::InactiveReceiver<Account>,
);

type GraphEvents = (
    async_broadcast::Sender<(Account, Channel, Account)>,
    async_broadcast::InactiveReceiver<(Account, Channel, Account)>,
);

type TicketParamEvents = (
    async_broadcast::Sender<TicketParameters>,
    async_broadcast::InactiveReceiver<TicketParameters>,
);

type SafeDeployEvents = (async_broadcast::Sender<Safe>, async_broadcast::InactiveReceiver<Safe>);

/// Represents a snapshot of the [`BlokliTestState`] inside a [`BlokliTestClient`].
#[derive(Clone)]
pub struct BlokliTestStateSnapshot {
    state: Arc<parking_lot::RwLock<BlokliTestState>>,
    snapshot: BlokliTestState,
}

impl BlokliTestStateSnapshot {
    /// Refreshes the snapshot by fetching it from the [`BlokliTestClient`].
    pub fn refresh(mut self) -> Self {
        {
            let state = self.state.read();
            self.snapshot = state.clone();
        }
        self
    }
}

impl AsRef<BlokliTestState> for BlokliTestStateSnapshot {
    fn as_ref(&self) -> &BlokliTestState {
        &self.snapshot
    }
}

impl std::ops::Deref for BlokliTestStateSnapshot {
    type Target = BlokliTestState;

    fn deref(&self) -> &Self::Target {
        &self.snapshot
    }
}

/// Blokli client for testing purposes.
///
/// This is useful to simulate Blokli server in unit tests.
/// The test client gets an initial [state](BlokliTestState).
///
/// Later transactions done using the client can [mutate](BlokliTestStateMutator) the state and
/// changes are propagated to the subscribers.
/// Mutations that remove accounts or channels are not allowed, so that a transaction cannot
/// make the state inconsistent.
///
/// Cloning the client will create a new client that shares the same state with the previous one.
/// This makes sense, however, only when the `mutator` can perform actual changes on the shared state.
#[derive(Clone)]
pub struct BlokliTestClient<M> {
    state: Arc<parking_lot::RwLock<BlokliTestState>>,
    mutator: M,
    accounts_channel: AccountEvents,
    channels_channel: GraphEvents,
    ticket_channel: TicketParamEvents,
    safe_deployed_channel: SafeDeployEvents,
    tx_simulation_delay: Duration,
}

fn channel_matches(channel: &Channel, selector: &ChannelSelector) -> bool {
    let filter = match selector.filter {
        Some(ChannelFilter::ChannelId(id)) => channel.concrete_channel_id == hex::encode(id),
        Some(ChannelFilter::DestinationKeyId(dst_id)) => channel.destination as u32 == dst_id,
        Some(ChannelFilter::SourceKeyId(src_id)) => channel.source as u32 == src_id,
        Some(ChannelFilter::SourceAndDestinationKeyIds(src_id, dst_id)) => {
            channel.source as u32 == src_id && channel.destination as u32 == dst_id
        }
        None => true,
    };
    filter && selector.status.is_none_or(|status| channel.status == status)
}

fn account_matches(account: &Account, selector: &AccountSelector) -> bool {
    match selector {
        AccountSelector::Address(address) => account.chain_key == hex::encode(address),
        AccountSelector::KeyId(id) => account.keyid as u32 == *id,
        AccountSelector::PacketKey(packet_key) => account.packet_key == hex::encode(packet_key),
        AccountSelector::Any => true,
    }
}

impl<M: BlokliTestStateMutator> BlokliTestClient<M> {
    /// Constructs a new client that owns the given [`initial_state`](BlokliTestState).
    ///
    /// After construction, the only way to mutate the state is when the client calls the given
    /// [`mutator`](BlokliTestStateMutator) based on a [submitted](BlokliTransactionClient) transaction.
    pub fn new(initial_state: BlokliTestState, mutator: M) -> Self {
        let (mut accounts_tx, accounts_rx) = async_broadcast::broadcast(1024);
        accounts_tx.set_await_active(false);
        accounts_tx.set_overflow(false);

        let (mut channels_tx, channels_rx) = async_broadcast::broadcast(1024);
        channels_tx.set_await_active(false);
        channels_tx.set_overflow(false);

        let (mut tickets_tx, tickets_rx) = async_broadcast::broadcast(1024);
        tickets_tx.set_await_active(false);
        tickets_tx.set_overflow(false);

        let (mut safes_tx, safes_rx) = async_broadcast::broadcast(1024);
        safes_tx.set_await_active(false);
        safes_tx.set_overflow(false);

        Self {
            state: Arc::new(parking_lot::RwLock::new(initial_state)),
            mutator,
            accounts_channel: (accounts_tx, accounts_rx.deactivate()),
            channels_channel: (channels_tx, channels_rx.deactivate()),
            ticket_channel: (tickets_tx, tickets_rx.deactivate()),
            safe_deployed_channel: (safes_tx, safes_rx.deactivate()),
            tx_simulation_delay: Duration::from_secs(1),
        }
    }

    /// Allows changing the mutator on the instance for another one of the same type.
    #[must_use]
    pub fn with_mutator(mut self, mutator: M) -> Self {
        self.mutator = mutator;
        self
    }

    /// Sets the delay before a simulated transaction is confirmed.
    ///
    /// The default is 1 second.
    #[must_use]
    pub fn with_tx_simulation_delay(mut self, tx_simulation_delay: Duration) -> Self {
        self.tx_simulation_delay = tx_simulation_delay;
        self
    }

    /// Returns the current snapshot of the internal state.
    ///
    /// The snapshot can be repeatedly [refreshed](BlokliTestStateSnapshot::refresh) to get the latest state.
    pub fn snapshot(&self) -> BlokliTestStateSnapshot {
        let state = self.state.read();
        BlokliTestStateSnapshot {
            state: self.state.clone(),
            snapshot: state.clone(),
        }
    }

    /// Allows updating the minimum ticket price and minimum ticket-winning probability.
    ///
    /// These changes are also broadcasted as events and take effect on the shared state
    /// for all clones of the client.
    pub fn update_price_and_win_prob(&self, new_price: Option<TokenValueString>, new_win_prob: Option<f64>) {
        let mut updated = false;
        let (new_price_param, new_win_prob_param) = {
            let mut state = self.state.write();

            let mut new_price_param = state.chain_info.ticket_price.clone();
            if let Some(new_price) = new_price {
                state.chain_info.ticket_price = new_price.clone();

                new_price_param = new_price;
                updated = true;
            }

            let mut new_win_prob_param = state.chain_info.min_ticket_winning_probability;
            if let Some(new_win_prob) = new_win_prob {
                state.chain_info.min_ticket_winning_probability = new_win_prob;

                new_win_prob_param = new_win_prob;
                updated = true;
            }
            (new_price_param, new_win_prob_param)
        };

        if updated
            && let Err(error) = self.ticket_channel.0.try_broadcast(TicketParameters {
                min_ticket_winning_probability: new_win_prob_param,
                ticket_price: new_price_param,
            })
        {
            tracing::error!(%error, "failed to broadcast ticket parameters update");
        }
    }

    fn do_query_channels(&self, selector: ChannelSelector) -> Result<Vec<Channel>> {
        Ok(self
            .state
            .read()
            .channels
            .values()
            .filter(|c| channel_matches(c, &selector))
            .cloned()
            .collect())
    }

    fn do_query_accounts(&self, selector: AccountSelector) -> Result<Vec<Account>> {
        Ok(self
            .state
            .read()
            .accounts
            .values()
            .filter(|a| account_matches(a, &selector))
            .cloned()
            .collect())
    }
}

#[async_trait::async_trait]
impl<M: BlokliTestStateMutator + Send + Sync> BlokliQueryClient for BlokliTestClient<M> {
    async fn count_accounts(&self, selector: AccountSelector) -> Result<u32> {
        Ok(match selector {
            AccountSelector::Any => self.state.read().accounts.len() as u32,
            selector => self.query_accounts(selector).await?.len() as u32,
        })
    }

    async fn query_accounts(&self, selector: AccountSelector) -> Result<Vec<Account>> {
        self.do_query_accounts(selector)
    }

    async fn query_native_balance(&self, address: &ChainAddress) -> Result<NativeBalance> {
        let address = hex::encode(address);
        self.state
            .read()
            .native_balances
            .get(&address)
            .cloned()
            .ok_or_else(|| ErrorKind::NoData.into())
    }

    async fn query_token_balance(&self, address: &ChainAddress) -> Result<HoprBalance> {
        let address = hex::encode(address);
        self.state
            .read()
            .token_balances
            .get(&address)
            .cloned()
            .ok_or_else(|| ErrorKind::NoData.into())
    }

    async fn query_transaction_count(&self, address: &ChainAddress) -> Result<u64> {
        let address = hex::encode(address);
        let state = self.state.upgradable_read();
        if let Some(value) = state.tx_counts.get(&address) {
            return Ok(*value);
        }

        let mut state = parking_lot::RwLockUpgradableReadGuard::upgrade(state);
        Ok(*state.tx_counts.entry(address).or_default())
    }

    async fn query_safe_allowance(&self, address: &ChainAddress) -> Result<SafeHoprAllowance> {
        let address = hex::encode(address);
        self.state
            .read()
            .safe_allowances
            .get(&address)
            .cloned()
            .ok_or_else(|| ErrorKind::NoData.into())
    }

    async fn query_safe(&self, selector: SafeSelector) -> Result<Option<Safe>> {
        let state = self.state.read();
        match selector {
            SafeSelector::SafeAddress(addr) => Ok(state.deployed_safes.get(&hex::encode(addr)).cloned()),
            SafeSelector::ChainKey(chain_key) => Ok(state
                .deployed_safes
                .values()
                .find(|s| s.chain_key == hex::encode(chain_key))
                .cloned()),
        }
    }

    async fn query_module_address_prediction(&self, input: ModulePredictionInput) -> Result<ChainAddress> {
        let hash = hopr_crypto_types::types::Hash::create(&[
            input.nonce.to_be_bytes().as_ref(),
            input.owner.as_ref(),
            input.safe_address.as_ref(),
        ]);

        hash.as_ref()[0..20].try_into().map_err(|_| ErrorKind::NoData.into())
    }

    async fn count_channels(&self, selector: ChannelSelector) -> Result<u32> {
        Ok(if selector.matches_all() {
            self.state.read().channels.len() as u32
        } else {
            self.query_channels(selector).await?.len() as u32
        })
    }

    async fn query_channels(&self, selector: ChannelSelector) -> Result<Vec<Channel>> {
        self.do_query_channels(selector)
    }

    async fn query_transaction_status(&self, tx_id: TxId) -> Result<Transaction> {
        self.state
            .read()
            .active_txs
            .get(&tx_id)
            .cloned()
            .ok_or_else(|| ErrorKind::NoData.into())
    }

    async fn query_chain_info(&self) -> Result<ChainInfo> {
        Ok(self.state.read().chain_info.clone())
    }

    async fn query_version(&self) -> Result<String> {
        Ok(self.state.read().version.clone())
    }

    async fn query_health(&self) -> Result<String> {
        Ok(self.state.read().health.clone())
    }
}

impl<M: BlokliTestStateMutator + Send + Sync> BlokliSubscriptionClient for BlokliTestClient<M> {
    fn subscribe_channels(&self, selector: ChannelSelector) -> Result<impl Stream<Item = Result<Channel>> + Send> {
        Ok(if selector.matches_all() {
            let channels = self.state.read().channels.clone();
            futures::stream::iter(channels.into_values())
                .map(Ok)
                .chain(
                    self.channels_channel
                        .1
                        .activate_cloned()
                        .map(|(_, channel, _)| Ok(channel)),
                )
                .boxed()
        } else {
            futures::stream::iter(self.do_query_channels(selector.clone())?)
                .map(Ok)
                .chain(
                    self.channels_channel
                        .1
                        .activate_cloned()
                        .filter(move |(_, c, _)| futures::future::ready(channel_matches(c, &selector)))
                        .map(|(_, channel, _)| Ok(channel)),
                )
                .boxed()
        })
    }

    fn subscribe_accounts(&self, selector: AccountSelector) -> Result<impl Stream<Item = Result<Account>> + Send> {
        Ok(match selector {
            AccountSelector::Any => {
                let accounts = self.state.read().accounts.clone();
                futures::stream::iter(accounts.into_values())
                    .map(Ok)
                    .chain(self.accounts_channel.1.activate_cloned().map(Ok))
                    .boxed()
            }
            selector => futures::stream::iter(self.do_query_accounts(selector.clone())?)
                .map(Ok)
                .chain(
                    self.accounts_channel
                        .1
                        .activate_cloned()
                        .filter(move |a| futures::future::ready(account_matches(a, &selector)))
                        .map(Ok),
                )
                .boxed(),
        })
    }

    fn subscribe_graph(&self) -> Result<impl Stream<Item = Result<OpenedChannelsGraphEntry>> + Send> {
        let (accounts, channels) = {
            let state = self.state.read();
            (state.accounts.clone(), state.channels.clone())
        };

        Ok(futures::stream::iter(channels.into_values().map(move |channel| {
            let source = accounts
                .get(&(channel.source as u32))
                .cloned()
                .ok_or_else(|| BlokliClientError::from(ErrorKind::NoData))?;
            let destination = accounts
                .get(&(channel.destination as u32))
                .cloned()
                .ok_or_else(|| BlokliClientError::from(ErrorKind::NoData))?;

            Ok::<_, BlokliClientError>(OpenedChannelsGraphEntry {
                channel,
                destination,
                source,
            })
        }))
        .chain(
            self.channels_channel
                .1
                .activate_cloned()
                .map(|(source, channel, destination)| {
                    Ok(OpenedChannelsGraphEntry {
                        channel,
                        destination,
                        source,
                    })
                }),
        ))
    }

    fn subscribe_ticket_params(&self) -> Result<impl Stream<Item = Result<TicketParameters>> + Send> {
        let info = self.state.read().chain_info.clone();
        Ok(futures::stream::once(futures::future::ready(TicketParameters {
            min_ticket_winning_probability: info.min_ticket_winning_probability,
            ticket_price: info.ticket_price,
        }))
        .chain(self.ticket_channel.1.activate_cloned())
        .map(Ok))
    }

    fn subscribe_safe_deployments(&self) -> Result<impl Stream<Item = Result<Safe>> + Send> {
        let safes = self.state.read().deployed_safes.clone();
        Ok(futures::stream::iter(safes.into_values())
            .chain(self.safe_deployed_channel.1.activate_cloned())
            .map(Ok))
    }
}

fn simulate_tx_execution(
    signed_tx: &[u8],
    state: &mut BlokliTestState,
    mutator: &dyn BlokliTestStateMutator,
    accounts_channel: &async_broadcast::Sender<Account>,
    channels_channel: &async_broadcast::Sender<(Account, Channel, Account)>,
    ticket_channel: &async_broadcast::Sender<TicketParameters>,
    safe_deployed_channel: &async_broadcast::Sender<Safe>,
) -> Result<()> {
    let old_state = state.clone();
    if let Err(error) = mutator.update_state(signed_tx, state) {
        *state = old_state;
        return Err(error);
    }

    if old_state.accounts.len() > state.accounts.len() {
        *state = old_state;
        return Err(ErrorKind::MockClientError(anyhow::anyhow!("mutation cannot remove accounts")).into());
    }

    if old_state.channels.len() > state.channels.len() {
        *state = old_state;
        return Err(ErrorKind::MockClientError(anyhow::anyhow!("mutation cannot remove channels")).into());
    }

    if old_state.native_balances.len() > state.native_balances.len() {
        *state = old_state;
        return Err(ErrorKind::MockClientError(anyhow::anyhow!("mutation cannot remove native balances")).into());
    }

    if old_state.token_balances.len() > state.token_balances.len() {
        *state = old_state;
        return Err(ErrorKind::MockClientError(anyhow::anyhow!("mutation cannot remove token balances")).into());
    }

    if old_state.safe_allowances.len() > state.safe_allowances.len() {
        *state = old_state;
        return Err(ErrorKind::MockClientError(anyhow::anyhow!("mutation cannot remove safe allowances")).into());
    }

    if old_state.active_txs.len() > state.active_txs.len() {
        *state = old_state;
        return Err(ErrorKind::MockClientError(anyhow::anyhow!("mutation cannot remove active txs")).into());
    }

    // Compare accounts and broadcast changes
    state
        .accounts
        .iter()
        .filter(|&(new_id, new_account)| {
            old_state.accounts.get(new_id).is_none_or(|old_account| {
                // Change is notified only if safe address or multi addresses changed
                old_account.safe_address != new_account.safe_address
                    || old_account.multi_addresses != new_account.multi_addresses
            })
        })
        .for_each(
            |(_, changed_account)| match accounts_channel.try_broadcast(changed_account.clone()) {
                Err(TrySendError::Full(_)) => {
                    tracing::error!("failed to broadcast account change - channel is full");
                }
                Err(TrySendError::Closed(_)) => {
                    tracing::error!("failed to broadcast account change - channel is closed");
                }
                _ => {}
            },
        );

    // Compare channels and broadcast changes
    state
        .channels
        .iter()
        .filter(|&(new_id, new_channel)| {
            old_state
                .channels
                .get(new_id)
                .is_none_or(|old_channel| old_channel != new_channel)
        })
        .filter_map(|(_, changed_channel)| {
            let source = state.accounts.get(&(changed_channel.source as u32)).cloned();
            let destination = state.accounts.get(&(changed_channel.destination as u32)).cloned();
            source
                .zip(destination)
                .map(|(source, destination)| (source, changed_channel.clone(), destination))
        })
        .for_each(|(source, changed_channel, destination)| {
            match channels_channel.try_broadcast((source, changed_channel, destination)) {
                Err(TrySendError::Full(_)) => {
                    tracing::error!("failed to broadcast channel change - channel is full");
                }
                Err(TrySendError::Closed(_)) => {
                    tracing::error!("failed to broadcast channel change - channel is closed");
                }
                _ => {}
            }
        });

    if state.chain_info.min_ticket_winning_probability != old_state.chain_info.min_ticket_winning_probability
        || state.chain_info.ticket_price != old_state.chain_info.ticket_price
    {
        match ticket_channel.try_broadcast(TicketParameters {
            min_ticket_winning_probability: state.chain_info.min_ticket_winning_probability,
            ticket_price: state.chain_info.ticket_price.clone(),
        }) {
            Err(TrySendError::Full(_)) => {
                tracing::error!("failed to broadcast ticket params change - channel is full");
            }
            Err(TrySendError::Closed(_)) => {
                tracing::error!("failed to broadcast ticket params change - channel is closed");
            }
            _ => {}
        }
    }

    // Compare safes and broadcast changes
    state
        .deployed_safes
        .iter()
        .filter(|&(new_id, new_safe)| {
            old_state
                .deployed_safes
                .get(new_id)
                .is_none_or(|old_safe| old_safe != new_safe)
        })
        .for_each(
            |(_, changed_safe)| match safe_deployed_channel.try_broadcast(changed_safe.clone()) {
                Err(TrySendError::Full(_)) => {
                    tracing::error!("failed to broadcast safe change - channel is full");
                }
                Err(TrySendError::Closed(_)) => {
                    tracing::error!("failed to broadcast safe change - channel is closed");
                }
                _ => {}
            },
        );

    Ok(())
}

#[async_trait::async_trait]
impl<M: BlokliTestStateMutator + Send + Sync> BlokliTransactionClient for BlokliTestClient<M> {
    async fn submit_transaction(&self, signed_tx: &[u8]) -> Result<TxReceipt> {
        let mut tx_receipt = [0u8; 32];
        rand::fill(&mut tx_receipt);

        let mut state = self.state.write();
        if let Err(error) = simulate_tx_execution(
            signed_tx,
            &mut state,
            &self.mutator,
            &self.accounts_channel.0,
            &self.channels_channel.0,
            &self.ticket_channel.0,
            &self.safe_deployed_channel.0,
        ) {
            tracing::error!(%error, signed_tx_data = hex::encode(signed_tx), "failed to execute transaction, state reverted");
        } else {
            tracing::debug!("transaction execution succeeded");
        }

        Ok(tx_receipt)
    }

    async fn submit_and_track_transaction(&self, signed_tx: &[u8]) -> Result<TxId> {
        let tx_id = hex::encode(rand::random_iter::<u8>().take(16).collect::<Vec<_>>());
        let tx_hash = hex::encode(rand::random_iter::<u8>().take(32).collect::<Vec<_>>());

        let mut state = self.state.write();

        let status = simulate_tx_execution(
            signed_tx,
            &mut state,
            &self.mutator,
            &self.accounts_channel.0,
            &self.channels_channel.0,
            &self.ticket_channel.0,
            &self.safe_deployed_channel.0,
        )
        .map(|_| {
            tracing::debug!("transaction execution succeeded");
            TransactionStatus::Confirmed
        })
        .unwrap_or_else(|error| {
            tracing::error!(%error, signed_tx_data = hex::encode(signed_tx), "failed to execute transaction, state reverted");
            TransactionStatus::Reverted
        });

        state.active_txs.insert(
            tx_id.clone(),
            Transaction {
                id: tx_id.clone().into(),
                status,
                submitted_at: DateTime(
                    chrono::DateTime::<chrono::Utc>::from(std::time::SystemTime::now()).to_rfc3339(),
                ),
                transaction_hash: Hex32(tx_hash),
            },
        );

        Ok(tx_id)
    }

    async fn submit_and_confirm_transaction(&self, signed_tx: &[u8], num_confirmations: usize) -> Result<TxReceipt> {
        futures_time::task::sleep((self.tx_simulation_delay * num_confirmations as u32).into()).await;

        let mut tx_receipt = [0u8; 32];
        rand::fill(&mut tx_receipt);

        let mut state = self.state.write();
        simulate_tx_execution(
            signed_tx,
            &mut state,
            &self.mutator,
            &self.accounts_channel.0,
            &self.channels_channel.0,
            &self.ticket_channel.0,
            &self.safe_deployed_channel.0,
        )
        .inspect_err(|error| {
            tracing::error!(%error, signed_tx_data = hex::encode(signed_tx), "failed to execute transaction, state reverted");
        })?;

        tracing::debug!("transaction execution succeeded");
        Ok(tx_receipt)
    }

    async fn track_transaction(&self, tx_id: TxId, client_timeout: Duration) -> Result<Transaction> {
        futures_time::task::sleep(self.tx_simulation_delay.min(client_timeout.div(2)).into()).await;
        let tx = self
            .state
            .write()
            .active_txs
            .shift_remove(&tx_id)
            .ok_or_else(|| BlokliClientError::from(ErrorKind::NoData))?;

        match tx.status {
            TransactionStatus::Confirmed => Ok(tx),
            TransactionStatus::Timeout => Err(ErrorKind::TrackingError(TrackingErrorKind::Timeout).into()),
            TransactionStatus::SubmissionFailed => {
                Err(ErrorKind::TrackingError(TrackingErrorKind::SubmissionFailed).into())
            }
            TransactionStatus::ValidationFailed => {
                Err(ErrorKind::TrackingError(TrackingErrorKind::ValidationFailed).into())
            }
            TransactionStatus::Reverted => Err(ErrorKind::TrackingError(TrackingErrorKind::Reverted).into()),
            _ => Err(ErrorKind::MockClientError(anyhow::anyhow!("unexpected transaction status")).into()),
        }
    }
}
