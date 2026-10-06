# P5: GrapheneOS talks to the same Eyre APIs. Not a COSMIC session.
# P7 (AOSP fork) is not this module.
{
  config,
  lib,
  ...
}:
let
  cfg = config.digitalWorkspace.mobile;
in
{
  options.digitalWorkspace.mobile = {
    enable = lib.mkEnableOption "Locus touch client settings (stock GrapheneOS, same ship)";

    eyreUrl = lib.mkOption {
      type = lib.types.str;
      default = "https://locus.example";
      description = "Eyre base URL the phone uses (LAN or M4 tunnel).";
    };
  };

  config = lib.mkIf cfg.enable {
    assertions = [
      {
        assertion = !(config.services.desktopManager.cosmic.enable or false);
        message = "P5 mobile is not a nested COSMIC session.";
      }
    ];
    environment.etc."locus/touch-apis.json".text = builtins.toJSON {
      eyre = cfg.eyreUrl;
      openapi = [
        "/notes/openapi.json"
        "/mail/openapi.json"
        "/calendar/openapi.json"
        "/photos/openapi.json"
        "/contacts/openapi.json"
        "/locus/openapi.json"
      ];
    };
  };
}
