/// Current Blokli client API.
pub mod api;
mod client;
/// Errors returned by the Blokli client.
pub mod errors;

pub const CLIENT_VERSION: &str = env!("CARGO_PKG_VERSION");

pub use client::{BlokliClient, BlokliClientConfig, ReqwestTransport};
#[cfg(feature = "testing")]
pub use client::{
    BlokliTestClient, BlokliTestState, BlokliTestStateMutator, BlokliTestStateSnapshot, GraphQlQueries, NopStateMutator,
};

#[cfg(feature = "testing")]
pub mod internal {
    pub use super::api::internal::*;
}

#[doc(hidden)]
pub mod exports {
    pub use url::Url;
    #[cfg(feature = "testing")]
    pub use {
        cynic::{Operation, StreamingOperation},
        indexmap::{IndexMap, map::Entry},
    };
}
