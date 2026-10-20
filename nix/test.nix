{ pkgs, module, package }:
let
  lib = pkgs.lib;

  common = {
    imports = [ module ];
    services.glimpse.enable = true;
    services.glimpse.package = package;
    hardware.graphics.enable = true;
    xdg.portal.enable = true;
    xdg.portal.config.common.default = "gtk";
    xdg.portal.extraPortals = [ pkgs.xdg-desktop-portal-gtk ];
    environment.systemPackages = [ pkgs.pamtester pkgs.wtype ];
    users.users.glimpse = {
      isNormalUser = true;
      uid = 1000;
      initialPassword = "test-password";
      extraGroups = [ "video" "render" ];
    };
    virtualisation.memorySize = 4096;
    virtualisation.qemu.options = [ "-vga none" "-device virtio-gpu-pci" ];
    specialisation.without.configuration.services.glimpse.enable = lib.mkForce false;
  };

  checks = ''
    bus = "XDG_RUNTIME_DIR=/run/user/1000 DBUS_SESSION_BUS_ADDRESS=unix:path=/run/user/1000/bus"

    def system_checks():
        machine.succeed("test -e /etc/pam.d/glimpse-lock")
        machine.succeed("test -e /etc/systemd/user/glimpse-session.target")
        machine.succeed("test -e /run/current-system/sw/share/dbus-1/services/me.aresa.Glimpse.Idle.service")
        machine.succeed("test -e /run/current-system/sw/share/dbus-1/services/me.aresa.Glimpse.Shot.service")
        machine.succeed("test -e /run/current-system/sw/share/dbus-1/services/me.aresa.Glimpse.Settings.service")
        machine.succeed("grep -q 'Screenshot=glimpse-shot' /etc/xdg/xdg-desktop-portal/niri-portals.conf")
        machine.succeed("grep -q 'Inhibit=glimpse' /etc/xdg/xdg-desktop-portal/glimpse-portals.conf")
        machine.succeed("grep -q 'Wallpaper=glimpse-wallpaper' /etc/xdg/xdg-desktop-portal/hyprland-portals.conf")
        machine.fail("grep -q '^default=' /etc/xdg/xdg-desktop-portal/hyprland-portals.conf")
        machine.succeed("test -x /run/wrappers/bin/unix_chkpwd")
        machine.succeed("grep -q pam_unix.so /etc/pam.d/glimpse-lock")
        machine.fail("grep -E '^auth .*pam_permit.so' /etc/pam.d/glimpse-lock")
        machine.succeed("printf 'test-password\\n' | pamtester glimpse-lock glimpse authenticate")
        machine.fail("printf 'wrong-password\\n' | pamtester glimpse-lock glimpse authenticate")

    def seed_user_config():
        machine.succeed("install -d -o glimpse -g users /home/glimpse/.config/glimpse")
        machine.succeed("printf '[appearance]\\ncolor-scheme = \"dark\"\\n' > /home/glimpse/.config/glimpse/config.toml")
        machine.succeed("chown glimpse:users /home/glimpse/.config/glimpse/config.toml")
        machine.succeed("cp /home/glimpse/.config/glimpse/config.toml /tmp/config-before")

    def session_checks(user):
        for service in ["glimpse-panel", "glimpse-wallpaper", "glimpse-idle", "glimpse-notifications", "glimpse-lock", "glimpse-settings-daemon"]:
            machine.wait_until_succeeds(f"{user} systemctl --user is-active {service}.service", timeout=60)
        machine.succeed(f"{user} glimpse-lock check")
        shown = machine.succeed(f"{user} glimpsectl config show")
        assert "${package}/share/glimpse/themes/adwaita/default.jpg" in shown, shown
        assert 'color-scheme = "dark"' in shown, shown
        machine.succeed(f"{user} systemctl --user stop glimpse-lock.service")
        machine.succeed(f"{user} glimpse-lock --standalone >/tmp/lock.log 2>&1 &")
        machine.wait_until_succeeds("grep -q 'locking generation' /tmp/lock.log", timeout=30)
        try:
            machine.succeed(f"{user} wtype wrong-password -k Return")
            machine.succeed("sleep 2")
            machine.succeed("pgrep -u glimpse glimpse-lock")
            machine.fail("grep -q 'authenticated; unlocking' /tmp/lock.log")
            machine.succeed(f"{user} wtype test-password -k Return")
            machine.wait_until_succeeds("grep -q 'authenticated; unlocking' /tmp/lock.log", timeout=30)
            machine.wait_until_fails("pgrep -u glimpse glimpse-lock", timeout=30)
        except Exception:
            print(machine.execute("cat /tmp/lock.log"))
            raise
        machine.succeed(f"{user} systemctl --user start glimpse-lock.service")
        machine.succeed(f"{user} systemctl --user stop glimpse-session.target")
        machine.wait_until_fails(f"{user} systemctl --user is-active glimpse-panel.service")
        machine.succeed(f"{user} systemctl --user is-active glimpse-lock.service")
        machine.succeed(f"{user} systemctl --user start glimpse-session.target")
        machine.wait_until_succeeds(f"{user} systemctl --user is-active glimpse-panel.service", timeout=60)

    def rollback_checks(user):
        machine.succeed("/run/current-system/specialisation/without/bin/switch-to-configuration test")
        machine.fail("test -e /etc/systemd/user/glimpse-session.target")
        machine.fail("test -e /etc/pam.d/glimpse-lock")
        machine.succeed("/run/booted-system/bin/switch-to-configuration test")
        machine.succeed("test -e /etc/systemd/user/glimpse-session.target")
        machine.succeed("grep -q 'Exec=${package}/bin/glimpse-idle' /run/current-system/sw/share/dbus-1/services/me.aresa.Glimpse.Idle.service")
        machine.succeed(f"{user} systemctl --user daemon-reload")
        machine.succeed(f"{user} systemctl --user restart glimpse-session.target")
        machine.wait_until_succeeds(f"{user} systemctl --user is-active glimpse-panel.service", timeout=60)
        machine.succeed("cmp /tmp/config-before /home/glimpse/.config/glimpse/config.toml")
  '';
in {
  niri = pkgs.testers.runNixOSTest {
    name = "glimpse-niri";
    nodes.machine = {
      imports = [ common ];
      environment.systemPackages = [ pkgs.weston pkgs.niri ];
    };
    testScript = ''
      ${checks}
      machine.start()
      machine.wait_for_unit("multi-user.target")
      system_checks()
      seed_user_config()
      machine.succeed("loginctl enable-linger glimpse")
      machine.wait_until_succeeds("test -S /run/user/1000/bus")
      machine.succeed("runuser -u glimpse -- env XDG_RUNTIME_DIR=/run/user/1000 LIBGL_ALWAYS_SOFTWARE=1 weston --backend=headless --renderer=gl --width=1024 --height=768 --socket=wayland-parent --no-config >/tmp/weston.log 2>&1 &")
      machine.wait_until_succeeds("test -S /run/user/1000/wayland-parent")
      machine.succeed("runuser -u glimpse -- env XDG_RUNTIME_DIR=/run/user/1000 WAYLAND_DISPLAY=wayland-parent LIBGL_ALWAYS_SOFTWARE=1 niri >/tmp/niri.log 2>&1 &")
      machine.wait_until_succeeds("test -S /run/user/1000/wayland-1", timeout=45)
      user = f"runuser -u glimpse -- env {bus} WAYLAND_DISPLAY=wayland-1 XDG_CURRENT_DESKTOP=niri LIBGL_ALWAYS_SOFTWARE=1"
      machine.succeed(f"{user} systemctl --user import-environment WAYLAND_DISPLAY XDG_CURRENT_DESKTOP LIBGL_ALWAYS_SOFTWARE")
      machine.succeed(f"{user} systemctl --user start glimpse-session.target")
      session_checks(user)
      rollback_checks(user)
    '';
  };

  hyprland = pkgs.testers.runNixOSTest {
    name = "glimpse-hyprland";
    nodes.machine = {
      imports = [ common ];
      programs.hyprland.enable = true;
      services.getty.autologinUser = "glimpse";
      environment.sessionVariables.LIBGL_ALWAYS_SOFTWARE = "1";
      environment.etc."glimpse-test/hyprland.conf".text = ''
        monitor = ,preferred,auto,1
        exec-once = systemctl --user import-environment WAYLAND_DISPLAY XDG_CURRENT_DESKTOP HYPRLAND_INSTANCE_SIGNATURE LIBGL_ALWAYS_SOFTWARE && systemctl --user start glimpse-session.target
      '';
      programs.bash.loginShellInit = ''
        if [ "$(tty)" = /dev/tty1 ]; then
          Hyprland --config /etc/glimpse-test/hyprland.conf >/tmp/hyprland.log 2>&1
        fi
      '';
    };
    testScript = ''
      ${checks}
      machine.start()
      machine.wait_for_unit("multi-user.target")
      system_checks()
      seed_user_config()
      try:
          machine.wait_until_succeeds("ls /run/user/1000/wayland-* | grep -v lock", timeout=90)
      except Exception:
          print(machine.execute("cat /tmp/hyprland.log; cat /run/user/1000/hypr/*/hyprland.log; ls -l /dev/dri"))
          raise
      display = machine.succeed("basename $(ls /run/user/1000/wayland-* | grep -v lock | head -1)").strip()
      user = f"runuser -u glimpse -- env {bus} WAYLAND_DISPLAY={display} XDG_CURRENT_DESKTOP=Hyprland"
      session_checks(user)
      rollback_checks(user)
    '';
  };
}
