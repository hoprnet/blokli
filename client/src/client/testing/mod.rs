use crate::{
    api::{types::*, *},
    errors::{BlokliClientError, ErrorKind},
};
use async_broadcast::TrySendError;
use futures::{Stream, StreamExt};
use std::ops::Div;
use std::sync::Arc;
use std::{collections::HashMap, time::Duration};

/// Represents a state for [`BlokliTestClient`].
#[derive(Clone, Debug, PartialEq)]
pub struct BlokliTestState {
    pub accounts: Vec<Account>,
    pub native_balances: HashMap<String, NativeBalance>,
    pub token_balances: HashMap<String, HoprBalance>,
    pub safe_allowances: HashMap<String, SafeHoprAllowance>,
    pub channels: Vec<Channel>,
    pub chain_info: ChainInfo,
    pub version: String,
    pub health: String,
    pub active_txs: HashMap<TxId, Transaction>,
}

impl Default for BlokliTestState {
    fn default() -> Self {
        Self {
            accounts: Default::default(),
            native_balances: Default::default(),
            token_balances: Default::default(),
            safe_allowances: Default::default(),
            channels: Default::default(),
            chain_info: ChainInfo {
                channel_closure_grace_period: Some(Uint64("300".into())),
                channel_dst: Some("0000000000000000000000000000000000000000000000000000000000000000".into()),
                block_number: 100,
                chain_id: 100,
                ledger_dst: Some("0000000000000000000000000000000000000000000000000000000000000000".into()),
                min_ticket_winning_probability: 1.0,
                safe_registry_dst: Some("0000000000000000000000000000000000000000000000000000000000000000".into()),
                ticket_price: TokenValueString("1".into()),
                network: "rotsee".into(),
                contract_addresses: ContractAddressMap(
                    r#"
                {
                    "announcements": "0xf1c143B1bA20C7606d56aA2FA94502D25744b982",
                    "channels": "0x77C9414043d27fdC98A6A2d73fc77b9b383092a7",
                    "module_implementation": "0x32863c4974fBb6253E338a0cb70C382DCeD2eFCb",
                    "node_safe_registry": "0x4F7C7dE3BA2B29ED8B2448dF2213cA43f94E45c0",
                    "node_stake_v2_factory": "0x791d190b2c95397F4BcE7bD8032FD67dCEA7a5F2",
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

/// Mutator for the [`BlokliTestState`].
pub trait BlokliTestStateMutator {
    /// Updates the state given the signed transaction.
    ///
    /// Mutations that remove accounts or channels are not allowed.
    fn update_state(&self, signed_tx: &[u8], state: &mut BlokliTestState) -> Result<()>;
}

/// No-op state mutator.
///
/// Useful for static tests.
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

/// Blokli client for testing purposes.
///
/// This is useful to simulate Blokli server in unit tests.
/// The test client gets an initial [state](BlokliTestState).
///
/// Later transactions done using the client can [mutate](BlokliTestStateMutator) the state and
/// changes are propagated to the subscribers.
/// Mutations that remove accounts or channels are not allowed.
pub struct BlokliTestClient {
    state: Arc<parking_lot::RwLock<BlokliTestState>>,
    mutator: Box<dyn BlokliTestStateMutator + Send + Sync>,
    accounts_channel: AccountEvents,
    channels_channel: GraphEvents,
}

impl BlokliTestClient {
    /// Constructs a new client that owns the given [`initial_state`](BlokliTestState).
    ///
    /// After construction, the only way to mutate the state is when the client calls the given [`mutator`](BlokliTestStateMutator)
    /// based on a [submitted](BlokliTransactionClient) transaction.
    pub fn new<M: BlokliTestStateMutator + Send + Sync + 'static>(initial_state: BlokliTestState, mutator: M) -> Self {
        let (mut accounts_tx, accounts_rx) = async_broadcast::broadcast(1024);
        accounts_tx.set_await_active(false);
        accounts_tx.set_overflow(false);

        let (mut channels_tx, channels_rx) = async_broadcast::broadcast(1024);
        channels_tx.set_await_active(false);
        channels_tx.set_overflow(false);

        Self {
            state: Arc::new(parking_lot::RwLock::new(initial_state)),
            mutator: Box::new(mutator),
            accounts_channel: (accounts_tx, accounts_rx.deactivate()),
            channels_channel: (channels_tx, channels_rx.deactivate()),
        }
    }
}

fn channel_matches(channel: &Channel, selector: &ChannelSelector) -> bool {
    let filter = match selector.filter {
        ChannelFilter::ChannelId(id) => channel.concrete_channel_id == hex::encode(id),
        ChannelFilter::DestinationKeyId(dst_id) => channel.destination as u32 == dst_id,
        ChannelFilter::SourceKeyId(src_id) => channel.source as u32 == src_id,
        ChannelFilter::SourceAndDestinationKeyIds(src_id, dst_id) => {
            channel.source as u32 == src_id && channel.destination as u32 == dst_id
        }
    };
    filter && selector.status.is_none_or(|status| channel.status == status)
}

fn account_matches(account: &Account, selector: &AccountSelector) -> bool {
    match selector {
        AccountSelector::Address(address) => account.chain_key == hex::encode(address),
        AccountSelector::KeyId(id) => account.keyid as u32 == *id,
        AccountSelector::PacketKey(packet_key) => account.packet_key == hex::encode(packet_key),
    }
}

impl BlokliTestClient {
    fn do_query_channels(&self, selector: ChannelSelector) -> Result<Vec<Channel>> {
        Ok(self
            .state
            .read()
            .channels
            .iter()
            .filter(|c| channel_matches(c, &selector))
            .cloned()
            .collect())
    }

    fn do_query_accounts(&self, selector: AccountSelector) -> Result<Vec<Account>> {
        Ok(self
            .state
            .read()
            .accounts
            .iter()
            .filter(|a| account_matches(a, &selector))
            .cloned()
            .collect())
    }
}

#[async_trait::async_trait]
impl BlokliQueryClient for BlokliTestClient {
    async fn count_accounts(&self, selector: Option<AccountSelector>) -> Result<u32> {
        Ok(match selector {
            None => self.state.read().accounts.len() as u32,
            Some(selector) => self.query_accounts(selector).await?.len() as u32,
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

    async fn query_safe_allowance(&self, address: &ChainAddress) -> Result<SafeHoprAllowance> {
        let address = hex::encode(address);
        self.state
            .read()
            .safe_allowances
            .get(&address)
            .cloned()
            .ok_or_else(|| ErrorKind::NoData.into())
    }

    async fn count_channels(&self, selector: Option<ChannelSelector>) -> Result<u32> {
        Ok(match selector {
            None => self.state.read().channels.len() as u32,
            Some(selector) => self.query_channels(selector).await?.len() as u32,
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

impl BlokliSubscriptionClient for BlokliTestClient {
    fn subscribe_channels(
        &self,
        selector: Option<ChannelSelector>,
    ) -> Result<impl Stream<Item = Result<Channel>> + Send> {
        Ok(match selector {
            None => {
                let channels = self.state.read().channels.clone();
                futures::stream::iter(channels)
                    .map(Ok)
                    .chain(
                        self.channels_channel
                            .1
                            .activate_cloned()
                            .map(|(_, channel, _)| Ok(channel)),
                    )
                    .boxed()
            }
            Some(selector) => futures::stream::iter(self.do_query_channels(selector.clone())?)
                .map(Ok)
                .chain(
                    self.channels_channel
                        .1
                        .activate_cloned()
                        .filter(move |(_, c, _)| futures::future::ready(channel_matches(c, &selector)))
                        .map(|(_, channel, _)| Ok(channel)),
                )
                .boxed(),
        })
    }

    fn subscribe_accounts(
        &self,
        selector: Option<AccountSelector>,
    ) -> Result<impl Stream<Item = Result<Account>> + Send> {
        Ok(match selector {
            None => {
                let accounts = self.state.read().accounts.clone();
                futures::stream::iter(accounts)
                    .map(Ok)
                    .chain(self.accounts_channel.1.activate_cloned().map(Ok))
                    .boxed()
            }
            Some(selector) => futures::stream::iter(self.do_query_accounts(selector.clone())?)
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

        Ok(futures::stream::iter(channels.into_iter().map(move |channel| {
            let source = accounts
                .iter()
                .find(|acc| acc.keyid == channel.source)
                .cloned()
                .ok_or_else(|| BlokliClientError::from(ErrorKind::NoData))?;
            let destination = accounts
                .iter()
                .find(|acc| acc.keyid == channel.destination)
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
}

fn submit_tx(
    signed_tx: &[u8],
    state: &mut BlokliTestState,
    mutator: &dyn BlokliTestStateMutator,
    accounts_channel: &async_broadcast::Sender<Account>,
    channels_channel: &async_broadcast::Sender<(Account, Channel, Account)>,
) -> Result<()> {
    let old_state = state.clone();
    mutator.update_state(signed_tx, state)?;

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
        .filter(|new_account| !old_state.accounts.contains(new_account))
        .for_each(
            |changed_account| match accounts_channel.try_broadcast(changed_account.clone()) {
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
        .filter(|new_channel| !old_state.channels.contains(new_channel))
        .filter_map(|changed_channel| {
            let source = state
                .accounts
                .iter()
                .find(|acc| acc.keyid == changed_channel.source)
                .cloned();
            let destination = state
                .accounts
                .iter()
                .find(|acc| acc.keyid == changed_channel.destination)
                .cloned();
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

    Ok(())
}

#[async_trait::async_trait]
impl BlokliTransactionClient for BlokliTestClient {
    async fn submit_transaction(&self, signed_tx: &[u8]) -> Result<TxReceipt> {
        let mut tx_receipt = [0u8; 32];
        rand::fill(&mut tx_receipt);

        let mut state = self.state.write();
        submit_tx(
            signed_tx,
            &mut state,
            &*self.mutator,
            &self.accounts_channel.0,
            &self.channels_channel.0,
        )?;

        Ok(tx_receipt)
    }

    async fn submit_and_track_transaction(&self, signed_tx: &[u8]) -> Result<TxId> {
        let tx_id = hex::encode(rand::random_iter::<u8>().take(16).collect::<Vec<_>>());
        let tx_hash = hex::encode(rand::random_iter::<u8>().take(32).collect::<Vec<_>>());

        let mut state = self.state.write();
        state.active_txs.insert(
            tx_id.clone(),
            Transaction {
                id: tx_id.clone().into(),
                status: TransactionStatus::Confirmed,
                submitted_at: DateTime(
                    chrono::DateTime::<chrono::Utc>::from(std::time::SystemTime::now()).to_rfc3339(),
                ),
                transaction_hash: Some(Hex32(tx_hash)),
            },
        );

        submit_tx(
            signed_tx,
            &mut state,
            &*self.mutator,
            &self.accounts_channel.0,
            &self.channels_channel.0,
        )?;

        Ok(tx_id)
    }

    async fn submit_and_confirm_transaction(&self, signed_tx: &[u8], num_confirmations: usize) -> Result<TxReceipt> {
        futures_time::task::sleep((Duration::from_millis(200) * num_confirmations as u32).into()).await;
        self.submit_transaction(signed_tx).await
    }

    async fn track_transaction(&self, tx_id: TxId, client_timeout: Duration) -> Result<Transaction> {
        futures_time::task::sleep(client_timeout.div(10).into()).await;
        self.state
            .write()
            .active_txs
            .remove(&tx_id)
            .ok_or_else(|| ErrorKind::NoData.into())
    }
}
