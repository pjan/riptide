{
  lib,
  rustPlatform,
  openssl,
  pkg-config,
  protobuf,
  makeWrapper,
  ffmpeg,
  cacert,
}:

rustPlatform.buildRustPackage {
  pname = "riptide";
  version = "0.1.0";

  src = ../riptide-rs;
  cargoLock.lockFile = ../riptide-rs/Cargo.lock;

  nativeBuildInputs = [
    pkg-config
    protobuf
    makeWrapper
  ];

  buildInputs = [ openssl ];

  cargoBuildFlags = [ "--workspace" ];

  SSL_CERT_FILE = "${cacert}/etc/ssl/certs/ca-bundle.crt";
  GIT_SSL_CAINFO = "${cacert}/etc/ssl/certs/ca-bundle.crt";
  NIX_SSL_CERT_FILE = "${cacert}/etc/ssl/certs/ca-bundle.crt";
  CURL_CA_BUNDLE = "${cacert}/etc/ssl/certs/ca-bundle.crt";
  CARGO_HTTP_CAINFO = "${cacert}/etc/ssl/certs/ca-bundle.crt";

  postInstall = ''
    mv "$out/bin/riptide-rs" "$out/bin/riptide"
    wrapProgram "$out/bin/riptide" \
      --prefix PATH : "${lib.makeBinPath [ ffmpeg ]}" \
      --set-default SSL_CERT_FILE "${cacert}/etc/ssl/certs/ca-bundle.crt" \
      --set-default CURL_CA_BUNDLE "${cacert}/etc/ssl/certs/ca-bundle.crt"
  '';

  doCheck = false;

  meta = {
    description = "CLI downloader for Tidal music";
    homepage = "https://github.com/pjan/riptide";
    license = lib.licenses.asl20;
    mainProgram = "riptide";
    platforms = lib.platforms.unix;
  };
}
