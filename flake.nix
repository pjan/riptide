{
  description = "riptide - CLI for downloading tidal tracks";

  nixConfig = {
    extra-substituters = [
      "https://cache.nixos.org"
      "s3://io-vandaele-nix-cache?endpoint=https://storage.googleapis.com&profile=gcp"
    ];
    extra-trusted-public-keys = [
      "cache.nixos.org-1:6NCHdD59X431o0gWypbMrAURkbJ16ZPMQFGspcDShjY="
      "io-vandaele-nix-cache:O9fdSbSnySVmQFa353NjmU/lP5GifyVfOmDM2XhBs8Q="
    ];
  };

  inputs = {
    nixpkgs.url = "github:NixOS/nixpkgs/nixos-unstable";
    systems.url = "github:nix-systems/default";
  };

  outputs =
    {
      self,
      nixpkgs,
      systems,
    }:
      let
        eachSystem = nixpkgs.lib.genAttrs (import systems);
      in
      {

        homeManagerModules ={
          riptide = ./modules/riptide-home.nix;
          default = self.homeManagerModules.riptide;
        };

        packages = eachSystem (system: {
          riptide = nixpkgs.legacyPackages.${system}.callPackage ./nix/package.nix { };
          default = self.packages.${system}.riptide;
        });

        apps = eachSystem (system: {
          default = {
            type = "app";
            program = "${self.packages.${system}.default}/bin/riptide";
          };
        });

        devShells.default = eachSystem (system: nixpkgs.legacyPackages.${system}.mkShell {
          buildInputs = with nixpkgs.legacyPackages.${system}; [
            python313
            ffmpeg
          ] ++ (with pkgs.python313Packages; [
            aiofiles
            aiohttp
            flask
            m3u8
            mutagen
            pydantic
            requests
            requests-cache
            typer
            pytest
            pytest-mock
          ]);

          shellHook = ''
            echo "riptide development environment"
            echo " > Python: $(python --version)"
            echo " > FFmpeg: $(ffmpeg -version | head -n1)"
          '';
        });
      };
}
