# Nix Integration Guide for riptide

This guide provides detailed information about integrating riptide into your Nix-based workflows.

## Quick Start

### Running riptide without installation

```bash
nix run github:pjan/riptide -- auth login
nix run github:pjan/riptide -- download url <tidal-url>
```

### Building from source

```bash
git clone https://github.com/pjan/riptide
cd riptide
nix build
./result/bin/riptide --help
```

## Installation Methods

### 1. Nix Profile (User-level)

Install riptide to your user profile:

```bash
nix profile install github:pjan/riptide
```

To update:

```bash
nix profile upgrade riptide
```

To remove:

```bash
nix profile remove riptide
```

### 2. NixOS System Configuration

Add riptide to your system packages:

```nix
# /etc/nixos/configuration.nix
{ config, pkgs, ... }:

{
  # Using flake input
  inputs = {
    nixpkgs.url = "github:NixOS/nixpkgs/nixos-unstable";
    riptide.url = "github:pjan/riptide";
  };

  outputs = { self, nixpkgs, riptide }: {
    nixosConfigurations.yourhostname = nixpkgs.lib.nixosSystem {
      system = "x86_64-linux";
      modules = [
        {
          environment.systemPackages = [
            riptide.packages.x86_64-linux.default
            # ffmpeg is already included as a runtime dependency
          ];
        }
      ];
    };
  };
}
```

### 3. Home Manager

Add riptide to your home-manager configuration:

```nix
# ~/.config/home-manager/home.nix or flake.nix
{ config, pkgs, ... }:

{
  inputs = {
    nixpkgs.url = "github:NixOS/nixpkgs/nixos-unstable";
    home-manager.url = "github:nix-community/home-manager";
    riptide.url = "github:pjan/riptide";
  };

  outputs = { self, nixpkgs, home-manager, riptide }: {
    homeConfigurations."yourusername@yourhostname" = home-manager.lib.homeManagerConfiguration {
      pkgs = nixpkgs.legacyPackages.x86_64-linux;
      modules = [
        {
          home.packages = [
            riptide.packages.x86_64-linux.default
          ];
          
          # Optional: Set default music directory
          home.sessionVariables = {
            RIPTIDE_PATH = "${config.home.homeDirectory}/Music/.riptide";
          };
        }
      ];
    };
  };
}
```

### 4. Nix Shell / Development

Create a temporary environment with riptide:

```bash
nix shell github:pjan/riptide
riptide --help
```

Use the development shell for contributing:

```bash
git clone https://github.com/pjan/riptide
cd riptide
nix develop
# Now you have all dependencies for development
python -m pytest
```

### 5. direnv Integration

For automatic environment loading when entering the project directory:

```bash
echo "use flake" > .envrc
direnv allow
```

This will automatically load the development environment whenever you `cd` into the riptide directory.

## Advanced Usage

### Using a Specific Commit or Branch

```bash
# Specific commit
nix run github:pjan/riptide/abc123def

# Specific branch
nix run github:pjan/riptide/development

# Specific tag
nix run github:pjan/riptide/v3.2.1
```

### Overlay for Custom Builds

Create an overlay to customize the build:

```nix
# overlays/riptide.nix
final: prev: {
  riptide = prev.callPackage ./path/to/riptide/nix/package.nix {
    # Custom Python version
    python3 = prev.python312;
  };
}
```

Apply the overlay in your configuration:

```nix
{ config, pkgs, ... }:

{
  nixpkgs.overlays = [
    (import ./overlays/riptide.nix)
  ];
  
  environment.systemPackages = [ pkgs.riptide ];
}
```

### Building with Different Python Versions

The package defaults to Python 3.13, but you can override it:

```bash
nix build --override-input nixpkgs github:NixOS/nixpkgs/nixos-unstable
```

Or in your configuration:

```nix
let
  riptide-custom = pkgs.callPackage ./riptide/nix/package.nix {
    python3 = pkgs.python312;
  };
in
{
  environment.systemPackages = [ riptide-custom ];
}
```

## Configuration Management with Nix

### Managing riptide Configuration

You can manage your riptide configuration declaratively:

```nix
{ config, pkgs, ... }:

{
  home.file.".riptide/config.toml".text = ''
    [download]
    quality = "MAX"
    output = "{album.artist}/{album.title}/{item.number:02d}. {item.title}"
    directory = "${config.home.homeDirectory}/Music"
    
    [metadata]
    embed_cover = true
    save_cover = false
    
    [api]
    skip_errors = false
  '';
}
```

### Secrets Management

For managing Tidal credentials securely with sops-nix or agenix:

```nix
# Using sops-nix
{ config, pkgs, ... }:

{
  sops.secrets.riptide-auth = {
    sopsFile = ./secrets.yaml;
    path = "${config.home.homeDirectory}/.riptide/auth.json";
    owner = config.home.username;
  };
  
  home.packages = [ pkgs.riptide ];
}
```

## Continuous Integration

### GitHub Actions with Nix

```yaml
name: Build riptide with Nix

on: [push, pull_request]

jobs:
  build:
    runs-on: ubuntu-latest
    steps:
      - uses: actions/checkout@v4
      - uses: cachix/install-nix-action@v25
        with:
          nix_path: nixpkgs=channel:nixos-unstable
      - uses: cachix/cachix-action@v14
        with:
          name: your-cache-name
          authToken: '${{ secrets.CACHIX_AUTH_TOKEN }}'
      - run: nix build
      - run: nix flake check
```

## Cross-Platform Support

### macOS (Darwin)

The flake supports macOS via `flake-utils.lib.eachDefaultSystem`:

```bash
# On macOS
nix run github:pjan/riptide
```

### Linux (NixOS and non-NixOS)

Works on any Linux distribution with Nix installed:

```bash
# Install Nix if not already installed
sh <(curl -L https://nixos.org/nix/install) --daemon

# Run riptide
nix run github:pjan/riptide --extra-experimental-features 'nix-command flakes'
```

## Troubleshooting

### Flakes not enabled

If you get "unrecognized flag" errors, enable flakes:

```bash
# Temporary
nix --extra-experimental-features 'nix-command flakes' run github:pjan/riptide

# Permanent (add to ~/.config/nix/nix.conf)
experimental-features = nix-command flakes
```

### Python version issues

The package requires Python 3.13+. If your nixpkgs version doesn't have it:

```bash
# Use nixos-unstable
nix run --override-input nixpkgs github:NixOS/nixpkgs/nixos-unstable github:pjan/riptide
```

### FFmpeg not found

FFmpeg should be automatically available. If issues persist:

```nix
# Explicitly add ffmpeg
environment.systemPackages = with pkgs; [
  riptide
  ffmpeg-full  # Use full ffmpeg build with all codecs
];
```

### Cache misses / slow builds

Use a binary cache:

```bash
# Use cachix (if available for riptide)
cachix use riptide

# Or build with substituters
nix build --option substituters 'https://cache.nixos.org'
```

## Development Workflow

### Local Development with Nix

```bash
# Clone and enter dev shell
git clone https://github.com/pjan/riptide
cd riptide
nix develop

# Make changes, then test
python -m riptide.cli.app --help

# Build to test the full package
nix build

# Run tests
pytest
```

### Creating a Development Environment

Customize the dev shell in `flake.nix`:

```nix
devShells.default = pkgs.mkShell {
  buildInputs = with pkgs; [
    python313
    ffmpeg
    # Add development tools
    ruff
    mypy
  ] ++ (with pkgs.python313Packages; [
    # Runtime dependencies
    aiofiles
    aiohttp
    m3u8
    mutagen
    pydantic
    requests
    requests-cache
    typer
    # Development dependencies
    pytest
    pytest-mock
    pytest-cov
    black
    isort
  ]);

  shellHook = ''
    export riptide_PATH="$PWD/.dev-riptide"
    echo "Development environment ready!"
    echo "riptide_PATH: $riptide_PATH"
  '';
};
```

## Contributing

When contributing to riptide with Nix:

1. Test the flake build: `nix build`
2. Check flake: `nix flake check`
3. Format nix files: `nix fmt` (if configured)
4. Update flake.lock: `nix flake update`
5. Test on multiple systems using GitHub Actions or NixOS tests

## Resources

- [Nix Package Manager](https://nixos.org/manual/nix/stable/)
- [Nix Flakes](https://nixos.wiki/wiki/Flakes)
- [NixOS Options](https://search.nixos.org/options)
- [Home Manager Options](https://nix-community.github.io/home-manager/options.html)
- [riptide Repository](https://github.com/pjan/riptide)
