{
  description = "riptide - CLI for downloading tidal tracks";

  inputs = {
    nixpkgs.url = "github:NixOS/nixpkgs/nixos-unstable";
    flake-utils.url = "github:numtide/flake-utils";
  };

  outputs = { self, nixpkgs, flake-utils }:
    flake-utils.lib.eachDefaultSystem (system:
      let
        pkgs = nixpkgs.legacyPackages.${system};
      in
      {
        packages = {
          riptide = pkgs.callPackage ./nix/package.nix { };
          default = self.packages.${system}.riptide;
        };

        apps = {
          default = {
            type = "app";
            program = "${self.packages.${system}.default}/bin/riptide";
          };
        };

        homeManagerModules = {
          riptide = ./modules/riptide-home.nix;
          default = self.homeManagerModules.riptide;
        };

        devShells.default = pkgs.mkShell {
          buildInputs = with pkgs; [
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
        };
      }
    );
}
