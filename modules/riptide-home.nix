{ config, lib, pkgs, ... }:

with lib;

let
  cfg = config.programs.riptide;

  tomlFormat = pkgs.formats.toml {};

  filterNulls = attrs:
    if isAttrs attrs then
      let
        filtered = filterAttrs (n: v: v != null) attrs;
      in
      mapAttrs (n: v: if isAttrs v then filterNulls v else v) filtered
    else
      attrs;

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

    settings = mkOption {
      type = types.submodule {
        freeformType = (pkgs.formats.toml {}).type;

        options = {
          download = mkOption {
            type = types.submodule {
              options = {
                audio_quality = mkOption {
                  type = types.enum [ "low" "normal" "high" "max" ];
                  default = "high";
                  description = "Audio quality (low: 96kbps, normal: 320kbps, high: 16bit 44.1kHz, max: up to 24bit 192kHz).";
                };

                skip_existing = mkOption {
                  type = types.bool;
                  default = true;
                  description = "Skip already downloaded files.";
                };

                concurrency = mkOption {
                  type = types.int;
                  default = 4;
                  description = "Number of concurrent download tasks.";
                };

                download_root = mkOption {
                  type = types.nullOr types.str;
                  default = null;
                  description = "Base directory for downloads.";
                };

                singles_mode = mkOption {
                  type = types.enum [ "none" "only" "include" ];
                  default = "include";
                  description = "Singles handling (none: albums only, only: singles only, include: both).";
                };

                templates = mkOption {
                  type = types.submodule {
                    options = {
                      default = mkOption {
                        type = types.str;
                        default = "{album.artists}/{album.date:%Y} - {album.title}/{item.number:02d} {item.title}";
                        description = "Default download path template.";
                      };

                      track = mkOption {
                        type = types.nullOr types.str;
                        default = null;
                        description = "Optional track-specific path template override.";
                      };

                      album = mkOption {
                        type = types.nullOr types.str;
                        default = null;
                        description = "Optional album-specific path template override.";
                      };

                      playlist = mkOption {
                        type = types.nullOr types.str;
                        default = null;
                        description = "Optional playlist-specific path template override.";
                      };
                    };
                  };
                  default = {};
                  description = "Download path templates.";
                };
              };
            };
            default = {};
            description = "Download configuration.";
          };

          metadata = mkOption {
            type = types.submodule {
              options = {
                enable = mkOption {
                  type = types.bool;
                  default = true;
                  description = "Embed metadata in downloaded files.";
                };

                lyrics = mkOption {
                  type = types.bool;
                  default = true;
                  description = "Fetch and embed lyrics.";
                };

                cover = mkOption {
                  type = types.bool;
                  default = true;
                  description = "Fetch and embed cover art.";
                };
              };
            };
            default = {};
            description = "Metadata configuration.";
          };

          m3u = mkOption {
            type = types.submodule {
              options = {
                save = mkOption {
                  type = types.bool;
                  default = false;
                  description = "Save M3U playlist files alongside downloads.";
                };
              };
            };
            default = {};
            description = "M3U playlist configuration.";
          };
        };
      };
      default = {};
      description = ''
        Configuration for riptide written to {file}`~/.config/riptide/config.toml`.
        See <https://github.com/pjan/riptide> for available options.
      '';
    };
  };

  config = mkIf cfg.enable {
    home.packages = [ cfg.package ];

    xdg.configFile."riptide/config.toml" = mkIf (cfg.settings != {}) {
      source = tomlFormat.generate "config.toml" cleanedSettings;
    };
  };
}
