# default.nix - Unified shell configuration
#
# Single comprehensive development environment that includes all tools needed
# for development, testing, CI/CD, and documentation workflows.
#
# Usage:
#   nix develop                          # Enter the development shell

{
  pkgs,
  pkgsUnstable,
  config,
  crane,
  pre-commit-check ? null,
  extraPackages ? [ ],
}:

let
  buildPlatform = pkgs.stdenv.buildPlatform;

  # Determine cargo target based on platform
  cargoTarget =
    if buildPlatform.config == "arm64-apple-darwin" then
      "aarch64-apple-darwin"
    else
      buildPlatform.config;

  # Use Rust toolchain from rust-toolchain.toml
  rustToolchain =
    (pkgs.pkgsBuildHost.rust-bin.fromRustupToolchainFile ../../rust-toolchain.toml).override
      {
        targets = [ cargoTarget ];
      };

  craneLib = (crane.mkLib pkgs).overrideToolchain rustToolchain;

  # Platform-specific packages
  linuxPackages = pkgs.lib.optionals pkgs.stdenv.isLinux (
    with pkgs;
    [
      mold # Fast linker (Linux only)
      autoPatchelfHook
    ]
  );

  # All packages in a single comprehensive environment
  allPackages =
    with pkgs;
    [
      # Core build tools
      bash
      coreutils
      curl
      findutils
      gnumake
      jq
      just
      llvmPackages.bintools
      lsof
      openssl
      patchelf
      pkg-config
      time
      which
      help2man

      # Database tools
      pkgsUnstable.postgresql_18 # PostgreSQL client (psql, pg_dump, etc.)

      # Formatting tools
      config.treefmt.build.wrapper

      # CI/CD tools
      lcov # Code coverage
      skopeo # Container image tools
      cargo-audit # Rust security auditing
      dive # Docker layer analysis
    ]
    ++ (pkgs.lib.attrValues config.treefmt.build.programs)
    ++ linuxPackages
    ++ extraPackages;

  # Pre-commit hooks if available
  preCommitHook = if pre-commit-check != null then pre-commit-check.shellHook else "";

  shellHook = preCommitHook;

  # mold is only supported on Linux, so falling back to lld on Darwin
  linker = if buildPlatform.isDarwin then "lld" else "mold";
in
craneLib.devShell {
  inherit shellHook;
  packages = allPackages;

  LD_LIBRARY_PATH = pkgs.lib.makeLibraryPath (
    [
      pkgs.pkgsBuildHost.openssl
      pkgs.pkgsBuildHost.curl
    ]
    ++ pkgs.lib.optionals pkgs.stdenv.isLinux [ pkgs.pkgsBuildHost.libgcc.lib ]
  );

  CARGO_BUILD_RUSTFLAGS = "-C link-arg=-fuse-ld=${linker}";
}
