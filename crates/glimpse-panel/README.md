# glimpse-panel

The panel: layer-shell bars, applets, popovers and notification popups.

Builds the binary named `glimpse`.

## Contents

- `main.rs` — GTK application, layer-shell setup, one bar per output across hotplug
- `panel.rs` — bar window, zones, applet placement
- `applets/` — one module per applet
- `popups/` — notification popups, OSD

## Rules

An applet renders topics and sends commands. It never opens a D-Bus connection, never reaches a
backend directly, and holds no state that outlives its own widget.

UI state never waits on a round trip. A slider updates its own widget immediately and sends the
command; the topic event that follows is reconciliation. This is safe because topics are state
cells — the daemon's value always wins and the panel cannot drift.

Update properties on existing widgets. Rebuilding widget trees per event is the most likely source
of visible stutter.

A surface that must exist once per session — the notification popup stack — is owned by an elected
bar, not by every bar. An unbound bar never owns it.

CSS providers are installed once and reloaded in place; installing twice stacks every rule. Every
provider connects `parsing-error`, because GTK4's loaders return nothing and a malformed stylesheet
is otherwise silent.

A programmatic state change must not re-emit its signal, or the handler that sends the command
re-enters itself.

A dead daemon is a normal state: widgets render empty, `stale` values render with reduced emphasis,
and reconnection restores everything with no special handling.

Configuration is the `[panel]` table of the shared `config.toml`, plus `panel.css`. Tables owned
by other binaries are ignored, not validated. Schema in
[`specs/010_configuration.md`](../../specs/010_configuration.md).

Spec: [`specs/004_panel.md`](../../specs/004_panel.md)
