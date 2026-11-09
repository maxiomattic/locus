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

      # GStreamer plugin directories for the desktop player's decoders. The
      # core package's default output holds only the tools; its plugins
      # (typefind and the other core elements) are in `out`.
      gstPluginPathFor =
        pkgs:
        pkgs.lib.makeSearchPath "lib/gstreamer-1.0" [
          pkgs.gst_all_1.gstreamer.out
          pkgs.gst_all_1.gst-plugins-base
          pkgs.gst_all_1.gst-plugins-good
          pkgs.gst_all_1.gst-plugins-bad
          pkgs.gst_all_1.gst-libav
        ];
    in
    {
      # Clan machines import these. CalDAV, Attic, and operator
      # accounts live in the separate personal NixOS flake — not here.
      nixosModules.server = {
        imports = [
          disko.nixosModules.disko
          ./nix/modules/server.nix
          self.nixosModules.packageDefaults
        ];
      };
      nixosModules.default = self.nixosModules.server;

      # The services alone (ship, gateway, desk deploy) with no disk layout,
      # for a host whose disk Locus does not own, such as a non-NixOS host
      # run with system-manager. Set locus.server.zfs.enable = false there.
      nixosModules.services = {
        imports = [
          ./nix/modules/services.nix
          self.nixosModules.packageDefaults
        ];
      };

      # This flake's packages as the modules' defaults.
      nixosModules.packageDefaults =
        { pkgs, lib, ... }:
        {
          locus.server.gateway.package =
            lib.mkDefault self.packages.${pkgs.stdenv.hostPlatform.system}.locus-gateway;
          # Every Locus server serves the browser app at /app/ out of the box.
          locus.server.gateway.webRoot =
            lib.mkDefault self.packages.${pkgs.stdenv.hostPlatform.system}.locus-web;
          # The desk and the deploy script, without the rest of the repo, so
          # an unrelated change does not redeploy the desk.
          locus.server.desk.source = lib.mkDefault (
            lib.fileset.toSource {
              root = ./.;
              fileset = lib.fileset.unions [
                ./desk
                ./scripts/desk-deploy.sh
                ./scripts/lib
              ];
            }
          );
        };

      # `nix flake new -t github:maxiomattic/locus#server <dir>`: the host flake
      # for a new clan server. See docs/install.html.
      templates.server = {
        path = ./templates/server;
        description = "A Locus clan server (fill in host.nix)";
      };

      nixosModules.client =
        { pkgs, lib, ... }:
        {
          imports = [ ./nix/modules/client.nix ];
          locus.client.apps =
            lib.mkDefault self.packages.${pkgs.stdenv.hostPlatform.system}.locus-desktop;
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

          gstPluginPath = gstPluginPathFor pkgs;

          # What the Rust workspace builds from. Docs, the desk, scripts and
          # the browser app (its own workspace) are left out, so a change
          # there does not recompile the desktop apps and the gateway.
          rustSrc = pkgs.lib.fileset.toSource {
            root = ./.;
            fileset = pkgs.lib.fileset.difference (pkgs.lib.fileset.unions [
              ./Cargo.toml
              ./Cargo.lock
              ./crates
            ]) (pkgs.lib.fileset.unions [
              ./crates/locus-web
              (pkgs.lib.fileset.maybeMissing ./crates/locus-desktop/target)
            ]);
          };

          # One vendored dependency set for every crate in the workspace.
          # Update the hash whenever Cargo.lock changes.
          cargoDeps = rustPlatform.fetchCargoVendor {
            src = rustSrc;
            name = "locus-cargo-deps";
            hash = "sha256-bAL5J3vtF1kt2casrS5QR8+Ew4BOC0FGyYp3c+e4p+U=";
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
            src = rustSrc;
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
            src = rustSrc;
            inherit cargoDeps;
            cargoBuildFlags = [ "-p" "notes-app" ];
            doCheck = false;
            nativeBuildInputs = [
              pkgs.pkg-config
              pkgs.cmake
              pkgs.libcosmicAppHook
              pkgs.rustPlatform.bindgenHook
              pkgs.makeWrapper
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
            src = rustSrc;
            inherit cargoDeps;
            # The apps and the `locus` command line, which can do everything
            # the apps do.
            cargoBuildFlags = [ "-p" "locus-desktop" "-p" "locus-command" ];
            doCheck = false;
            nativeBuildInputs = [
              pkgs.pkg-config
              pkgs.cmake
              pkgs.libcosmicAppHook
              pkgs.rustPlatform.bindgenHook
              pkgs.makeWrapper
            ];
            buildInputs = [
              pkgs.fontconfig
              pkgs.freetype
              pkgs.expat
              pkgs.glib
              pkgs.dbus
              pkgs.gst_all_1.gstreamer
              pkgs.gst_all_1.gst-plugins-base
              pkgs.gst_all_1.gst-plugins-good
              pkgs.gst_all_1.gst-plugins-bad
              pkgs.gst_all_1.gst-libav
            ];
            # Each app is its own program (`locus-notes`, app id
            # org.locus.Notes), so COSMIC gives it its own taskbar entry and
            # tiles it separately. They are all one binary, locus-app.
            postInstall = ''
              mkdir -p $out/share/applications
              entry() { # app-id name program icon comment
                cat > "$out/share/applications/$1.desktop" <<EOF
              [Desktop Entry]
              Type=Application
              Name=$2
              Comment=$5
              Exec=$out/bin/$3
              Icon=$4
              StartupWMClass=$1
              StartupNotify=true
              Terminal=false
              Categories=Office;
              EOF
                sed -i 's/^ *//' "$out/share/applications/$1.desktop"
              }
              entry org.locus.Notes "Locus Notes" locus-notes org.locus.Notes "Notes"
              entry org.locus.Calendar "Locus Calendar" locus-calendar org.locus.Calendar "Calendar and tasks"
              entry org.locus.Photos "Locus Photos" locus-photos org.locus.Photos "Photos"
              entry org.locus.Files "Locus Files" locus-files org.locus.Files "Files"
              entry org.locus.Contacts "Locus Contacts" locus-contacts org.locus.Contacts "Contacts"
              entry org.locus.Movies "Locus Movies" locus-movies org.locus.Movies "Movies"
              entry org.locus.Music "Locus Music" locus-music org.locus.Music "Music"
              entry org.locus.Messages "Locus Messages" locus-messages org.locus.Messages "Messages"
              # Icons. An app with a COSMIC counterpart uses that app's icon
              # (GPL-3.0, from the COSMIC package); the others are drawn in
              # the same style, in share/icons.
              cosmicIcon() { # pkg cosmic-id locus-id
                for f in "$1"/share/icons/hicolor/*/apps/"$2".svg; do
                  size=$(basename "$(dirname "$(dirname "$f")")")
                  install -Dm644 "$f" "$out/share/icons/hicolor/$size/apps/$3.svg"
                done
              }
              cosmicIcon ${pkgs.cosmic-files} com.system76.CosmicFiles org.locus.Files
              cosmicIcon ${pkgs.cosmic-edit} com.system76.CosmicEdit org.locus.Notes
              cosmicIcon ${pkgs.cosmic-player} com.system76.CosmicPlayer org.locus.Movies
              for f in ${./share/icons}/*.svg; do
                install -Dm644 "$f" "$out/share/icons/hicolor/scalable/apps/$(basename "$f")"
              done
              # GStreamer finds its plugins through this variable, so the
              # binaries carry it.
              for b in $out/bin/locus-app $out/bin/locus; do
                wrapProgram "$b" --prefix GST_PLUGIN_SYSTEM_PATH_1_0 : "${gstPluginPath}"
              done
              # `locus-notes` and so on. Each names its app: the wrappers
              # above exec scripts, so the program name does not reach the
              # binary and cannot pick the app.
              for app in notes todo calendar photos files contacts movies music messages; do
                printf '#!${pkgs.runtimeShell}\nexec %s %s "$@"\n' "$out/bin/locus-app" "$app" > "$out/bin/locus-$app"
                chmod +x "$out/bin/locus-$app"
              done
            '';
            meta = {
              description = "Locus apps (libcosmic) and the locus command line";
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
              pkgs.gst_all_1.gstreamer
              pkgs.gst_all_1.gst-plugins-base
              pkgs.gst_all_1.gst-plugins-good
              pkgs.gst_all_1.gst-plugins-bad
              pkgs.gst_all_1.gst-libav
              pkgs.git
              pkgs.cosmic-icons
              pkgs.adwaita-icon-theme
            ];
            LIBCLANG_PATH = "${pkgs.libclang.lib}/lib";
            GST_PLUGIN_SYSTEM_PATH_1_0 = gstPluginPathFor pkgs;
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
              echo "  cargo run -p locus-desktop -- notes|todo|...  # a desktop app"
              echo "  cargo run -p locus-command -- --help   # the locus command line"
              echo "  ./scripts/web-build.sh      # browser app -> dist/web"
              echo "  cargo test --workspace      # shared core, client, gateway"
              echo "  ./scripts/server-vm.sh      # local test VM ship"
              echo "  ./scripts/desk-deploy.sh    # safe desk deploy to a real ship"
              export XDG_DATA_DIRS="${pkgs.cosmic-icons}/share:${pkgs.adwaita-icon-theme}/share''${XDG_DATA_DIRS:+:$XDG_DATA_DIRS}"
            '';
          };
        }
      );
    };
}
