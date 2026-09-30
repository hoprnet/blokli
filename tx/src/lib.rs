//! Stand-alone filtering of signed Ethereum transactions for Blokli.
//!
//! Decodes a signed transaction (legacy and EIP-1559), recovers the sender, and authorizes the
//! effective calls against a caller-supplied allow-set of `(contract, selector)` pairs. Depends
//! only on `alloy` (through `hopr-bindings`) and HOPR helper crates, not on Blokli internals.
//!
//! See [`TransactionFilter`] for how the effective calls are resolved and what the filter cannot
//! verify, and [`FilterError`] for every rejection reason.
//!
//! # Example
//!
//! ```
//! use blokli_tx::{FilterError, TransactionFilter};
//! use hopr_types::primitive::prelude::Address;
//!
//! let token = Address::from([0x02u8; 20]);
//! let approve = [0x09, 0x5e, 0xa7, 0xb3]; // ERC-20 `approve`
//! let filter = TransactionFilter::from_pairs([(token, approve)]);
//!
//! assert_eq!(filter.filter_transaction(&[]), Err(FilterError::Empty));
//! ```

mod errors;
mod filter;

pub use errors::{FilterError, Result};
pub use filter::{AuthorizedCall, FilteredTransaction, Selector, TransactionFilter};
