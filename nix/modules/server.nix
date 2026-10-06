{
  config,
  lib,
  pkgs,
  ...
}:
let
  cfg = config.digitalWorkspace.server;
in
{
  imports = [ ./urbit.nix ];

  options.digitalWorkspace.server = {
    enable = lib.mkEnableOption "household server (Disko ZFS, pier dataset, snapshots, Urbit unit)";

    diskDevice = lib.mkOption {
      type = lib.types.str;
      example = "/dev/disk/by-id/nvme-SERVER-DISK";
      description = ''
        Dedicated server disk for Disko (prefer /dev/disk/by-id/...).
        Format applies to this device only. Do not point this at a laptop
        or any disk that holds unrelated data.
      '';
    };

    poolName = lib.mkOption {
      type = lib.types.str;
      default = "rpool";
      description = "ZFS pool name. Pier dataset is \${poolName}/urbit/pier.";
    };

    pierMountpoint = lib.mkOption {
      type = lib.types.str;
      default = "/var/lib/urbit/pier";
      description = "Mountpoint for the Urbit pier dataset.";
    };

    extraDatasets = lib.mkOption {
      type = lib.types.attrs;
      default = { };
      description = ''
        Extra Disko ZFS datasets merged into the server pool (home, attic
        blobs, bulk cloud storage). Same shape as entries in nix/disko.nix.
      '';
    };

    hatch = {
      enable = lib.mkEnableOption "Steam + Blender on this server, outside the Urbit VM (P6)";
      apps = lib.mkOption {
        type = lib.types.listOf lib.types.str;
        default = [
          "steam"
          "blender"
        ];
        description = "Allowed escape-hatch binaries. Launch is queued on %locus (escape.launch).";
      };
    };

    tunnel = {
      enable = lib.mkEnableOption "Cloudflare Tunnel to Eyre (M4). The tunnel is a pipe; data stays here.";
    };
  };

  config = lib.mkMerge [
    (lib.mkIf cfg.enable {
      disko.devices = import ../disko.nix {
        inherit (cfg) diskDevice poolName pierMountpoint extraDatasets;
      };

      boot.supportedFilesystems = [ "zfs" ];
      # Default true in nixpkgs; keep it so an unclean installer/poweroff can
      # still import the dedicated server pool. Host pool is not named rpool.
      boot.zfs.forceImportRoot = true;

      # Regular snapshots of datasets with com.sun:auto-snapshot=true
      # (root and urbit/pier). nix is opted out in disko.nix.
      services.zfs.autoSnapshot = {
        enable = true;
        frequent = 4;
        hourly = 24;
        daily = 7;
        weekly = 4;
        monthly = 12;
      };

      environment.systemPackages =
        [ pkgs.zfs ]
        ++ lib.optionals cfg.tunnel.enable [ pkgs.cloudflared ];

      # notes-app (and any other unprivileged process) must not be able to
      # destroy snapshots. ZFS destroy is root-only unless `zfs allow` delegates
      # it — do not delegate destroy. Do not add sudoers for zfs. The urbit
      # unit runs as user urbit without CAP_SYS_ADMIN (see urbit.nix).
    })
    # P6: Steam is unfree. Keep this block off the example server / ISO so
    # eval does not fetch Steam. Enable only on a dedicated hatch host.
    (lib.mkIf (cfg.enable && cfg.hatch.enable) {
      nixpkgs.config.allowUnfreePredicate =
        pkg:
        builtins.elem (lib.getName pkg) [
          "steam"
          "steam-unwrapped"
          "steam-original"
          "steam-run"
        ];
      environment.systemPackages = [
        pkgs.steam
        pkgs.blender
      ];
      hardware.graphics.enable = true;
    })
  ];
}
