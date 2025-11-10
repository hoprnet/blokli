#[cfg(test)]
pub(super) mod test_helpers {
    use std::sync::Arc;

    use alloy::{primitives::B256, sol_types::private::IntoLogData};
    use async_trait::async_trait;
    use blokli_chain_rpc::HoprIndexerRpcOperations;
    use blokli_chain_types::ContractAddresses;
    use blokli_db::{BlokliDbAllOperations, accounts::BlokliDbAccountOperations, db::BlokliDb};
    use hex_literal::hex;
    use hopr_crypto_types::{
        keypairs::Keypair,
        prelude::{ChainKeypair, OffchainKeypair},
    };
    use hopr_internal_types::channels::ChannelStatus;
    use hopr_primitive_types::{
        prelude::{Address, HoprBalance, SerializableLog, XDaiBalance},
        traits::IntoEndian,
    };

    use super::super::ContractEventHandlers;
    use crate::{IndexerState, state::IndexerEvent};

    lazy_static::lazy_static! {
        pub static ref SELF_PRIV_KEY: OffchainKeypair = OffchainKeypair::from_secret(&hex!("492057cf93e99b31d2a85bc5e98a9c3aa0021feec52c227cc8170e8f7d047775")).expect("lazy static keypair should be constructible");
        pub static ref SELF_CHAIN_KEYPAIR: ChainKeypair = ChainKeypair::from_secret(&hex!("492057cf93e99b31d2a85bc5e98a9c3aa0021feec52c227cc8170e8f7d047775")).expect("lazy static chain keypair should be constructible");
        pub static ref SELF_CHAIN_ADDRESS: Address = SELF_CHAIN_KEYPAIR.public().to_address();
        pub static ref COUNTERPARTY_PRIV_KEY: OffchainKeypair = OffchainKeypair::from_secret(&hex!("5e6a9defd47decd18dc7a80c7e774bc85e92b5c1a6593dcf2f8b0b9532b8a2e8")).expect("lazy static counterparty keypair should be constructible");
        pub static ref COUNTERPARTY_CHAIN_ADDRESS: Address = "1234567890abcdef1234567890abcdef12345678".parse().expect("lazy static address should be constructible"); // just a dummy
        pub static ref SAFE_INSTANCE_ADDR: Address = "fedcba0987654321fedcba0987654321fedcba09".parse().expect("lazy static address should be constructible"); // just a dummy
        pub static ref STAKE_ADDRESS: Address = "4331eaa9542b6b034c43090d9ec1c2198758dbc3".parse().expect("lazy static address should be constructible");
        pub static ref CHANNELS_ADDR: Address = "bab20aea98368220baa4e3b7f151273ee71df93b".parse().expect("lazy static address should be constructible"); // just a dummy
        pub static ref TOKEN_ADDR: Address = "47d1677e018e79dcdd8a9c554466cb1556fa5007".parse().expect("lazy static address should be constructible"); // just a dummy
        pub static ref NODE_SAFE_REGISTRY_ADDR: Address = "0dcd1bf9a1b36ce34237eeafef220932846bcd82".parse().expect("lazy static address should be constructible"); // just a dummy
        pub static ref ANNOUNCEMENTS_ADDR: Address = "11db4791bf45ef31a10ea4a1b5cb90f46cc72c7e".parse().expect("lazy static address should be constructible"); // just a dummy
        pub static ref TICKET_PRICE_ORACLE_ADDR: Address = "11db4391bf45ef31a10ea4a1b5cb90f46cc72c7e".parse().expect("lazy static address should be constructible"); // just a dummy
        pub static ref WIN_PROB_ORACLE_ADDR: Address = "00db4391bf45ef31a10ea4a1b5cb90f46cc64c7e".parse().expect("lazy static address should be constructible"); // just a dummy
    }

    mockall::mock! {
        pub(crate) IndexerRpcOperations {}

        #[async_trait::async_trait]
        impl HoprIndexerRpcOperations for IndexerRpcOperations {
            async fn block_number(&self) -> blokli_chain_rpc::errors::Result<u64>;

        async fn get_hopr_allowance(&self, owner: Address, spender: Address) -> blokli_chain_rpc::errors::Result<HoprBalance>;

        async fn get_xdai_balance(&self, address: Address) -> blokli_chain_rpc::errors::Result<XDaiBalance>;

        async fn get_hopr_balance(&self, address: Address) -> blokli_chain_rpc::errors::Result<HoprBalance>;

        fn try_stream_logs<'a>(
            &'a self,
            start_block_number: u64,
            filters: blokli_chain_rpc::FilterSet,
            is_synced: bool,
        ) -> blokli_chain_rpc::errors::Result<std::pin::Pin<Box<dyn futures::Stream<Item=blokli_chain_rpc::BlockWithLogs> + Send + 'a> > >;
        }
    }

    #[derive(Clone)]
    pub struct ClonableMockOperations {
        pub inner: Arc<MockIndexerRpcOperations>,
    }

    #[async_trait::async_trait]
    impl HoprIndexerRpcOperations for ClonableMockOperations {
        async fn block_number(&self) -> blokli_chain_rpc::errors::Result<u64> {
            self.inner.block_number().await
        }

        async fn get_hopr_allowance(
            &self,
            owner: Address,
            spender: Address,
        ) -> blokli_chain_rpc::errors::Result<HoprBalance> {
            self.inner.get_hopr_allowance(owner, spender).await
        }

        async fn get_xdai_balance(&self, address: Address) -> blokli_chain_rpc::errors::Result<XDaiBalance> {
            self.inner.get_xdai_balance(address).await
        }

        async fn get_hopr_balance(&self, address: Address) -> blokli_chain_rpc::errors::Result<HoprBalance> {
            self.inner.get_hopr_balance(address).await
        }

        fn try_stream_logs<'a>(
            &'a self,
            start_block_number: u64,
            filters: blokli_chain_rpc::FilterSet,
            is_synced: bool,
        ) -> blokli_chain_rpc::errors::Result<
            std::pin::Pin<Box<dyn futures::Stream<Item = blokli_chain_rpc::BlockWithLogs> + Send + 'a>>,
        > {
            self.inner.try_stream_logs(start_block_number, filters, is_synced)
        }
    }

    /// Test helper to create handlers with event capture capability
    pub fn init_handlers_with_events<
        T: HoprIndexerRpcOperations + Clone + Send + 'static,
        Db: BlokliDbAllOperations + Clone,
    >(
        rpc_operations: T,
        db: Db,
    ) -> (
        ContractEventHandlers<T, Db>,
        IndexerState,
        async_broadcast::Receiver<IndexerEvent>,
    ) {
        let indexer_state = IndexerState::default();
        let event_receiver = indexer_state.subscribe_to_events();

        let handlers = ContractEventHandlers::new(
            ContractAddresses {
                channels: *CHANNELS_ADDR,
                token: *TOKEN_ADDR,
                node_safe_registry: *NODE_SAFE_REGISTRY_ADDR,
                announcements: *ANNOUNCEMENTS_ADDR,
                ticket_price_oracle: *TICKET_PRICE_ORACLE_ADDR,
                winning_probability_oracle: *WIN_PROB_ORACLE_ADDR,
                node_stake_v2_factory: Default::default(),
            },
            db,
            rpc_operations,
            indexer_state.clone(),
        );

        (handlers, indexer_state, event_receiver)
    }

    /// Test helper to create handlers without event capture (for tests that don't need it)
    pub fn init_handlers<T: HoprIndexerRpcOperations + Clone + Send + 'static, Db: BlokliDbAllOperations + Clone>(
        rpc_operations: T,
        db: Db,
    ) -> ContractEventHandlers<T, Db> {
        let (handlers, ..) = init_handlers_with_events(rpc_operations, db);
        handlers
    }

    /// Helper to get published events (non-blocking check)
    pub fn try_recv_event(receiver: &mut async_broadcast::Receiver<IndexerEvent>) -> Option<IndexerEvent> {
        receiver.try_recv().ok()
    }

    pub fn test_log() -> SerializableLog {
        SerializableLog { ..Default::default() }
    }

    /// Converts an Alloy event struct into a SerializableLog for testing.
    ///
    /// This helper uses the contract bindings' IntoLogData trait to properly encode
    /// indexed parameters as topics and non-indexed parameters as data, matching
    /// the actual on-chain event structure.
    pub fn event_to_log<E: IntoLogData>(event: E, contract_address: Address) -> SerializableLog {
        let log_data = event.to_log_data();
        SerializableLog {
            address: contract_address,
            topics: log_data.topics().iter().map(|t| t.0).collect(),
            data: log_data.data.to_vec(),
            ..Default::default()
        }
    }

    /// Helper function to create test accounts for channel operations
    pub async fn create_test_accounts(db: &BlokliDb) -> anyhow::Result<()> {
        db.upsert_account(None, *SELF_CHAIN_ADDRESS, *SELF_PRIV_KEY.public(), None, 1, 0, 0)
            .await?;
        db.upsert_account(
            None,
            *COUNTERPARTY_CHAIN_ADDRESS,
            *COUNTERPARTY_PRIV_KEY.public(),
            None,
            1,
            0,
            1,
        )
        .await?;
        Ok(())
    }

    /// Encodes channel state into bytes32 format as emitted by contract events
    pub fn encode_channel_state(
        balance: HoprBalance,
        ticket_index: u32,
        closure_time: u32,
        epoch: u32,
        status: ChannelStatus,
    ) -> B256 {
        let mut bytes = [0u8; 32];

        // Balance (bytes 6-17)
        let balance_bytes = balance.to_be_bytes();
        bytes[6..18].copy_from_slice(&balance_bytes[20..32]);

        // Ticket index (bytes 18-23)
        let ticket_index_bytes = (ticket_index as u64).to_be_bytes();
        bytes[18..24].copy_from_slice(&ticket_index_bytes[2..8]);

        // Closure time (bytes 24-27)
        bytes[24..28].copy_from_slice(&closure_time.to_be_bytes());

        // Epoch (bytes 28-30)
        let epoch_bytes = epoch.to_be_bytes();
        bytes[28..31].copy_from_slice(&epoch_bytes[1..4]);

        // Status (byte 31)
        bytes[31] = match status {
            ChannelStatus::Closed => 0,
            ChannelStatus::Open => 1,
            ChannelStatus::PendingToClose(_) => 2,
        };

        B256::from(bytes)
    }
}
