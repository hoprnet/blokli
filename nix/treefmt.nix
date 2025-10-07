# treefmt.nix - Code formatting configuration
#
# Defines formatters for all file types in the monorepo.
# Used by treefmt-nix for consistent code formatting across the project.

{
  config,
  pkgs,
}:

{
  # Project root detection file
  inherit (config.flake-root) projectRootFile;

  # Global exclusions - files and directories to never format
  settings.global.excludes = [
    # Binary and lock files
    "**/*.id"
    "**/.cargo-ok"
    "**/.gitignore"

    # Configuration files that shouldn't be formatted
    ".actrc"
    ".dockerignore"
    ".editorconfig"
    ".gcloudignore"
    ".gitattributes"
    ".yamlfmt"
    "LICENSE"
    "Makefile"

    # Generated code - don't format to avoid churn
    "db/entity/src/codegen/*"

    # External configuration
    "deploy/compose/grafana/config.monitoring"
    "deploy/nfpm/nfpm.yaml"
    ".github/workflows/build-binaries.yaml"

    # Documentation and test data
    "docs/*"

    # Build artifacts
    "target/*"

    # Vendor code
    "vendor/*"

    # Other specific files
    "bloklid/.dockerignore"
    "nix/setup-hook-darwin.sh"
    "tests/pytest.ini"
  ];

  # Shell script formatting
  programs.shfmt.enable = true;
  settings.formatter.shfmt.includes = [
    "*.sh"
    "deploy/compose/.env.sample"
    "deploy/compose/.env-secrets.sample"
  ];

  # YAML formatting
  programs.yamlfmt.enable = true;
  settings.formatter.yamlfmt.includes = [
    ".github/labeler.yml"
    ".github/workflows/*.yaml"
  ];
  settings.formatter.yamlfmt.settings = {
    formatter.type = "basic";
    formatter.max_line_length = 120;
    formatter.trim_trailing_whitespace = true;
    formatter.scan_folded_as_literal = true;
    formatter.include_document_start = true;
  };

  # Markdown and JSON formatting with Prettier
  programs.prettier.enable = true;
  settings.formatter.prettier.includes = [
    "*.md"
    "*.json"
  ];
  settings.formatter.prettier.excludes = [
    "*.yml"
    "*.yaml"
  ];

  # Rust formatting with nightly for unstable features
  programs.rustfmt.enable = true;
  settings.formatter.rustfmt = {
    command = "${pkgs.rust-bin.selectLatestNightlyWith (toolchain: toolchain.default)}/bin/rustfmt";
    options = [
      "--config-path"
      "."
    ];
  };

  # Nix formatting using official Nixpkgs style
  programs.nixfmt.enable = true;

  # TOML formatting
  programs.taplo.enable = true;
  settings.formatter.taplo.options = [
    "-o"
    "align_entries=true"
    "-o"
    "reorder_keys=true"
    "-o"
    "reorder_arrays=true"
  ];

  # Python formatting with Ruff
  programs.ruff-format.enable = true;
}
