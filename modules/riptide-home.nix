{ config, lib, pkgs, ... }:

with lib;

let
  cfg = config.programs.riptide;

  # TOML format helper
  tomlFormat = pkgs.formats.toml {};

  # Recursively filter out null values from attribute sets
  filterNulls = attrs:
    if isAttrs attrs then
      let
        filtered = filterAttrs (n: v: v != null) attrs;
      in
      mapAttrs (n: v: if isAttrs v then filterNulls v else v) filtered
    else
      attrs;

  # Build the config TOML from user settings with nulls filtered out
  cleanedSettings = filterNulls cfg.settings;
in
{
  options.programs.riptide = {
    enable = mkEnableOption "riptide - CLI for downloading tidal tracks";

    package = mkOption {
      type = types.package;
      default = pkgs.riptide or (pkgs.callPackage ../nix/package.nix { });
      defaultText = literalExpression "pkgs.riptide";
      description = "The riptide package to use.";
    };

    service = {
      enable = mkEnableOption "riptide listener service";
    };

    settings = mkOption {
      type = types.submodule {
        freeformType = (pkgs.formats.toml {}).type;

        options = {
          enable_cache = mkOption {
            type = types.bool;
            default = true;
            description = "Cache API requests for improved speed.";
          };

          debug = mkOption {
            type = types.bool;
            default = false;
            description = "Enable debug mode to save API calls.";
          };

          metadata = mkOption {
            type = types.submodule {
              options = {
                enable = mkOption {
                  type = types.bool;
                  default = true;
                  description = "Embed metadata in files.";
                };

                lyrics = mkOption {
                  type = types.bool;
                  default = true;
                  description = "Embed lyrics in metadata.";
                };

                cover = mkOption {
                  type = types.bool;
                  default = true;
                  description = "Embed track cover in the track file.";
                };

                album_review = mkOption {
                  type = types.bool;
                  default = false;
                  description = "Embed album review text to track COMMENT metadata field.";
                };

                save_lyrics_to_lrc = mkOption {
                  type = types.bool;
                  default = true;
                  description = "Save lyrics to a separate .lrc file.";
                };
              };
            };
            default = {};
            description = "Metadata configuration.";
          };

          cover = mkOption {
            type = types.submodule {
              options = {
                save = mkOption {
                  type = types.bool;
                  default = false;
                  description = "Save cover to distinct file.";
                };

                size = mkOption {
                  type = types.int;
                  default = 1280;
                  description = "Size of cover (max 1280x1280).";
                };

                allowed = mkOption {
                  type = types.listOf (types.enum ["track" "album" "playlist"]);
                  default = [];
                  description = "Resource types to save covers for.";
                };

                templates = mkOption {
                  type = types.attrsOf types.str;
                  default = {};
                  description = "Cover file path templates.";
                  example = {
                    track = "covers/{item.id}";
                    album = "covers/{album.artist} - {album.title}";
                  };
                };
              };
            };
            default = {};
            description = "Cover configuration.";
          };

          download = mkOption {
            type = types.submodule {
              options = {
                track_quality = mkOption {
                  type = types.enum ["low" "normal" "high" "max"];
                  default = "high";
                  description = "Track quality (low: 96kbps, normal: 320kbps, high: 16bit 44.1kHz, max: up to 24bit 192kHz).";
                };

                video_quality = mkOption {
                  type = types.enum ["sd" "hd" "fhd"];
                  default = "fhd";
                  description = "Video quality (sd: 360p, hd: 720p, fhd: 1080p).";
                };

                skip_existing = mkOption {
                  type = types.bool;
                  default = true;
                  description = "Skip already downloaded files.";
                };

                threads_count = mkOption {
                  type = types.int;
                  default = 4;
                  description = "Number of concurrent download threads.";
                };

                download_path = mkOption {
                  type = types.nullOr types.str;
                  default = null;
                  description = "Base directory for downloads.";
                };

                scan_path = mkOption {
                  type = types.nullOr types.str;
                  default = null;
                  description = "Directory to scan for existing downloads.";
                };

                singles_filter = mkOption {
                  type = types.enum ["none" "only" "include"];
                  default = "none";
                  description = "Filter for artist singles (none: only albums, only: only singles, include: both).";
                };

                videos_filter = mkOption {
                  type = types.enum ["none" "only" "allow"];
                  default = "none";
                  description = "Videos handling (none: exclude, only: only videos, allow: include).";
                };

                update_mtime = mkOption {
                  type = types.bool;
                  default = false;
                  description = "Update modification time of existing files.";
                };

                rewrite_metadata = mkOption {
                  type = types.bool;
                  default = false;
                  description = "Rewrite metadata for already downloaded tracks.";
                };

                templates = mkOption {
                  type = types.attrsOf types.str;
                  default = {};
                  description = "File path templates for downloads.";
                  example = {
                    default = "{album.artists}/{album.date:%Y} - {album.title}/{item.number:02d} {item.title}";
                    track = "tracks/{item.id}";
                  };
                };
              };
            };
            default = {};
            description = "Download configuration.";
          };

          m3u = mkOption {
            type = types.submodule {
              options = {
                save = mkOption {
                  type = types.bool;
                  default = false;
                  description = "Save M3U playlist files.";
                };

                allowed = mkOption {
                  type = types.listOf (types.enum ["album" "playlist" "mix"]);
                  default = [];
                  description = "Resource types to save M3U files for.";
                };

                templates = mkOption {
                  type = types.attrsOf types.str;
                  default = {};
                  description = "M3U file path templates.";
                  example = {
                    album = "m3u/{album.artist} - {album.title}";
                  };
                };
              };
            };
            default = {};
            description = "M3U playlist configuration.";
          };

          list = mkOption {
            type = types.submodule {
              options = {
                format = mkOption {
                  type = types.str;
                  default = "{item.id} | {item.artist} | {album.title} | {item.number:02d} | {item.title}";
                  description = "Format string for list command output.";
                };
              };
            };
            default = {};
            description = "List command configuration.";
          };

          listener = mkOption {
            type = types.submodule {
              options = {
                port = mkOption {
                  type = types.port;
                  default = 8123;
                  description = "Port for the HTTP listener server.";
                };

                secret = mkOption {
                  type = types.str;
                  default = "";
                  description = "Authentication secret for API requests (empty = no auth).";
                };

                concurrent_downloads = mkOption {
                  type = types.int;
                  default = 4;
                  description = "Number of tracks to download in parallel.";
                };
              };
            };
            default = {};
            description = "Listener configuration.";
          };
        };
      };
      default = {};
      description = ''
        Configuration for riptide written to {file}`~/.config/riptide/config.toml`.
        See <https://github.com/pjan/riptide/blob/main/docs/config.example.toml> for all options.
      '';
      example = literalExpression ''
        {
          download = {
            track_quality = "max";
            download_path = "\''${config.home.homeDirectory}/Music/Tidal";
            threads_count = 8;
          };
          listener = {
            port = 8123;
            secret = "my-secret-key";
            concurrent_downloads = 5;
          };
        }
      '';
    };
  };

  config = mkIf cfg.enable {
    home.packages = [ cfg.package ];

    xdg.configFile."riptide/config.toml" = mkIf (cfg.settings != {}) {
      source = tomlFormat.generate "config.toml" cleanedSettings;
    };

    # Linux: systemd service
    systemd.user.services.riptide-listener = mkIf (cfg.service.enable && pkgs.stdenv.isLinux) {
      Unit = {
        Description = "Riptide Listener - HTTP API for downloading Tidal tracks";
        After = [ "network-online.target" ];
        Wants = [ "network-online.target" ];
      };

      Service = {
        Type = "simple";
        ExecStart = "${cfg.package}/bin/riptide listen";
        Restart = "on-failure";
        RestartSec = "10s";
      };
    };

    # macOS: launchd service
    launchd.agents.riptide-listener = mkIf (cfg.service.enable && pkgs.stdenv.isDarwin) {
      enable = true;
      config = {
        ProgramArguments = [ "${cfg.package}/bin/riptide" "listen" ];
        Label = "org.riptide.listener";
        RunAtLoad = true;
        KeepAlive = {
          Crashed = true;
          SuccessfulExit = false;
        };
        ProcessType = "Background";
        EnvironmentVariables = {
          RIPTIDE_PATH = "${config.home.homeDirectory}/.config/riptide";
        };
        StandardOutPath = "${config.home.homeDirectory}/Library/Logs/riptide-listener.log";
        StandardErrorPath = "${config.home.homeDirectory}/Library/Logs/riptide-listener.log";
      };
    };
  };
}
