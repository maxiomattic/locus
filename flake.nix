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
      nixosModules.server =
        { pkgs, lib, ... }:
        {
          imports = [
            disko.nixosModules.disko
            ./nix/modules/server.nix
          ];
          locus.server.gateway.package =
            lib.mkDefault self.packages.${pkgs.stdenv.hostPlatform.system}.locus-gateway;
          # Every Locus server serves the browser app at /app/ out of the box.
          locus.server.gateway.webRoot =
            lib.mkDefault self.packages.${pkgs.stdenv.hostPlatform.system}.locus-web;
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

          # One vendored dependency set for every crate in the workspace.
          # Update the hash whenever Cargo.lock changes.
          cargoDeps = rustPlatform.fetchCargoVendor {
            src = pkgs.lib.cleanSource ./.;
            name = "locus-cargo-deps";
            hash = "sha256-p/TaBK4e/Nm9djXw5E8oWKN1C1Rz1rJI7PAm5QAmlpE=";
          };

          # Vere (urbit runtime) pin: nixpkgs-unstable `pkgs.urbit` 4.6
          # (vere-v4.6, Arvo 408k — same kelvin as desk/sys/kelvin). Locked by flake.lock.
          urbit = pkgs.urbit;
        in
        {
          inherit urbit;
          server = urbit;
          locus-cosmic-defaults = pkgs.callPackage ./nix/cosmic-defaults.nix { };

          locus-gateway = rustPlatform.buildRustPackage {
            pname = "locus-gateway";
            version = "0.1.0";
            src = pkgs.lib.cleanSource ./.;
            inherit cargoDeps;
            cargoBuildFlags = [ "-p" "locus-gateway" ];
            doCheck = false;
            meta = {
              description = "Locus gateway: browser app, file bytes, Eyre proxy";
              mainProgram = "locus-gateway";
            };
          };

          notes-app = rustPlatform.buildRustPackage {
            pname = "notes-app";
            version = "0.1.0";
            src = pkgs.lib.cleanSource ./.;
            inherit cargoDeps;
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

          locus-desktop = rustPlatform.buildRustPackage {
            pname = "locus-desktop";
            version = "0.1.0";
            src = pkgs.lib.cleanSource ./.;
            inherit cargoDeps;
            cargoBuildFlags = [ "-p" "locus-desktop" ];
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
            # One launcher for all sections plus one per section. Each section
            # is its own app (`locus notes`, app id org.locus.Notes), so COSMIC
            # gives it its own taskbar entry and tiles it separately.
            postInstall = ''
              mkdir -p $out/share/applications
              entry() { # app-id name exec-args icon comment
                cat > "$out/share/applications/$1.desktop" <<EOF
              [Desktop Entry]
              Type=Application
              Name=$2
              Comment=$5
              Exec=$out/bin/locus $3
              Icon=$4
              StartupWMClass=$1
              StartupNotify=true
              Terminal=false
              Categories=Office;
              EOF
                sed -i 's/^ *//' "$out/share/applications/$1.desktop"
              }
              entry org.locus.Desktop "Locus" "" view-app-grid-symbolic "Notes, tasks, calendar, photos, files and contacts"
              entry org.locus.Notes "Locus Notes" notes accessories-text-editor-symbolic "Notes"
              entry org.locus.Todo "Locus Todo" todo object-select-symbolic "Tasks from your notes"
              entry org.locus.Calendar "Locus Calendar" calendar x-office-calendar-symbolic "Calendar"
              entry org.locus.Photos "Locus Photos" photos image-x-generic-symbolic "Photos"
              entry org.locus.Files "Locus Files" files folder-symbolic "Files"
              entry org.locus.Contacts "Locus Contacts" contacts x-office-address-book-symbolic "Contacts"
            '';
            meta = {
              description = "Locus desktop shell (libcosmic)";
              mainProgram = "locus";
            };
          };

          # Browser app bundle for the gateway's --web-root (own Cargo
          # workspace and lockfile; see nix/pkgs/locus-web.nix).
          locus-web = pkgs.callPackage ./nix/pkgs/locus-web.nix {
            rustToolchain = pkgs.rust-bin.stable.latest.default.override {
              targets = [ "wasm32-unknown-unknown" ];
            };
            wasm-bindgen-cli = pkgs.callPackage ./nix/pkgs/wasm-bindgen-cli.nix { };
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
            # Browser build of locus-web (upstream iced, webgl).
            targets = [ "wasm32-unknown-unknown" ];
          };
        in
        {
          default = pkgs.mkShell {
            packages = [
              rustToolchain
              # Must match the wasm-bindgen crate pinned in crates/locus-web.
              (pkgs.callPackage ./nix/pkgs/wasm-bindgen-cli.nix { })
              pkgs.binaryen
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
