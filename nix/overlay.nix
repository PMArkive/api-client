final: prev: {
  demostf-api = final.callPackage ./demostf-api.nix {};
  demostf-parser = final.callPackage ./demostf-parser.nix {};
  test-runner = final.callPackage ./test-runner.nix {};
}
