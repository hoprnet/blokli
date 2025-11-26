# blokli-client.nix - Blokli client library package definitions
#
# Defines the development variant of the blokli-client Rust crate.
# Blokli client is a GraphQL client library for connecting to the Blokli API.

{
  lib,
  builders,
  sources,
  blokliClientCrateInfo,
  rev,
  nixLib,
}:

let
  # Common build arguments for blokli-client variants
  mkblokliClientBuildArgs =
    { src, depsSrc }:
    {
      inherit src depsSrc rev;
      cargoExtraArgs = "";
      cargoToml = ./../../client/Cargo.toml;
    };
in
{
  # Development build - for local testing and debugging
  blokli-client-dev = builders.local.callPackage nixLib.mkRustPackage (
    (mkblokliClientBuildArgs {
      src = sources.main;
      depsSrc = sources.deps;
    })
    // {
      CARGO_PROFILE = "dev";
    }
  );
}
