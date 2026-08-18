# Glimpse reference (LLM-optimized)

Single-file references optimized for code generation and documentation tasks. Each page is self-contained enough to use without following the human docs first. Duplication across pages is intentional.

Agents can start from [llms.txt](/llms.txt), which lists the canonical LLM-facing references for this docs site.

| Page | Covers |
|---|---|
| [Configuration](./configuration.md) | Main `config.toml`: discovery, panels, theme mode, services, wallpaper, lock, calendar, and custom applet package placement. |
| [Applet Configuration](./applets.md) | Built-in applet `[applets.*]` sections, defaults, enum values, placeholders, and package applet examples. |
| [Command Applet](./command.md) | The experimental `command` applet: launcher buttons and menus that run commands. |
| [Exec Applet](./exec.md) | The experimental `exec` applet: applet tooling, package files, line protocol, widgets, events, and SDK starters for Python, TypeScript, Rust, and Go. |
