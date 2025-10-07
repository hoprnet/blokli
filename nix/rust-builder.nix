# rust-builder.nix - Cross-compilation Rust builder factory
#
# Creates Rust build environments for cross-compilation to different platforms.
# Configures toolchains, linkers, and platform-specific settings for building
# Rust applications that can run on various architectures and operating systems.

{
  crane, # Crane build system for Rust
  crossSystem ? localSystem, # Target system for cross-compilation
  isCross ? false, # Whether this is a cross-compilation build
  isStatic ? false, # Whether to create statically linked binaries
  localSystem, # Host system where compilation occurs
  nixpkgs, # Nixpkgs package set
  rust-overlay, # Rust toolchain overlay
  useRustNightly ? false, # Whether to use nightly Rust toolchain
}@args:
let
  crossSystem0 = crossSystem;
in
let
  pkgsLocal = import nixpkgs {
    localSystem = args.localSystem;
    overlays = [
      rust-overlay.overlays.default
    ];
  };

  localSystem = pkgsLocal.lib.systems.elaborate args.localSystem;
  crossSystem =
    let
      system = pkgsLocal.lib.systems.elaborate crossSystem0;
    in
    if crossSystem0 == null || pkgsLocal.lib.systems.equals system localSystem then
      localSystem
    else
      system;

  pkgs = import nixpkgs {
    inherit localSystem crossSystem;
    overlays = [
      rust-overlay.overlays.default
    ];
  };

  # `buildPlatform` is the local host platform
  # `hostPlatform` is the cross-compilation output platform
  buildPlatform = pkgs.stdenv.buildPlatform;
  hostPlatform = pkgs.stdenv.hostPlatform;

  envCase = triple: pkgsLocal.lib.strings.toUpper (builtins.replaceStrings [ "-" ] [ "_" ] triple);

  cargoTarget =
    if hostPlatform.config == "arm64-apple-darwin" then "aarch64-apple-darwin" else hostPlatform.config;

  rustToolchainFun =
    if useRustNightly then
      p: p.rust-bin.selectLatestNightlyWith (toolchain: toolchain.default)
    else
      p:
      (p.pkgsBuildHost.rust-bin.fromRustupToolchainFile ../rust-toolchain.toml).override {
        targets = [ cargoTarget ];
      };

  craneLib = (crane.mkLib pkgs).overrideToolchain rustToolchainFun;

  # mold is only supported on Linux builds, so falling back to lld for Darwin
  linker = if buildPlatform.isDarwin then "lld" else "mold";

  buildEnvBase = {
    CARGO_BUILD_TARGET = cargoTarget;
    "CARGO_TARGET_${envCase cargoTarget}_LINKER" = "${pkgs.stdenv.cc.targetPrefix}cc";
    HOST_CC = "${pkgs.stdenv.cc.nativePrefix}cc";
  };
  buildEnvCross =
    if isCross then
      {
        # For cross-compilation, don't use mold/lld as it can cause issues
        CARGO_BUILD_RUSTFLAGS = "";
      }
    else
      {
        CARGO_BUILD_RUSTFLAGS = "-C link-arg=-fuse-ld=${linker}";
      };
  buildEnvStatic =
    if isStatic then
      {
        CARGO_BUILD_RUSTFLAGS = "${buildEnvCross.CARGO_BUILD_RUSTFLAGS} -C target-feature=+crt-static";
      }
    else
      { };

  buildEnv = buildEnvBase // buildEnvCross // buildEnvStatic;

in
{
  callPackage = (
    package: args:
    let
      crate = pkgs.callPackage package (
        args
        // {
          inherit
            craneLib
            isCross
            isStatic
            ;
        }
      );
    in
    # Override the derivation to add cross-compilation environment variables.
    crate.overrideAttrs (
      previous:
      buildEnv
      // {
        # We also have to override the `cargoArtifacts` derivation with the same changes.
        cargoArtifacts =
          if previous.cargoArtifacts != null then
            previous.cargoArtifacts.overrideAttrs (previous: buildEnv)
          else
            null;
      }
    )
  );
}
