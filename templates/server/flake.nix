{
  description = "A Locus clan server";

  inputs = {
    locus.url = "github:maxiomattic/locus";
    nixpkgs.follows = "locus/nixpkgs";
  };

  outputs =
    { locus, nixpkgs, ... }:
    {
      # The disk formatter, pinned with Locus (docs/install.html step 3).
      packages.x86_64-linux.disko = locus.inputs.disko.packages.x86_64-linux.disko;

      nixosConfigurations.server = nixpkgs.lib.nixosSystem {
        system = "x86_64-linux";
        modules = [
          locus.nixosModules.server
          ./host.nix
        ];
      };
    };
}
