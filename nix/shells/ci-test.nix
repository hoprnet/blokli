# ci-test.nix - CI test shell configuration
#
# Shell environment for running integration tests in CI.
# Includes pre-built BLOKLID binaries for faster test execution.

{
  pkgs,
  config,
  crane,
  bloklid,
  extraPackages ? [ ],
}:

import ./test.nix {
  inherit
    pkgs
    config
    crane
    ;

  # Include pre-built binaries for testing
  extraPackages = [
    bloklid
  ]
  ++ extraPackages;
}
