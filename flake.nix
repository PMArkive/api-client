{
  inputs = {
    nixpkgs.url = "nixpkgs/nixos-24.11";
    flakelight = {
      url = "github:nix-community/flakelight";
      inputs.nixpkgs.follows = "nixpkgs";
    };
    mill-scale = {
      url = "github:icewind1991/mill-scale";
      inputs.flakelight.follows = "flakelight";
    };
  };
  outputs = {mill-scale, ...}:
    mill-scale ./. {
      cargoTest = false;
      withOverlays = [(import ./nix/overlay.nix)];
      packages = {
        test-runner = pkgs: pkgs.test-runner;
      };
      checks = {
        test = pkgs: pkgs.nixosTest (import ./nix/test.nix);
      };
    };
}
