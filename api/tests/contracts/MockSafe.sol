// SPDX-License-Identifier: MIT
pragma solidity ^0.8.28;

/// @title MockSafe
/// @notice Minimal mock implementation of Gnosis Safe for testing transaction count queries
/// @dev Only implements the nonce() function needed for transactionCount API testing
contract MockSafe {
    /// @notice Internal nonce counter
    uint256 private _nonce;

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

    /// @notice Increments the nonce by 1
    /// @dev Simplified version of Safe transaction execution
    ///      In a real Safe, nonce increments after successful transaction execution
    function incrementNonce() public {
        _nonce++;
    }
}
