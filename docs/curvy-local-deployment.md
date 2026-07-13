# Curvy local contract deployment

Curvy support is opt-in and development-only. The stock Blokli binary and Anvil
image remain HOPR-only.

## Dependency handoff

The review branch pins `curvy-bindings` to a full Curvy Git commit. The repository
must be anonymously readable over HTTPS. After the crate is published, replace the
workspace dependency with the exact registry version and refresh `Cargo.lock`:

```toml
curvy-bindings = { version = "=1.0.0-rc-1" }
```

```bash
cargo update -p curvy-bindings
cargo check --locked -p bloklid --bin blokli-contract-deployer
cargo check --locked -p bloklid --bin blokli-contract-deployer \
  --features curvy-test-deployment
```

After acceptance and the production legal review, publish stable `1.0.0` and move
Blokli to that exact version.

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

Verify the stock regression separately:

```bash
nix build -L .#docker-bloklid-anvil-x86_64-linux --out-link result-stock-image
docker load < result-stock-image
mkdir -p /tmp/blokli-stock-data
docker run --rm -v /tmp/blokli-stock-data:/data bloklid-anvil:latest
test ! -e /tmp/blokli-stock-data/curvy_deployed_addresses.json
```

The Curvy-enabled image must emit the exact Ignition-compatible key set documented
by `curvy-bindings`. Run rs-core's strict `curvy-e2e` flow against ports 8545 and
8080 before handing the image off.
