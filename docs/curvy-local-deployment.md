# Curvy local contract deployment

Curvy indexing is opt-in through `indexer.enable_curvy_indexing` and requires a
non-zero Aggregator address. The stock Blokli binary stays HOPR-only; the local
Anvil image described below is the Curvy development environment and always
deploys the suite.

## Dependencies

Both Curvy crates come from crates.io and are pinned exactly, so a routine
`cargo update` cannot move them:

```toml
curvy-bindings = "=0.1.0-rc.4"
curvy-core = "=0.1.0-rc.2"
```

`curvy-bindings` supplies the contract bindings and the development deployer;
`curvy-core` supplies the notes-tree primitives. Both are release candidates.
When Curvy publishes stable releases, bump the pins together and refresh
`Cargo.lock`:

```bash
cargo update -p curvy-bindings -p curvy-core
cargo check --locked -p bloklid --bin blokli-contract-deployer
cargo check --locked -p bloklid --bin blokli-contract-deployer \
  --features curvy-test-deployment
```

A `curvy-bindings` bump changes how many transactions `deploy_for_testing`
issues, which shifts the absolute block numbers recorded in the Curvy pipeline
snapshots. Re-accept those snapshots after bumping; the event payloads should be
unchanged.

## Linux/Nix validation

Run these checks in the Linux VM from a clean Blokli checkout:

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

Start the image with a writable data directory. The entrypoint writes
`curvy_deployed_addresses.json` only after both HOPR and Curvy deployment succeed.

```bash
mkdir -p /tmp/blokli-curvy-data
docker run --rm --name bloklid-anvil-curvy \
  -e ANVIL_HOST=0.0.0.0 \
  -p 8545:8545 -p 8080:8080 \
  -v /tmp/blokli-curvy-data:/data \
  bloklid-anvil-curvy:latest
```

There is one anvil image and it always deploys Curvy. The entrypoint passes
`--with-curvy` unconditionally and sets `enable_curvy_indexing = true`, and only the
`-curvy` binary's deployer accepts that flag, so there is no stock variant to build
or regression-test against. Deploying the suite and then not indexing its events was
the failure this removed: it surfaces at a consumer as a notes-root mismatch, with
nothing pointing back at the image.

Confirm both halves of that in a running container:

```bash
jq 'keys' /tmp/blokli-curvy-data/curvy_deployed_addresses.json
curl -s http://127.0.0.1:8080/graphql -H 'content-type: application/json' \
  --data '{"query":"{ curvySyncCheckpoint { __typename } }"}'
```

The image must emit the exact Ignition-compatible key set documented by
`curvy-bindings`. Run rs-sdk's `curvy-e2e` flow against ports 8545 and 8080 before
handing the image off.
