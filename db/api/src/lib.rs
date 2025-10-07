//! Crate for abstracting the required DB behavior of a HOPR node.
//!
//! Functionality defined here is meant to be used mostly by other higher-level crates.

pub mod errors;
pub mod info;
pub mod logs;

use crate::logs::BlokliDbLogOperations;

/// Convenience trait that contains all HOPR DB operation interfaces.
pub trait BlokliDbAllAbstractedOperations: BlokliDbLogOperations {}

#[doc(hidden)]
pub mod prelude {
    pub use super::*;
    pub use crate::{errors::*, info::*, logs::*};
}
