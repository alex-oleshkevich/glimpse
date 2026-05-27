---
layout: home

hero:
  name: "Glimpse"
  text: "A cohesive desktop layer for Niri."
  tagline: "Panel, wallpaper, lock screen, idle policy, night light, and custom applets built to feel like one Wayland desktop."
  actions:
    - theme: brand
      text: "Install"
      link: /installation
    - theme: alt
      text: "Configure"
      link: /configuration

features:
  - title: "One Desktop Surface"
    details: "Panel, lock screen, wallpaper, notifications, and session controls share the same configuration and visual language."
  - title: "Plain Files, Real Control"
    details: "Configure layouts, applets, services, themes, calendars, and idle behavior with readable TOML and CSS."
  - title: "Extensible By Design"
    details: "Add launchers, menus, live widgets, and script-driven popovers without rebuilding the shell."
---

## Built Around The Session

Glimpse covers the parts of a desktop session that usually sit around the compositor: panel status, notifications, wallpaper, locking, idle behavior, night light, and session actions.

| Surface | Role |
|---|---|
| **Panel** | Workspaces, applets, tray, media, network, battery, weather, notifications, and session controls. |
| **Lock** | PAM-backed session locking with wallpaper integration, status buttons, and themeable CSS. |
| **Wallpaper** | Solid colors, image backgrounds, fit modes, transitions, and blurred backdrop support. |
| **Idle and sunset** | Automatic locking, monitor power commands, suspend rules, and night-light scheduling. |
| **Custom applets** | Command launchers and long-running exec widgets with typed SDKs and popover components. |

## Designed For Daily Use

Glimpse keeps the desktop understandable. System services run as regular user units, configuration stays in plain files, and custom applets can be developed outside the main codebase.

The result is a desktop that remains hackable without feeling assembled from unrelated pieces.

## Choose Your Path

| Goal | Page |
|---|---|
| Install the packaged desktop services | [Installation](./installation.md) |
| Define panels and applets | [Configuration](./configuration.md) |
| Tune built-in applets | [Applets](./applets/) |
| Build custom panel widgets | [Custom Applets](./custom-applets/) |
| Theme the shell and lock screen | [Theming](./theming.md) |
| Configure wallpaper, lock, idle, or night light | [Wallpaper](./wallpaper.md), [Lock](./lock.md), [Idle](./idle.md), [Sunset](./sunset.md) |
