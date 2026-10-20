{
  description = "A Locus clan server";

  inputs = {
    locus.url = "github:maxiomattic/locus";
    nixpkgs.follows = "locus/nixpkgs";
  };

  outputs =
    { locus, nixpkgs, ... }:
    {
      nixosConfigurations.server = nixpkgs.lib.nixosSystem {
        system = "x86_64-linux";
        modules = [
          locus.nixosModules.server
          ./host.nix
        ];
      };
    };
}
