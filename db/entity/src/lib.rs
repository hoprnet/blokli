//! This lib re-exports SeaORM generated bindings for HOPR DB.

#![allow(clippy::all)]

#[cfg_attr(rustfmt, rustfmt_skip)]
pub mod codegen;

pub mod conversions;

pub mod errors;

// Re-export codegen entities
#[cfg_attr(rustfmt, rustfmt_skip)]
pub use codegen::*;

/// Register only public-facing entities for GraphQL API exposure.
///
/// This function selectively registers a subset of database entities to the Seaography
/// GraphQL schema. Only the following entities are exposed:
/// - `account`: User accounts with chain and packet keys
/// - `announcement`: Network announcements and multiaddresses
/// - `channel`: Payment channels between nodes
///
/// Internal entities (log, log_status, corrupted_channel, log_topic_info, chain_info,
/// node_info) are not exposed through the GraphQL API but remain available for
/// internal Rust usage.
pub fn register_public_entities<'a>(builder: seaography::Builder<'a>) -> seaography::Builder<'a> {
    seaography::register_entity_modules!(
        builder,
        [
            codegen::sqlite::account,
            codegen::sqlite::announcement,
            codegen::sqlite::channel,
        ]
    )
}
