//! Rust client for the Blokli GraphQL API.
//!
//! # DNS override
//!
//! By default, [`BlokliClient`] uses the system DNS resolver through `reqwest`. Callers that need to keep Blokli
//! communication working while DNS is unreliable can configure [`BlokliClientConfig::dns_override`] to connect the
//! Blokli URL hostname to a fixed IP address.
//!
//! The request URL is not rewritten. For example, a client configured with `https://blokli.example.org` and a DNS
//! override still sends requests to `https://blokli.example.org/graphql`, preserving HTTP `Host`, TLS SNI, and
//! certificate validation while bypassing system DNS for that hostname.
//!
//! ```no_run
//! use std::net::IpAddr;
//!
//! use blokli_client::{BlokliClient, BlokliClientConfig, BlokliDnsOverride};
//!
//! let client = BlokliClient::new(
//!     "https://blokli.example.org".parse()?,
//!     BlokliClientConfig {
//!         dns_override: Some(BlokliDnsOverride {
//!             ip: IpAddr::from([203, 0, 113, 10]),
//!             port: None,
//!         }),
//!         ..Default::default()
//!     },
//! );
//! # let _ = client;
//! # Ok::<(), Box<dyn std::error::Error>>(())
//! ```
//!
//! Leave `dns_override` as `None` to use normal system DNS resolution.
/// Current Blokli client API.
pub mod api;
mod client;
pub mod compatibility;
/// Errors returned by the Blokli client.
pub mod errors;

pub const CLIENT_VERSION: &str = env!("CARGO_PKG_VERSION");

pub use client::{BlokliClient, BlokliClientConfig, BlokliDnsOverride, ReqwestTransport};
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
