# NixOS installer ISO that Disko-formats the VM's /dev/vda and installs #server-vm.
# Runs only inside qemu/kvm. Never point this at a host disk.
{
  lib,
  pkgs,
  modulesPath,
  self,
  ...
}:
let
  server = self.nixosConfigurations.server-vm;
in
{
  imports = [ "${modulesPath}/installer/cd-dvd/installation-cd-minimal.nix" ];

  image.fileName = "locus-server-installer.iso";
  isoImage.squashfsCompression = "zstd -Xcompression-level 6";

  networking.hostId = "d05e0002";
  networking.hostName = "locus-installer";
  boot.supportedFilesystems = [ "zfs" ];
  boot.zfs.forceImportRoot = false;
  users.users.nixos.openssh.authorizedKeys.keys = [
    "ssh-ed25519 AAAAC3NzaC1lZDI1NTE5AAAAIG1byTJMrnwjNLnr5fK0FCbVVaEJpK1ke5eZlzeIzLh7 locus-server-vm"
  ];
  users.users.root.openssh.authorizedKeys.keys = [
    "ssh-ed25519 AAAAC3NzaC1lZDI1NTE5AAAAIG1byTJMrnwjNLnr5fK0FCbVVaEJpK1ke5eZlzeIzLh7 locus-server-vm"
  ];
  boot.kernelParams = [
    "console=ttyS0,115200n8"
    "console=tty0"
  ];

  systemd.services.locus-install = {
    description = "Install Locus server onto VM /dev/vda";
    wantedBy = [ "multi-user.target" ];
    after = [ "local-fs.target" ];
    path = with pkgs; [
      coreutils
      util-linux
      systemd
      zfs
      nix
      git
      gnugrep
      gawk
      nixos-install-tools
    ];
    serviceConfig = {
      Type = "oneshot";
      TimeoutStartSec = "2h";
      StandardOutput = "journal+console";
      StandardError = "journal+console";
    };
    script = ''
      set -euxo pipefail
      virt=$(systemd-detect-virt || true)
      case "$virt" in
        kvm|qemu) ;;
        *)
          echo "locus-install: refusing to run outside qemu/kvm (virt=''${virt})"
          exit 1
          ;;
      esac
      if [ ! -b /dev/vda ]; then
        echo "locus-install: /dev/vda missing"
        exit 1
      fi
      if blkid /dev/vda /dev/vda1 /dev/vda2 >/dev/null 2>&1; then
        echo "locus-install: /dev/vda already partitioned; importing and installing without reformat"
        zpool import -f -N rpool || zpool import -f rpool || true
        mkdir -p /mnt
        mount -t zfs rpool/root /mnt || true
        mkdir -p /mnt/nix /mnt/boot /mnt/var/lib/urbit/pier
        mount -t zfs rpool/nix /mnt/nix || true
        mount -t zfs rpool/urbit/pier /mnt/var/lib/urbit/pier || true
        mount /dev/vda1 /mnt/boot || mount /dev/disk/by-partlabel/ESP /mnt/boot || true
      else
        ${server.config.system.build.diskoScript}
      fi
      nixos-install \
        --no-root-passwd \
        --no-channel-copy \
        --system ${server.config.system.build.toplevel} \
        --root /mnt
      # If OVMF drops to the UEFI shell, run the ESP fallback loader.
      printf '%s\n' '\\EFI\\BOOT\\BOOTX64.EFI' > /mnt/boot/startup.nsh
      echo LOCUS_INSTALL_COMPLETE
      systemctl poweroff --force
    '';
  };
}
