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
  # - flake-parts: Modular flake framework for better organization
  # - nixpkgs: The main Nix package repository (using release 25.05 for stability)
  # - nix-lib: HOPR Nix library with reusable Rust build functions
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
    flake-parts.url = "github:hercules-ci/flake-parts";
    nixpkgs.url = "github:NixOS/nixpkgs/release-25.05";
    nixpkgs-unstable.url = "github:NixOS/nixpkgs/nixos-unstable";

    # HOPR Nix Library (provides rust-overlay, crane, flake-utils)
    nix-lib.url = "github:hoprnet/nix-lib";

    # Development tools and quality assurance
    pre-commit.url = "github:cachix/git-hooks.nix";
    flake-root.url = "github:srid/flake-root";

    # Input dependency optimization
    flake-parts.inputs.nixpkgs-lib.follows = "nixpkgs";
    pre-commit.inputs.nixpkgs.follows = "nixpkgs";
    nix-lib.inputs.nixpkgs.follows = "nixpkgs";
  };

  outputs =
    {
      self,
      nixpkgs,
      nixpkgs-unstable,
      flake-parts,
      nix-lib,
      pre-commit,
      ...
    }@inputs:
    flake-parts.lib.mkFlake { inherit inputs; } {
      # Import flake modules for additional functionality
      imports = [
        inputs.nix-lib.flakeModules.default
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

          # Nixpkgs with rust-overlay (from nix-lib)
          overlays = [
            (import nix-lib.inputs.rust-overlay)
          ];
          pkgs = import nixpkgs {
            inherit system overlays;
          };
          pkgsUnstable = import nixpkgs-unstable {
            inherit system overlays;
          };

          # Platform information
          buildPlatform = pkgs.stdenv.buildPlatform;

          # Import nix-lib for this system
          nixLib = nix-lib.lib.${system};

          # Crane library for Rust builds (for crate info extraction)
          craneLib = (nix-lib.inputs.crane.mkLib pkgs).overrideToolchain (
            p: p.rust-bin.stable.latest.default
          );

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

          # Create source trees for different build contexts using nix-lib
          sources = {
            main = nixLib.mkSrc {
              root = ./.;
              inherit fs;
            };
            test = nixLib.mkTestSrc {
              root = ./.;
              inherit fs;
            };
            deps = nixLib.mkDepsSrc {
              root = ./.;
              inherit fs;
            };
          };

          # Create all Rust builders for cross-compilation using nix-lib
          builders = nixLib.mkRustBuilders {
            rustToolchainFile = ./rust-toolchain.toml;
          };

          # Import package definitions
          bloklidPackages = import ./nix/packages/bloklid.nix {
            inherit
              lib
              builders
              sources
              bloklidCrateInfo
              rev
              buildPlatform
              nixLib
              ;
          };

          # Combine all packages
          packages = bloklidPackages // {
            # Additional standalone packages

            # Pre-commit hooks check
            pre-commit-check = pkgs.callPackage ./nix/packages/pre-commit-check.nix {
              inherit pre-commit system config;
            };

            # Man pages
            bloklid-man = nixLib.mkManPage {
              pname = "bloklid";
              binary = bloklidPackages.bloklid-dev;
              description = "BLOKLID node executable";
            };
          };

          # Import Docker configurations
          # Docker images need Linux packages, even when building on macOS
          pkgsLinux = import nixpkgs {
            system = "x86_64-linux";
            inherit overlays;
          };

          # Docker images using nix-lib
          bloklidDocker = {
            bloklid-docker = nixLib.mkDockerImage {
              name = "bloklid";
              Entrypoint = [ "${bloklidPackages.bloklid-x86_64-linux}/bin/bloklid" ];
              pkgsLinux = pkgsLinux;
            };
            bloklid-dev-docker = nixLib.mkDockerImage {
              name = "bloklid-dev";
              Entrypoint = [ "${bloklidPackages.bloklid-x86_64-linux-dev}/bin/bloklid" ];
              pkgsLinux = pkgsLinux;
            };
            bloklid-profile-docker = nixLib.mkDockerImage {
              name = "bloklid-profile";
              Entrypoint = [ "${bloklidPackages.bloklid-x86_64-linux-profile}/bin/bloklid" ];
              pkgsLinux = pkgsLinux;
            };
          };

          # Application definitions using nix-lib
          dockerUploadApps = {
            bloklid-docker-build-and-upload = nixLib.mkDockerUploadApp bloklidDocker.bloklid-docker;
            bloklid-dev-docker-build-and-upload = nixLib.mkDockerUploadApp bloklidDocker.bloklid-dev-docker;
            bloklid-profile-docker-build-and-upload = nixLib.mkDockerUploadApp bloklidDocker.bloklid-profile-docker;
          };

          utilityApps = {
            update-github-labels = nixLib.mkUpdateGithubLabelsApp;
            audit = nixLib.mkAuditApp;
            check = nixLib.mkCheckApp { inherit system; };
          };

          # Rust toolchains
          stableToolchain =
            (pkgs.pkgsBuildHost.rust-bin.fromRustupToolchainFile ./rust-toolchain.toml).override
              {
                targets = [
                  (
                    if buildPlatform.config == "arm64-apple-darwin" then
                      "aarch64-apple-darwin"
                    else
                      buildPlatform.config
                  )
                ];
              };

          nightlyToolchain = (pkgs.pkgsBuildHost.rust-bin.nightly.latest.default).override {
            targets = [
              (
                if buildPlatform.config == "arm64-apple-darwin" then
                  "aarch64-apple-darwin"
                else
                  buildPlatform.config
              )
            ];
            extensions = [
              "rust-src"
              "rust-analyzer"
              "clippy"
              "rustfmt"
            ];
          };

          # Development shells using nix-lib
          shells = {
            default = nixLib.mkDevShell {
              rustToolchain = stableToolchain;
              shellName = "Development";
              treefmtWrapper = config.treefmt.build.wrapper;
              treefmtPrograms = pkgs.lib.attrValues config.treefmt.build.programs;
              includePostgres = true;
              postgresPackage = pkgsUnstable.postgresql_18;
              shellHook = packages.pre-commit-check.shellHook;
            };

            experiment = nixLib.mkDevShell {
              rustToolchain = nightlyToolchain;
              shellName = "Experimental Nightly";
              treefmtWrapper = config.treefmt.build.wrapper;
              treefmtPrograms = pkgs.lib.attrValues config.treefmt.build.programs;
              includePostgres = true;
              postgresPackage = pkgsUnstable.postgresql_18;
              shellHook = packages.pre-commit-check.shellHook;
            };
          };

          # Import checks
          checks = import ./nix/checks.nix {
            inherit pkgs bloklidCrateInfo;
            packages = bloklidPackages;
          };
        in
        {
          # Configure treefmt using nix-lib options
          nix-lib.treefmt = {
            globalExcludes = [
              # Generated code - don't format to avoid churn
              "db/entity/src/codegen/*"

              # External configuration
              "deploy/compose/grafana/config.monitoring"
              "deploy/nfpm/nfpm.yaml"
              ".github/workflows/build-binaries.yaml"

              # Documentation and test data
              "docs/*"

              # Other specific files
              "bloklid/.dockerignore"
              "tests/pytest.ini"
            ];
            extraFormatters = {
              settings.formatter.shfmt.includes = [
                "*.sh"
                "deploy/compose/.env.sample"
                "deploy/compose/.env-secrets.sample"
              ];
              settings.formatter.yamlfmt.includes = [
                ".github/labeler.yml"
                ".github/workflows/*.yaml"
              ];
            };
          };

          # Export checks for CI
          inherit checks;

          # Export applications using nix-lib
          apps = dockerUploadApps // utilityApps;

          # Export packages
          packages = packages // {
            # Docker images
            inherit (bloklidDocker) bloklid-docker bloklid-dev-docker bloklid-profile-docker;

            # Set default package
            default = bloklidPackages.bloklid;
          };

          # Export development shells
          devShells = shells;

          # Formatter is automatically exported by nix-lib.flakeModules.default
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
