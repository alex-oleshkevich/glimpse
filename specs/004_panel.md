---
state: draft
---

# 004 — Panel

Bars, zones, applets, popovers and notification popups. Crate `glimpse-panel` builds the binary
`glimpse`, which is the artifact that renders it.

## Problem

The visible surface of the suite. It must start after the daemon, survive being restarted dozens of
times a day during development without disturbing anything else, and render correctly on a
multi-output setup where monitors come and go.

Multi-output is where the naive design breaks. One bar per output means one *instance of everything*
per output, and some things must exist exactly once in a session — a notification popup stack drawn
on three monitors is one popup that needs three dismissals.

Styling breaks differently. GTK4's CSS loaders return nothing on a parse error, so a stylesheet with
one bad line degrades silently, and CSS providers accumulate rather than replace, so a reload that
adds without removing is a slow leak of cascading rules.

## Goals

- Render topics and send commands; hold no state that outlives a widget.
- Restart with no visible consequence beyond the bar disappearing and reappearing.
- Survive a dead daemon by rendering empty rather than crashing.
- One bar per output, tracked across hotplug, with session-singleton surfaces owned exactly once.
- A broken stylesheet produces a diagnostic, not a silently half-styled panel.

## Non-goals

- No D-Bus connections, no PipeWire, no NetworkManager. Everything comes over the socket.
- No settings UI. Configuration is hot-reloaded TOML.
- No state written anywhere.

## Tech

### Bars and outputs

One bar per output by default, from `[[panels]]`. A new output means a new bar; a removed output
means the bar and its widgets are dropped. A `[[panels]]` entry with `monitor` set binds to that
connector; one without appears on every output.

### Zones and applets

Three zones per bar — `left`, `center`, `right` — each an ordered list of applet names.
`__dynamic__` expands to the applets not named elsewhere, so listing a few keeps the rest without
enumerating them.

An applet name resolves to an `[applets.<name>]` entry, or to a built-in type of the same name.
Several instances of one type coexist under different names, which is what `extends` is for.

An applet renders topics and sends commands. It never opens a D-Bus connection, never reaches a
backend directly, and holds no state that outlives its own widget.

### Session-singleton surfaces

Some surfaces must exist once per session, not once per bar. The notification popup stack is the
case that matters: every bar would otherwise raise its own, they would stack at the same anchor, and
dismissing what looks like one popup would take one click per monitor.

Ownership is elected, not assumed:

| Configuration                        | Owner                                                        |
| ------------------------------------ | ------------------------------------------------------------ |
| popup monitor names a connector      | the bar on that connector; no popup at all while it is absent |
| popup monitor unset                  | the bar on the alphabetically first connected output          |
| the bar is not bound to an output    | never — it cannot prove it is the singleton                   |

Alphabetical-first is arbitrary but deterministic, which is the property that matters: every bar
runs the same election and exactly one wins, with no coordination and no configuration required.

### Styling

Four CSS providers on the display, lowest priority first:

| Priority          | Source                                      | Role                            |
| ----------------- | ------------------------------------------- | ------------------------------- |
| `APPLICATION`     | built-in base                               | light defaults and dark tokens  |
| `USER`            | the theme pack's `panel.css`                | pack-level overrides            |
| `USER + 1`        | built-in remap                              | dark-scheme and media rules     |
| `USER + 2`        | `$XDG_CONFIG_HOME/glimpse/themes/panel.css` | the user's final word           |

**Providers are installed once and reloaded in place.** Installing again stacks a second copy of
every rule, and the symptom is a panel that grows slower and styles more strangely the longer it
runs.

**Every provider logs parse errors.** GTK4's `load_from_path` and `load_from_string` return nothing;
errors arrive only on the `parsing-error` signal, so a provider that does not connect it turns a
malformed stylesheet into silent partial styling with no diagnostic anywhere.

### Widget rules

- **Update properties on existing widgets.** Rebuilding a widget tree per event is the most likely
  source of visible stutter; rebuild only when the set of items changes.
- **A programmatic state change must not re-emit its signal.** Setting a toggle from a topic event
  otherwise re-enters the handler that sends the command, and the two fight.
- **CSS class updates are idempotent and additive-safe.** Base classes, state classes and
  caller-supplied extras coexist; recomputing from the same input changes nothing.
- **A count that gates UI counts what this process owns**, not the global total. An indicator keyed
  on "any inhibitor exists" never flips on a system with one permanent inhibitor.

### Optimistic updates

A slider or toggle updates its own widget immediately and sends the command. The topic event that
follows is reconciliation, not the source of the frame. This is safe because topics are state cells:
the daemon's value always wins and the panel cannot drift.

### Tray

Icon names resolve through the GTK icon theme, honouring `IconThemePath`; pixmap items load the PNG
path published by the daemon. Menus are fetched on pointer-enter, not on click, so the menu is ready
when the click lands.

### Notifications

Popups render from the notifications topics on the elected singleton owner. Actions are sent back as
commands and the daemon does the D-Bus round trip.

### External applets

Spawned by the panel, one process each, a line protocol over stdin and stdout. A crashed external
applet leaves a placeholder and is restarted with backoff.

The protocol is versioned by tolerance rather than by number: a field a client does not send must
still parse, so an applet built against an older panel keeps working.

### A dead daemon

The panel keeps running. Widgets whose topics have no value render an empty or placeholder state;
`stale` values render with reduced emphasis rather than disappearing. Reconnect is automatic with
backoff, and resubscribing restores the full picture with no special handling.

## The binary

```
glimpse [OPTIONS]
```

No subcommands and no arguments.

| Flag                    | Default                                  | Purpose                                                     |
| ----------------------- | ---------------------------------------- | ----------------------------------------------------------- |
| `-c`, `--config <PATH>` | the layered stack                        | Use exactly this file                                       |
| `--socket <PATH>`       | `$XDG_RUNTIME_DIR/glimpse/glimpsed.sock` | Daemon socket                                               |
| `--css <PATH>`          | from config                              | Override the user stylesheet                                |
| `--output <NAME>`       | all outputs                              | Show a bar only on this output; repeatable. Debugging aid   |
| `--check-config`        | off                                      | Validate configuration and stylesheet, print problems, exit |
| `--inspector`           | off                                      | Open the GTK inspector at startup                           |
| `--log <FILTER>`        | `info`                                   | `tracing-subscriber` filter                                 |
| `-V`, `--version`       |                                          |                                                             |
| `-h`, `--help`          |                                          |                                                             |

### Environment

| Variable              | Use                                                                              |
| --------------------- | -------------------------------------------------------------------------------- |
| `WAYLAND_DISPLAY`     | required                                                                         |
| `XDG_RUNTIME_DIR`     | socket path, tray icon files                                                     |
| `GLIMPSE_APP_ID`      | override the GTK application id; used to run a dev instance beside a release one |
| `GLIMPSE_CONFIG_PATH` | default for `--config`                                                           |
| `GLIMPSE_SOCKET_PATH` | default for `--socket`                                                           |
| `GLIMPSE_CSS_PATH`    | default for `--css`                                                              |
| `GTK_DEBUG`           | standard GTK debugging                                                           |

### Files

| Path                                        | Role                                                           |
| ------------------------------------------- | -------------------------------------------------------------- |
| `$XDG_CONFIG_HOME/glimpse/config.toml`      | `[[panels]]` and `[applets.*]`; schema in `010`                |
| `$XDG_CONFIG_HOME/glimpse/themes/panel.css` | user override CSS; the daemon watches it, the panel reloads it |
| `$XDG_RUNTIME_DIR/glimpse/tray/*.png`       | tray pixmaps written by the daemon                             |

### Exit codes

| Code | Meaning                                                           |
| ---- | ----------------------------------------------------------------- |
| 0    | clean exit                                                        |
| 1    | configuration or stylesheet invalid, reported by `--check-config` |
| 2    | usage error                                                       |
| 5    | no Wayland display, or no layer-shell support                     |

Neither an invalid configuration nor an unparseable stylesheet is an exit. Both log and fall back to
the built-in defaults at startup, and are dropped on reload — see `010_configuration.md`.

## Risks

- **Technical** — per-event widget rebuilding is the most likely cause of visible stutter. Update
  properties on existing widgets; rebuild trees only when the set of items changes.
- **Technical** — if `Failed to initialize layer surface` appears at startup, the cause is library
  ordering: `libwayland-client` winning symbol resolution over `libgtk4-layer-shell` in the
  binary's `DT_NEEDED` list, so the protocol interception never fires. Check the gtk4-layer-shell
  version before reaching for a custom linker wrapper.

## Changelog

- 2026-08-20 — created, split out of `001_architecture.md`.
- 2026-08-20 — recorded the layer-surface symbol-ordering symptom as a risk after removing the
  `.cargo/config.toml` linker wrapper.
- 2026-08-20 — configuration moved into the shared `config.toml` under `[panel]`; `panels.css` corrected to `panel.css`.
- 2026-08-20 — invalid configuration logs and falls back to defaults instead of exiting; exit 1 is now only `--check-config`.
- 2026-08-20 — stylesheet and config changes arrive from the daemon's `watcher` service rather than a watch the panel opens itself; with the daemon down the panel keeps what it loaded.
- 2026-08-20 — configuration is `[[panels]]` and `[applets.*]` per `_old`; the override stylesheet is `themes/panel.css`.
- 2026-08-20 — renamed from `004_glimpse_panel.md` and reorganised around panel behaviour, with the binary and its flags as one section rather than the subject.
- 2026-08-20 — specified from `_old/glimpse-shell`: singleton popup election across bars, the four-provider CSS stack with install-once-reload-in-place, `parsing-error` logging because GTK4 loaders return nothing, and the widget rules for signal re-entry, idempotent class updates and owned-versus-global counts.
