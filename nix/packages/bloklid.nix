# bloklid.nix - BLOKLID daemon package definitions
#
# Defines all variants of the BLOKLID daemon for different platforms and profiles.
# BLOKLID is the companion indexer for the HOPR network.

{
  lib,
  pkgs,
  builders,
  sources,
  buildVersion,
  rev,
  nixLib,
}:

let
  # Build a bloklid variant with `builder`; `args` extend or override the defaults.
  mkBloklid =
    builder: args:
    builder.callPackage nixLib.mkRustPackage (
      {
        inherit rev buildVersion;
        src = sources.main;
        depsSrc = sources.deps;
        cargoExtraArgs = "--bins"; # Build all binary targets
        cargoToml = ./../../bloklid/Cargo.toml;
      }
      // args
    );

  # Production builds for each platform, plus a dev profile variant on Linux.
  # prependPackageName = false lets us select the package explicitly via cargoExtraArgs.
  mkBloklidPlatformPackages =
    platform:
    let
      args = {
        prependPackageName = false;
        cargoExtraArgs = "-p bloklid --bins --locked";
      };
      name = "binary-bloklid-${platform}";
    in
    {
      "${name}" = mkBloklid builders.${platform} args;
    }
    // lib.optionalAttrs (lib.hasSuffix "-linux" platform) {
      "${name}-dev" = mkBloklid builders.${platform} (args // { CARGO_PROFILE = "dev"; });
    };

  blokliSchema = builders.local.callPackage nixLib.mkRustPackage {
    src = sources.schema;
    depsSrc = sources.deps;
    rev = "schema";
    cargoToml = ./../../api/Cargo.toml;
    CARGO_PROFILE = "dev";
    prependPackageName = false;
    cargoExtraArgs = "-p blokli-api --bin blokli-api -p blokli-db-migration --bin migration";
    postInstall = ''
      mkdir -p "$out/share/blokli"
      "$out/bin/migration" up \
        -u "sqlite://$TMPDIR/blokli-schema.db?mode=rwc"
      "$out/bin/blokli-api" export-schema \
        --database-url "sqlite://$TMPDIR/blokli-schema.db" \
        --output "$out/share/blokli/schema.graphql"
      # Keep the output schema-only so Crane does not scan the large dev binaries for source references.
      rm -rf "$out/bin"
    '';
  };

  buildPlatform = pkgs.stdenv.buildPlatform;
in
lib.mergeAttrsList (
  map mkBloklidPlatformPackages [
    "x86_64-linux"
    "aarch64-linux"
    "x86_64-darwin"
    "aarch64-darwin"
  ]
)
// {
  # Development builds - for local testing and debugging
  bloklid = mkBloklid builders.local { };
  bloklid-dev = mkBloklid builders.local { CARGO_PROFILE = "dev"; };

  # Test and quality assurance builds
  bloklid-test = mkBloklid builders.local {
    src = sources.test;
    runTests = true;
  };

  bloklid-nextest = mkBloklid builders.local {
    src = sources.test;
    runNextest = true;
    testCargoProfile = "ci-test";
    prependPackageName = false;
    cargoExtraArgs = "--workspace";
    extraNativeBuildInputs = [ pkgs.foundry-bin ];
  };

  bloklid-test-nightly = mkBloklid builders.localNightly {
    src = sources.test;
    runTests = true;
    cargoExtraArgs = "--locked -Z panic-abort-tests"; # Nightly feature for test optimization
  };

  bloklid-clippy = mkBloklid builders.local {
    runClippy = true;
    prependPackageName = false;
    # --all-features ensures optional, cfg-gated code such as the Curvy test
    # deployment is linted by CI too.
    cargoExtraArgs = "--workspace --all-features";
  };

  # Coverage runs the unit-test suite through nextest and emits the LCOV report.
  bloklid-coverage = mkBloklid builders.localCoverage {
    src = sources.test;
    runCoverage = true;
    cargoLlvmCovCommand = "nextest";
    testCargoProfile = "ci-test";
    # Nix renders nextest's in-place progress bar as repeated, truncated log lines.
    cargoExtraArgs = "--show-progress=none";
    extraNativeBuildInputs = [
      pkgs.cargo-nextest
      pkgs.foundry-bin
    ];
  };

  bloklid-bench = mkBloklid builders.local { runBench = true; };

  blokli-schema = blokliSchema;

  schema-validation =
    pkgs.runCommand "schema-validation"
      {
        nativeBuildInputs = [
          (pkgs.python3.withPackages (pythonPackages: [ pythonPackages.graphql-core ]))
        ];
      }
      ''
        mkdir -p work/design "$out"
        cp ${./../../design/target-api-schema.graphql} work/design/target-api-schema.graphql
        cp ${blokliSchema}/share/blokli/schema.graphql work/schema.graphql

        cd work
        python ${./../../scripts/check_schema.py}
        cp schema.graphql "$out/schema.graphql"
      '';

  # Candidate build - used for smoke testing before release
  # Builds as static binary on Linux x86_64 for better test coverage
  bloklid-candidate = mkBloklid (
    if buildPlatform.isLinux && buildPlatform.isx86_64 then builders.x86_64-linux else builders.local
  ) { CARGO_PROFILE = "candidate"; };

  # Documentation build using nightly Rust for unstable doc features
  bloklid-docs = mkBloklid builders.localNightly { buildDocs = true; };
}
