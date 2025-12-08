/// Current Blokli client API.
pub mod api;
mod client;
/// Errors returned by the Blokli client.
pub mod errors;

pub use client::{BlokliClient, BlokliClientConfig};
#[cfg(feature = "testing")]
pub use client::{
    BlokliTestClient, BlokliTestState, BlokliTestStateMutator, BlokliTestStateSnapshot, GraphQlQueries, NopStateMutator,
};

#[cfg(feature = "testing")]
pub mod internal {
    pub use super::api::internal::*;
}

#[cfg(feature = "testing")]
pub mod exports {
    pub use cynic::{Operation, StreamingOperation};
    pub use indexmap::{IndexMap, map::Entry};
}
