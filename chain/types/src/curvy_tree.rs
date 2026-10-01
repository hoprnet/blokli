//! Canonical Curvy notes-tree primitives used by the indexer.
//!
//! Cryptographic behavior and the versioned frontier snapshot format live in
//! `curvy-core`; this module only exposes them through the chain-types boundary.

pub use curvy_core::{
    field::fr_to_be_32,
    imt::{CompletedShard, FrontierAppend, NotesFrontier, TreeError},
};

/// Schema version persisted with blokli Curvy checkpoints.
pub const NOTES_TREE_VERSION: i64 = curvy_core::imt::NOTES_TREE_VERSION as i64;
/// Depth of the production Curvy notes tree.
pub const NOTES_TREE_DEPTH: usize = curvy_core::imt::NOTES_TREE_DEPTH;
/// Height of each production Curvy notes-tree shard.
pub const NOTES_SHARD_HEIGHT: usize = curvy_core::imt::NOTES_SHARD_HEIGHT;
