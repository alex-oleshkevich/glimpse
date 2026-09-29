{
  description = "Glimpse desktop shell for Wayland";

  inputs.nixpkgs.url = "github:NixOS/nixpkgs/nixos-26.05";

  outputs = { self, nixpkgs }:
    let
      system = "x86_64-linux";
      pkgs = nixpkgs.legacyPackages.${system};
    in {
      packages.${system}.default = pkgs.callPackage ./nix/package.nix { };
      nixosModules.default = import ./nix/module.nix;
      checks.${system} = import ./nix/test.nix {
        inherit pkgs;
        module = self.nixosModules.default;
        package = self.packages.${system}.default;
      };
    };
}
