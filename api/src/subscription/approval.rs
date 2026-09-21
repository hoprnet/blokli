use std::result::Result as StdResult;

use async_broadcast::{Receiver, RecvError};
use async_graphql::Result;
use async_stream::try_stream;
use blokli_api_types::{SafeHoprApproval, TokenValueString};
use blokli_chain_indexer::state::IndexerEvent;
use futures::{Future, Stream};
use hopr_types::primitive::prelude::{Address, HoprBalance, ToHex};
use tracing::{info, warn};

use crate::errors;

fn approval(owner: Address, spender: Address, allowance: HoprBalance) -> SafeHoprApproval {
    SafeHoprApproval {
        owner: owner.to_hex(),
        spender: spender.to_hex(),
        allowance: TokenValueString(allowance.to_string()),
    }
}

fn shutdown(result: StdResult<(), RecvError>) -> Result<()> {
    match result {
        Ok(()) => info!("safeHoprApproval subscription shutting down due to reorg"),
        Err(RecvError::Closed) => warn!("Shutdown channel closed for safeHoprApproval"),
        Err(RecvError::Overflowed(count)) => {
            return Err(errors::graphql_subscription_snapshot_lagged_error(
                "safeHoprApproval shutdown signal",
                count,
            ));
        }
    }
    Ok(())
}

pub(super) fn safe_hopr_approval_stream<F>(
    owner: Address,
    spender: Address,
    initial: F,
    mut event_receiver: Receiver<IndexerEvent>,
    mut shutdown_receiver: Receiver<()>,
) -> impl Stream<Item = Result<SafeHoprApproval>>
where
    F: Future<Output = Result<HoprBalance>> + Send + 'static,
{
    try_stream! {
        // A reorg must also cancel a pending snapshot, so it cannot later emit stale state.
        let allowance = tokio::select! {
            biased;
            result = shutdown_receiver.recv() => shutdown(result).map(|()| None),
            result = initial => result.map(Some),
        }?;
        let Some(allowance) = allowance else { return; };
        yield approval(owner, spender, allowance);

        loop {
            let event = tokio::select! {
                biased;
                result = shutdown_receiver.recv() => shutdown(result).map(|()| None),
                result = event_receiver.recv() => {
                    match result {
                        Ok(event) => Ok(Some(event)),
                        Err(RecvError::Closed) => {
                            info!("Event bus closed, ending safeHoprApproval subscription");
                            Ok(None)
                        }
                        Err(RecvError::Overflowed(count)) => {
                            Err(errors::graphql_subscription_snapshot_lagged_error(
                                "safeHoprApproval event bus", count,
                            ))
                        }
                    }
                }
            }?;
            let Some(event) = event else { return; };
            // Token provenance is enforced by the indexer's configured-token handler.
            if let IndexerEvent::HoprApprovalUpdated {
                owner: event_owner, spender: event_spender, allowance,
            } = event
                && event_owner == owner && event_spender == spender
            {
                yield approval(owner, spender, allowance);
            }
        }
    }
}
