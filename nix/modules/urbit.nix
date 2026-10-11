{
  config,
  lib,
  pkgs,
  ...
}:
let
  cfg = config.locus.server;
  urbitCfg = cfg.urbit;
  pierDataset = "${cfg.poolName}/urbit/pier";
  verePier = "${cfg.pierMountpoint}/${urbitCfg.verePierName}";

  # Vere -c fails if the target path already exists. The Disko mountpoint
  # /var/lib/urbit/pier is therefore the dataset, and the ship directory is
  # a child of that mount (still on rpool/urbit/pier).
  urbitStart = pkgs.writeShellApplication {
    name = "urbit-start";
    runtimeInputs = [
      urbitCfg.package
      pkgs.util-linux
      pkgs.coreutils
    ];
    text = ''
      mount=${lib.escapeShellArg cfg.pierMountpoint}
      dataset=${lib.escapeShellArg pierDataset}
      pier=${lib.escapeShellArg verePier}

      fstype=$(findmnt -n -o FSTYPE --target "$mount")
      if [ "$fstype" != "zfs" ]; then
        echo "urbit-start: $mount fstype is ''${fstype:-missing}; pier must live on ZFS ($dataset)" >&2
        exit 1
      fi

      source=$(findmnt -n -o SOURCE --target "$mount")
      if [ "$source" != "$dataset" ]; then
        echo "urbit-start: $mount is $source, want $dataset" >&2
        exit 1
      fi

      extra=(
        -t
        --no-dock
        --no-demand
        --http-port ${toString urbitCfg.httpPort}
        -b ${lib.escapeShellArg urbitCfg.httpIp}
        --loom ${toString urbitCfg.loom}
      )
      ${lib.optionalString (urbitCfg.amesPort != null) ''
        extra+=(--ames-port ${toString urbitCfg.amesPort})
      ''}

      if [ -d "$pier/.urb" ]; then
        exec urbit "''${extra[@]}" "$pier"
      fi

      ${
        if urbitCfg.ship == "comet" then
          ''
            exec urbit "''${extra[@]}" -c "$pier"
          ''
        else
          ''
            exec urbit "''${extra[@]}" -w ${lib.escapeShellArg (
              lib.removePrefix "~" urbitCfg.name
            )} -k ${lib.escapeShellArg urbitCfg.keyFile} -c "$pier"
          ''
      }
    '';
  };

  khanEval = "[0 %fyrd %base %khan-eval %noun %ted-eval '=/  m  (strand ,vase)  ;<  our=@p  bind:m  get-our  ;<  code=@p  bind:m  (scry @p /j/code/(scot %p our))  (pure:m !>((crip (slag 1 (scow %p code)))))']";

  # Live +code via conn.sock (do not scrape /~/login).
  urbitCode = pkgs.writeShellApplication {
    name = "urbit-code";
    runtimeInputs = [
      urbitCfg.package
      pkgs.netcat-openbsd
      pkgs.coreutils
      pkgs.gnugrep
    ];
    text = ''
      pier=${lib.escapeShellArg verePier}
      sock="$pier/.urb/conn.sock"
      if [ ! -S "$sock" ]; then
        echo "urbit-code: ship is not running (missing $sock)" >&2
        exit 1
      fi
      raw=$(
        printf '%s\n' ${lib.escapeShellArg khanEval} \
          | urbit eval -jn \
          | nc -U -W 5 "$sock" \
          | urbit eval -cn
      )
      code=$(printf '%s\n' "$raw" | grep -Eo '[a-z]{3,8}(-[a-z]{3,8}){3}' | head -n 1)
      if [ -z "$code" ]; then
        echo "urbit-code: could not parse +code from conn.sock" >&2
        exit 1
      fi
      printf '%s\n' "$code"
    '';
  };
in
{
  options.locus.server.urbit = {
    enable = lib.mkOption {
      type = lib.types.bool;
      default = true;
      description = ''
        systemd unit that boots the clan ship on the pier dataset.
        Ignored unless locus.server.enable.
      '';
    };

    package = lib.mkPackageOption pkgs "urbit" { };

    ship = lib.mkOption {
      type = lib.types.enum [
        "comet"
        "moon"
        "planet"
      ];
      default = "comet";
      description = ''
        Ship class. A comet needs no keys. A moon or planet needs name and
        keyFile (the unit boots it with -w, -k).
      '';
    };

    name = lib.mkOption {
      type = lib.types.nullOr lib.types.str;
      default = null;
      example = "sampel-palnet";
      description = "@p without the leading ~. Required when ship is moon or planet.";
    };

    keyFile = lib.mkOption {
      # Deliberately a string, not a path: a Nix path literal is copied into
      # the world-readable Nix store and would leak the private key.
      type = lib.types.nullOr lib.types.str;
      default = null;
      example = "/var/lib/urbit/keys/sampel-palnet.key";
      description = ''
        Absolute path to the ship's private key file, passed to Vere with -k.
        Required when ship is moon or planet. Install the file on the server
        out of band (owned by the urbit user, mode 0400), not from the Nix
        store. Delete it after the first successful boot.
      '';
    };

    verePierName = lib.mkOption {
      type = lib.types.str;
      default = "ship";
      description = ''
        Directory name for the Vere pier under pierMountpoint. Vere -c cannot
        target the ZFS mount itself because that path already exists.
      '';
    };

    httpIp = lib.mkOption {
      type = lib.types.str;
      default = "0.0.0.0";
      description = "Eyre bind address (-b / --http-ip). 0.0.0.0 is LAN-reachable.";
    };

    httpPort = lib.mkOption {
      type = lib.types.port;
      default = 80;
      description = "Eyre HTTP port (--http-port).";
    };

    amesPort = lib.mkOption {
      type = lib.types.nullOr lib.types.port;
      default = null;
      description = "Optional Ames UDP port (-p). Unset lets Vere choose.";
    };

    loom = lib.mkOption {
      type = lib.types.ints.between 20 33;
      default = 31;
      description = "Vere --loom (exponent of two; 31 is 2GiB).";
    };
  };

  config = lib.mkIf (cfg.enable && urbitCfg.enable) {
    assertions = [
      {
        assertion = builtins.match "[^/]+" urbitCfg.verePierName != null;
        message = "locus.server.urbit.verePierName must be a single directory name (no slashes).";
      }
      {
        assertion =
          urbitCfg.ship == "comet"
          || (
            urbitCfg.name != null
            && urbitCfg.name != ""
            && urbitCfg.keyFile != null
          );
        message = "locus.server.urbit.ship = \"moon\" or \"planet\" requires name and keyFile.";
      }
      {
        assertion =
          urbitCfg.keyFile == null
          || (
            lib.hasPrefix "/" urbitCfg.keyFile
            && !lib.hasPrefix "/nix/store" urbitCfg.keyFile
          );
        message = "locus.server.urbit.keyFile must be an absolute path and must not be in /nix/store.";
      }
    ];

    users.users.urbit = {
      isSystemUser = true;
      group = "urbit";
      home = cfg.pierMountpoint;
      homeMode = "0750";
      createHome = false;
      description = "Locus Urbit ship";
    };
    users.groups.urbit = { };

    # notes-app (and urbit) must not be able to destroy snapshots.
    # ZFS destroy is root-only unless `zfs allow` delegates it — do not
    # delegate destroy, mount, or canmount. Do not add sudoers for zfs.
    # This unit runs as urbit, not root, without CAP_SYS_ADMIN, and with
    # /dev/zfs hidden (PrivateDevices).

    environment.systemPackages = [
      urbitCfg.package
      urbitCode
    ];

    networking.firewall.allowedTCPPorts = [ urbitCfg.httpPort ];

    systemd.services.urbit = {
      description = "Locus Urbit ship";
      wantedBy = [ "multi-user.target" ];
      after = [
        "network-online.target"
        "local-fs.target"
        "zfs-import.target"
      ];
      wants = [ "network-online.target" ];
      unitConfig.RequiresMountsFor = [ cfg.pierMountpoint ];
      serviceConfig = {
        Environment = "SSL_CERT_FILE=${pkgs.cacert}/etc/ssl/certs/ca-bundle.crt";
        User = "urbit";
        Group = "urbit";
        UMask = "0077";
        ExecStartPre = [
          "+${pkgs.coreutils}/bin/chown urbit:urbit ${cfg.pierMountpoint}"
          "+${pkgs.coreutils}/bin/chmod 0750 ${cfg.pierMountpoint}"
        ];
        ExecStart = "${urbitStart}/bin/urbit-start";
        Restart = "on-failure";
        RestartSec = "10s";
        TimeoutStopSec = "120s";
        LimitNOFILE = "65536";
        NoNewPrivileges = true;
        ProtectSystem = "strict";
        ProtectHome = true;
        PrivateTmp = true;
        # Hide /dev/zfs so the urbit user cannot zfs destroy. Do not use
        # PrivateDevices=true — Vere Clay unix-sync needs a normal /dev
        # (and |commit from a mounted desk fails without it).
        InaccessiblePaths = [ "/dev/zfs" ];
        PrivateDevices = false;
        ProtectKernelTunables = true;
        ProtectKernelModules = true;
        ProtectControlGroups = true;
        RestrictSUIDSGID = true;
        RestrictRealtime = true;
        LockPersonality = true;
        SystemCallArchitectures = "native";
        RestrictAddressFamilies = "AF_UNIX AF_INET AF_INET6 AF_NETLINK";
        ReadWritePaths = [ cfg.pierMountpoint ];
        # "-" makes the path optional: the key may be deleted after first boot
        # and the unit must still start.
        ReadOnlyPaths = lib.mkIf (urbitCfg.keyFile != null) [ "-${urbitCfg.keyFile}" ];
        CapabilityBoundingSet =
          if urbitCfg.httpPort < 1024 then "CAP_NET_BIND_SERVICE" else "";
        AmbientCapabilities = lib.mkIf (urbitCfg.httpPort < 1024) "CAP_NET_BIND_SERVICE";
      };
    };
  };
}
