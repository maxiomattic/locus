{
  config,
  lib,
  pkgs,
  ...
}:
let
  cfg = config.locus.server;
in
{
  # The NixOS clan server: the services (services.nix) on a Disko-formatted
  # ZFS disk. Options shared with non-NixOS hosts live in services.nix.
  imports = [ ./services.nix ];

  options.locus.server = {
    diskDevice = lib.mkOption {
      type = lib.types.str;
      example = "/dev/disk/by-id/nvme-SERVER-DISK";
      description = ''
        Dedicated server disk for Disko (prefer /dev/disk/by-id/...).
        Format applies to this device only. Do not point this at a laptop
        or any disk that holds unrelated data.
      '';
    };

    extraDatasets = lib.mkOption {
      type = lib.types.attrs;
      default = { };
      description = ''
        Extra Disko ZFS datasets merged into the server pool (home, attic
        blobs, bulk cloud storage). Same shape as entries in nix/disko.nix.
      '';
    };
  };

  config = lib.mkIf (cfg.enable && cfg.zfs.enable) {
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

    environment.systemPackages = [ pkgs.zfs ];

    # notes-app (and any other unprivileged process) must not be able to
    # destroy snapshots. ZFS destroy is root-only unless `zfs allow` delegates
    # it — do not delegate destroy. Do not add sudoers for zfs. The urbit
    # unit runs as user urbit without CAP_SYS_ADMIN (see urbit.nix).
  };
}
