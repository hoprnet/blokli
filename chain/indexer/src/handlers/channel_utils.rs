use std::{
    ops::Add,
    time::{Duration, SystemTime},
};

use alloy::primitives::B256;
use hopr_internal_types::channels::ChannelStatus;
use hopr_primitive_types::{prelude::HoprBalance, traits::IntoEndian};
use tracing::error;

/// Represents the decoded state of a channel from the packed bytes32 format emitted by contract events.
///
/// The channel state is packed into 256 bits (32 bytes) with the following layout (left-to-right):
/// - Bytes 0-5: Padding (48 bits)
/// - Bytes 6: status (8 bits)
/// - Bytes 7-9: epoch (24 bits)
/// - Bytes 10-13: closureTime (32 bits)
/// - Bytes 14-19: ticketIndex (48 bits)
/// - Bytes 20-31: balance (96 bits)

#[derive(Debug, Clone)]
#[allow(dead_code)]
pub(super) struct DecodedChannel {
    pub balance: HoprBalance,
    pub ticket_index: u64,
    pub closure_time: u32,
    // TODO(Phase 3): Use epoch when channel update API is refactored to support channel_state table
    #[allow(dead_code)]
    pub epoch: u32,
    pub status: ChannelStatus,
}

/// Decodes a packed channel bytes32 into its constituent fields.
///
/// The contract emits channel state as a packed bytes32 value. This function
/// extracts each field according to the Solidity struct packing rules.
///
/// # Arguments
/// * `channel` - The packed bytes32 channel value from the contract event
///
/// # Returns
/// * `DecodedChannel` - Struct containing all decoded channel fields
#[allow(dead_code)]
pub(super) fn decode_channel(channel: B256) -> DecodedChannel {
    let bytes = channel.as_slice();

    // Extract balance (bytes 20-31, 96 bits = 12 bytes)
    let mut balance_bytes = [0u8; 32];
    balance_bytes[20..32].copy_from_slice(&bytes[20..32]);
    let balance = HoprBalance::from_be_bytes(balance_bytes);

    tracing::debug!(?balance, ?balance_bytes, ?bytes, "Decoded channel balance");

    // Extract ticketIndex (bytes 14-19, 48 bits = 6 bytes)
    let mut ticket_index_bytes = [0u8; 8];
    ticket_index_bytes[2..8].copy_from_slice(&bytes[14..20]);
    let ticket_index = u64::from_be_bytes(ticket_index_bytes);

    // Extract closureTime (bytes 10-13, 32 bits = 4 bytes)
    let mut closure_time_bytes = [0u8; 4];
    closure_time_bytes.copy_from_slice(&bytes[10..14]);
    let closure_time = u32::from_be_bytes(closure_time_bytes);

    // Extract epoch (bytes 7-9, 24 bits = 3 bytes)
    let mut epoch_bytes = [0u8; 4];
    epoch_bytes[1..4].copy_from_slice(&bytes[7..10]);
    let epoch = u32::from_be_bytes(epoch_bytes);

    // Extract status (byte 6, 8 bits = 1 byte)
    let status_byte = bytes[6];
    let status = match status_byte {
        0 => ChannelStatus::Closed,
        1 => ChannelStatus::Open,
        2 => ChannelStatus::PendingToClose(SystemTime::UNIX_EPOCH.add(Duration::from_secs(closure_time as u64))),
        _ => {
            error!("Invalid channel status byte: {}", status_byte);
            ChannelStatus::Closed // Default to Closed for safety
        }
    };

    DecodedChannel {
        balance,
        ticket_index,
        closure_time,
        epoch,
        status,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn create_packed_channel(balance: u128, ticket_index: u64, closure_time: u32, epoch: u32, status: u8) -> B256 {
        let mut bytes = [0u8; 32];

        // Bytes 0-5: Padding (leave as zeros)

        // Bytes 20-31: balance (96 bits = 12 bytes)
        // Take the lower 96 bits (12 bytes) of the balance
        let balance_bytes = balance.to_be_bytes(); // 16 bytes for u128
        bytes[20..32].copy_from_slice(&balance_bytes[4..16]); // Skip the first 4 bytes, take the last 12

        // Bytes 14-19: ticketIndex (48 bits = 6 bytes)
        let ticket_index_bytes = ticket_index.to_be_bytes(); // 8 bytes for u64
        bytes[14..20].copy_from_slice(&ticket_index_bytes[2..8]); // Skip the first 2 bytes, take the last 6

        // Bytes 10-13: closureTime (32 bits = 4 bytes)
        let closure_time_bytes = closure_time.to_be_bytes();
        bytes[10..14].copy_from_slice(&closure_time_bytes);

        // Bytes 7-9: epoch (24 bits = 3 bytes)
        let epoch_bytes = epoch.to_be_bytes(); // 4 bytes for u32
        bytes[7..10].copy_from_slice(&epoch_bytes[1..4]); // Skip the first byte, take the last 3

        // Byte 6: status (8 bits = 1 byte)
        bytes[6] = status;

        B256::from(bytes)
    }

    #[test]
    fn test_decode_channel_open_status() {
        // Create a channel with Open status
        let balance = 1_000_000_u128;
        let ticket_index = 42_u64;
        let closure_time = 0_u32; // Not used for Open status
        let epoch = 5_u32;
        let status = 1_u8; // Open

        let packed = create_packed_channel(balance, ticket_index, closure_time, epoch, status);
        let decoded = super::decode_channel(packed);

        assert_eq!(decoded.balance, HoprBalance::from(balance));
        assert_eq!(decoded.ticket_index, ticket_index);
        assert_eq!(decoded.closure_time, closure_time);
        assert_eq!(decoded.epoch, epoch);
        assert!(matches!(decoded.status, ChannelStatus::Open));
    }

    #[test]
    fn test_decode_channel_closed_status() {
        // Create a channel with Closed status
        let balance = 500_000_u128;
        let ticket_index = 100_u64;
        let closure_time = 1234567890_u32;
        let epoch = 10_u32;
        let status = 0_u8; // Closed

        let packed = create_packed_channel(balance, ticket_index, closure_time, epoch, status);
        let decoded = super::decode_channel(packed);

        assert_eq!(decoded.balance, HoprBalance::from(balance));
        assert_eq!(decoded.ticket_index, ticket_index);
        assert_eq!(decoded.closure_time, closure_time);
        assert_eq!(decoded.epoch, epoch);
        assert!(matches!(decoded.status, ChannelStatus::Closed));
    }

    #[test]
    fn test_decode_channel_pending_to_close_status() {
        // Create a channel with PendingToClose status
        let balance = 750_000_u128;
        let ticket_index = 200_u64;
        let closure_time = 1700000000_u32; // Some timestamp
        let epoch = 15_u32;
        let status = 2_u8; // PendingToClose

        let packed = create_packed_channel(balance, ticket_index, closure_time, epoch, status);
        let decoded = super::decode_channel(packed);

        assert_eq!(decoded.balance, HoprBalance::from(balance));
        assert_eq!(decoded.ticket_index, ticket_index);
        assert_eq!(decoded.closure_time, closure_time);
        assert_eq!(decoded.epoch, epoch);

        // Verify PendingToClose with correct timestamp
        match decoded.status {
            ChannelStatus::PendingToClose(time) => {
                let expected_time = SystemTime::UNIX_EPOCH.add(Duration::from_secs(closure_time as u64));
                assert_eq!(time, expected_time);
            }
            _ => panic!("Expected PendingToClose status"),
        }
    }

    #[test]
    fn test_decode_channel_invalid_status() {
        // Test with invalid status bytes - should default to Closed
        let balance = 100_u128;
        let ticket_index = 1_u64;
        let closure_time = 0_u32;
        let epoch = 1_u32;

        // Test status = 3 (invalid)
        let packed = create_packed_channel(balance, ticket_index, closure_time, epoch, 3);
        let decoded = super::decode_channel(packed);
        assert!(matches!(decoded.status, ChannelStatus::Closed));

        // Test status = 255 (invalid)
        let packed = create_packed_channel(balance, ticket_index, closure_time, epoch, 255);
        let decoded = super::decode_channel(packed);
        assert!(matches!(decoded.status, ChannelStatus::Closed));
    }

    #[test]
    fn test_decode_channel_zero_values() {
        // Test decoding with all zeros
        let packed = B256::ZERO;
        let decoded = super::decode_channel(packed);

        assert_eq!(decoded.balance, HoprBalance::from(0_u128));
        assert_eq!(decoded.ticket_index, 0);
        assert_eq!(decoded.closure_time, 0);
        assert_eq!(decoded.epoch, 0);
        assert!(matches!(decoded.status, ChannelStatus::Closed));
    }

    #[test]
    fn test_decode_channel_max_values() {
        // Test with maximum values for each field
        // Balance: max 96-bit value = 2^96 - 1
        let max_balance_96bit = (1_u128 << 96) - 1;

        // TicketIndex: max 48-bit value = 2^48 - 1
        let max_ticket_index_48bit = (1_u64 << 48) - 1;

        // ClosureTime: max 32-bit value
        let max_closure_time = u32::MAX;

        // Epoch: max 24-bit value = 2^24 - 1
        let max_epoch_24bit = (1_u32 << 24) - 1;

        let status = 1_u8; // Open

        let packed = create_packed_channel(
            max_balance_96bit,
            max_ticket_index_48bit,
            max_closure_time,
            max_epoch_24bit,
            status,
        );
        let decoded = super::decode_channel(packed);

        assert_eq!(decoded.balance, HoprBalance::from(max_balance_96bit));
        assert_eq!(decoded.ticket_index, max_ticket_index_48bit);
        assert_eq!(decoded.closure_time, max_closure_time);
        assert_eq!(decoded.epoch, max_epoch_24bit);
        assert!(matches!(decoded.status, ChannelStatus::Open));
    }

    #[test]
    fn test_decode_channel_byte_boundaries() {
        // Test that each field extracts from the correct byte positions
        // Use distinct recognizable patterns for each field

        // Balance: Use pattern 0x123456789ABC (12 bytes when padded)
        let balance = 0x123456789ABC_u128;

        // TicketIndex: Use pattern 0xAABBCCDDEEFF (6 bytes)
        let ticket_index = 0xAABBCCDDEEFF_u64;

        // ClosureTime: Use pattern 0x11223344 (4 bytes)
        let closure_time = 0x11223344_u32;

        // Epoch: Use pattern 0x556677 (3 bytes)
        let epoch = 0x556677_u32;

        // Status: 1 (Open)
        let status = 1_u8;

        let packed = create_packed_channel(balance, ticket_index, closure_time, epoch, status);
        let decoded = super::decode_channel(packed);

        // Verify each field is correctly extracted
        assert_eq!(decoded.balance, HoprBalance::from(balance));
        assert_eq!(decoded.ticket_index, ticket_index);
        assert_eq!(decoded.closure_time, closure_time);
        assert_eq!(decoded.epoch, epoch);
        assert!(matches!(decoded.status, ChannelStatus::Open));

        // Also verify the raw bytes to ensure packing is correct
        let bytes = packed.as_slice();
        // Verify padding (bytes 0-5 should be 0)
        assert_eq!(&bytes[0..6], &[0u8; 6]);
        // Verify balance starts at byte 6
        let balance_bytes = balance.to_be_bytes();
        assert_eq!(&bytes[6..18], &balance_bytes[4..16]);
        // Verify status at byte 31
        assert_eq!(bytes[31], status);
    }
}
