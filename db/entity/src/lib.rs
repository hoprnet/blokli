//! This lib re-exports SeaORM generated bindings for HOPR DB.

#![allow(clippy::all)]

#[cfg_attr(rustfmt, rustfmt_skip)]
pub mod codegen;

pub mod conversions;

pub mod errors;

// Re-export codegen entities
#[cfg_attr(rustfmt, rustfmt_skip)]
pub use codegen::*;

// Define a module with selective entity re-exports and seaography macro
mod public_entities {
    // Re-export only the public-facing entities
    pub use super::codegen::{account, announcement, channel, hopr_balance, native_balance};

    // Generate the register function for these entities only
    seaography::register_entity_modules!([account, announcement, channel, hopr_balance, native_balance,]);
}

/// Register only public-facing entities for GraphQL API exposure.
///
/// This function selectively registers a subset of database entities to the Seaography
/// GraphQL schema. Only the following entities are exposed:
/// - `account`: User accounts with chain and packet keys
/// - `announcement`: Network announcements (has 1-N relationship with account)
/// - `channel`: Payment channels between nodes
/// - `hopr_balance`: wxHOPR token balances for addresses
/// - `native_balance`: xDai native token balances for addresses
///
/// Internal entities (hopr_safe_contract, log, log_status, corrupted_channel,
/// log_topic_info, chain_info, node_info) are not exposed through the GraphQL API but
/// remain available for internal Rust usage.
pub fn register_public_entities(builder: seaography::Builder) -> seaography::Builder {
    public_entities::register_entity_modules(builder)
}
