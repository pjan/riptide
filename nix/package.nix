{ lib
, python3
, fetchFromGitHub
, ffmpeg
}:

python3.pkgs.buildPythonApplication rec {
  pname = "riptide";
  version = "3.2.1";
  pyproject = true;

  src = ../.;

  build-system = with python3.pkgs; [
    setuptools
    wheel
  ];

  dependencies = with python3.pkgs; [
    aiofiles
    aiohttp
    flask
    m3u8
    mutagen
    pydantic
    requests
    requests-cache
    typer
  ];

  # Ensure ffmpeg is available at runtime
  makeWrapperArgs = [
    "--prefix PATH : ${lib.makeBinPath [ ffmpeg ]}"
  ];

  # Tests require network access and Tidal credentials
  doCheck = false;

  pythonImportsCheck = [ "riptide" ];

  meta = with lib; {
    description = "CLI downloader for Tidal music";
    homepage = "https://github.com/pjan/riptide";
    license = licenses.asl20;
    maintainers = [ ];
    mainProgram = "riptide";
    platforms = platforms.unix;
  };
}
