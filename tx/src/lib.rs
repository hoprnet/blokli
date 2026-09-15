//! Stand-alone filtering of signed Ethereum transactions for Blokli.
//!
//! This crate decodes signed Ethereum transactions (legacy and EIP-1559), recovers the sender via
//! ECDSA signature recovery, extracts the 4-byte function selector from the calldata, and enforces an
//! allow-set of `(contract, selector)` pairs. Safe-module `execTransactionFromModule` calls are
//! unwrapped so the *inner* call's `(to, selector)` is matched against the allow-set, and a module
//! delegate call into the canonical Gnosis Safe `MultiSend` singleton is unpacked so every batched
//! call is matched individually. It depends only on `alloy` (through `hopr-bindings`) and HOPR
//! helper crates, not on any Blokli internals.
//!
//! The allow-set is injected by the caller — Blokli derives it from the network's contract addresses.
//! Transactions are rejected when they are empty, undecodable, of an unsupported type,
//! contract-creation transactions, fail sender recovery, carry calldata shorter than four bytes,
//! carry trailing bytes after the envelope, request an unsupported Safe delegate call, or whose
//! effective `(contract, selector)` pair is not allowed. See [`FilterError`] for the full set of
//! rejection reasons.
//!
//! # Trust assumptions
//!
//! The filter has no source of truth for the per-node Safe module addresses, so it unwraps
//! `execTransactionFromModule` on the outer selector alone and authorizes on the inner call. See
//! [`TransactionFilter`] for what that implies and which operations it cannot express.
//!
//! # Example
//!
//! ```
//! use blokli_tx::{FilterError, TransactionFilter};
//! use hopr_types::primitive::prelude::Address;
//!
//! let token = Address::from([0x02u8; 20]);
//! let approve = [0x09, 0x5e, 0xa7, 0xb3]; // ERC-20 `approve`
//!
//! // The allow-set is injected by the caller (Blokli derives it from the network's contracts).
//! let filter = TransactionFilter::from_pairs([(token, approve)]);
//!
//! // An empty payload is always rejected.
//! assert_eq!(filter.filter_transaction(&[]), Err(FilterError::Empty));
//! ```

mod errors;
mod filter;

pub use errors::{FilterError, Result};
pub use filter::{AuthorizedCall, FilteredTransaction, Selector, TransactionFilter};
