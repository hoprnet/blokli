# Curvy local contract deployment

Curvy indexing is opt-in through `indexer.enable_curvy_indexing` and requires a
non-zero Aggregator proxy supplied through `curvy_aggregator`. During startup,
Blokli reads `curvyVault()` and `portalFactory()` from that Aggregator and uses
the resolved addresses for typed contract reads. Curvy configuration therefore
does not replace the HOPR addresses resolved for the selected network.

The stock Blokli binary and Anvil image remain HOPR-only. The explicitly named
Curvy Anvil image compiles the development deployer, deploys both suites, and
enables Curvy indexing.

## Dependencies

Both Curvy crates come from crates.io and are pinned exactly, so a routine
`cargo update` cannot move them:

```toml
curvy-bindings = "=0.1.0-rc.5"
curvy-core = "=0.1.0-rc.3"
```

`curvy-bindings` supplies the contract bindings and development deployer;
`curvy-core` supplies the notes-tree primitives. Both are release candidates.
When Curvy publishes stable releases, bump the pins together and refresh
`Cargo.lock`:

```bash
cargo update -p curvy-bindings -p curvy-core
cargo check --locked -p bloklid --bin blokli-contract-deployer
cargo check --locked -p bloklid --bin blokli-contract-deployer \
  --features curvy-test-deployment
```

A `curvy-bindings` bump can change how many transactions `deploy_for_testing`
issues, which shifts the absolute block numbers recorded in the Curvy pipeline
snapshots. Re-accept those snapshots after verifying that event payloads and
ordering are unchanged.

## Linux/Nix validation

Run these checks in the Linux development environment from a clean checkout:

```bash
cargo fetch --locked
cargo check --locked --offline -p bloklid --bin blokli-contract-deployer
cargo check --locked --offline -p bloklid --bin blokli-contract-deployer \
  --features curvy-test-deployment

nix build -L .#binary-bloklid-x86_64-linux-curvy
nix build -L .#binary-bloklid-aarch64-linux-curvy
nix build -L .#docker-bloklid-anvil-curvy-x86_64-linux \
  --out-link result-curvy-image
docker load < result-curvy-image
```

Start the Curvy image with a writable data directory. Its entrypoint passes
`--with-curvy`, enables Curvy indexing, and publishes
`curvy_deployed_addresses.json` only after both HOPR and Curvy deployment
succeed. It also registers the local wxHOPR contract in the Curvy Vault after
the native asset and Curvy mock token, so the local wxHOPR token id is `3`.

```bash
mkdir -p /tmp/blokli-curvy-data
docker run --rm --name bloklid-anvil-curvy \
  -e ANVIL_HOST=0.0.0.0 \
  -p 8545:8545 -p 8080:8080 \
  -v /tmp/blokli-curvy-data:/data \
  bloklid-anvil-curvy:latest
```

Confirm that the address artifact exists and that the Curvy API is active:

```bash
jq 'keys' /tmp/blokli-curvy-data/curvy_deployed_addresses.json
curl -s http://127.0.0.1:8080/graphql -H 'content-type: application/json' \
  --data '{"query":"{ curvySyncCheckpoint { __typename } }"}'
```

The Curvy image must emit the exact Ignition-compatible key set documented by
`curvy-bindings`. Run `just smoke-test-curvy` to validate the deployment
artifact, generated Blokli configuration, on-chain bytecode, and local wxHOPR
registration. Validate the stock regression separately with
`docker-bloklid-anvil-x86_64-linux`; it must not emit the Curvy address artifact.
