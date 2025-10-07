# bloklid.nix - BLOKLID Docker image definitions
#
# Defines Docker images for the BLOKLID daemon with different profiles.
# Images are built as layered containers for efficient caching and smaller sizes.

{
  pkgs,
  dockerBuilder,
  packages,
}:

let
  # Create the Docker entrypoint script
  # Handles container-specific initialization and configuration
  mkDockerEntrypoint = pkgs.writeShellScriptBin "docker-entrypoint.sh" ''
    set -euo pipefail

    # Default to running bloklid with any provided arguments
    exec /bin/bloklid "$@"
  '';

  # Profile-specific dependencies for debugging and profiling
  profileDeps = with pkgs; [
    gdb # GNU debugger for debugging
    rust-bin.stable.latest.minimal # Minimal Rust toolchain for analysis
    valgrind # Memory debugging and profiling
    gnutar # For extracting pcap files from container
  ];

  # Base Docker image configuration for BLOKLID
  mkBloklidDocker =
    {
      package,
      extraDeps ? [ ],
      nameSuffix ? "",
    }:
    dockerBuilder {
      inherit pkgs;
      name = "bloklid${nameSuffix}";
      extraContents = [
        (mkDockerEntrypoint)
        package
      ]
      ++ extraDeps;
      Entrypoint = [ "/bin/docker-entrypoint.sh" ];
      Cmd = [ "bloklid" ];
    };
in
{
  # Production Docker image - minimal size, optimized build
  bloklid-docker = mkBloklidDocker {
    package = packages.bloklid-x86_64-linux;
  };

  # Development Docker image - dev profile for debugging
  bloklid-dev-docker = mkBloklidDocker {
    package = packages.bloklid-x86_64-linux-dev;
    nameSuffix = "-dev";
  };

  # Profiling Docker image - includes debugging tools
  bloklid-profile-docker = mkBloklidDocker {
    package = packages.bloklid-x86_64-linux-profile;
    extraDeps = profileDeps;
    nameSuffix = "-profile";
  };
}
