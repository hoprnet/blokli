# Query Update Guide

Blokli uses **header-based schema versioning** (`X-Blokli-Schema-Version: N`) to maintain backward compatibility across server and client
deployments. Each schema version is a complete GraphQL schema; unchanged resolvers live in `QueryRoot` / `SubscriptionRoot` and are composed
into every version via `#[MergedObject]` / `#[MergedSubscription]`.

Clients that omit the header are served **v1** (the original contract). Updated clients send the version they were built for. Old and new
clients can safely coexist against the same server.

---

## Decision tree

```
Is the change backward-compatible?
├─ Yes → Non-breaking path (section 1)
└─ No  → Breaking path (section 2 or 3)
```

---

## 1. Non-breaking changes (no new version needed)

A change is non-breaking when **existing clients continue to work unchanged**:

- Adding an optional field to an existing type
- Adding a completely new query / mutation / subscription
- Changing internal resolver logic without altering the schema shape
- Adding a new argument with a default value

### Steps

1. Implement the change directly in the existing resolver or in `QueryRoot` / `SubscriptionRoot`.
2. Run `just quick && just test`.
3. Optionally update `client/` types if the client needs to use the new field.

No version bump is needed.

---

## 2. Breaking change to an existing query or mutation

A change is breaking when **existing clients would receive a validation error or a semantically different result** if served the new schema:

- Removing a field or argument
- Renaming a field or argument
- Changing an argument from optional to required
- Changing the return type in an incompatible way

### Steps

#### Server side

1. **Determine the next version number** — currently `LATEST_SCHEMA_VERSION` in `api/src/schema.rs`. If the current latest is `N`, the new
   version is `N+1`.

2. **Create the new resolver** in a versioned module, e.g. `api/src/query_v2.rs`:

   ```rust
   // Only implement the resolver(s) that change.
   // Unchanged resolvers stay in QueryRoot and are composed in automatically.
   #[derive(Default)]
   pub struct QueryRootV2;

   #[Object]
   impl QueryRootV2 {
       async fn my_query(&self, ctx: &Context<'_>, new_arg: String) -> MyType {
           // new shape
       }
   }
   ```

3. **Compose with the shared resolvers** using `#[MergedObject]`:

   ```rust
   // api/src/query_v2.rs (or schema_v2.rs)
   #[derive(MergedObject, Default)]
   pub struct MergedQueryV2(QueryRootV2, QueryRoot);
   ```

   `QueryRoot` contains every query that was NOT changed between versions.

4. **Register the new schema version** in `build_version_registry` (`api/src/schema.rs`):

   ```rust
   pub fn build_version_registry(...) -> HashMap<u32, Arc<dyn ErasedSchema>> {
       let v1: Arc<dyn ErasedSchema> = Arc::new(build_schema_v1(...));
       let v2: Arc<dyn ErasedSchema> = Arc::new(build_schema_v2(...));
       HashMap::from([(1, v1), (2, v2)])
   }
   ```

5. **Bump `LATEST_SCHEMA_VERSION`** in `api/src/schema.rs`:

   ```rust
   pub const LATEST_SCHEMA_VERSION: u32 = 2;
   ```

6. Run `just quick && just test`.

#### Client side

1. **Bump `SCHEMA_VERSION`** in `client/src/client/mod.rs`:

   ```rust
   pub const SCHEMA_VERSION: u32 = 2;
   ```

   The client sends this in every request header. Old clients keep sending `1` and continue to be served by the v1 schema.

2. **Update the cynic types** in `client/src/api/v1/graphql/` (or add a `v2/` module) to match the new query shape.

3. Run `just quick && just test`.

---

## 3. Breaking change to an existing subscription

Same principle as section 2 but for subscription roots.

1. Create the new subscription resolver in a versioned module.
2. Compose with `SubscriptionRoot` using `#[MergedSubscription]`.
3. Build a new `Schema<MergedQueryVN, MutationRoot, MergedSubscriptionVN>` in the registry.
4. Bump `LATEST_SCHEMA_VERSION` and the client's `SCHEMA_VERSION`.

---

## 4. Removing a query entirely

Removing a query is a breaking change — existing clients that call it will receive a GraphQL validation error.

1. Follow the breaking change steps (section 2): create v(N+1) where the query is absent.
2. Keep v(N) in the registry until all deployed clients have migrated to v(N+1).
3. Once the transition window has passed, remove v(N) from `build_version_registry`.

---

## 5. Version lifecycle and retirement

- Keep the previous version(s) in the registry while old clients are still in deployment.
- Retire a version by removing its entry from `build_version_registry` and deleting its resolver module. Old clients will receive a
  `400 UNSUPPORTED_SCHEMA_VERSION` error — a clear signal to upgrade.
- A reasonable retirement window is one full deployment cycle (typically one release after the breaking change ships).

---

## Quick reference

| Scenario                          | New version? | `LATEST_SCHEMA_VERSION` bump? | Client `SCHEMA_VERSION` bump? |
| --------------------------------- | ------------ | ----------------------------- | ----------------------------- |
| Add optional field                | No           | No                            | No (optional)                 |
| Add new query / subscription      | No           | No                            | No (optional)                 |
| Remove / rename field or argument | Yes          | Yes                           | Yes                           |
| Change return type incompatibly   | Yes          | Yes                           | Yes                           |
| Remove query entirely             | Yes          | Yes                           | Yes                           |
