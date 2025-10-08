# flake.nix - HOPR blokli Nix flake configuration
#
# This is the main entry point for the Nix flake. It combines modular components
# from the nix/ directory to provide a complete build and development environment.
#
# Structure:
# - nix/inputs.nix: External dependency definitions
# - nix/lib/: Utility functions and builders
# - nix/packages/: Package definitions (bloklid)
# - nix/docker/: Docker image configurations
# - nix/shells/: Development shell environments
# - nix/apps/: Executable scripts and utilities
# - nix/checks.nix: CI/CD quality checks
# - nix/treefmt.nix: Code formatting configuration

{
  description = "HOPR blokli - the companion indexer and chain operator for the HOPR network";

  # External dependencies - kept in main flake for Nix flake requirements
  #
  # INPUTS REFERENCE:
  #
  # Core Nix ecosystem dependencies:
  # - flake-utils: Provides utility functions for working with flakes across multiple systems
  # - flake-parts: Modular flake framework for better organization
  # - nixpkgs: The main Nix package repository (using release 25.05 for stability)
  #
  # Rust toolchain and build system:
  # - rust-overlay: Provides up-to-date Rust toolchains with cross-compilation support
  # - crane: Incremental Rust build system for Nix with excellent caching
  #
  # Development tools and quality assurance:
  # - pre-commit: Git hooks for code quality enforcement
  # - treefmt-nix: Universal code formatter integration for Nix
  # - flake-root: Utilities for finding flake root directory
  #
  # Input optimization strategy:
  # All inputs follow nixpkgs where possible to reduce closure size and improve caching.
  # This is achieved through the "follows" directive below.
  inputs = {
    # Core Nix ecosystem dependencies
    flake-utils.url = "github:numtide/flake-utils";
    flake-parts.url = "github:hercules-ci/flake-parts";
    nixpkgs.url = "github:NixOS/nixpkgs/release-25.05";

    # Rust toolchain and build system
    rust-overlay.url = "github:oxalica/rust-overlay/master";
    crane.url = "github:ipetkov/crane/v0.21.0";

    # Development tools and quality assurance
    pre-commit.url = "github:cachix/git-hooks.nix";
    treefmt-nix.url = "github:numtide/treefmt-nix";
    flake-root.url = "github:srid/flake-root";

    # Input dependency optimization
    flake-parts.inputs.nixpkgs-lib.follows = "nixpkgs";
    pre-commit.inputs.nixpkgs.follows = "nixpkgs";
    rust-overlay.inputs.nixpkgs.follows = "nixpkgs";
    treefmt-nix.inputs.nixpkgs.follows = "nixpkgs";
  };

  outputs =
    {
      self,
      nixpkgs,
      flake-utils,
      flake-parts,
      rust-overlay,
      crane,
      pre-commit,
      ...
    }@inputs:
    flake-parts.lib.mkFlake { inherit inputs; } {
      # Import flake modules for additional functionality
      imports = [
        inputs.treefmt-nix.flakeModule
        inputs.flake-root.flakeModule
      ];

      # Per-system configuration
      # Each system gets its own set of packages, shells, etc.
      perSystem =
        {
          config,
          lib,
          system,
          ...
        }:
        let
          # Git revision for version tracking
          rev = toString (self.shortRev or self.dirtyShortRev);

          # Filesystem utilities for source filtering
          fs = lib.fileset;

          # System configuration
          localSystem = system;

          # Nixpkgs with overlays for Rust
          overlays = [
            (import rust-overlay)
          ];
          pkgs = import nixpkgs {
            system = localSystem;
            inherit overlays;
          };

          # Platform information
          buildPlatform = pkgs.stdenv.buildPlatform;

          # Crane library for Rust builds
          craneLib = (crane.mkLib pkgs).overrideToolchain (p: p.rust-bin.stable.latest.default);

          # bloklid crate information
          bloklidCrateInfoOriginal = craneLib.crateNameFromCargoToml {
            cargoToml = ./bloklid/Cargo.toml;
          };
          bloklidCrateInfo = {
            pname = "bloklid";
            # Normalize version to major.minor.patch for consistent caching
            version = pkgs.lib.strings.concatStringsSep "." (
              pkgs.lib.lists.take 3 (builtins.splitVersion bloklidCrateInfoOriginal.version)
            );
          };

          # Import library modules
          sourcesLib = import ./nix/lib/sources.nix { inherit lib; };
          rustBuildersLib = import ./nix/lib/rust-builders.nix {
            inherit
              nixpkgs
              rust-overlay
              crane
              localSystem
              ;
          };

          # Create source trees for different build contexts
          sources = {
            main = sourcesLib.mkSrc {
              root = ./.;
              inherit fs;
            };
            test = sourcesLib.mkTestSrc {
              root = ./.;
              inherit fs;
            };
            deps = sourcesLib.mkDepsSrc {
              root = ./.;
              inherit fs;
            };
          };

          # Create all Rust builders for cross-compilation
          builders = rustBuildersLib.mkAllBuilders { };

          # Import package definitions
          bloklidPackages = import ./nix/packages/bloklid.nix {
            inherit
              lib
              builders
              sources
              bloklidCrateInfo
              rev
              buildPlatform
              ;
          };

          # Combine all packages
          packages = bloklidPackages // {
            # Additional standalone packages

            # Pre-commit hooks check
            pre-commit-check = pkgs.callPackage ./nix/packages/pre-commit-check.nix {
              inherit pre-commit system config;
            };

            # Man pages - import as individual packages
            bloklid-man =
              (pkgs.callPackage ./nix/man-pages.nix {
                bloklid = bloklidPackages.bloklid-dev;
              }).bloklid-man;
          };

          # Import Docker configurations
          dockerBuilder = import ./nix/docker-builder.nix;
          bloklidDocker = import ./nix/docker/bloklid.nix {
            inherit pkgs dockerBuilder;
            packages = bloklidPackages;
          };

          # Import application definitions
          dockerUploadLib = import ./nix/apps/docker-upload.nix {
            inherit pkgs flake-utils;
          };
          utilities = import ./nix/apps/utilities.nix {
            inherit pkgs system flake-utils;
          };

          # Import unified shell configuration
          shells = {
            default = import ./nix/shells/default.nix {
              inherit pkgs config crane;
              pre-commit-check = packages.pre-commit-check;
            };
          };

          # Import checks
          checks = import ./nix/checks.nix {
            inherit pkgs bloklidCrateInfo;
            packages = bloklidPackages;
          };

          # Import treefmt configuration
          treefmtConfig = import ./nix/treefmt.nix {
            inherit config pkgs;
          };
        in
        {
          # Configure treefmt
          treefmt = treefmtConfig;

          # Export checks for CI
          inherit checks;

          # Export applications
          apps = {
            # Docker upload scripts
            bloklid-docker-build-and-upload = dockerUploadLib.mkDockerUploadApp bloklidDocker.bloklid-docker;
            bloklid-dev-docker-build-and-upload = dockerUploadLib.mkDockerUploadApp bloklidDocker.bloklid-dev-docker;
            bloklid-profile-docker-build-and-upload = dockerUploadLib.mkDockerUploadApp bloklidDocker.bloklid-profile-docker;

            # Utility scripts
            inherit (utilities)
              update-github-labels
              audit
              check
              ;
          };

          # Export packages
          packages = packages // {
            # Docker images
            inherit (bloklidDocker) bloklid-docker bloklid-dev-docker bloklid-profile-docker;

            # Set default package
            default = bloklidPackages.bloklid;
          };

          # Export development shells
          devShells = shells;

          # Export formatter
          formatter = config.treefmt.build.wrapper;
        };

      # Supported systems for building
      systems = [
        "x86_64-linux"
        "aarch64-linux"
        "aarch64-darwin"
        "x86_64-darwin"
      ];
    };
}
