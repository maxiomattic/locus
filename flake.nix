{
  description = "Locus — NixOS/COSMIC clients and Urbit-backed clan server";

  inputs = {
    nixpkgs.url = "github:NixOS/nixpkgs/nixos-unstable";
    rust-overlay = {
      url = "github:oxalica/rust-overlay";
      inputs.nixpkgs.follows = "nixpkgs";
    };
    disko = {
      url = "github:nix-community/disko";
      inputs.nixpkgs.follows = "nixpkgs";
    };
  };

  outputs =
    {
      self,
      nixpkgs,
      rust-overlay,
      disko,
    }:
    let
      systems = [
        "x86_64-linux"
        "aarch64-linux"
        "x86_64-darwin"
        "aarch64-darwin"
      ];
      forAllSystems = nixpkgs.lib.genAttrs systems;

      # Placeholder so the example config evaluates without matching a real disk.
      # Set locus.server.diskDevice to the server's /dev/disk/by-id/...
      # before Disko format.
      serverDiskPlaceholder = "/dev/disk/by-id/REPLACE-ME-server-disk";
    in
    {
      # Clan machines import these. CalDAV, Attic, and operator
      # accounts live in the separate personal NixOS flake — not here.
      nixosModules.server = {
        imports = [
          disko.nixosModules.disko
          ./nix/modules/server.nix
        ];
      };
      nixosModules.default = self.nixosModules.server;

      nixosModules.client = {
        imports = [ ./nix/modules/client.nix ];
      };

      diskoConfigurations.server = {
        disko.devices = import ./nix/disko.nix {
          diskDevice = serverDiskPlaceholder;
        };
      };

      nixosConfigurations.server = nixpkgs.lib.nixosSystem {
        system = "x86_64-linux";
        modules = [
          self.nixosModules.server
          {
            locus.server = {
              enable = true;
              diskDevice = serverDiskPlaceholder;
            };
            nixpkgs.hostPlatform = "x86_64-linux";
            system.stateVersion = "25.05";
            networking.hostName = "server";
            # ZFS requires a unique 8-hex-char hostId per pool-importing host.
            networking.hostId = "d05e0001";
            boot.loader.systemd-boot.enable = true;
            boot.loader.efi.canTouchEfiVariables = true;
          }
        ];
      };

      # Bring-up VM: Disko on dedicated virtio /dev/vda (not the laptop disk,
      # not another VM's disk). ISO: nix build .#server-vm-iso
      nixosConfigurations.server-vm = nixpkgs.lib.nixosSystem {
        system = "x86_64-linux";
        modules = [
          self.nixosModules.server
          ./nix/modules/server-vm.nix
        ];
      };

      nixosConfigurations.server-vm-iso = nixpkgs.lib.nixosSystem {
        system = "x86_64-linux";
        specialArgs = { inherit self; };
        modules = [ ./nix/installer-iso.nix ];
      };

      # Evaluable Locus desktop. Placeholder root disk — not the laptop.
      nixosConfigurations.client = nixpkgs.lib.nixosSystem {
        system = "x86_64-linux";
        modules = [
          self.nixosModules.client
          {
            locus.client.enable = true;
            nixpkgs.hostPlatform = "x86_64-linux";
            system.stateVersion = "25.05";
            networking.hostName = "client";
            boot.loader.systemd-boot.enable = true;
            boot.loader.efi.canTouchEfiVariables = true;
            fileSystems."/" = {
              device = "/dev/disk/by-id/REPLACE-ME-client-disk";
              fsType = "ext4";
            };
          }
        ];
      };

      packages = forAllSystems (
        system:
        let
          overlays = [ (import rust-overlay) ];
          pkgs = import nixpkgs { inherit system overlays; };

          # libcosmic (pop-os) requires rust-version 1.93+.
          rustToolchain = pkgs.rust-bin.stable.latest.default.override {
            extensions = [
              "rust-src"
              "rust-analyzer"
              "clippy"
              "rustfmt"
            ];
          };
          rustPlatform = pkgs.makeRustPlatform {
            cargo = rustToolchain;
            rustc = rustToolchain;
          };

          # Vere (urbit runtime) pin: nixpkgs-unstable `pkgs.urbit` 4.6
          # (vere-v4.6, Arvo 408k — same kelvin as desk/sys/kelvin). Locked by flake.lock.
          urbit = pkgs.urbit;
        in
        {
          inherit urbit;
          server = urbit;
          locus-cosmic-defaults = pkgs.callPackage ./nix/cosmic-defaults.nix { };

          notes-app = rustPlatform.buildRustPackage {
            pname = "notes-app";
            version = "0.1.0";
            src = pkgs.lib.cleanSource ./.;
            cargoHash = "sha256-sFkU9I1ZRk4g8dZ/D9VdgFr4+kAJ1/bp83/RCOxmb9o=";
            cargoBuildFlags = [ "-p" "notes-app" ];
            doCheck = false;
            nativeBuildInputs = [
              pkgs.pkg-config
              pkgs.cmake
              pkgs.libcosmicAppHook
              pkgs.rustPlatform.bindgenHook
            ];
            buildInputs = [
              pkgs.fontconfig
              pkgs.freetype
              pkgs.expat
              pkgs.glib
              pkgs.dbus
            ];
            meta = {
              description = "Locus notes client (libcosmic)";
              mainProgram = "notes-app";
            };
          };

          default = self.packages.${system}.notes-app;
        }
        // nixpkgs.lib.optionalAttrs (system == "x86_64-linux") {
          server-vm-iso = self.nixosConfigurations.server-vm-iso.config.system.build.isoImage;
        }
      );

      checks.x86_64-linux =
        let
          pkgs = nixpkgs.legacyPackages.x86_64-linux;
          server = self.nixosConfigurations.server.config;
          client = self.nixosConfigurations.client.config;
        in
        {
          server-eval = pkgs.runCommand "locus-server-eval" { } ''
            set -eu
            [ "${server.networking.hostName}" = server ]
            [ "${server.fileSystems."/var/lib/urbit/pier".mountPoint}" = /var/lib/urbit/pier ]
            [ "${server.fileSystems."/var/lib/locus/photos".mountPoint}" = /var/lib/locus/photos ]
            touch "$out"
          '';
          client-eval = pkgs.runCommand "locus-client-eval" { } ''
            set -eu
            [ "${client.networking.hostName}" = client ]
            touch "$out"
          '';
        };

      devShells = forAllSystems (
        system:
        let
          overlays = [ (import rust-overlay) ];
          pkgs = import nixpkgs { inherit system overlays; };
          rustToolchain = pkgs.rust-bin.stable.latest.default.override {
            extensions = [
              "rust-src"
              "rust-analyzer"
              "clippy"
              "rustfmt"
            ];
          };
        in
        {
          default = pkgs.mkShell {
            packages = [
              rustToolchain
              pkgs.pkg-config
              pkgs.cmake
              pkgs.fontconfig
              pkgs.freetype
              pkgs.expat
              pkgs.glib
              pkgs.dbus
              pkgs.libxkbcommon
              pkgs.wayland
              pkgs.libGL
              pkgs.vulkan-loader
              pkgs.git
              pkgs.cosmic-icons
              pkgs.adwaita-icon-theme
            ];
            LIBCLANG_PATH = "${pkgs.libclang.lib}/lib";
            LD_LIBRARY_PATH = pkgs.lib.makeLibraryPath [
              pkgs.wayland
              pkgs.libxkbcommon
              pkgs.libGL
              pkgs.vulkan-loader
              pkgs.fontconfig
              pkgs.freetype
            ];
            XDG_DATA_DIRS = "${pkgs.cosmic-icons}/share:${pkgs.adwaita-icon-theme}/share";
            shellHook = ''
              echo "Locus dev shell"
              echo "  rustc: $(rustc --version)"
              echo "  cargo run --release -p notes-app"
              echo "  cargo run -p locus-command --bin locus -- help"
              echo "  nix build .#notes-app"
              echo "  nix build .#server    # Vere 4.6 binary"
              echo "  ./scripts/cosmic-check.sh  # COSMIC client eval (not a laptop rebuild)"
              echo "  ./scripts/check.sh          # photos cargo tests + photos dataset eval"
              echo "  nix flake check --no-build  # flake eval checks"
              echo "  # ship: nixosConfigurations.server → urbit.service (docs/urbit.md)"
              export XDG_DATA_DIRS="${pkgs.cosmic-icons}/share:${pkgs.adwaita-icon-theme}/share''${XDG_DATA_DIRS:+:$XDG_DATA_DIRS}"
            '';
          };
        }
      );
    };
}
