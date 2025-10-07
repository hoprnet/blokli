# checks.nix - CI/CD quality checks
#
# Defines automated checks that run in CI to ensure code quality.
# These checks can also be run locally for pre-push validation.

{
  pkgs,
  packages,
  bloklidCrateInfo,
}:

{
  # Rust linting checks
  bloklid-clippy = packages.bloklid-clippy;
}
