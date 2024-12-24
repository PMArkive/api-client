{
  rustPlatform,
  openssl,
  pkg-config,
  lib,
}: let
  inherit (lib.sources) sourceByRegex;
  inherit (builtins) fromTOML readFile;
  src = sourceByRegex ../. ["Cargo.*" "(src|tests)(/.*)?"];
  cargoPackage = (fromTOML (readFile ../Cargo.toml)).package;
in
  rustPlatform.buildRustPackage {
    pname = cargoPackage.name;
    inherit (cargoPackage) version;

    inherit src;

    buildInputs = [
      openssl
    ];

    nativeBuildInputs = [
      pkg-config
    ];

    doCheck = false;

    buildPhase = ''
      cargo build --tests
    '';

    installPhase = ''
      mkdir -p $out/bin
      cp target/debug/deps/tests-???????????????? $out/bin/api-test
    '';

    cargoLock = {
      lockFile = ../Cargo.lock;
    };

    meta.mainProgram = "api-test";
  }
