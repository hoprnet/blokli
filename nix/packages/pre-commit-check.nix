# pre-commit-check.nix - Pre-commit hooks configuration package
#
# Defines the pre-commit hooks that run automatically before each commit
# to ensure code quality, formatting, and basic validation.

{
  pre-commit,
  system,
  config,
  pkgs,
  stableToolchain,
}:

let
  lib = pkgs.lib;

  # Wrapper script that provides cargo and other tools to the export-db-schema hook.
  # Pre-commit system hooks run outside the devshell, so tools must be explicitly
  # added to PATH.
  exportDbSchemaWrapper = pkgs.writeShellScript "export-db-schema-hook" ''
    # Skip in nix build sandbox where cargo can't compile (no network, no system libs).
    # NIX_BUILD_TOP is set inside all nix build derivations.
    if [ -n "''${NIX_BUILD_TOP:-}" ]; then
      echo "Skipping export-db-schema (nix build sandbox detected)"
      exit 0
    fi
    export PATH="${lib.makeBinPath [
      stableToolchain
      pkgs.just
      pkgs.sqlite
      pkgs.pgformatter
    ]}:$PATH"
    exec ${pkgs.just}/bin/just export-db-schema
  '';
in

pre-commit.lib.${system}.run {
  src = ./../..; # Root of the project

  # Configure the pre-commit hooks to run
  hooks = {
    # Use treefmt for code formatting (disabled by default, enabled via package)
    treefmt.enable = false;
    treefmt.package = config.treefmt.build.wrapper;

    # Shell script validation
    check-executables-have-shebangs.enable = true;
    check-shebang-scripts-are-executable.enable = true;

    # File system checks
    check-case-conflicts.enable = true;
    check-symlinks.enable = true;
    check-merge-conflicts.enable = true;
    check-added-large-files.enable = true;

    # Commit message formatting
    commitizen.enable = true;

    # Export database schema when migrations change
    export-db-schema = {
      enable = true;
      name = "generate database schema";
      entry = toString exportDbSchemaWrapper;
      files = "db/migration/src/.*\\.rs$";
      language = "system";
      pass_filenames = false;
    };

    # Custom immutable files check (disabled by default)
    immutable-files = {
      enable = false;
      name = "Immutable files - the files should not change";
      entry = "bash .github/scripts/immutable-files-check.sh";
      files = "";
      language = "system";
    };
  };

  # Tools available to the pre-commit environment
  tools = pkgs;

  # Exclude certain paths from pre-commit checks
  excludes = [
    "vendor/" # Third-party code
    "ethereum/contracts/" # Generated/external contracts
    "ethereum/bindings/src/codegen" # Generated bindings
    ".gcloudignore" # Cloud configuration
  ];
}
