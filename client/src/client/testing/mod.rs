use std::{collections::HashMap, time::Duration};

use futures::{Stream, StreamExt};

use crate::{
    api::{types::*, *},
    errors::{BlokliClientError, ErrorKind},
};

/// Blokli client for testing purposes.
pub struct BlokliTestClient {
    pub accounts: Vec<Account>,
    pub native_balances: HashMap<String, NativeBalance>,
    pub token_balances: HashMap<String, HoprBalance>,
    pub safe_allowances: HashMap<String, SafeHoprAllowance>,
    pub channels: Vec<Channel>,
    pub chain_info: ChainInfo,
    pub version: String,
    pub health: String,
    pub tx_client: Option<MockBlokliTransactionClientImpl>,
}

impl std::fmt::Debug for BlokliTestClient {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("BlokliTestClient")
            .field("accounts", &self.accounts)
            .field("native_balances", &self.native_balances)
            .field("token_balances", &self.token_balances)
            .field("safe_allowances", &self.safe_allowances)
            .field("channels", &self.channels)
            .field("chain_info", &self.chain_info)
            .field("version", &self.version)
            .finish_non_exhaustive()
    }
}

mockall::mock! {
    pub BlokliTransactionClientImpl {}
    #[async_trait::async_trait]
    impl BlokliTransactionClient for BlokliTransactionClientImpl {
        async fn submit_transaction(&self, signed_tx: &[u8]) -> Result<TxReceipt>;
        async fn submit_and_track_transaction(&self, signed_tx: &[u8]) -> Result<TxId>;
        async fn submit_and_confirm_transaction(&self, signed_tx: &[u8], num_confirmations: usize) -> Result<TxReceipt>;
        async fn track_transaction(&self, tx_id: TxId, client_timeout: Duration) -> Result<Transaction>;
    }
}

impl Default for BlokliTestClient {
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
            tx_client: None,
        }
    }
}

impl BlokliTestClient {
    fn do_query_channels(&self, selector: ChannelSelector) -> Result<Vec<Channel>> {
        let status_filter: Box<dyn Fn(&Channel) -> bool> = if let Some(status) = &selector.status {
            Box::new(|c: &Channel| c.status == *status)
        } else {
            Box::new(|_: &Channel| true)
        };

        Ok(match selector.filter {
            ChannelFilter::ChannelId(id) => {
                let id = hex::encode(id);
                self.channels
                    .iter()
                    .filter(|c| c.concrete_channel_id == id && status_filter(c))
                    .cloned()
                    .collect()
            }
            ChannelFilter::DestinationKeyId(dst_id) => self
                .channels
                .iter()
                .filter(|c| c.destination as u32 == dst_id && status_filter(c))
                .cloned()
                .collect(),
            ChannelFilter::SourceKeyId(src_id) => self
                .channels
                .iter()
                .filter(|c| c.source as u32 == src_id && status_filter(c))
                .cloned()
                .collect(),
            ChannelFilter::SourceAndDestinationKeyIds(src_id, dst_id) => self
                .channels
                .iter()
                .filter(|c| c.source as u32 == src_id && c.destination as u32 == dst_id && status_filter(c))
                .cloned()
                .collect(),
        })
    }

    fn do_query_accounts(&self, selector: AccountSelector) -> Result<Vec<Account>> {
        Ok(match selector {
            AccountSelector::Address(address) => {
                let address = hex::encode(address);
                self.accounts
                    .iter()
                    .filter(|acc| acc.chain_key == address)
                    .cloned()
                    .collect()
            }
            AccountSelector::KeyId(id) => self
                .accounts
                .iter()
                .filter(|acc| acc.keyid as u32 == id)
                .cloned()
                .collect(),
            AccountSelector::PacketKey(packet_key) => {
                let packet_key = hex::encode(packet_key);
                self.accounts
                    .iter()
                    .filter(|acc| acc.packet_key == packet_key)
                    .cloned()
                    .collect()
            }
        })
    }
}

#[async_trait::async_trait]
impl BlokliQueryClient for BlokliTestClient {
    async fn count_accounts(&self, selector: Option<AccountSelector>) -> Result<u32> {
        Ok(match selector {
            None => self.accounts.len() as u32,
            Some(selector) => self.query_accounts(selector).await?.len() as u32,
        })
    }

    async fn query_accounts(&self, selector: AccountSelector) -> Result<Vec<Account>> {
        self.do_query_accounts(selector)
    }

    async fn query_native_balance(&self, address: &ChainAddress) -> Result<NativeBalance> {
        let address = hex::encode(address);
        self.native_balances
            .get(&address)
            .cloned()
            .ok_or_else(|| ErrorKind::NoData.into())
    }

    async fn query_token_balance(&self, address: &ChainAddress) -> Result<HoprBalance> {
        let address = hex::encode(address);
        self.token_balances
            .get(&address)
            .cloned()
            .ok_or_else(|| ErrorKind::NoData.into())
    }

    async fn query_safe_allowance(&self, address: &ChainAddress) -> Result<SafeHoprAllowance> {
        let address = hex::encode(address);
        self.safe_allowances
            .get(&address)
            .cloned()
            .ok_or_else(|| ErrorKind::NoData.into())
    }

    async fn count_channels(&self, selector: Option<ChannelSelector>) -> Result<u32> {
        Ok(match selector {
            None => self.channels.len() as u32,
            Some(selector) => self.query_channels(selector).await?.len() as u32,
        })
    }

    async fn query_channels(&self, selector: ChannelSelector) -> Result<Vec<Channel>> {
        self.do_query_channels(selector)
    }

    async fn query_transaction_status(&self, _tx_id: TxId) -> Result<Transaction> {
        Err(ErrorKind::MockClientError(anyhow::anyhow!("mock cannot query transaction status")).into())
    }

    async fn query_chain_info(&self) -> Result<ChainInfo> {
        Ok(self.chain_info.clone())
    }

    async fn query_version(&self) -> Result<String> {
        Ok(self.version.clone())
    }

    async fn query_health(&self) -> Result<String> {
        Ok(self.health.clone())
    }
}

impl BlokliSubscriptionClient for BlokliTestClient {
    fn subscribe_channels(
        &self,
        selector: Option<ChannelSelector>,
    ) -> Result<impl Stream<Item = Result<Channel>> + Send> {
        Ok(match selector {
            None => futures::stream::iter(self.channels.iter().cloned())
                .map(Ok)
                .chain(futures::stream::pending())
                .boxed(),
            Some(selector) => futures::stream::iter(self.do_query_channels(selector)?)
                .map(Ok)
                .chain(futures::stream::pending())
                .boxed(),
        })
    }

    fn subscribe_accounts(
        &self,
        selector: Option<AccountSelector>,
    ) -> Result<impl Stream<Item = Result<Account>> + Send> {
        Ok(match selector {
            None => futures::stream::iter(self.accounts.iter().cloned())
                .map(Ok)
                .chain(futures::stream::pending())
                .boxed(),
            Some(selector) => futures::stream::iter(self.do_query_accounts(selector)?)
                .map(Ok)
                .chain(futures::stream::pending())
                .boxed(),
        })
    }

    fn subscribe_graph(&self) -> Result<impl Stream<Item = Result<OpenedChannelsGraphEntry>> + Send> {
        Ok(futures::stream::iter(self.channels.iter().cloned().map(|channel| {
            let source = self
                .accounts
                .iter()
                .find(|acc| acc.keyid == channel.source)
                .cloned()
                .ok_or_else(|| BlokliClientError::from(ErrorKind::NoData))?;
            let destination = self
                .accounts
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
        .chain(futures::stream::pending()))
    }
}

#[async_trait::async_trait]
impl BlokliTransactionClient for BlokliTestClient {
    async fn submit_transaction(&self, signed_tx: &[u8]) -> Result<TxReceipt> {
        if let Some(client) = &self.tx_client {
            client.submit_transaction(signed_tx).await
        } else {
            Err(ErrorKind::MockClientError(anyhow::anyhow!("no transaction client configured")).into())
        }
    }

    async fn submit_and_track_transaction(&self, signed_tx: &[u8]) -> Result<TxId> {
        if let Some(client) = &self.tx_client {
            client.submit_and_track_transaction(signed_tx).await
        } else {
            Err(ErrorKind::MockClientError(anyhow::anyhow!("no transaction client configured")).into())
        }
    }

    async fn submit_and_confirm_transaction(&self, signed_tx: &[u8], num_confirmations: usize) -> Result<TxReceipt> {
        if let Some(client) = &self.tx_client {
            client
                .submit_and_confirm_transaction(signed_tx, num_confirmations)
                .await
        } else {
            Err(ErrorKind::MockClientError(anyhow::anyhow!("no transaction client configured")).into())
        }
    }

    async fn track_transaction(&self, tx_id: TxId, client_timeout: Duration) -> Result<Transaction> {
        if let Some(client) = &self.tx_client {
            client.track_transaction(tx_id, client_timeout).await
        } else {
            Err(ErrorKind::MockClientError(anyhow::anyhow!("no transaction client configured")).into())
        }
    }
}
