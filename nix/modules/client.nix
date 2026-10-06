# Locus desktop client (P3). COSMIC with cosmic-comp — not a separate TWM.
# Do not nixos-rebuild switch this on the laptop unless that machine should
# leave GNOME. Browser is a Nix-declared client app, not an Urbit agent.
{
  config,
  lib,
  pkgs,
  ...
}:
let
  cfg = config.digitalWorkspace.client;
  defaults = pkgs.callPackage ../cosmic-defaults.nix { };
in
{
  options.digitalWorkspace.client = {
    enable = lib.mkEnableOption "Locus COSMIC desktop client (cosmic-comp, tiling, declared browser)";

    browser = lib.mkOption {
      type = lib.types.package;
      default = pkgs.firefox;
      defaultText = lib.literalExpression "pkgs.firefox";
      description = ''
        Household browser package. Ordinary client software: declare it in Nix,
        permission it through the command plane later. Do not port it into Urbit.
      '';
    };
  };

  config = lib.mkIf cfg.enable {
    assertions = [
      {
        assertion = !config.programs.sway.enable;
        message = "Locus client keeps cosmic-comp; do not enable Sway.";
      }
      {
        assertion = !config.programs.hyprland.enable;
        message = "Locus client keeps cosmic-comp; do not enable Hyprland.";
      }
      {
        assertion = !config.programs.niri.enable;
        message = "Locus client keeps cosmic-comp; do not enable niri.";
      }
    ];

    nixpkgs.overlays = [ (import ../overlays/cosmic.nix) ];

    services.desktopManager.cosmic.enable = true;
    services.displayManager.cosmic-greeter.enable = true;

    programs.firefox.enable = true;

    environment.systemPackages = [
      cfg.browser
      defaults
    ];

    hardware.graphics.enable = lib.mkDefault true;
  };
}
