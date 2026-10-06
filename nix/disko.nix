# Disko layout for the household **server** disk only.
#
# Datasets: rpool/root, rpool/nix, rpool/urbit, rpool/urbit/pier,
# rpool/locus/photos (P4 bytes), rpool/locus/hatch (P6 escape-hatch data).
# This file does not mention any other disk, so Disko will not format them.
# `diskDevice` must be the dedicated server disk (by-id). Format wipes that device.

{
  diskDevice,
  poolName ? "rpool",
  pierMountpoint ? "/var/lib/urbit/pier",
  extraDatasets ? { },
}:
{
  disk.server = {
    type = "disk";
    device = diskDevice;
    content = {
      type = "gpt";
      partitions = {
        ESP = {
          size = "512M";
          type = "EF00";
          content = {
            type = "filesystem";
            format = "vfat";
            mountpoint = "/boot";
            mountOptions = [ "umask=0077" ];
          };
        };
        zfs = {
          size = "100%";
          content = {
            type = "zfs";
            pool = poolName;
          };
        };
      };
    };
  };

  zpool.${poolName} = {
    type = "zpool";
    options.ashift = "12";
    rootFsOptions = {
      canmount = "off";
      mountpoint = "none";
      compression = "zstd";
      acltype = "posixacl";
      xattr = "sa";
      atime = "off";
      normalization = "formD";
    };
    datasets = {
      root = {
        type = "zfs_fs";
        mountpoint = "/";
        options = {
          mountpoint = "legacy";
          "com.sun:auto-snapshot" = "true";
        };
      };
      nix = {
        type = "zfs_fs";
        mountpoint = "/nix";
        options = {
          mountpoint = "legacy";
          "com.sun:auto-snapshot" = "false";
        };
      };
      urbit = {
        type = "zfs_fs";
        options = {
          canmount = "off";
          mountpoint = "none";
          "com.sun:auto-snapshot" = "false";
        };
      };
      "urbit/pier" = {
        type = "zfs_fs";
        mountpoint = pierMountpoint;
        options = {
          mountpoint = "legacy";
          "com.sun:auto-snapshot" = "true";
        };
      };
      locus = {
        type = "zfs_fs";
        options = {
          canmount = "off";
          mountpoint = "none";
          "com.sun:auto-snapshot" = "false";
        };
      };
      "locus/photos" = {
        type = "zfs_fs";
        mountpoint = "/var/lib/locus/photos";
        options = {
          mountpoint = "legacy";
          "com.sun:auto-snapshot" = "true";
        };
      };
      "locus/hatch" = {
        type = "zfs_fs";
        mountpoint = "/var/lib/locus/hatch";
        options = {
          mountpoint = "legacy";
          "com.sun:auto-snapshot" = "true";
        };
      };
    }
    // extraDatasets;
  };
}
