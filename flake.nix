# flake.nix - HOPR blokli Nix flake configuration
#
# This is the main entry point for the Nix flake. It uses the HOPR nix-lib
# for reusable Rust build functions and formatting configuration.
#
# Structure:
# - nix/packages/: Package definitions (bloklid)
# - nix/checks.nix: CI/CD quality checks
# - nix-lib (external): Rust builders, Docker images, treefmt, and utilities

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

    # HOPR Nix Library (provides flake-utils and reusable build functions)
    nix-lib.url = "github:hoprnet/nix-lib";

    # Rust build system
    crane.url = "github:ipetkov/crane";
    rust-overlay.url = "github:oxalica/rust-overlay";

    # Development tools and quality assurance
    pre-commit.url = "github:cachix/git-hooks.nix";
    flake-root.url = "github:srid/flake-root";
    foundry.url = "github:shazow/foundry.nix";
    solc.url = "github:hellwolf/solc.nix";

    # Input dependency optimization
    flake-parts.inputs.nixpkgs-lib.follows = "nixpkgs";
    pre-commit.inputs.nixpkgs.follows = "nixpkgs";
    nix-lib.inputs.nixpkgs.follows = "nixpkgs";
    nix-lib.inputs.crane.follows = "crane";
    nix-lib.inputs.rust-overlay.follows = "rust-overlay";
    rust-overlay.inputs.nixpkgs.follows = "nixpkgs";
    foundry.inputs.nixpkgs.follows = "nixpkgs";
    solc.inputs.nixpkgs.follows = "nixpkgs";
  };

  outputs =
    {
      self,
      nixpkgs,
      nixpkgs-unstable,
      flake-parts,
      nix-lib,
      crane,
      rust-overlay,
      pre-commit,
      foundry,
      solc,
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

          # Nixpkgs with rust-overlay, foundry overlay, and solc overlay
          overlays = [
            rust-overlay.overlays.default
            foundry.overlay
            solc.overlay
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

          pkgsLinuxAarch64 = import nixpkgs {
            system = "aarch64-linux";
            inherit overlays;
          };

          # Docker images using nix-lib
          bloklidDocker = {
            # x86_64-linux Docker images
            bloklid-docker-amd64 = nixLib.mkDockerImage {
              name = "bloklid";
              Entrypoint = [ "${bloklidPackages.bloklid-x86_64-linux}/bin/bloklid" ];
              pkgsLinux = pkgsLinux;
            };
            bloklid-dev-docker-amd64 = nixLib.mkDockerImage {
              name = "bloklid-dev";
              Entrypoint = [ "${bloklidPackages.bloklid-x86_64-linux-dev}/bin/bloklid" ];
              pkgsLinux = pkgsLinux;
            };
            bloklid-profile-docker-amd64 = nixLib.mkDockerImage {
              name = "bloklid-profile";
              Entrypoint = [ "${bloklidPackages.bloklid-x86_64-linux-profile}/bin/bloklid" ];
              pkgsLinux = pkgsLinux;
            };

            # aarch64-linux Docker images
            bloklid-docker-aarch64 = nixLib.mkDockerImage {
              name = "bloklid";
              Entrypoint = [ "${bloklidPackages.bloklid-aarch64-linux}/bin/bloklid" ];
              pkgsLinux = pkgsLinuxAarch64;
            };
            bloklid-dev-docker-aarch64 = nixLib.mkDockerImage {
              name = "bloklid-dev";
              Entrypoint = [ "${bloklidPackages.bloklid-aarch64-linux-dev}/bin/bloklid" ];
              pkgsLinux = pkgsLinuxAarch64;
            };
            bloklid-profile-docker-aarch64 = nixLib.mkDockerImage {
              name = "bloklid-profile";
              Entrypoint = [ "${bloklidPackages.bloklid-aarch64-linux-profile}/bin/bloklid" ];
              pkgsLinux = pkgsLinuxAarch64;
            };
          };

          # Docker security scanning using nix-lib
          # Trivy vulnerability scans for each architecture
          bloklidSecurity = {
            # x86_64-linux security scans
            bloklid-docker-amd64-scan = nixLib.mkTrivyScan {
              image = bloklidDocker.bloklid-docker-amd64;
              name = "bloklid-trivy-scan-amd64";
              severity = "HIGH,CRITICAL";
              format = "sarif";
              exitCode = 1;
            };

            bloklid-dev-docker-amd64-scan = nixLib.mkTrivyScan {
              image = bloklidDocker.bloklid-dev-docker-amd64;
              name = "bloklid-dev-trivy-scan-amd64";
              severity = "HIGH,CRITICAL";
              format = "sarif";
              exitCode = 0; # Don't fail dev builds on vulnerabilities
            };

            # aarch64-linux security scans
            bloklid-docker-aarch64-scan = nixLib.mkTrivyScan {
              image = bloklidDocker.bloklid-docker-aarch64;
              name = "bloklid-trivy-scan-arm64";
              severity = "HIGH,CRITICAL";
              format = "sarif";
              exitCode = 1;
            };

            bloklid-dev-docker-aarch64-scan = nixLib.mkTrivyScan {
              image = bloklidDocker.bloklid-dev-docker-aarch64;
              name = "bloklid-dev-trivy-scan-arm64";
              severity = "HIGH,CRITICAL";
              format = "sarif";
              exitCode = 0; # Don't fail dev builds on vulnerabilities
            };

            # SBOM generation for each architecture
            # x86_64-linux SBOMs
            bloklid-docker-amd64-sbom = nixLib.mkSBOM {
              image = bloklidDocker.bloklid-docker-amd64;
              name = "bloklid-sbom-amd64";
              formats = [
                "spdx-json"
                "cyclonedx-json"
              ];
            };

            bloklid-dev-docker-amd64-sbom = nixLib.mkSBOM {
              image = bloklidDocker.bloklid-dev-docker-amd64;
              name = "bloklid-dev-sbom-amd64";
              formats = [
                "spdx-json"
                "cyclonedx-json"
              ];
            };

            # aarch64-linux SBOMs
            bloklid-docker-aarch64-sbom = nixLib.mkSBOM {
              image = bloklidDocker.bloklid-docker-aarch64;
              name = "bloklid-sbom-arm64";
              formats = [
                "spdx-json"
                "cyclonedx-json"
              ];
            };

            bloklid-dev-docker-aarch64-sbom = nixLib.mkSBOM {
              image = bloklidDocker.bloklid-dev-docker-aarch64;
              name = "bloklid-dev-sbom-arm64";
              formats = [
                "spdx-json"
                "cyclonedx-json"
              ];
            };
          };

          # Multi-architecture Docker manifests using nix-lib
          # Note: aarch64 temporarily disabled until GitHub runner supports it
          bloklidDockerMultiArch = {
            # Production multi-arch manifest (amd64 only until runner supports aarch64)
            bloklid-docker-manifest = nixLib.mkMultiArchManifest {
              name = "bloklid";
              tag = bloklidCrateInfo.version;
              images = [
                {
                  image = bloklidDocker.bloklid-docker-amd64;
                  platform = "linux/amd64";
                }
                # Disabled until GitHub runner supports aarch64
                # {
                #   image = bloklidDocker.bloklid-docker-aarch64;
                #   platform = "linux/arm64";
                # }
              ];
            };

            # Development multi-arch manifest (amd64 only until runner supports aarch64)
            bloklid-dev-docker-manifest = nixLib.mkMultiArchManifest {
              name = "bloklid-dev";
              tag = bloklidCrateInfo.version;
              images = [
                {
                  image = bloklidDocker.bloklid-dev-docker-amd64;
                  platform = "linux/amd64";
                }
                # Disabled until GitHub runner supports aarch64
                # {
                #   image = bloklidDocker.bloklid-dev-docker-aarch64;
                #   platform = "linux/arm64";
                # }
              ];
            };

            # Profile multi-arch manifest (amd64 only until runner supports aarch64)
            bloklid-profile-docker-manifest = nixLib.mkMultiArchManifest {
              name = "bloklid-profile";
              tag = bloklidCrateInfo.version;
              images = [
                {
                  image = bloklidDocker.bloklid-profile-docker-amd64;
                  platform = "linux/amd64";
                }
                # Disabled until GitHub runner supports aarch64
                # {
                #   image = bloklidDocker.bloklid-profile-docker-aarch64;
                #   platform = "linux/arm64";
                # }
              ];
            };
          };

          # Application definitions using nix-lib
          # Multi-arch manifest upload apps - always use these to ensure both architectures are available
          dockerUploadApps = {
            bloklid-docker-manifest-upload = nixLib.mkMultiArchUploadApp bloklidDockerMultiArch.bloklid-docker-manifest;
            bloklid-dev-docker-manifest-upload = nixLib.mkMultiArchUploadApp bloklidDockerMultiArch.bloklid-dev-docker-manifest;
            bloklid-profile-docker-manifest-upload = nixLib.mkMultiArchUploadApp bloklidDockerMultiArch.bloklid-profile-docker-manifest;
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
          shellArgs = {
            treefmtWrapper = config.treefmt.build.wrapper;
            treefmtPrograms = pkgs.lib.attrValues config.treefmt.build.programs;
            shellHook = packages.pre-commit-check.shellHook;
            extraPackages = [
              pkgs.foundry-bin
              pkgs.solc
              pkgs.kubernetes-helm
            ];
          };
          shells = {
            default = nixLib.mkDevShell (
              {
                rustToolchain = stableToolchain;
                shellName = "Development";
              }
              // shellArgs
            );

            experiment = nixLib.mkDevShell (
              {
                rustToolchain = nightlyToolchain;
                shellName = "Experimental Nightly";
              }
              // shellArgs
            );
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

              # Helm templates (contain Go template syntax that yamlfmt can't parse)
              "charts/blokli/templates/*"

              # Other specific files
              "bloklid/.dockerignore"
              "tests/pytest.ini"

              # Generated GraphQL schema (formatted separately)
              "schema.graphql"
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
              # GraphQL formatter
              settings.formatter.format-graphql = {
                command = pkgs.writeShellApplication {
                  name = "format-graphql";
                  runtimeInputs = [
                    pkgs.nodejs
                    pkgs.nodePackages.npm
                  ];
                  text = ''
                    npx --yes format-graphql --write=true "$@"
                  '';
                };
                includes = [ "design/*.graphql" ];
              };
              # GraphQL linter
              settings.formatter.graphql-schema-linter = {
                command = pkgs.writeShellApplication {
                  name = "graphql-schema-linter";
                  runtimeInputs = [
                    pkgs.nodejs
                    pkgs.nodePackages.npm
                  ];
                  text = ''
                    npx --yes graphql-schema-linter "$@"
                  '';
                };
                includes = [ "design/*.graphql" ];
              };
              # Markdown linter
              settings.formatter.markdownlint-cli2 = {
                command = pkgs.writeShellApplication {
                  name = "markdownlint-cli2";
                  runtimeInputs = [
                    pkgs.nodejs
                    pkgs.nodePackages.npm
                  ];
                  text = ''
                    npx --yes markdownlint-cli2 --fix "$@"
                  '';
                };
                includes = [
                  "**/*.md"
                  "*.md"
                ];
              };
            };
          };

          # Export checks for CI
          inherit checks;

          # Export applications using nix-lib
          apps = dockerUploadApps // utilityApps;

          # Export packages
          packages = packages // {
            # Docker images - x86_64-linux
            inherit (bloklidDocker)
              bloklid-docker-amd64
              bloklid-dev-docker-amd64
              bloklid-profile-docker-amd64
              ;

            # Docker images - aarch64-linux
            inherit (bloklidDocker)
              bloklid-docker-aarch64
              bloklid-dev-docker-aarch64
              bloklid-profile-docker-aarch64
              ;

            # Security scans - Trivy vulnerability scanning
            inherit (bloklidSecurity)
              bloklid-docker-amd64-scan
              bloklid-docker-aarch64-scan
              bloklid-dev-docker-amd64-scan
              bloklid-dev-docker-aarch64-scan
              ;

            # SBOMs - Software Bill of Materials
            inherit (bloklidSecurity)
              bloklid-docker-amd64-sbom
              bloklid-docker-aarch64-sbom
              bloklid-dev-docker-amd64-sbom
              bloklid-dev-docker-aarch64-sbom
              ;

            # Multi-arch manifests
            inherit (bloklidDockerMultiArch)
              bloklid-docker-manifest
              bloklid-dev-docker-manifest
              bloklid-profile-docker-manifest
              ;

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
