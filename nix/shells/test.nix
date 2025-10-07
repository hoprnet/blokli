# test.nix - Test shell configuration
#
# Provides an environment configured for running tests and quality checks.
# Forms the base for other shell environments.

{
  pkgs,
  config,
  crane,
  shellHook ? "",
  shellPackages ? [ ],
  useRustNightly ? false,
  extraPackages ? [ ],
}:

let
  mkShell = import ./base.nix { };

  finalShellHook = ''
    uv sync --frozen
    unset SOURCE_DATE_EPOCH
  ''
  + pkgs.lib.optionalString pkgs.stdenv.isLinux ''
    autoPatchelf ./.venv
  ''
  + shellHook;

  # Base packages for testing environment
  basePackages = with pkgs; [
    uv # Python package manager
    python313 # Python runtime for tests

    # Documentation and formatting tools
    html-tidy # HTML validation
    pandoc # Universal document converter
  ];

  # Combine all packages
  allShellPackages = basePackages ++ shellPackages ++ extraPackages;
in
mkShell {
  inherit
    pkgs
    config
    crane
    useRustNightly
    ;
  shellPackages = allShellPackages;
  shellHook = finalShellHook;
}
