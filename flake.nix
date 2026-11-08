# flake.nix - HOPR blokli Nix flake configuration
#
# Structure:
# - nix/packages/: Package definitions (bloklid, pre-commit hooks)
# - nix-lib (external): Rust builders, dev shells, Docker images, treefmt, and utilities

{
  description = "HOPR blokli - the companion indexer and chain operator for the HOPR network";

  inputs = {
    flake-parts.url = "github:hercules-ci/flake-parts";
    nixpkgs.url = "github:NixOS/nixpkgs/release-26.05";

    # HOPR Nix library: Rust builders, dev shells, Docker images, and treefmt
    nix-lib.url = "github:hoprnet/nix-lib/v2.0.0";

    # Only used to pin the crane version nix-lib builds with (see follows below)
    crane.url = "github:ipetkov/crane";
    # Rust toolchains (rust-bin) for this flake's dev shells; nix-lib follows it too
    rust-overlay.url = "github:oxalica/rust-overlay";

    pre-commit.url = "github:cachix/git-hooks.nix";
    foundry.url = "github:shazow/foundry.nix";
    solc.url = "github:hellwolf/solc.nix";

    # Share one nixpkgs (and crane/rust-overlay) to reduce closure size and improve caching
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
      flake-parts,
      nix-lib,
      rust-overlay,
      pre-commit,
      foundry,
      solc,
      ...
    }@inputs:
    flake-parts.lib.mkFlake { inherit inputs; } {
      imports = [ nix-lib.flakeModules.default ];

      systems = [
        "x86_64-linux"
        "aarch64-linux"
        "aarch64-darwin"
        "x86_64-darwin"
      ];

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
          buildVersionEnv = builtins.getEnv "BUILD_VERSION";
          buildVersion = if buildVersionEnv == "" then null else buildVersionEnv;

          fs = lib.fileset;

          # Since the 2026-09-05 nightly, `forge` and `cast` link against
          # libudev.so.1 (hardware-wallet support). foundry.nix does not declare
          # that dependency, so autoPatchelfHook fails the build on Linux.
          # Darwin is unaffected, hence the isLinux guard.
          foundryUdevOverlay = final: prev: {
            foundry-bin = prev.foundry-bin.overrideAttrs (old: {
              buildInputs =
                (old.buildInputs or [ ]) ++ prev.lib.optional prev.stdenv.hostPlatform.isLinux prev.udev;
            });
          };

          overlays = [
            rust-overlay.overlays.default
            foundry.overlay
            foundryUdevOverlay
            solc.overlay
          ];

          # Nixpkgs for a given system; Docker images always need Linux packages,
          # even when building on macOS.
          pkgsFor =
            targetSystem:
            import nixpkgs {
              system = targetSystem;
              inherit overlays;
            };
          pkgs = pkgsFor system;
          linuxPkgs = lib.genAttrs [ "x86_64-linux" "aarch64-linux" ] (
            targetSystem: if targetSystem == system then pkgs else pkgsFor targetSystem
          );

          nixLib = nix-lib.lib.${system};

          sources = {
            main = nixLib.mkSrc {
              inherit fs;
              root = ./.;
              extraExtensions = [
                "csv"
                "graphql"
              ];
            };
            test = nixLib.mkTestSrc {
              inherit fs;
              root = ./.;
              extraExtensions = [
                "csv"
                "graphql"
                "pem"
                "snap"
              ];
            };
            schema = nixLib.mkSrc {
              inherit fs;
              root = ./.;
              extraExtensions = [ "csv" ];
            };
            deps = nixLib.mkDepsSrc {
              inherit fs;
              root = ./.;
            };
          };

          builders = nixLib.mkRustBuilders {
            rustToolchainFile = ./rust-toolchain.toml;
          };

          bloklidPackages = import ./nix/packages/bloklid.nix {
            inherit
              lib
              pkgs
              builders
              sources
              buildVersion
              rev
              nixLib
              ;
          };

          # Docker image for a target platform. variant is null (release) or "dev".
          # nix-lib links each package's bin/ into the image's /bin, which makes
          # the entrypoints available at stable paths.
          mkBloklidDocker =
            targetPlatform: variant:
            let
              suffix = lib.optionalString (variant != null) "-${variant}";
            in
            nixLib.mkDockerImage {
              name = "bloklid${suffix}";
              Entrypoint = [ "/bin/bloklid" ];
              pkgsLinux = linuxPkgs.${targetPlatform};
              extraContents = [ bloklidPackages."binary-bloklid-${targetPlatform}${suffix}" ];
            };

          # Local Anvil image with HOPR and Curvy contracts deployed.
          mkBloklidAnvilDocker =
            targetPlatform:
            let
              platformPkgs = linuxPkgs.${targetPlatform};
            in
            nixLib.mkDockerImage {
              name = "bloklid-anvil";
              Entrypoint = [ "/bin/blokli-anvil-entrypoint" ];
              pkgsLinux = platformPkgs;
              env = [ "BLOKLI_DEPLOY_CURVY=true" ];
              extraContents = [
                bloklidPackages."binary-bloklid-${targetPlatform}"
                platformPkgs.curl
                # The Nixpkgs Foundry 1.7.1 package embeds vulnerable quinn-proto
                # 0.11.14. The pinned foundry.nix nightly uses 0.11.16.
                platformPkgs.foundry-bin
                (platformPkgs.writeShellScriptBin "blokli-anvil-entrypoint" (
                  builtins.readFile ./docker/blokli-anvil-entrypoint.sh
                ))
              ];
            };

          stableToolchain = pkgs.rust-bin.fromRustupToolchainFile ./rust-toolchain.toml;
          nightlyToolchain = pkgs.rust-bin.nightly.latest.default.override {
            extensions = [
              "rust-src"
              "rust-analyzer"
              "clippy"
              "rustfmt"
            ];
          };

          pre-commit-check = pkgs.callPackage ./nix/packages/pre-commit-check.nix {
            inherit
              pre-commit
              system
              config
              stableToolchain
              ;
          };

          treefmtArgs = {
            treefmtWrapper = config.treefmt.build.wrapper;
            treefmtPrograms = lib.attrValues config.treefmt.build.programs;
          };

          mkDevShell =
            args:
            nixLib.mkDevShell (
              treefmtArgs
              // {
                shellHook = pre-commit-check.shellHook;
                extraPackages = with pkgs; [
                  gh
                  bun
                  nodejs
                  ast-grep
                  foundry-bin
                  pkgs.solc # `solc` alone would resolve to the flake input
                  kubernetes-helm
                  cargo-insta
                  cargo-machete
                  cargo-nextest
                  cargo-release
                  cargo-shear
                  yq
                  uv
                  sqlite
                  pgformatter
                  postgresql
                ];
              }
              // args
            );
        in
        {
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
              "charts/blokli/README.md"

              # Other specific files
              "bloklid/.dockerignore"
              "tests/pytest.ini"

              # Generated GraphQL schema (formatted separately)
              "schema.graphql"
              "design/target-api-schema.graphql"

              # locally installed npm packages
              ".npm/"
            ];
            extraFormatters = {
              programs.nixfmt.package = pkgs.nixfmt;
              programs.prettier.package = pkgs.prettier;
              settings.formatter.shfmt.includes = [
                "*.sh"
                "deploy/compose/.env.sample"
                "deploy/compose/.env-secrets.sample"
              ];
              settings.formatter.yamlfmt.includes = [
                ".github/labeler.yml"
                ".github/workflows/*.yaml"
              ];
              settings.formatter.format-graphql = {
                command = pkgs.writeShellApplication {
                  name = "format-graphql";
                  runtimeInputs = [ pkgs.prettier ];
                  text = ''
                    prettier --parser graphql --write "$@"
                  '';
                };
                includes = [ "design/*.graphql" ];
              };
              settings.formatter.graphql-schema-linter = {
                command = pkgs.writeShellApplication {
                  name = "graphql-schema-linter";
                  runtimeInputs = [ pkgs.nodejs ];
                  text = ''
                    npx --yes graphql-schema-linter "$@"
                  '';
                };
                includes = [ "design/*.graphql" ];
              };
              settings.formatter.deno = {
                command = pkgs.writeShellApplication {
                  name = "deno-fmt";
                  runtimeInputs = [ pkgs.deno ];
                  text = ''
                    deno fmt --config deno.json "$@"
                  '';
                };
                includes = [
                  "**/*.md"
                  "*.md"
                ];
              };
              settings.formatter.actionlint = {
                command = pkgs.writeShellApplication {
                  name = "actionlint";
                  runtimeInputs = [ pkgs.actionlint ];
                  text = ''
                    actionlint "$@"
                  '';
                };
                includes = [ ".github/workflows/*.yaml" ];
              };
              # Rust code linter using AST-based pattern matching
              settings.formatter.ast-grep = {
                command = pkgs.writeShellApplication {
                  name = "ast-grep-check";
                  runtimeInputs = [ pkgs.ast-grep ];
                  text = ''
                    ast-grep scan "$@"
                  '';
                };
                includes = [ "**/*.rs" ];
                excludes = [ "db/entity/src/codegen/*.rs" ];
              };
            };
          };

          checks = {
            inherit (bloklidPackages) bloklid-clippy;
          };

          apps = {
            update-github-labels = nixLib.mkUpdateGithubLabelsApp;
            audit = nixLib.mkAuditApp { };
            check = nixLib.mkCheckApp { inherit system; };
            coverage-unit = {
              type = "app";
              program = toString (
                pkgs.writeShellScript "coverage-unit" ''
                  nix build -L .#bloklid-coverage -o coverage.lcov
                ''
              );
            };
          };

          packages = bloklidPackages // {
            default = bloklidPackages.bloklid;

            inherit pre-commit-check;

            bloklid-man = nixLib.mkManPage {
              pname = "bloklid";
              binary = bloklidPackages.bloklid-dev;
              description = "BLOKLID node executable";
            };

            docker-bloklid-x86_64-linux = mkBloklidDocker "x86_64-linux" null;
            docker-bloklid-x86_64-linux-dev = mkBloklidDocker "x86_64-linux" "dev";
            docker-bloklid-anvil-x86_64-linux = mkBloklidAnvilDocker "x86_64-linux";
            docker-bloklid-aarch64-linux = mkBloklidDocker "aarch64-linux" null;
            docker-bloklid-aarch64-linux-dev = mkBloklidDocker "aarch64-linux" "dev";
            docker-bloklid-anvil-aarch64-linux = mkBloklidAnvilDocker "aarch64-linux";
          };

          devShells = {
            default = mkDevShell {
              rustToolchain = stableToolchain;
              shellName = "Development";
            };

            experiment = mkDevShell {
              rustToolchain = nightlyToolchain;
              shellName = "Experimental Nightly";
            };

            # Used by the hopr-workflows dependency and zizmor checks
            ci = nixLib.mkDevShell (
              treefmtArgs
              // {
                rustToolchainFile = ./rust-toolchain.toml;
                shellName = "blokli CI";
                extraPackages = with pkgs; [
                  cargo-machete
                  cargo-release
                  cargo-shear
                  zizmor
                ];
              }
            );
          };
        };
    };
}
