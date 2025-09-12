# sources.nix - File source management utilities
#
# Provides functions for creating filtered source trees for different build contexts.
# This helps reduce build closure sizes and improves caching by only including
# necessary files for each build step.

{ lib }:

rec {
  # Create a filtered source for dependency-only builds
  # Only includes files necessary for resolving Rust dependencies
  mkDepsSrc =
    { root, fs }:
    fs.toSource {
      inherit root;
      fileset = fs.unions [
        # Cargo configuration
        (root + "/.cargo/config.toml")
        (root + "/Cargo.lock")
        # Include all Cargo.toml files for workspace resolution
        (fs.fileFilter (file: file.name == "Cargo.toml") root)
      ];
    };

  # Create a filtered source for main Rust builds
  # Includes all necessary source files and resources
  mkSrc =
    { root, fs }:
    let
      fileset = fs.unions [
        # Cargo configuration
        (root + "/.cargo/config.toml")
        (root + "/Cargo.lock")
        (root + "/README.md")

        # Runtime data and configuration
        (root + "/ethereum/contracts/contracts-addresses.json")
        (root + "/ethereum/contracts/foundry.in.toml")
        (root + "/ethereum/contracts/remappings.txt")
        (root + "/bloklid/example_cfg.yaml")

        # Source files
        (fs.fileFilter (file: file.hasExt "rs") root)
        (fs.fileFilter (file: file.hasExt "toml") root)
        (fs.fileFilter (file: file.hasExt "sol") (root + "/vendor/solidity"))
        (fs.fileFilter (file: file.hasExt "sol") (root + "/ethereum/contracts/src"))
      ];
    in
    fs.toSource {
      inherit root fileset;
    };

  # Create a filtered source for test builds
  # Includes additional test data and fixtures
  mkTestSrc =
    { root, fs }:
    let
      fileset = fs.unions [
        # Cargo configuration
        (root + "/.cargo/config.toml")
        (root + "/Cargo.lock")
        (root + "/README.md")

        # Runtime data and configuration
        (root + "/ethereum/contracts/contracts-addresses.json")
        (root + "/ethereum/contracts/foundry.in.toml")
        (root + "/ethereum/contracts/remappings.txt")
        (root + "/bloklid/example_cfg.yaml")

        # Source files
        (fs.fileFilter (file: file.hasExt "rs") root)
        (fs.fileFilter (file: file.hasExt "toml") root)
        (fs.fileFilter (file: file.hasExt "sol") (root + "/vendor/solidity"))
        (fs.fileFilter (file: file.hasExt "sol") (root + "/ethereum/contracts/src"))
      ];
    in
    fs.toSource {
      inherit root fileset;
    };
}
