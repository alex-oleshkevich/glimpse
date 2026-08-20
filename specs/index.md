# Specs

[glossary.md](glossary.md) defines the vocabulary these specs use.

| #   | Title                                               | Description                                                                   | Beads | State |
| --- | --------------------------------------------------- | ----------------------------------------------------------------------------- | ----- | ----- |
| 001 | [Architecture](001_architecture.md)                 | One daemon owns session state; UI binaries are stateless clients on a socket. | —     | draft |
| 002 | [Repository and module structure](002_structure.md) | Crate boundaries, dependency direction, module layout, file placement.        | —     | draft |
| 003 | [Daemon](003_daemon.md)                             | Topics, broker, services and demand; the only process talking to backends.  | —     | draft |
| 004 | [Panel](004_panel.md)                               | Bars, zones, applets, singleton popups, the CSS provider stack.              | —     | draft |
| 005 | [Wallpaper](005_wallpaper.md)                       | Sources, decoding and cache, transitions, overview backdrop.                | —     | draft |
| 006 | [Lock](006_lock.md)                                 | Session lock surfaces, PAM, rate limiting, lockout diagnostics.             | —     | draft |
| 007 | [glimpsectl](007_glimpsectl.md)                     | CLI and TUI: read, watch, call, inspect.                                      | —     | draft |
| 008 | [glimpse-devtools](008_glimpse_devtools.md)         | Widget previewer. Development only, never installed.                          | —     | draft |
| 009 | [systemd integration](009_systemd.md)               | User units, ordering, readiness, restart policy, sandboxing rules.            | —     | draft |
| 010 | [Configuration](010_configuration.md)               | One shared `config.toml`: tables, layers, drop-ins, per-service reload.      | —     | draft |
| 011 | [Watcher](011_watcher.md)                           | The service watching config and stylesheets; digests, not contents.          | —     | draft |
