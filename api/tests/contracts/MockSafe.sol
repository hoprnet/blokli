// SPDX-License-Identifier: MIT
pragma solidity ^0.8.28;

/// @title MockSafe
/// @notice Minimal mock implementation of Gnosis Safe for testing transaction count queries
/// @dev Implements nonce() and getThreshold() functions needed for transactionCount API testing
contract MockSafe {
    /// @notice Internal nonce counter
    uint256 private _nonce;

    /// @notice Fixed threshold value for this mock Safe
    /// @dev Real Safe contracts have configurable thresholds, we use 1 for simplicity
    uint256 private constant _threshold = 1;

    /// @notice Constructor initializes nonce to 0
    constructor() {
        _nonce = 0;
    }

    /// @notice Returns the current nonce
    /// @dev This matches the Gnosis Safe nonce() interface
    /// @return Current nonce value
    function nonce() public view returns (uint256) {
        return _nonce;
    }

    /// @notice Returns the threshold (minimum number of signers required)
    /// @dev This matches the Gnosis Safe getThreshold() interface
    /// @return Threshold value (constant 1 for this mock)
    function getThreshold() public view returns (uint256) {
        return _threshold;
    }

    /// @notice Increments the nonce by 1
    /// @dev Simplified version of Safe transaction execution
    ///      In a real Safe, nonce increments after successful transaction execution
    function incrementNonce() public {
        _nonce++;
    }
}
