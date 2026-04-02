//! Minimal Safe Smart Account event ABI used by the indexer.
//!
//! Event definitions here are derived from Safe's official Smart Account contract
//! documentation and ABI surface, reduced to the subset currently indexed by Blokli:
//! `SafeSetup`, `AddedOwner`, `RemovedOwner`, `ChangedThreshold`,
//! `ExecutionSuccess`, and `ExecutionFailure`.
//!
//! Reference docs:
//! - https://docs.safe.global/reference-smart-account/events/SafeSetup
//! - https://docs.safe.global/reference-smart-account/owners/addOwnerWithThreshold
//! - https://docs.safe.global/reference-smart-account/owners/removeOwner
//! - https://docs.safe.global/reference-smart-account/owners/changeThreshold
//! - https://docs.safe.global/reference-smart-account/events/ExecutionFailure
//!
//! `ExecutionSuccess` comes from the same standard Safe Smart Account event interface.

use hopr_bindings::exports::alloy::sol;

sol!(
    #![sol(abi)]
    contract SafeContract {
        event SafeSetup(
            address indexed initiator,
            address[] owners,
            uint256 threshold,
            address initializer,
            address fallbackHandler
        );
        event AddedOwner(address owner);
        event RemovedOwner(address owner);
        event ChangedThreshold(uint256 threshold);
        event ExecutionSuccess(bytes32 txHash, uint256 payment);
        event ExecutionFailure(bytes32 txHash, uint256 payment);
    }
);
