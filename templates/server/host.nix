# This machine. Fill in every line marked EDIT, then follow docs/install.html.
{ ... }:
{
  locus.server = {
    enable = true;
    # EDIT: the disk to erase and install on. Everything on it is lost.
    # List candidates with: ls -l /dev/disk/by-id/
    diskDevice = "/dev/disk/by-id/EDIT";
  };

  # EDIT: a name for the machine on the network.
  networking.hostName = "clan-server";
  # EDIT: 8 hex digits, unique per machine (ZFS needs it).
  # Make one with: head -c4 /dev/urandom | od -A none -t x4 | tr -d ' '
  networking.hostId = "00000000";

  networking.useDHCP = true;
  boot.loader.systemd-boot.enable = true;
  boot.loader.efi.canTouchEfiVariables = true;

  # The administrator's login. Sign in over SSH with the key below.
  users.users.admin = {
    isNormalUser = true;
    extraGroups = [ "wheel" ];
    # EDIT: your SSH public key (the contents of ~/.ssh/id_ed25519.pub).
    openssh.authorizedKeys.keys = [ "ssh-ed25519 EDIT" ];
  };
  security.sudo.wheelNeedsPassword = false;
  services.openssh.enable = true;
  services.openssh.settings.PasswordAuthentication = false;

  system.stateVersion = "25.05";
}
