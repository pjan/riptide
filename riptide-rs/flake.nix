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
          pkgs.pkg-config
          pkgs.protobuf
          pkgs.rustfmt
          pkgs.cacert
        ];
        commonBuildInputs = [
          pkgs.openssl
        ];
        runtimeInputs = [
          pkgs.ffmpeg
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
          buildInputs = commonBuildInputs;
          inherit buildPhase;
          doCheck = false;
          installPhase = ''
            mkdir -p "$out"
            touch "$out/done"
          '';
        } // certEnv);
        riptide-rs = pkgs.rustPlatform.buildRustPackage ({
          pname = "riptide-rs";
          version = "0.1.0";
          src = ./.;
          cargoLock.lockFile = ./Cargo.lock;
          nativeBuildInputs = commonNativeBuildInputs ++ [
            pkgs.makeWrapper
          ];
          buildInputs = commonBuildInputs;
          cargoBuildFlags = [ "--workspace" ];
          cargoTestFlags = [ "--workspace" ];
          postInstall = ''
            wrapProgram "$out/bin/riptide-rs" \
              --prefix PATH : "${pkgs.lib.makeBinPath runtimeInputs}" \
              --set-default SSL_CERT_FILE "${pkgs.cacert}/etc/ssl/certs/ca-bundle.crt" \
              --set-default CURL_CA_BUNDLE "${pkgs.cacert}/etc/ssl/certs/ca-bundle.crt"
          '';
        } // certEnv);
      in
      {
        packages = {
          default = riptide-rs;
          riptide-rs = riptide-rs;
        };

        apps = {
          default = {
            type = "app";
            program = "${riptide-rs}/bin/riptide-rs";
          };
          riptide-rs = {
            type = "app";
            program = "${riptide-rs}/bin/riptide-rs";
          };
        };

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
            pkgs.ffmpeg
          ];
          shellHook = ''
            cert_file="${pkgs.cacert}/etc/ssl/certs/ca-bundle.crt"
            if [ -f "$HOME/.certs/root_bundle.pem" ]; then
              cert_file="$HOME/.certs/root_bundle.pem"
            fi
            export SSL_CERT_FILE="$cert_file"
            export GIT_SSL_CAINFO="$cert_file"
            export NIX_SSL_CERT_FILE="$cert_file"
            export CURL_CA_BUNDLE="$cert_file"
            export CARGO_HTTP_CAINFO="$cert_file"
          '';
        });
      });
}
