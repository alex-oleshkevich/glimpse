# glimpse-config

Layered TOML configuration, shared by the daemon and every UI binary.

## What it does

- Resolves the layered stack: built-in defaults, system, user, drop-ins, environment, CLI
- Merges layers and validates the result
- Watches the resolved files and reports changes for hot reload
- Reports the exact location of a parse or validation error

An invalid edit never replaces a working configuration. The running config stays, and the failure is
reported with its location.

Each binary reads only its own file: `glimpsed.toml`, `panel.toml`, `lock.toml`, `wallpaper.toml`.
A binary never reads another binary's configuration — if it needs a value from elsewhere, that value
is a topic.

Spec: [`specs/001_architecture.md`](../../specs/001_architecture.md)
