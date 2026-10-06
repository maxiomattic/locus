# Bring-up VM only. Dedicated virtio disk; Disko formats /dev/vda as rpool.
# Do not nixos-rebuild switch this on the laptop. Do not reuse another VM's disk.
{
  config,
  lib,
  pkgs,
  modulesPath,
  ...
}:
{
  imports = [ "${modulesPath}/profiles/qemu-guest.nix" ];

  digitalWorkspace.server = {
    enable = true;
    diskDevice = "/dev/vda";
    urbit.loom = 31; # 2GiB; first comet boot of the 4.6 pill OOMs (`bail: meme`) at loom 30.
  };

  nixpkgs.hostPlatform = "x86_64-linux";
  system.stateVersion = "25.05";
  networking.hostName = "locus-server";
  networking.hostId = "d05e0002";
  networking.useDHCP = true;
  networking.firewall.allowedTCPPorts = [ 22 ];

  boot.loader.systemd-boot.enable = true;
  # Write BootOrder into OVMF_VARS during nixos-install so disk boot
  # does not drop to the UEFI shell.
  boot.loader.efi.canTouchEfiVariables = true;
  boot.loader.systemd-boot.graceful = true;
  boot.zfs.forceImportRoot = lib.mkForce true;
  boot.loader.timeout = 1;
  boot.initrd.availableKernelModules = [
    "virtio_pci"
    "virtio_scsi"
    "virtio_blk"
    "virtio_net"
    "ahci"
    "xhci_pci"
    "sr_mod"
    "sd_mod"
  ];
  boot.initrd.kernelModules = [
    "virtio_pci"
    "virtio_blk"
    "virtio_scsi"
  ];
  # qemu virtio-blk often has no /dev/disk/by-id/*; initrd import then fails.
  boot.zfs.devNodes = "/dev";
  boot.kernelParams = [
    "console=ttyS0,115200n8"
    "console=tty0"
  ];

  services.openssh.enable = true;
  services.openssh.settings.PasswordAuthentication = false;
  services.openssh.openFirewall = true;

  # Laptop operator key (public). notes-occ.sh needs passwordless sudo on the server.
  users.users.root.openssh.authorizedKeys.keys = [
    "ssh-ed25519 AAAAC3NzaC1lZDI1NTE5AAAAINAbdx9MTfeHCXU4cRJft6pwtSubhZEzSWw2RPqU9gFA here-nor-there"
    "ssh-ed25519 AAAAC3NzaC1lZDI1NTE5AAAAIG1byTJMrnwjNLnr5fK0FCbVVaEJpK1ke5eZlzeIzLh7 locus-server-vm"
  ];
  users.users.locus = {
    isNormalUser = true;
    extraGroups = [ "wheel" ];
    openssh.authorizedKeys.keys = [
      "ssh-ed25519 AAAAC3NzaC1lZDI1NTE5AAAAINAbdx9MTfeHCXU4cRJft6pwtSubhZEzSWw2RPqU9gFA here-nor-there"
      "ssh-ed25519 AAAAC3NzaC1lZDI1NTE5AAAAIG1byTJMrnwjNLnr5fK0FCbVVaEJpK1ke5eZlzeIzLh7 locus-server-vm"
    ];
  };
  security.sudo.wheelNeedsPassword = false;

  environment.systemPackages = [
    pkgs.curl
    pkgs.python3
    pkgs.rsync
    pkgs.netcat-openbsd
    pkgs.git
  ];

  systemd.services."serial-getty@ttyS0".enable = true;

  nix.settings.experimental-features = [
    "nix-command"
    "flakes"
  ];
}
