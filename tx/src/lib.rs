//! Stand-alone filtering of signed Ethereum transactions for Blokli.
//!
//! This crate decodes signed Ethereum transactions (legacy and EIP-1559),
//! recovers the sender via ECDSA signature recovery, extracts the 4-byte
//! function selector from the calldata, and enforces a whitelist of
//! `(sender, contract, selector)` triples. It depends only on `alloy` (through
//! `hopr-bindings`) and HOPR helper crates, not on any Blokli internals.
//!
//! Transactions are rejected when they are empty, undecodable, of an
//! unsupported type, contract-creation transactions, fail sender recovery,
//! carry calldata shorter than four bytes, or are not present in the whitelist.
//! See [`FilterError`] for the full set of rejection reasons.
//!
//! # Example
//!
//! ```
//! use blokli_tx::{FilterError, TransactionFilter};
//! use hopr_types::primitive::prelude::Address;
//!
//! let sender = Address::from([0x01u8; 20]);
//! let contract = Address::from([0x02u8; 20]);
//! let selector = [0xa9, 0x05, 0x9c, 0xbb]; // ERC-20 `transfer`
//!
//! // The whitelist is injected by the caller.
//! let filter = TransactionFilter::from_triples([(sender, contract, selector)]);
//!
//! // An empty payload is always rejected.
//! assert_eq!(filter.filter_transaction(&[]), Err(FilterError::Empty));
//! ```

mod errors;
mod filter;

pub use errors::{FilterError, Result};
pub use filter::{FilteredTransaction, Selector, TransactionFilter};
