# Changelog

All notable changes to Glimpse are documented in this file.

The format is based on [Keep a Changelog](https://keepachangelog.com/en/1.1.0/),
and this project adheres to [Semantic Versioning](https://semver.org/spec/v2.0.0.html).

Releases before 0.12.0 are documented in the
[GitHub Releases](https://github.com/alex-oleshkevich/glimpse/releases) and
in the git tag history.

## [Unreleased]

## [0.12.0] - 2026-05-23

This release is a large refactor of the panel's widget system. Built-in
applets now share a single `PanelIndicator` button and a library of
reusable popover widgets (`Hero`, `Tile`, `Message`, `BatteryHero`,
`DateHero`, `CircleBox`, `PopoverShell`, `KeyValueGrid`, `Badge`,
`StatusDot`, `Row`, `ButtonRow`, `Container`, `Text`). Popovers across
audio, battery, idle, notifications, removable, and clock were rebuilt
on top of these primitives.

### Added

- `next_event` applet showing the upcoming calendar event.
- `PanelIndicator` widget used by every built-in applet button.
- `Hero`, `BatteryHero`, `DateHero`, `Tile`, `SegmentedTile`,
  `PopoverShell`, `Message`, `KeyValueGrid`, `CircleBox`, `Row`,
  `ButtonRow`, `Container`, `Text`, `StatusDot`, `Badge`, `Separator`
  widgets.
- MPRIS seek support and a redesigned now-playing popover.
- Notification grouping with urgency remap and a badge style enum.
- Manual location override for location-aware services.
- Typography CSS variables aligned with GNOME tokens.

### Changed

- Renamed the `brightness` applet to `display`.
- Migrated layout helpers from `components/` into `utils/` and `widgets/`.
- Ported audio, idle, battery, notifications, and removable popovers
  onto the shared widget system.
- Refactored the clock popover's calendar into custom GTK widgets.
- Revamped the exec applet widget protocol; SDK widget catalogue
  (Rust, Python, TypeScript, Go) updated to match.
- Notification surfaces now own their size/shadow; refreshed
  empty-state icon.
- CSS tokens aligned with GNOME's typography and color scheme.

### Fixed

- Microphone privacy indicator no longer stays on after recording
  stops.
- Webcam privacy detection now seeds node properties from the
  PipeWire registry on startup.
- Lock screen refreshes user info on activation.
- Exec applet popover only toggles when popover content is available.
- Animated popover widget interactions.

### Removed

- `devgallery` applet (developer-only widget previewer).
- Legacy `components/` directory (migrated into `utils/` + `widgets/`).
- Applet popover lifecycle outputs (no longer needed by the new
  widget system).

[Unreleased]: https://github.com/alex-oleshkevich/glimpse/compare/v0.12.0...HEAD
[0.12.0]: https://github.com/alex-oleshkevich/glimpse/compare/v0.11.0...v0.12.0
