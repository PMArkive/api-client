{
  php,
  fetchFromGitHub,
}: let
  phpWithExtensions = php.withExtensions ({
    enabled,
    all,
  }:
    enabled ++ (with all; [pdo apcu]));
in
  phpWithExtensions.buildComposerProject (finalAttrs: {
    pname = "demostf-api";
    version = "0.1.0";

    src = fetchFromGitHub {
      owner = "demostf";
      repo = "api";
      rev = "1a8380360b993226ae1c6fcc226011e03a6c1467";
      hash = "sha256-JcBRU1N44tt0QDLnj6z9MCT3V2s2dkf+JbpWb1rmXnY=";
    };

    vendorHash = "sha256-EYWCR2aJAoyWvEX+SML4Fb3F3KGcUtwCgqhAGT6ZjZ4=";

    composerStrictValidation = false;

    doCheck = false;
  })
