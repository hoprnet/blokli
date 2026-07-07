//! Query resolvers for schema version 2.
//!
//! Only resolvers that **differ** from v1 belong here.
//! Every resolver that is unchanged between versions stays in [`crate::query`] and
//! is composed into this schema automatically via [`MergedQueryV2`].
//!
//! # How to introduce a breaking query change
//!
//! 1. Implement the new resolver shape in a new `impl QueryRootV2` block below.
//! 2. Remove the old resolver from `QueryRoot` in `query.rs` (v1 clients are unaffected because they are served the v1
//!    schema).
//! 3. Uncomment / extend `MergedQueryV2` so it picks up the new impl.
//! 4. Uncomment the v2 registration block in `build_version_registry` (`schema.rs`).
//! 5. Bump `LATEST_SCHEMA_VERSION` in `schema.rs`.
//! 6. Bump `SCHEMA_VERSION` in `client/src/client/mod.rs` and update the client cynic types to match the new shape.
//!
//! See `UPDATE.md` at the workspace root for the full decision tree.

use async_graphql::{MergedObject, Object};

use crate::query::QueryRoot;

// ---------------------------------------------------------------------------
// V2-specific resolver delta
//
// Add one `impl QueryRootV2` block per breaking query.  Only the queries whose
// shape changes need to appear here — everything else is inherited from
// `QueryRoot` through the `MergedQueryV2` composition.
// ---------------------------------------------------------------------------

/// Root type carrying only the resolvers that changed between v1 and v2.
///
/// Leave this struct empty until the first breaking change ships.  An empty
/// `#[Object]` impl is valid in async-graphql when composed via `MergedObject`.
#[derive(Default)]
pub struct QueryRootV2;

#[Object]
impl QueryRootV2 {
    // Replace this placeholder with the actual breaking resolver when ready.
    //
    // Example — renamed field + new optional argument:
    //
    // /// Returns the native balance for `address`, optionally at a given block.
    // async fn native_balance_v2(
    //     &self,
    //     ctx: &Context<'_>,
    //     address: String,
    //     block_number: Option<u64>,
    // ) -> NativeBalanceResult {
    //     todo!()
    // }

    /// Placeholder resolver so the empty `#[Object]` impl compiles.
    ///
    /// Remove this once a real breaking resolver is added.
    #[graphql(name = "_v2Placeholder", deprecation = "internal scaffolding — do not use")]
    async fn placeholder(&self) -> bool {
        false
    }
}

// ---------------------------------------------------------------------------
// Composed v2 root
//
// `MergedQueryV2` merges:
//   - `QueryRootV2` — the resolvers that changed in v2
//   - `QueryRoot`   — every resolver that stayed the same (from query.rs)
//
// async-graphql enforces at compile time that no resolver name appears in both
// halves, so a name collision between a v2 override and the v1 implementation
// it replaces is a hard compile error.  Move the old resolver out of `QueryRoot`
// before adding its replacement here.
// ---------------------------------------------------------------------------

/// Merged query root for schema version 2.
#[derive(MergedObject, Default)]
pub struct MergedQueryV2(QueryRootV2, QueryRoot);
