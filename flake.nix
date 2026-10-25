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

        devShells.default = eachSystem (system:
          let pkgs = nixpkgs.legacyPackages.${system};
          in pkgs.mkShell {
            nativeBuildInputs = with pkgs; [
              rustc
              cargo
              cargo-nextest
              clippy
              rustfmt
              pkg-config
              protobuf
              cacert
            ];

            buildInputs = with pkgs; [
              openssl
              ffmpeg
            ];

            shellHook = ''
              echo "riptide development environment"
              echo " > Rust: $(rustc --version)"
              echo " > FFmpeg: $(ffmpeg -version | head -n1)"
            '';
          });
      };
}
