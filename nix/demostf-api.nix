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
      rev = "9595b7f6f520fffb6e31c31c08d897b5b7593574";
      hash = "sha256-HncThFvIQ02QD8jpdsj70kvE+OlVO/loKM3hCVgJ2tk=";
    };

    vendorHash = "sha256-EYWCR2aJAoyWvEX+SML4Fb3F3KGcUtwCgqhAGT6ZjZ4=";

    composerStrictValidation = false;

    doCheck = false;
  })
