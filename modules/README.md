# Riptide Home Manager Module

This directory contains the Home Manager module for riptide, allowing easy installation and configuration via Home Manager.

## Usage

### Basic Setup

Add riptide to your Home Manager configuration:

```nix
{
  inputs = {
    nixpkgs.url = "github:NixOS/nixpkgs/nixos-unstable";
    home-manager.url = "github:nix-community/home-manager";
    riptide.url = "github:pjan/riptide";
  };

  outputs = { self, nixpkgs, home-manager, riptide, ... }: {
    homeConfigurations.yourusername = home-manager.lib.homeManagerConfiguration {
      pkgs = nixpkgs.legacyPackages.x86_64-linux;
      modules = [
        riptide.homeManagerModules.default
        {
          programs.riptide.enable = true;
        }
      ];
    };
  };
}
```

### With Configuration

Configure riptide settings directly in your Home Manager configuration:

```nix
{
  programs.riptide = {
    enable = true;
    settings = {
      download = {
        track_quality = "max";
        download_path = "${config.home.homeDirectory}/Music/Tidal";
        threads_count = 8;
      };
      listener = {
        port = 8123;
        secret = "my-secret-key";
        concurrent_downloads = 5;
      };
    };
  };
}
```

### With Listener Service

Enable the listener to run automatically on startup:

```nix
{
  programs.riptide = {
    enable = true;
    service.enable = true;  # Auto-start listener
    settings = {
      listener = {
        port = 8123;
        secret = "my-secret-key";
        concurrent_downloads = 5;
      };
      download = {
        download_path = "${config.home.homeDirectory}/Music/Tidal";
      };
    };
  };
}
```

### Using a Specific Package

If you want to use a custom or overridden riptide package:

```nix
{
  programs.riptide = {
    enable = true;
    package = pkgs.riptide.override {
      # your overrides here
    };
  };
}
```

### Installation

After adding the module to your configuration:

```bash
home-manager switch
```

The `riptide` command will be available in your PATH.

## Module Options

### `programs.riptide.enable`

- **Type:** `boolean`
- **Default:** `false`
- **Description:** Whether to enable riptide.

### `programs.riptide.package`

- **Type:** `package`
- **Default:** `pkgs.riptide`
- **Description:** The riptide package to use.

### `programs.riptide.service.enable`

- **Type:** `boolean`
- **Default:** `false`
- **Description:** Whether to enable the riptide listener service. When enabled, runs `riptide listen` as a background service that starts automatically.
  - **Linux**: Uses systemd user service
  - **macOS**: Uses launchd agent

### `programs.riptide.settings`

- **Type:** `submodule`
- **Default:** `{}`
- **Description:** Configuration for riptide written to `~/.config/riptide/config.toml`.

#### Available Settings

All settings from the [config.example.toml](../docs/config.example.toml) are supported. Key options include:

**`settings.enable_cache`** (bool, default: `true`)
- Cache API requests for improved speed.

**`settings.debug`** (bool, default: `false`)
- Enable debug mode to save API calls.

**`settings.download.track_quality`** (enum, default: `"high"`)
- Track quality: `"low"` (96kbps), `"normal"` (320kbps), `"high"` (16bit 44.1kHz), `"max"` (up to 24bit 192kHz)

**`settings.download.video_quality`** (enum, default: `"fhd"`)
- Video quality: `"sd"` (360p), `"hd"` (720p), `"fhd"` (1080p)

**`settings.download.download_path`** (string, default: `"${HOME}/Downloads/riptide"`)
- Base directory for downloads.

**`settings.download.threads_count`** (int, default: `4`)
- Number of concurrent download threads.

**`settings.download.templates`** (attrs, default: `{}`)
- File path templates for downloads.

**`settings.listener.port`** (int, default: `8123`)
- Port for the HTTP listener server.

**`settings.listener.secret`** (string, default: `""`)
- Authentication secret for API requests (empty = no auth).

**`settings.listener.concurrent_downloads`** (int, default: `4`)
- Number of tracks to download in parallel.

**`settings.metadata.enable`** (bool, default: `true`)
- Embed metadata in files.

**`settings.m3u.save`** (bool, default: `false`)
- Save M3U playlist files.

See the module source for all available options.

## Configuration Methods

There are two ways to configure riptide:

1. **Via Home Manager** (recommended): Use the `settings` option as shown above. This will generate `~/.config/riptide/config.toml` automatically.

2. **Manual TOML file**: Create `~/.config/riptide/config.toml` directly. See the [main documentation](../docs/config.example.toml) for all options.

Note: If you use `settings` in your Home Manager configuration, it will overwrite any manual changes to the config file.

## Complete Example

Full flake configuration with settings:

```nix
{
  description = "My home configuration";

  inputs = {
    nixpkgs.url = "github:NixOS/nixpkgs/nixos-unstable";
    home-manager = {
      url = "github:nix-community/home-manager";
      inputs.nixpkgs.follows = "nixpkgs";
    };
    riptide.url = "github:pjan/riptide";
  };

  outputs = { nixpkgs, home-manager, riptide, ... }: {
    homeConfigurations."myuser" = home-manager.lib.homeManagerConfiguration {
      pkgs = import nixpkgs { system = "x86_64-linux"; };
      modules = [
        riptide.homeManagerModules.default
        ./home.nix
      ];
    };
  };
}
```

And in your `home.nix`:

```nix
{ config, pkgs, ... }:

{
  programs.riptide = {
    enable = true;
    settings = {
      download = {
        track_quality = "max";
        download_path = "${config.home.homeDirectory}/Music/Tidal";
        threads_count = 8;
        skip_existing = true;
        templates = {
          default = "{album.artists}/{album.date:%Y} - {album.title}/{item.number:02d} {item.title}";
        };
      };
      
      metadata = {
        enable = true;
        lyrics = true;
        save_lyrics_to_lrc = true;
      };
      
      listener = {
        port = 8123;
        secret = "my-secret-key";
        concurrent_downloads = 5;
      };
      
      m3u = {
        save = true;
        allowed = ["album" "playlist"];
      };
    };
  };

  # Your other home-manager configuration...
}
```

## Testing

To test the module without committing:

```bash
# In the riptide repository
nix flake check
```

## Advanced Examples

### Minimal Configuration

Just enable with defaults:

```nix
{
  programs.riptide.enable = true;
}
```

### High Quality Downloads

Configure for maximum quality:

```nix
{
  programs.riptide = {
    enable = true;
    settings = {
      download = {
        track_quality = "max";
        video_quality = "fhd";
        threads_count = 8;
      };
    };
  };
}
```

### Listener Mode for API

Set up the listener for browser extension integration:

```nix
{
  programs.riptide = {
    enable = true;
    settings = {
      listener = {
        port = 8123;
        secret = "change-me-to-a-secure-secret";
        concurrent_downloads = 6;
      };
      download = {
        download_path = "${config.home.homeDirectory}/Music/Tidal";
      };
    };
  };
}
```

### Listener Service (Auto-start)

Enable the listener as a systemd service that starts automatically:

```nix
{
  programs.riptide = {
    enable = true;
    service.enable = true;  # Start listener on login
    settings = {
      listener = {
        port = 8123;
        secret = "my-secret-key";
        concurrent_downloads = 5;
      };
      download = {
        download_path = "${config.home.homeDirectory}/Music/Tidal";
      };
    };
  };
}
```

The service will:
- Start automatically when you log in
- Restart on failure
- Run in the background listening for download requests

**On Linux**, check service status with:
```bash
systemctl --user status riptide-listener
journalctl --user -u riptide-listener -f
```

**On macOS**, check service status with:
```bash
launchctl list | grep riptide
tail -f ~/Library/Logs/riptide-listener.log
```

## Service Management

When `service.enable` is true, you can manage the listener service:

### Linux (systemd)

```bash
# Check status
systemctl --user status riptide-listener

# View logs
journalctl --user -u riptide-listener -f

# Restart service
systemctl --user restart riptide-listener

# Stop service
systemctl --user stop riptide-listener

# Start service
systemctl --user start riptide-listener
```

The systemd service runs with security hardening:
- Private `/tmp` directory
- Read-only home directory (except config and download paths)
- No privilege escalation
- System protection enabled

### macOS (launchd)

```bash
# Check status
launchctl list | grep riptide

# View logs
tail -f ~/Library/Logs/riptide-listener.log

# Restart service
launchctl kickstart -k gui/$(id -u)/org.riptide.listener

# Stop service
launchctl stop org.riptide.listener

# Start service
launchctl start org.riptide.listener

# Unload (disable)
launchctl unload ~/Library/LaunchAgents/org.riptide.listener.plist

# Load (enable)
launchctl load ~/Library/LaunchAgents/org.riptide.listener.plist
```

Logs are written to `~/Library/Logs/riptide-listener.log` on macOS.

## Future Enhancements

Potential future additions:
- `programs.riptide.enableShellCompletion` - Shell completion scripts
- `programs.riptide.service.verbose` - Enable verbose logging in service

Contributions welcome!