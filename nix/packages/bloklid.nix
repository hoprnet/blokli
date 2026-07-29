# bloklid.nix - BLOKLID daemon package definitions
#
# Defines all variants of the BLOKLID daemon for different platforms and profiles.
# BLOKLID is the companion indexer for the HOPR network.

{
  lib,
  pkgs,
  builders,
  sources,
  bloklidCrateInfo,
  buildVersion,
  rev,
  buildPlatform,
  nixLib,
}:

let
  mkbloklidBuildArgs =
    { src, depsSrc }:
    {
      inherit
        src
        depsSrc
        rev
        buildVersion
        ;
      cargoExtraArgs = "--bins"; # Build all binary targets
      cargoToml = ./../../bloklid/Cargo.toml;
    };

  # Production builds includes bloklid in each platform derivation.
  # prependPackageName = false lets us specify both packages explicitly via cargoExtraArgs.
  mkBloklidPlatformPackages =
    platform:
    let
      args =
        (mkbloklidBuildArgs {
          src = sources.main;
          depsSrc = sources.deps;
        })
        // {
          prependPackageName = false;
          cargoExtraArgs = "-p bloklid --bins";
        };
      name = "binary-bloklid-${platform}";
    in
    {
      "${name}" = builders.${platform}.callPackage nixLib.mkRustPackage args;
    }
    // lib.optionalAttrs (lib.hasSuffix "-linux" platform) {
      "${name}-dev" = builders.${platform}.callPackage nixLib.mkRustPackage (
        args // { CARGO_PROFILE = "dev"; }
      );
    };

  bloklidPackages = builtins.foldl' (a: b: a // b) { } (
    map mkBloklidPlatformPackages [
      "x86_64-linux"
      "aarch64-linux"
      "x86_64-darwin"
      "aarch64-darwin"
    ]
  );

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
    '';
  };
in
{
  # Development builds - for local testing and debugging
  bloklid = builders.local.callPackage nixLib.mkRustPackage (mkbloklidBuildArgs {
    src = sources.main;
    depsSrc = sources.deps;
  });

  bloklid-dev = builders.local.callPackage nixLib.mkRustPackage (
    (mkbloklidBuildArgs {
      src = sources.main;
      depsSrc = sources.deps;
    })
    // {
      CARGO_PROFILE = "dev";
    }
  );
}
// bloklidPackages
// {
  # Test and quality assurance builds
  bloklid-test = builders.local.callPackage nixLib.mkRustPackage (
    (mkbloklidBuildArgs {
      src = sources.test;
      depsSrc = sources.deps;
    })
    // {
      runTests = true;
    }
  );

  bloklid-nextest = builders.local.callPackage nixLib.mkRustPackage (
    (mkbloklidBuildArgs {
      src = sources.test;
      depsSrc = sources.deps;
    })
    // {
      runNextest = true;
      testCargoProfile = "ci-test";
      prependPackageName = false;
      cargoExtraArgs = "--workspace";
      extraNativeBuildInputs = [ pkgs.foundry-bin ];
    }
  );

  bloklid-test-nightly = builders.localNightly.callPackage nixLib.mkRustPackage (
    (mkbloklidBuildArgs {
      src = sources.test;
      depsSrc = sources.deps;
    })
    // {
      runTests = true;
      cargoExtraArgs = "-Z panic-abort-tests"; # Nightly feature for test optimization
    }
  );

  bloklid-clippy = builders.local.callPackage nixLib.mkRustPackage (
    (mkbloklidBuildArgs {
      src = sources.main;
      depsSrc = sources.deps;
    })
    // {
      runClippy = true; # Run Clippy linter
      prependPackageName = false;
      cargoExtraArgs = "--workspace";
    }
  );

  bloklid-coverage = builders.localCoverage.callPackage nixLib.mkRustPackage (
    (mkbloklidBuildArgs {
      src = sources.test;
      depsSrc = sources.deps;
    })
    // {
      runCoverage = true;
      cargoLlvmCovCommand = "nextest";
      testCargoProfile = "ci-test";
      cargoExtraArgs = "--lib";
      extraNativeBuildInputs = [
        pkgs.cargo-nextest
        pkgs.foundry-bin
      ];
    }
  );

  bloklid-bench = builders.local.callPackage nixLib.mkRustPackage (
    (mkbloklidBuildArgs {
      src = sources.main;
      depsSrc = sources.deps;
    })
    // {
      runBench = true; # Run benchmarks
    }
  );

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
  bloklid-candidate =
    if buildPlatform.isLinux && buildPlatform.isx86_64 then
      builders.x86_64-linux.callPackage nixLib.mkRustPackage (
        (mkbloklidBuildArgs {
          src = sources.main;
          depsSrc = sources.deps;
        })
        // {
          CARGO_PROFILE = "candidate";
        }
      )
    else
      builders.local.callPackage nixLib.mkRustPackage (
        (mkbloklidBuildArgs {
          src = sources.main;
          depsSrc = sources.deps;
        })
        // {
          CARGO_PROFILE = "candidate";
        }
      );

  # Documentation build using nightly Rust for unstable doc features
  bloklid-docs = builders.localNightly.callPackage nixLib.mkRustPackage (
    (mkbloklidBuildArgs {
      src = sources.main;
      depsSrc = sources.deps;
    })
    // {
      buildDocs = true;
    }
  );
}
