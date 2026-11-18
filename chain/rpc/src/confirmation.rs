//! Confirmation targets for synchronous transaction submission.
//!
//! A sync-mode transaction either waits for the chain tip tracker to confirm the block that
//! included it, or for a requested number of blocks. The tip wait is kept free of network access so
//! it can be tested with a scripted receipt lookup and a watch channel.

use std::{sync::Arc, time::Duration};

use async_trait::async_trait;
use hopr_bindings::exports::alloy::{primitives::B256, providers::Provider};
use tokio::{sync::watch, time::sleep};
use tracing::debug;

use crate::tip::ChainTip;

/// How long a sync-mode transaction waits before it is reported as confirmed.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum ConfirmationTarget {
    /// Wait until the receipt block is at least this many blocks behind the chain head.
    Blocks(u64),
    /// Wait until the chain tip tracker reports the receipt block as confirmed.
    #[default]
    ConfirmedTip,
}

impl ConfirmationTarget {
    /// Maps an optional explicit block count to a confirmation target.
    ///
    /// `None` follows the chain tip tracker; an explicit count waits for that many blocks.
    pub fn from_blocks(confirmations: Option<u64>) -> Self {
        confirmations.map_or(Self::ConfirmedTip, Self::Blocks)
    }
}

/// The parts of a transaction receipt needed to confirm it against the chain tip.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct ConfirmedReceipt {
    /// Block that included the transaction.
    pub block_number: u64,
    /// Hash of the block that included the transaction.
    pub block_hash: B256,
    /// Whether the transaction succeeded.
    pub success: bool,
}

/// Errors produced while waiting for the chain tip to confirm a transaction block.
#[derive(Debug, Clone, PartialEq, Eq, thiserror::Error)]
pub enum TipWaitError {
    /// The receipt could not be read from the endpoint.
    #[error("receipt lookup failed: {0}")]
    Receipt(String),
    /// The tracker no longer publishes updates, so its tip cannot advance.
    #[error("chain tip tracker stopped publishing updates")]
    TrackerStopped,
}

/// Looks up the receipt of the transaction that is waiting for confirmation.
#[async_trait]
pub trait ReceiptLookup {
    /// Returns the current receipt, or `None` while the transaction is not in a block.
    async fn receipt(&self) -> Result<Option<ConfirmedReceipt>, String>;
}

/// [`ReceiptLookup`] backed by an alloy provider.
pub struct ProviderReceiptLookup<P: Provider> {
    provider: Arc<P>,
    tx_hash: B256,
}

impl<P: Provider> ProviderReceiptLookup<P> {
    /// Creates a lookup for the transaction with the given hash.
    pub fn new(provider: Arc<P>, tx_hash: B256) -> Self {
        Self { provider, tx_hash }
    }
}

#[async_trait]
impl<P: Provider + Send + Sync> ReceiptLookup for ProviderReceiptLookup<P> {
    async fn receipt(&self) -> Result<Option<ConfirmedReceipt>, String> {
        let receipt = self
            .provider
            .get_transaction_receipt(self.tx_hash)
            .await
            .map_err(|error| format!("Receipt error: {error}"))?;

        let Some(receipt) = receipt else {
            return Ok(None);
        };

        let (Some(block_number), Some(block_hash)) = (receipt.block_number, receipt.block_hash) else {
            return Ok(None);
        };

        Ok(Some(ConfirmedReceipt {
            block_number,
            block_hash,
            success: receipt.status(),
        }))
    }
}

/// Waits until the chain tip confirms the block that included the transaction.
///
/// The lookup is polled until it returns a receipt, the tip is awaited until it reaches the receipt
/// block, and the receipt is read again to detect a reorg. When the receipt is gone or sits in
/// another block, the wait restarts with the new receipt. The caller bounds the whole wait with the
/// confirmation timeout.
///
/// A tracker whose publisher is gone ends the wait with [`TipWaitError::TrackerStopped`], because
/// the tip can no longer advance.
pub async fn wait_for_confirmed_tip<L: ReceiptLookup>(
    tips: &mut watch::Receiver<Option<ChainTip>>,
    lookup: &L,
    receipt_poll_interval: Duration,
) -> Result<ConfirmedReceipt, TipWaitError> {
    loop {
        let receipt = wait_for_receipt(lookup, receipt_poll_interval).await?;

        wait_for_confirmation_height(tips, receipt.block_number).await?;

        match lookup.receipt().await {
            Ok(Some(current))
                if current.block_number == receipt.block_number && current.block_hash == receipt.block_hash =>
            {
                if current_confirmation_height(tips).is_some_and(|height| height >= receipt.block_number) {
                    debug!(
                        block_number = current.block_number,
                        "chain tip confirmed the transaction block"
                    );
                    return Ok(current);
                }

                debug!(
                    block_number = receipt.block_number,
                    "confirmation height dropped while reading the receipt, waiting for it to recover"
                );
            }
            Ok(current) => {
                debug!(
                    expected_block = receipt.block_number,
                    expected_hash = %receipt.block_hash,
                    current_block = ?current.map(|receipt| receipt.block_number),
                    "transaction receipt changed while waiting for the chain tip"
                );
            }
            Err(error) => return Err(TipWaitError::Receipt(error)),
        }
    }
}

async fn wait_for_receipt<L: ReceiptLookup>(
    lookup: &L,
    poll_interval: Duration,
) -> Result<ConfirmedReceipt, TipWaitError> {
    loop {
        match lookup.receipt().await {
            Ok(Some(receipt)) => return Ok(receipt),
            Ok(None) => {}
            Err(error) => return Err(TipWaitError::Receipt(error)),
        }

        sleep(poll_interval).await;
    }
}

async fn wait_for_confirmation_height(
    tips: &mut watch::Receiver<Option<ChainTip>>,
    block_number: u64,
) -> Result<(), TipWaitError> {
    loop {
        {
            let current = tips.borrow_and_update();
            if current
                .as_ref()
                .and_then(|tip| tip.confirmation_height)
                .is_some_and(|height| height >= block_number)
            {
                return Ok(());
            }
        }

        tips.changed().await.map_err(|_| TipWaitError::TrackerStopped)?;
    }
}

fn current_confirmation_height(tips: &watch::Receiver<Option<ChainTip>>) -> Option<u64> {
    tips.borrow().as_ref().and_then(|tip| tip.confirmation_height)
}

#[cfg(test)]
mod tests {
    use std::{
        collections::VecDeque,
        sync::{
            Mutex,
            atomic::{AtomicUsize, Ordering},
        },
        time::Duration,
    };

    use hopr_bindings::exports::alloy::primitives::B256;
    use tokio::{
        sync::watch,
        time::{Instant, sleep, timeout},
    };

    use super::{ConfirmationTarget, ConfirmedReceipt, ReceiptLookup, TipWaitError, wait_for_confirmed_tip};
    use crate::tip::{ChainTip, FcrState, HeadSource};

    const RECEIPT_POLL_INTERVAL: Duration = Duration::from_millis(100);

    fn tip_with_confirmation_height(confirmed: u64, confirmation_height: Option<u64>) -> Option<ChainTip> {
        Some(ChainTip {
            latest: confirmed,
            safe: None,
            finalized: None,
            confirmed,
            confirmation_height,
            source: HeadSource::Polling,
            fcr: FcrState::Unavailable,
        })
    }

    fn tip(confirmed: u64) -> Option<ChainTip> {
        tip_with_confirmation_height(confirmed, Some(confirmed))
    }

    fn receipt(block_number: u64, hash_byte: u8) -> ConfirmedReceipt {
        ConfirmedReceipt {
            block_number,
            block_hash: B256::from([hash_byte; 32]),
            success: true,
        }
    }

    struct ScriptedLookup {
        receipts: Mutex<VecDeque<Result<Option<ConfirmedReceipt>, String>>>,
    }

    impl ScriptedLookup {
        fn new(receipts: Vec<Result<Option<ConfirmedReceipt>, String>>) -> Self {
            Self {
                receipts: Mutex::new(receipts.into()),
            }
        }

        fn remaining(&self) -> usize {
            self.receipts
                .lock()
                .expect("receipts lock should not be poisoned")
                .len()
        }
    }

    #[async_trait::async_trait]
    impl ReceiptLookup for ScriptedLookup {
        async fn receipt(&self) -> Result<Option<ConfirmedReceipt>, String> {
            self.receipts
                .lock()
                .expect("receipts lock should not be poisoned")
                .pop_front()
                .unwrap_or_else(|| Err("no scripted receipt left".to_string()))
        }
    }

    struct RegressionOnSecondReadLookup {
        receipts: Mutex<VecDeque<Result<Option<ConfirmedReceipt>, String>>>,
        tips: watch::Sender<Option<ChainTip>>,
        reads: AtomicUsize,
    }

    impl RegressionOnSecondReadLookup {
        fn new(receipts: Vec<Result<Option<ConfirmedReceipt>, String>>, tips: watch::Sender<Option<ChainTip>>) -> Self {
            Self {
                receipts: Mutex::new(receipts.into()),
                tips,
                reads: AtomicUsize::new(0),
            }
        }
    }

    #[async_trait::async_trait]
    impl ReceiptLookup for RegressionOnSecondReadLookup {
        async fn receipt(&self) -> Result<Option<ConfirmedReceipt>, String> {
            if self.reads.fetch_add(1, Ordering::SeqCst) == 1 {
                self.tips.send_replace(tip_with_confirmation_height(100, Some(98)));
            }

            self.receipts
                .lock()
                .expect("receipts lock should not be poisoned")
                .pop_front()
                .unwrap_or_else(|| Err("no scripted receipt left".to_string()))
        }
    }

    #[test]
    fn test_confirmation_target_default_is_confirmed_tip() {
        assert_eq!(ConfirmationTarget::default(), ConfirmationTarget::ConfirmedTip);
    }

    #[test]
    fn test_confirmation_target_from_blocks() {
        assert_eq!(ConfirmationTarget::from_blocks(None), ConfirmationTarget::ConfirmedTip);
        assert_eq!(ConfirmationTarget::from_blocks(Some(7)), ConfirmationTarget::Blocks(7));
    }

    #[tokio::test(start_paused = true)]
    async fn test_wait_returns_when_tip_already_confirmed_the_receipt() {
        let (_tips_tx, mut tips_rx) = watch::channel(tip(100));
        let lookup = ScriptedLookup::new(vec![Ok(Some(receipt(100, 1))), Ok(Some(receipt(100, 1)))]);

        let confirmed = timeout(
            Duration::from_secs(5),
            wait_for_confirmed_tip(&mut tips_rx, &lookup, RECEIPT_POLL_INTERVAL),
        )
        .await
        .expect("an already confirmed receipt should not wait")
        .expect("the wait should succeed");

        assert_eq!(confirmed.block_number, 100);
        assert_eq!(lookup.remaining(), 0, "the receipt should be read once");
    }

    #[tokio::test(start_paused = true)]
    async fn test_wait_uses_the_current_confirmation_height() {
        let (tips_tx, mut tips_rx) = watch::channel(tip_with_confirmation_height(100, Some(98)));
        let lookup = ScriptedLookup::new(vec![Ok(Some(receipt(99, 1))), Ok(Some(receipt(99, 1)))]);

        let publisher = tokio::spawn(async move {
            sleep(Duration::from_secs(1)).await;
            tips_tx.send_replace(tip_with_confirmation_height(100, Some(99)));
        });

        let started = Instant::now();
        let confirmed = timeout(
            Duration::from_secs(10),
            wait_for_confirmed_tip(&mut tips_rx, &lookup, RECEIPT_POLL_INTERVAL),
        )
        .await
        .expect("the confirmation height update should end the wait")
        .expect("the wait should succeed");

        assert_eq!(confirmed.block_number, 99);
        assert!(
            started.elapsed() >= Duration::from_secs(1),
            "the wait must use the current confirmation height, not the monotonic confirmed block"
        );
        assert_eq!(lookup.remaining(), 0);
        publisher.await.expect("the publisher should finish");
    }

    #[tokio::test(start_paused = true)]
    async fn test_wait_loops_when_the_confirmation_height_drops_after_the_receipt_matched() {
        let (tips_tx, mut tips_rx) = watch::channel(tip_with_confirmation_height(100, Some(99)));
        let lookup = RegressionOnSecondReadLookup::new(
            vec![
                Ok(Some(receipt(99, 1))),
                Ok(Some(receipt(99, 1))),
                Ok(Some(receipt(99, 1))),
                Ok(Some(receipt(99, 1))),
            ],
            tips_tx.clone(),
        );

        let publisher = tokio::spawn(async move {
            sleep(Duration::from_secs(1)).await;
            tips_tx.send_replace(tip_with_confirmation_height(100, Some(99)));
        });

        let started = Instant::now();
        let confirmed = timeout(
            Duration::from_secs(10),
            wait_for_confirmed_tip(&mut tips_rx, &lookup, RECEIPT_POLL_INTERVAL),
        )
        .await
        .expect("the recovered confirmation height should end the wait")
        .expect("the wait should succeed");

        assert_eq!(confirmed.block_number, 99);
        assert!(
            started.elapsed() >= Duration::from_secs(1),
            "a confirmation height that drops after the receipt matched must restart the wait"
        );
        publisher.await.expect("the publisher should finish");
    }

    #[tokio::test(start_paused = true)]
    async fn test_wait_holds_while_the_confirmation_height_is_unknown() {
        let (tips_tx, mut tips_rx) = watch::channel(tip_with_confirmation_height(100, None));
        let lookup = ScriptedLookup::new(vec![Ok(Some(receipt(99, 1))), Ok(Some(receipt(99, 1)))]);

        let publisher = tokio::spawn(async move {
            sleep(Duration::from_secs(1)).await;
            tips_tx.send_replace(tip_with_confirmation_height(100, Some(99)));
        });

        let started = Instant::now();
        let confirmed = timeout(
            Duration::from_secs(10),
            wait_for_confirmed_tip(&mut tips_rx, &lookup, RECEIPT_POLL_INTERVAL),
        )
        .await
        .expect("the known confirmation height should end the wait")
        .expect("the wait should succeed");

        assert_eq!(confirmed.block_number, 99);
        assert!(
            started.elapsed() >= Duration::from_secs(1),
            "an unknown confirmation height must not confirm the receipt"
        );
        assert_eq!(lookup.remaining(), 0);
        publisher.await.expect("the publisher should finish");
    }

    #[tokio::test(start_paused = true)]
    async fn test_wait_returns_after_a_tip_update_confirms_the_receipt() {
        let (tips_tx, mut tips_rx) = watch::channel(tip(99));
        let lookup = ScriptedLookup::new(vec![Ok(Some(receipt(100, 1))), Ok(Some(receipt(100, 1)))]);

        let publisher = tokio::spawn(async move {
            sleep(Duration::from_secs(1)).await;
            tips_tx.send_replace(tip(100));
        });

        let confirmed = timeout(
            Duration::from_secs(10),
            wait_for_confirmed_tip(&mut tips_rx, &lookup, RECEIPT_POLL_INTERVAL),
        )
        .await
        .expect("the tip update should end the wait")
        .expect("the wait should succeed");

        assert_eq!(confirmed.block_number, 100);
        assert_eq!(
            lookup.remaining(),
            0,
            "the receipt should be read again after the tip update"
        );
        publisher.await.expect("the publisher should finish");
    }

    #[tokio::test(start_paused = true)]
    async fn test_wait_follows_a_receipt_that_moved_to_another_block() {
        let (tips_tx, mut tips_rx) = watch::channel(tip(100));
        let lookup = ScriptedLookup::new(vec![
            Ok(Some(receipt(100, 1))),
            Ok(Some(receipt(101, 2))),
            Ok(Some(receipt(101, 2))),
            Ok(Some(receipt(101, 2))),
        ]);

        let publisher = tokio::spawn(async move {
            sleep(Duration::from_secs(1)).await;
            tips_tx.send_replace(tip(101));
        });

        let confirmed = timeout(
            Duration::from_secs(10),
            wait_for_confirmed_tip(&mut tips_rx, &lookup, RECEIPT_POLL_INTERVAL),
        )
        .await
        .expect("the second block should be awaited")
        .expect("the wait should succeed");

        assert_eq!(confirmed.block_number, 101);
        assert_eq!(confirmed.block_hash, B256::from([2u8; 32]));
        assert_eq!(lookup.remaining(), 0);
        publisher.await.expect("the publisher should finish");
    }

    #[tokio::test(start_paused = true)]
    async fn test_wait_restarts_when_the_receipt_disappears_and_comes_back() {
        let (_tips_tx, mut tips_rx) = watch::channel(tip(100));
        let lookup = ScriptedLookup::new(vec![
            Ok(Some(receipt(100, 1))),
            Ok(None),
            Ok(Some(receipt(100, 1))),
            Ok(Some(receipt(100, 1))),
        ]);

        let confirmed = timeout(
            Duration::from_secs(10),
            wait_for_confirmed_tip(&mut tips_rx, &lookup, RECEIPT_POLL_INTERVAL),
        )
        .await
        .expect("the republished receipt should end the wait")
        .expect("the wait should succeed");

        assert_eq!(confirmed.block_number, 100);
        assert_eq!(lookup.remaining(), 0);
    }

    #[tokio::test(start_paused = true)]
    async fn test_wait_times_out_while_the_tip_does_not_advance() {
        let (_tips_tx, mut tips_rx) = watch::channel(tip(99));
        let lookup = ScriptedLookup::new(vec![Ok(Some(receipt(100, 1)))]);

        let result = timeout(
            Duration::from_secs(3),
            wait_for_confirmed_tip(&mut tips_rx, &lookup, RECEIPT_POLL_INTERVAL),
        )
        .await;

        assert!(result.is_err(), "the wait must not finish without a tip update");
    }

    #[tokio::test(start_paused = true)]
    async fn test_wait_ends_when_the_tip_publisher_is_gone() {
        let (tips_tx, mut tips_rx) = watch::channel(tip(99));
        drop(tips_tx);
        let lookup = ScriptedLookup::new(vec![Ok(Some(receipt(100, 1)))]);

        let error = wait_for_confirmed_tip(&mut tips_rx, &lookup, RECEIPT_POLL_INTERVAL)
            .await
            .expect_err("a stopped tracker cannot confirm the receipt");

        match error {
            TipWaitError::TrackerStopped => {}
            other => panic!("expected a stopped tracker error, got {other:?}"),
        }
    }
}
