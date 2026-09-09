use std::{collections::BTreeSet, sync::Arc};

use blokli_chain_rpc::BlockWithLogs;
use blokli_chain_types::AlloyAddressExt;
use blokli_db::{db::BlokliDb, info::BlokliDbInfoOperations};
use futures::poll;
use hopr_bindings::{
    exports::alloy::{
        primitives::{Address as AlloyAddress, B256, U256},
        sol_types::SolEvent,
    },
    hopr_token::HoprToken::Approval,
};

use super::{Indexer, LogFilterPhase};
use crate::{
    handlers::{
        ContractEventHandlers,
        test_utils::test_helpers::{
            CHANNELS_ADDR, ClonableMockOperations, MockIndexerRpcOperations, SAFE_INSTANCE_ADDR, TOKEN_ADDR,
            XHOPR_TOKEN_ADDR, event_to_log, init_handlers_with_events,
        },
    },
    state::IndexerEvent,
    traits::ChainLogHandler,
};

type TestIndexer = Indexer<ClonableMockOperations, ContractEventHandlers<ClonableMockOperations, BlokliDb>, BlokliDb>;

fn approval(value: U256) -> Approval {
    Approval {
        owner: AlloyAddress::from_hopr_address(*SAFE_INSTANCE_ADDR),
        spender: AlloyAddress::from_hopr_address(*CHANNELS_ADDR),
        value,
    }
}

#[tokio::test]
async fn continuous_approval_filters_only_match_configured_token() -> anyhow::Result<()> {
    let db = BlokliDb::new_in_memory().await?;
    let rpc = ClonableMockOperations {
        inner: Arc::new(MockIndexerRpcOperations::new()),
    };
    let (handlers, ..) = init_handlers_with_events(rpc, db.clone());
    let log = event_to_log(approval(U256::MAX), *TOKEN_ADDR);
    let topics: Vec<B256> = log.topics.into_iter().map(B256::from).collect();

    for enable_safe_indexing in [false, true] {
        let filters =
            TestIndexer::generate_log_filters(&db, &handlers, LogFilterPhase::Continuous, enable_safe_indexing).await?;
        for group in [&filters.all, &filters.token] {
            assert!(group.iter().any(|filter| {
                filter.matches_address(AlloyAddress::from_hopr_address(*TOKEN_ADDR)) && filter.matches_topics(&topics)
            }));
            for other_token in [
                AlloyAddress::from_hopr_address(*XHOPR_TOKEN_ADDR),
                AlloyAddress::from([0x42; 20]),
            ] {
                assert!(
                    !group
                        .iter()
                        .any(|filter| filter.matches_address(other_token) && filter.matches_topics(&topics))
                );
            }
        }
        assert!(!filters.no_token.iter().any(|filter| {
            filter.matches_address(AlloyAddress::from_hopr_address(*TOKEN_ADDR)) && filter.matches_topics(&topics)
        }));
    }
    assert!(
        handlers
            .contract_address_topics(*TOKEN_ADDR)
            .contains(&Approval::SIGNATURE_HASH)
    );
    Ok(())
}

#[tokio::test]
async fn approval_publication_uses_coordinated_committed_block_path() -> anyhow::Result<()> {
    let db = BlokliDb::new_in_memory().await?;
    let rpc = ClonableMockOperations {
        inner: Arc::new(MockIndexerRpcOperations::new()),
    };
    let (handlers, state, mut receiver) = init_handlers_with_events(rpc, db.clone());
    let log = event_to_log(approval(U256::MAX), *TOKEN_ADDR);
    let block = BlockWithLogs {
        block_id: log.block_number,
        logs: BTreeSet::from([log]),
    };
    TestIndexer::store_block_logs(&db, &handlers, &block).await?;

    // Snapshot initialization excludes block processing until its watermark lock is released.
    let watermark = state.acquire_watermark_lock().await;
    let mut processing = Box::pin(TestIndexer::process_block(
        &db, &handlers, block, false, true, &state, true,
    ));
    assert!(poll!(&mut processing).is_pending());
    assert!(receiver.try_recv().is_err());
    drop(watermark);
    assert!(processing.await.is_some());

    assert_eq!(db.get_indexer_state_info(None).await?.latest_block_number, 10);
    assert!(matches!(receiver.try_recv()?, IndexerEvent::HoprApprovalUpdated { .. }));
    assert!(receiver.try_recv().is_err());
    Ok(())
}

#[tokio::test]
async fn approval_reorg_signals_shutdown_and_skips_removed_logs() -> anyhow::Result<()> {
    let db = BlokliDb::new_in_memory().await?;
    let rpc = ClonableMockOperations {
        inner: Arc::new(MockIndexerRpcOperations::new()),
    };
    let (handlers, state, mut receiver) = init_handlers_with_events(rpc, db.clone());
    let mut shutdown = state.subscribe_to_shutdown();
    let mut removed = event_to_log(approval(U256::MAX), *TOKEN_ADDR);
    removed.removed = true;
    let mut replacement = event_to_log(approval(U256::ZERO), *TOKEN_ADDR);
    replacement.block_number = 11;
    let block = BlockWithLogs {
        block_id: 11,
        logs: BTreeSet::from([removed, replacement]),
    };
    TestIndexer::store_block_logs(&db, &handlers, &block).await?;
    assert!(
        TestIndexer::process_block(&db, &handlers, block, false, true, &state, true)
            .await
            .is_some()
    );

    assert!(shutdown.try_recv().is_ok());
    match receiver.try_recv()? {
        IndexerEvent::HoprApprovalUpdated { allowance, .. } => assert_eq!(allowance.amount().to_string(), "0"),
        event => panic!("Expected replacement Approval update, got {event:?}"),
    }
    assert!(receiver.try_recv().is_err(), "removed Approval must not be published");
    Ok(())
}
