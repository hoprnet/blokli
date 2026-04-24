# bloklid.nix - BLOKLID daemon package definitions
#
# Defines all variants of the BLOKLID daemon for different platforms and profiles.
# BLOKLID is the companion indexer for the HOPR network.

{
  lib,
  builders,
  sources,
  bloklidCrateInfo,
  rev,
  buildPlatform,
  nixLib,
}:

let
  mkbloklidBuildArgs =
    { src, depsSrc }:
    {
      inherit src depsSrc rev;
      cargoExtraArgs = "--bins"; # Build all binary targets
      cargoToml = ./../../bloklid/Cargo.toml;
    };

  inspectorBuildArgs = {
    inherit rev;
    src = sources.main;
    depsSrc = sources.deps;
    cargoToml = ./../../inspector/Cargo.toml;
    cargoExtraArgs = "--bins";
  };

  # Production builds for blokli-inspector across all supported platforms
  inspectorPackages = builtins.listToAttrs (
    map (platform: {
      name = "binary-blokli-inspector-${platform}";
      value = builders.${platform}.callPackage nixLib.mkRustPackage inspectorBuildArgs;
    }) [ "x86_64-linux" "aarch64-linux" "x86_64-darwin" "aarch64-darwin" ]
  );

  # Production builds for bloklid across all supported platforms
  # Linux platforms additionally include a dev variant
  mkBloklidPlatformPackages =
    platform:
    let
      args = mkbloklidBuildArgs {
        src = sources.main;
        depsSrc = sources.deps;
      };
      name = "binary-blokli-${platform}";
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

  bloklidPackages = builtins.foldl' (a: b: a // b) { } (
    map mkBloklidPlatformPackages [ "x86_64-linux" "aarch64-linux" "x86_64-darwin" "aarch64-darwin" ]
  );
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
// inspectorPackages
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
