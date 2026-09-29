{ config, lib, pkgs, ... }:
let
  cfg = config.services.glimpse;
in {
  options.services.glimpse = {
    enable = lib.mkEnableOption "Glimpse desktop shell";

    package = lib.mkOption {
      type = lib.types.package;
      default = pkgs.callPackage ./package.nix { };
      description = "Glimpse package used by the desktop session.";
    };
  };

  config = lib.mkIf cfg.enable {
    environment.systemPackages = [ cfg.package ];
    systemd.packages = [ cfg.package ];
    services.dbus.packages = [ cfg.package ];
    security.pam.services.glimpse-lock = { };
    services.geoclue2.enable = lib.mkDefault true;
    services.geoclue2.appConfig.glimpse = {
      isAllowed = true;
      isSystem = true;
    };
    xdg.portal.extraPortals = lib.optional config.xdg.portal.enable cfg.package;
    xdg.portal.config = lib.genAttrs [ "glimpse" "niri" ] (_: {
      "org.freedesktop.impl.portal.Inhibit" = lib.mkDefault "glimpse";
      "org.freedesktop.impl.portal.Screenshot" = lib.mkDefault "glimpse-shot";
    }) // {
      Hyprland."org.freedesktop.impl.portal.Inhibit" = lib.mkDefault "glimpse";
    };
  };
}
