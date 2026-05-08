{
  description = "Rust workspace for riptide-rs";

  inputs = {
    nixpkgs.url = "github:NixOS/nixpkgs/nixos-unstable";
    flake-utils.url = "github:numtide/flake-utils";
  };

  outputs = { self, nixpkgs, flake-utils }:
    flake-utils.lib.eachDefaultSystem (system:
      let
        pkgs = nixpkgs.legacyPackages.${system};
        rustToolchain = pkgs.rustc;
        cargo = pkgs.cargo;
        commonNativeBuildInputs = [
          cargo
          rustToolchain
          pkgs.cargo-nextest
          pkgs.clippy
          pkgs.rustfmt
          pkgs.cacert
        ];
        certEnv = {
          SSL_CERT_FILE = "${pkgs.cacert}/etc/ssl/certs/ca-bundle.crt";
          GIT_SSL_CAINFO = "${pkgs.cacert}/etc/ssl/certs/ca-bundle.crt";
          NIX_SSL_CERT_FILE = "${pkgs.cacert}/etc/ssl/certs/ca-bundle.crt";
          CURL_CA_BUNDLE = "${pkgs.cacert}/etc/ssl/certs/ca-bundle.crt";
          CARGO_HTTP_CAINFO = "${pkgs.cacert}/etc/ssl/certs/ca-bundle.crt";
        };
        cargoCheck = name: buildPhase: pkgs.rustPlatform.buildRustPackage ({
          pname = name;
          version = "0.1.0";
          src = ./.;
          cargoLock.lockFile = ./Cargo.lock;
          nativeBuildInputs = commonNativeBuildInputs;
          inherit buildPhase;
          doCheck = false;
          installPhase = ''
            mkdir -p "$out"
            touch "$out/done"
          '';
        } // certEnv);
      in
      {
        packages.default = pkgs.rustPlatform.buildRustPackage ({
          pname = "riptide-rs";
          version = "0.1.0";
          src = ./.;
          cargoLock.lockFile = ./Cargo.lock;
          nativeBuildInputs = commonNativeBuildInputs;
          cargoBuildFlags = [ "--workspace" ];
          cargoTestFlags = [ "--workspace" ];
        } // certEnv);

        checks = {
          inherit (self.packages.${system}) default;

          fmt = pkgs.runCommand "riptide-rs-fmt" ({
            src = ./.;
            nativeBuildInputs = commonNativeBuildInputs;
          } // certEnv) ''
            cp -R "$src" ./src
            chmod -R u+w ./src
            cd ./src
            cargo fmt --check
            touch "$out"
          '';

          clippy = cargoCheck "riptide-rs-clippy" ''
            cargo clippy --workspace --all-targets -- -D warnings
          '';

          nextest = cargoCheck "riptide-rs-nextest" ''
            cargo nextest run --workspace
          '';
        };

        devShells.default = pkgs.mkShell (certEnv // {
          nativeBuildInputs = commonNativeBuildInputs ++ [
            pkgs.cargo-audit
            pkgs.cargo-deny
          ];
          shellHook = ''
            export SSL_CERT_FILE="${pkgs.cacert}/etc/ssl/certs/ca-bundle.crt"
            export GIT_SSL_CAINFO="${pkgs.cacert}/etc/ssl/certs/ca-bundle.crt"
            export NIX_SSL_CERT_FILE="${pkgs.cacert}/etc/ssl/certs/ca-bundle.crt"
            export CURL_CA_BUNDLE="${pkgs.cacert}/etc/ssl/certs/ca-bundle.crt"
            export CARGO_HTTP_CAINFO="${pkgs.cacert}/etc/ssl/certs/ca-bundle.crt"
          '';
        });
      });
}
