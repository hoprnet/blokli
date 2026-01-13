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
  # Common build arguments for all bloklid variants
  # These are shared across all build configurations
  mkbloklidBuildArgs =
    { src, depsSrc }:
    {
      inherit src depsSrc rev;
      cargoExtraArgs = "--bins"; # Build all binary targets
      cargoToml = ./../../bloklid/Cargo.toml;
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

  # Production builds - optimized for deployment
  # x86_64 Linux builds with static linking for maximum portability
  binary-blokli-x86_64-linux =
    builders.x86_64-linux.callPackage nixLib.mkRustPackage
      (mkbloklidBuildArgs {
        src = sources.main;
        depsSrc = sources.deps;
      });

  binary-blokli-x86_64-linux-profile = builders.x86_64-linux.callPackage nixLib.mkRustPackage (
    (mkbloklidBuildArgs {
      src = sources.main;
      depsSrc = sources.deps;
    })
  );

  binary-blokli-x86_64-linux-dev = builders.x86_64-linux.callPackage nixLib.mkRustPackage (
    (mkbloklidBuildArgs {
      src = sources.main;
      depsSrc = sources.deps;
    })
    // {
      CARGO_PROFILE = "dev";
    }
  );

  # ARM64 Linux builds for ARM servers and devices
  binary-blokli-aarch64-linux =
    builders.aarch64-linux.callPackage nixLib.mkRustPackage
      (mkbloklidBuildArgs {
        src = sources.main;
        depsSrc = sources.deps;
      });

  binary-blokli-aarch64-linux-profile = builders.aarch64-linux.callPackage nixLib.mkRustPackage (
    (mkbloklidBuildArgs {
      src = sources.main;
      depsSrc = sources.deps;
    })
  );

  binary-blokli-aarch64-linux-dev = builders.aarch64-linux.callPackage nixLib.mkRustPackage (
    (mkbloklidBuildArgs {
      src = sources.main;
      depsSrc = sources.deps;
    })
    // {
      CARGO_PROFILE = "dev";
    }
  );

  # macOS builds - require building from Darwin systems
  # x86_64 macOS (Intel Macs)
  binary-blokli-x86_64-darwin =
    builders.x86_64-darwin.callPackage nixLib.mkRustPackage
      (mkbloklidBuildArgs {
        src = sources.main;
        depsSrc = sources.deps;
      });

  binary-blokli-x86_64-darwin-profile = builders.x86_64-darwin.callPackage nixLib.mkRustPackage (
    (mkbloklidBuildArgs {
      src = sources.main;
      depsSrc = sources.deps;
    })
  );

  # ARM64 macOS (Apple Silicon)
  binary-blokli-aarch64-darwin =
    builders.aarch64-darwin.callPackage nixLib.mkRustPackage
      (mkbloklidBuildArgs {
        src = sources.main;
        depsSrc = sources.deps;
      });

  binary-blokli-aarch64-darwin-profile = builders.aarch64-darwin.callPackage nixLib.mkRustPackage (
    (mkbloklidBuildArgs {
      src = sources.main;
      depsSrc = sources.deps;
    })
  );

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
