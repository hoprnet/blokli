# curvy-bindings

Rust bindings for the Curvy v2 contract suite — a faithful structural mirror of
hoprnet's [`hopr-bindings`](https://crates.io/crates/hopr-bindings) crate (4.9.1), so
that blokli/hoprnet-side consumers see exactly the pattern they already use:

```
curvy-bindings/
  src/codegen/*.rs      committed `forge bind --alloy` output, one snake_case module
                        per contract, #[rustfmt::skip] via lib.rs (never hand-edited;
                        regenerate with ../generate.sh)
  src/config.rs         CurvyContractAddresses (named serde slots, DisplayFromStr)
                        CurvyContractInstances<P>:
                          new(&addresses, provider)
                          deploy_for_testing(provider, deployer)   ← the FULL suite:
                            CreateX bootstrap → PoseidonT4 → aggregator impl (library-
                            linked) → ERC1967 proxies + initialize → 3 verifiers +
                            registration → PortalFactory (CreateX deployCreate2) →
                            Multicall3/ERC20Mock → bilateral wiring → dev funding →
                            setPerTokenGasFees + setFeeNotePublicKey → read-back verify
                          get_contract_addresses()
  src/constants.rs      dev/protocol constants: CreateX signed deploy tx + addresses,
                        create2 salt, verifier dims, dev gas-fee table, PRECOMPUTED
                        commitment-gas-fee root (no arkworks in this crate, ever),
                        DEV_FEE_COLLECTOR key, unlinked aggregator bytecode
  src/lib.rs            `pub use codegen::*` + `exports { pub use alloy }`
```

A blokli-style consumer does exactly one thing:

```rust
let curvy = CurvyContractInstances::deploy_for_testing(provider.clone(), deployer).await?;
let addresses = curvy.get_contract_addresses();   // → serialize / to_ignition_json()
```

The deploy/wire/init logic is a 1:1 port of the validated `curvy-deployer` crate
(rs-core `sdk/curvy-deployer`, which itself replicates `Devenv.ts` minus ENS and
passed full regression repeatedly) — ported, not reinvented.

## Versioning policy (lockstep)

This crate's version tracks the contracts package version
(`packages/contracts/evm/package.json`, currently **1.0.0**), the same way
hopr-bindings versions in lockstep with hoprnet/contracts releases. On every contract
release: bump both, rerun `../generate.sh`, commit the refreshed codegen.

Hosting is postponed (owner decision): consumed as a path dependency for now;
git/crates.io hosting is the final step and changes only the consumer's `Cargo.toml`
line.

## Codegen & determinism

```
../generate.sh           # regenerate src/codegen/** + the unlinked aggregator bytecode
../generate.sh --check   # verify committed codegen is reproducible (regenerate → diff)
```

- Generator: `forge bind --alloy --module` pinned to **forge 1.5.1**
  (`foundryup -i 1.5.1`; the script uses `~/.foundry/versions/v1.5.1/forge` directly,
  the active foundry default is untouched). forge 1.5.1 emits the same codegen shape
  as hopr-bindings 4.9.1 (`FooInstance<P, N>`, snake_case module files).
- Compiler profile: the package-root `foundry.toml` mirrors `hardhat.config.ts`
  (solc 0.8.28, optimizer runs 200, evm cancun, ipfs metadata). Forge output is
  isolated under `curvy-bindings/.forge/` — Hardhat's `cache/`/`artifacts/` are never
  touched, the two toolchains coexist.
- Deterministic: same sources + same pinned forge ⇒ byte-identical output
  (`--check` proves it).

## Bytecode parity gate

`generate.sh` hard-fails unless, for EVERY bound contract, the forge-built creation
bytecode matches the Hardhat artifact the validated deploy pipeline used. Result at
generation time (all 11 contracts):

| contract | verdict |
|---|---|
| CurvyVaultV2, CurvyAggregatorAlphaV2, Portal, CurvyAggregationVerifier, CurvyPendingNotesCommitmentVerifier, CurvyWithdrawalVerifier, PoseidonT4, ERC1967Proxy, ERC20Mock, Multicall3 | MODULO-METADATA (1 CBOR blob) |
| PortalFactory | MODULO-METADATA (3 CBOR blobs — it embeds Portal/SolanaPortal creation code, each carrying its own blob) |

“MODULO-METADATA” means: byte-identical executable bytecode; the ONLY difference is
the solc CBOR metadata blob (`a26469706673…0033`, the ipfs hash of the compiler
input), which legitimately differs between the Hardhat and Foundry builds of the same
source (different source-unit naming, e.g. Hardhat's `project/src/...` vs forge's
`src/...`). Library link placeholders (`__$…$__`, keccak of the build-relative fully
qualified library name) are normalised before comparison for the same reason.

**Known consequence**: PortalFactory is deployed via CreateX `deployCreate2`, and
CREATE2 hashes the *full* init code including metadata blobs — so its deterministic
address moves from the Hardhat pipeline's `0x3c0C573B618D88F1a370bf18000f437c450D8125`
to `0x410607362be76701CcE07841281e7352E63f2072` (still fully deterministic per build).
No consumer hardcodes it; the address flows through the emitted addresses JSON.

## Library linking (PoseidonT4)

`CurvyAggregatorAlphaV2` links the `PoseidonT4` library, so `forge bind` emits its
codegen module without a `BYTECODE` static. The unlinked creation bytecode is carried
in `curvy_aggregator_alpha_v2_unlinked.hex` (regenerated by `generate.sh`, included
via `include_str!` in `constants.rs`); `deploy_for_testing` substitutes the single
20-byte link placeholder with the freshly deployed library address — the same proven
scheme the validated deployer used.

## Deliberate deviations from hopr-bindings

- **No `contracts-addresses.json` / `build.rs` / address-dump bin**: hopr bundles a
  book of *published network* addresses; no such book exists for Curvy yet (localnet
  addresses are deploy-order-dependent). Add the same machinery when Curvy has
  published deployments.
- **`to_ignition_json()` on the addresses type**: Curvy's downstream consumers
  (curvy-e2e / curvy-hopr-runner) read the Hardhat-Ignition-style
  `deployed_addresses.json`; the exact key set is a stable contract and lives here.
- **Read-back verification inside `deploy_for_testing`** (assert-style, matching
  hopr's own `assert!` post-conditions in `deploy_safe_suites`): the two init calls
  are consensus-critical for the circuits, so the round-trip check from the validated
  deployer is kept.
- **Dependency set trimmed** to what's actually used (no `anyhow`/`thiserror`/
  `hex-literal`/`tokio` in the lib; `tokio` is dev-only). `alloy` is pinned `=2.1.0`
  with hopr-bindings' exact feature list.
- `mod.rs` keeps forge 1.5.1's raw-identifier module lines (`pub mod r#…;`) verbatim —
  we commit what the tool emits, unlike hoprnet's post-processed variant without `r#`.
