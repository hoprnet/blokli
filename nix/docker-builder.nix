# docker-builder.nix - Docker image builder utility
#
# Creates layered Docker images with optimized caching and minimal size.
# Provides a consistent base for all HOPR container images.

{
  Cmd ? [ ], # Default command to run in container
  Entrypoint, # Container entrypoint script or binary
  env ? [ ], # Environment variables for the container
  extraContents ? [ ], # Additional packages to include in image
  name, # Name of the Docker image
  pkgs, # Nixpkgs package set
}:
let
  # Library path for essential system libraries
  libPath = pkgs.lib.makeLibraryPath [ pkgs.openssl ];

  # Base packages included in all Docker images
  # These provide essential runtime dependencies
  copyToRoot = pkgs.buildEnv {
    name = "image-root";
    paths =
      with pkgs;
      [
        bash
        cacert
        coreutils
        dnsutils
        findutils
        iana-etc
        nettools
        util-linux
      ]
      ++ extraContents;
    pathsToLink = [ "/bin" ];
  };
  Env = [
    "NO_COLOR=true" # suppress colored log output
    # "RUST_LOG=info"   # 'info' level is set by default with some spamming components set to override
    "RUST_BACKTRACE=full"
    "LD_LIBRARY_PATH=${libPath}"
  ]
  ++ env;
  sharedDockerArgs = {
    inherit name copyToRoot;
    tag = "latest";
    # breaks binary reproducibility, but makes usage easier
    created = "now";
    config = { inherit Cmd Entrypoint Env; };
  };
  # Use buildImage on macOS to avoid fakeroot issues
  # buildLayeredImage requires fakeroot which doesn't work on recent macOS
  dockerBuilder =
    if pkgs.stdenv.isDarwin then pkgs.dockerTools.buildImage else pkgs.dockerTools.buildLayeredImage;
in
dockerBuilder sharedDockerArgs
