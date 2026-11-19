//! Confirmation targets for synchronous transaction submission.
//!
//! A sync-mode transaction either waits for the chain tip tracker to confirm the block that
//! included it, or for a requested number of blocks. The tip wait is kept free of network access so
//! it can be tested with a scripted receipt lookup and a watch channel.

use std::{sync::Arc, time::Duration};

use async_trait::async_trait;
use hopr_bindings::exports::alloy::{primitives::B256, providers::Provider};
use tokio::{
    sync::watch,
    time::{sleep, timeout},
};
use tracing::debug;

use crate::tip::ChainTip;

/// How long a sync-mode transaction waits before it is reported as confirmed.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum ConfirmationTarget {
    /// Wait until the endpoint reports this many confirmations, counting the inclusion block as
    /// the first one.
    ///
    /// The count is passed to the endpoint unchanged, so a transaction included at height 100 is
    /// confirmed at height 106 by `Blocks(7)`. This is a different rule from the depth configured
    /// for the chain tip tracker, which subtracts the depth from the observed head.
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
/// block, and the receipt is read again to detect a reorg. The receipt is also re-read on every
/// poll interval while the tip is awaited, so a transaction that a reorg moved to another block,
/// including one that is already confirmed, resumes the wait with the new inclusion block. A
/// receipt lookup error is retried the same way instead of ending the wait. The caller bounds the
/// whole wait with the confirmation timeout.
///
/// A tracker whose publisher is gone ends the wait with [`TipWaitError::TrackerStopped`], because
/// the tip can no longer advance.
pub async fn wait_for_confirmed_tip<L: ReceiptLookup>(
    tips: &mut watch::Receiver<Option<ChainTip>>,
    lookup: &L,
    receipt_poll_interval: Duration,
) -> Result<ConfirmedReceipt, TipWaitError> {
    let mut receipt = wait_for_receipt(lookup, receipt_poll_interval).await?;

    loop {
        if confirmation_height(tips).is_some_and(|height| height >= receipt.block_number) {
            match lookup.receipt().await {
                Ok(Some(current)) if is_same_block(&current, &receipt) => {
                    if confirmation_height(tips).is_some_and(|height| height >= current.block_number) {
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
                Ok(Some(current)) => {
                    debug!(
                        expected_block = receipt.block_number,
                        expected_hash = %receipt.block_hash,
                        current_block = current.block_number,
                        "transaction receipt changed while waiting for the chain tip"
                    );
                    receipt = current;
                    continue;
                }
                Ok(None) => {
                    debug!(
                        expected_block = receipt.block_number,
                        "transaction receipt disappeared while waiting for the chain tip"
                    );
                    receipt = wait_for_receipt(lookup, receipt_poll_interval).await?;
                    continue;
                }
                Err(error) => {
                    debug!(%error, "receipt lookup failed while waiting for the chain tip, retrying");
                }
            }
        }

        // Wait for the next tip update, and keep reading the receipt on the poll interval so a
        // reorg that moves the transaction to an already confirmed block does not wait for one.
        wait_for_tip_change(tips, receipt_poll_interval).await?;

        match lookup.receipt().await {
            Ok(Some(current)) => receipt = current,
            Ok(None) => receipt = wait_for_receipt(lookup, receipt_poll_interval).await?,
            Err(error) => {
                debug!(%error, "receipt lookup failed while waiting for the chain tip, retrying");
            }
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
            Err(error) => debug!(%error, "receipt lookup failed, retrying"),
        }

        sleep(poll_interval).await;
    }
}

async fn wait_for_tip_change(
    tips: &mut watch::Receiver<Option<ChainTip>>,
    poll_interval: Duration,
) -> Result<(), TipWaitError> {
    match timeout(poll_interval, tips.changed()).await {
        Ok(update) => update.map_err(|_| TipWaitError::TrackerStopped),
        Err(_elapsed) => Ok(()),
    }
}

fn is_same_block(current: &ConfirmedReceipt, expected: &ConfirmedReceipt) -> bool {
    current.block_number == expected.block_number && current.block_hash == expected.block_hash
}

fn confirmation_height(tips: &watch::Receiver<Option<ChainTip>>) -> Option<u64> {
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

    fn tip_at(latest: u64, confirmed: u64, confirmation_height: Option<u64>) -> Option<ChainTip> {
        Some(ChainTip {
            latest,
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
        last: Mutex<Option<ConfirmedReceipt>>,
    }

    impl ScriptedLookup {
        fn new(receipts: Vec<Result<Option<ConfirmedReceipt>, String>>) -> Self {
            Self {
                receipts: Mutex::new(receipts.into()),
                last: Mutex::new(None),
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
            let scripted = self
                .receipts
                .lock()
                .expect("receipts lock should not be poisoned")
                .pop_front();

            let current = match scripted {
                Some(scripted) => scripted,
                // The wait reads the receipt on every poll interval, so the last scripted receipt
                // stays current instead of exhausting the script.
                None => return Ok(*self.last.lock().expect("last receipt lock should not be poisoned")),
            };

            if let Ok(Some(receipt)) = current {
                *self.last.lock().expect("last receipt lock should not be poisoned") = Some(receipt);
            }

            current
        }
    }

    struct RegressionOnSecondReadLookup {
        receipt: ConfirmedReceipt,
        tips: watch::Sender<Option<ChainTip>>,
        reads: AtomicUsize,
    }

    impl RegressionOnSecondReadLookup {
        fn new(receipt: ConfirmedReceipt, tips: watch::Sender<Option<ChainTip>>) -> Self {
            Self {
                receipt,
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

            Ok(Some(self.receipt))
        }
    }

    struct ReorgOnSecondReadLookup {
        tips: watch::Sender<Option<ChainTip>>,
        receipt: ConfirmedReceipt,
        regression: ChainTip,
        reads: AtomicUsize,
    }

    impl ReorgOnSecondReadLookup {
        fn new(tips: watch::Sender<Option<ChainTip>>, receipt: ConfirmedReceipt, regression: ChainTip) -> Self {
            Self {
                tips,
                receipt,
                regression,
                reads: AtomicUsize::new(0),
            }
        }
    }

    #[async_trait::async_trait]
    impl ReceiptLookup for ReorgOnSecondReadLookup {
        async fn receipt(&self) -> Result<Option<ConfirmedReceipt>, String> {
            if self.reads.fetch_add(1, Ordering::SeqCst) == 1 {
                self.tips.send_replace(Some(self.regression));
            }

            Ok(Some(self.receipt))
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
        let lookup = RegressionOnSecondReadLookup::new(receipt(99, 1), tips_tx.clone());

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

        assert!(
            matches!(error, TipWaitError::TrackerStopped),
            "expected a stopped tracker error, got {error:?}"
        );
    }

    #[tokio::test(start_paused = true)]
    async fn test_wait_retries_a_receipt_lookup_error() {
        let (_tips_tx, mut tips_rx) = watch::channel(tip(100));
        let lookup = ScriptedLookup::new(vec![
            Err("receipt lookup timed out".to_string()),
            Ok(Some(receipt(100, 1))),
            Ok(Some(receipt(100, 1))),
        ]);

        let confirmed = timeout(
            Duration::from_secs(5),
            wait_for_confirmed_tip(&mut tips_rx, &lookup, RECEIPT_POLL_INTERVAL),
        )
        .await
        .expect("a transient receipt lookup error must not end the wait")
        .expect("the wait should succeed once the receipt is readable");

        assert_eq!(confirmed.block_number, 100);
        assert_eq!(lookup.remaining(), 0);
    }

    #[tokio::test(start_paused = true)]
    async fn test_wait_follows_a_receipt_that_moved_to_an_already_confirmed_block() {
        let (_tips_tx, mut tips_rx) = watch::channel(tip(99));
        let lookup = ScriptedLookup::new(vec![
            Ok(Some(receipt(100, 1))),
            Ok(Some(receipt(99, 2))),
            Ok(Some(receipt(99, 2))),
        ]);

        let confirmed = timeout(
            Duration::from_secs(5),
            wait_for_confirmed_tip(&mut tips_rx, &lookup, RECEIPT_POLL_INTERVAL),
        )
        .await
        .expect("the wait must re-read the receipt below the old inclusion height")
        .expect("a receipt at an already confirmed block should end the wait without a tip update");

        assert_eq!(confirmed.block_number, 99);
        assert_eq!(confirmed.block_hash, B256::from([2u8; 32]));
        assert_eq!(lookup.remaining(), 0);
    }

    #[tokio::test(start_paused = true)]
    async fn test_wait_restarts_when_a_reorg_lowers_the_confirmation_height() {
        let (tips_tx, mut tips_rx) = watch::channel(tip_at(102, 100, Some(100)));
        let lookup = ReorgOnSecondReadLookup::new(
            tips_tx.clone(),
            receipt(100, 1),
            ChainTip {
                latest: 101,
                safe: None,
                finalized: None,
                confirmed: 100,
                confirmation_height: Some(99),
                source: HeadSource::Polling,
                fcr: FcrState::Unavailable,
            },
        );

        let publisher = tokio::spawn(async move {
            sleep(Duration::from_secs(1)).await;
            tips_tx.send_replace(tip_at(102, 100, Some(100)));
        });

        let started = Instant::now();
        let confirmed = timeout(
            Duration::from_secs(10),
            wait_for_confirmed_tip(&mut tips_rx, &lookup, RECEIPT_POLL_INTERVAL),
        )
        .await
        .expect("the recovered confirmation height should end the wait")
        .expect("the wait should succeed");

        assert_eq!(confirmed.block_number, 100);
        assert!(
            started.elapsed() >= Duration::from_secs(1),
            "a reorg that lowers the confirmation height below the receipt block must restart the wait"
        );
        publisher.await.expect("the publisher should finish");
    }
}
