# blokli-client.nix - Blokli client library package definitions
#
# Defines all variants of the blokli-client library for different platforms and profiles.
# Blokli client is a GraphQL client library for connecting to the Blokli API.

{
  lib,
  builders,
  sources,
  blokliClientCrateInfo,
  rev,
  buildPlatform,
  nixLib,
}:

let
  # Common build arguments for blokli-client variants
  mkblokliClientBuildArgs =
    { src, depsSrc }:
    {
      inherit src depsSrc rev;
      cargoExtraArgs = "-p blokli-client";
      cargoToml = ./../../client/Cargo.toml;
    };

  mkBlokliClientPlatformPackages =
    platform:
    let
      args = mkblokliClientBuildArgs {
        src = sources.main;
        depsSrc = sources.deps;
      };
      name = "binary-blokli-client-${platform}";
    in
    {
      "${name}" = builders.${platform}.callPackage nixLib.mkRustPackage args;
      "${name}-profile" = builders.${platform}.callPackage nixLib.mkRustPackage args;
    }
    // lib.optionalAttrs (lib.hasSuffix "-linux" platform) {
      "${name}-dev" = builders.${platform}.callPackage nixLib.mkRustPackage (
        args // { CARGO_PROFILE = "dev"; }
      );
    };

  blokliClientPlatformPackages = builtins.foldl' (a: b: a // b) { } (
    map mkBlokliClientPlatformPackages [
      "x86_64-linux"
      "aarch64-linux"
      "x86_64-darwin"
      "aarch64-darwin"
    ]
  );
in
{
  # Development builds - for local testing and debugging
  blokli-client = builders.local.callPackage nixLib.mkRustPackage (mkblokliClientBuildArgs {
    src = sources.main;
    depsSrc = sources.deps;
  });

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
// blokliClientPlatformPackages
