# Custom Applets

Custom applets are user-owned panel items. Use them when the built-in applets do not show exactly what you want, or when you want a personal launcher, menu, script, or live widget in the panel.

## Choose A Path

| You want to | Use | Start here |
| --- | --- | --- |
| Open an app, URL, folder, script, screenshot tool, or power menu | `command` | [Command Applet](./command.md) |
| Show changing text, icons, state, or custom popover content | `exec` | [Getting Started](./getting-started.md) |

If the applet only needs to run something when you click it, start with a command applet. If it needs to stay alive, update itself, or react to panel events, use an exec applet.

## How The Pieces Fit

Custom applets are package files. A package file gives the applet an `id`, selects its `type`, and stores the applet-specific options.

For normal use, package files live in:

```text
~/.config/glimpse/applets
```

A command package runs commands directly from the panel. It is best for launchers and small menus.

An exec package starts a program that controls the panel item. The program can send status updates, render popovers, and receive click or scroll events.

## Development Flow

For exec applets, the usual flow is:

1. Create a project with [Applet Tooling](./tooling.md).
2. Run it in development mode.
3. Use the `__dev__` panel slot while you iterate.
4. Link the applet when it is ready for normal use.

The dev slot shows applets started by the development command. The default panel keeps `__dev__` at the end of the left section:

```toml
[[panels]]
left = ["pager", "mpris", "__dev__"]
```

If you replace the default panel layout, keep or add `__dev__` wherever you want active development applets to appear.

## Recommended Reading

| Page | Use it for |
| --- | --- |
| [Command Applet](./command.md) | Launchers, menus, one-shot actions, and shell examples. |
| [Getting Started](./getting-started.md) | Your first live exec applet. |
| [Applet Tooling](./tooling.md) | Creating, running, linking, and diagnosing applet projects. |
| [Exec Applet](./exec.md) | Exec package config, lifecycle, restart behavior, and environment handling. |
| [Exec SDK](../applets/exec-sdk.md) | Building exec applets in supported languages. |
| [Line Protocol](./exec-protocol.md) | The raw protocol used by exec applets. |
| [Components](./exec-components.md) | Built-in status and popover component reference. |
