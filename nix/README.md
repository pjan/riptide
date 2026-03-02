# Nix Package for riptide

This directory contains Nix expressions for building and using riptide in a Nix-managed environment.

## Usage

### Using with Nix Flakes (Recommended)

If you have flakes enabled, you can run riptide directly without installation:

```bash
nix run github:pjan/riptide
```

Or from the local repository:

```bash
nix run .
```

### Building the Package

Build riptide using Nix flakes:

```bash
nix build .
```

The built package will be available in `./result/bin/riptide`.

### Development Shell

Enter a development shell with all dependencies:

```bash
nix develop
```

This provides Python 3.13, ffmpeg, and all required Python packages.

### Installing to Profile

Install riptide to your Nix profile:

```bash
nix profile install .
```

### Using without Flakes

If you're using traditional Nix (without flakes), you can build the package:

```bash
nix-build nix/default.nix
```

Or import it in your own Nix expressions:

```nix
let
  riptide = pkgs.callPackage ./path/to/riptide/nix/package.nix { };
in
  # use riptide here
```

## Integration with NixOS

Add riptide to your NixOS configuration:

```nix
{
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
          ];
        }
      ];
    };
  };
}
```

## Integration with home-manager

Add riptide to your home-manager configuration:

```nix
{
  inputs = {
    nixpkgs.url = "github:NixOS/nixpkgs/nixos-unstable";
    home-manager.url = "github:nix-community/home-manager";
    riptide.url = "github:pjan/riptide";
  };

  outputs = { self, nixpkgs, home-manager, riptide }: {
    homeConfigurations.yourusername = home-manager.lib.homeManagerConfiguration {
      pkgs = nixpkgs.legacyPackages.x86_64-linux;
      modules = [
        {
          home.packages = [
            riptide.packages.x86_64-linux.default
          ];
        }
      ];
    };
  };
}
```

## Package Details

The Nix package:
- Builds riptide using Python 3.13
- Includes ffmpeg as a runtime dependency (required for track conversion)
- Uses setuptools as the build backend
- Disables tests (they require network access and Tidal credentials)
- Works on Unix-like systems (Linux, macOS)

## Dependencies

The package includes all required Python dependencies:
- aiofiles
- aiohttp
- m3u8
- mutagen
- pydantic
- requests
- requests-cache
- typer

FFmpeg is automatically added to PATH when running riptide.

## Troubleshooting

### Python version mismatch

The package requires Python 3.13+. If you're using an older nixpkgs snapshot, you may need to update to nixos-unstable or use an overlay to get a newer Python version.

### Missing ffmpeg

The package automatically wraps riptide to include ffmpeg in PATH. If you still encounter ffmpeg-related errors, ensure ffmpeg is available in your nixpkgs.

### Build failures

If you encounter build failures, try:
1. Updating your nixpkgs: `nix flake update`
2. Clearing the build cache: `nix-collect-garbage`
3. Checking that all Python dependencies are available in nixpkgs
