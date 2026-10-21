{
  description = "Glimpse desktop shell for Wayland";

  inputs.nixpkgs.url = "github:NixOS/nixpkgs/nixos-26.05";

  outputs =
    { self, nixpkgs }:
    let
      systems = [
        "x86_64-linux"
        "aarch64-linux"
      ];

      forAllSystems = f: nixpkgs.lib.genAttrs systems (system: f nixpkgs.legacyPackages.${system});
    in
    {
      packages = forAllSystems (pkgs: rec {
        glimpse = pkgs.callPackage ./nix/package.nix { };
        default = glimpse;
      });

      nixosModules.default = import ./nix/module.nix;

      checks = forAllSystems (
        pkgs:
        import ./nix/test.nix {
          inherit pkgs;
          module = self.nixosModules.default;
          package = self.packages.${pkgs.stdenv.hostPlatform.system}.default;
        }
      );
    };
}
