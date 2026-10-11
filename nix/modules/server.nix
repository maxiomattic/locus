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
  imports = [ ./urbit.nix ];

  options.locus.server = {
    enable = lib.mkEnableOption "clan server (Disko ZFS, pier dataset, snapshots, Urbit unit)";

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

    gateway = {
      enable = lib.mkOption {
        type = lib.types.bool;
        default = true;
        description = ''
          Host gateway next to the ship: serves the browser app, stores file and
          photo bytes (SHA-256 addressed), and proxies Eyre so the browser sees
          one origin. Byte requests are checked against the ship's login.
        '';
      };
      package = lib.mkOption {
        type = lib.types.package;
        description = "locus-gateway package (set by the flake module).";
      };
      webRoot = lib.mkOption {
        type = lib.types.nullOr lib.types.path;
        default = null;
        description = "Compiled browser app served under /app/. Null serves nothing there.";
      };
      port = lib.mkOption {
        type = lib.types.port;
        default = 8090;
        description = "Gateway listen port (all interfaces).";
      };
      openFirewall = lib.mkOption {
        type = lib.types.bool;
        default = true;
        description = "Open the gateway port in the firewall.";
      };
      maxUploadMb = lib.mkOption {
        type = lib.types.ints.positive;
        default = 2048;
        description = "Largest single upload accepted, in MiB.";
      };
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

  config = lib.mkIf cfg.enable {
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

    # Bytes live under /var/lib/locus/files on the root dataset (snapshotted).
    # A dedicated dataset can come later; adding one to an existing pool needs
    # a manual `zfs create`, so it is not declared here.
    systemd.services.locus-gateway = lib.mkIf cfg.gateway.enable {
      description = "Locus gateway (browser app, file bytes, Eyre proxy)";
      wantedBy = [ "multi-user.target" ];
      after = [
        "network-online.target"
        "urbit.service"
      ];
      wants = [ "network-online.target" ];
      serviceConfig = {
        ExecStart = lib.concatStringsSep " " (
          [
            "${cfg.gateway.package}/bin/locus-gateway"
            "--listen 0.0.0.0:${toString cfg.gateway.port}"
            "--eyre http://127.0.0.1:${toString cfg.urbit.httpPort}"
            "--data-dir /var/lib/locus/files"
            "--max-upload-mb ${toString cfg.gateway.maxUploadMb}"
          ]
          ++ lib.optional (cfg.gateway.webRoot != null) "--web-root ${cfg.gateway.webRoot}"
        );
        DynamicUser = true;
        StateDirectory = "locus/files";
        StateDirectoryMode = "0700";
        Restart = "on-failure";
        RestartSec = "5s";
        NoNewPrivileges = true;
        ProtectSystem = "strict";
        ProtectHome = true;
        PrivateTmp = true;
        PrivateDevices = true;
        ProtectKernelTunables = true;
        ProtectKernelModules = true;
        ProtectControlGroups = true;
        RestrictSUIDSGID = true;
        LockPersonality = true;
        RestrictAddressFamilies = "AF_UNIX AF_INET AF_INET6";
        SystemCallArchitectures = "native";
      };
    };
    networking.firewall.allowedTCPPorts = lib.mkIf (
      cfg.gateway.enable && cfg.gateway.openFirewall
    ) [ cfg.gateway.port ];

    environment.systemPackages = [ pkgs.zfs ];

    # notes-app (and any other unprivileged process) must not be able to
    # destroy snapshots. ZFS destroy is root-only unless `zfs allow` delegates
    # it — do not delegate destroy. Do not add sudoers for zfs. The urbit
    # unit runs as user urbit without CAP_SYS_ADMIN (see urbit.nix).
  };
}
