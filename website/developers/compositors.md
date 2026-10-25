# Compositor providers

Glimpse clients don't use your compositor's own IPC. A compositor provider does, and exposes the
compositor's state and commands over D-Bus for the clients to use. To support another compositor,
write a provider that implements the contract on this page.

| Address | Value |
| --- | --- |
| Bus name | `me.aresa.Glimpse.Compositor` |
| Object path | `/me/aresa/Glimpse/Compositor` |
| Interface | `me.aresa.Glimpse.Compositor1` |

## Add a provider

Any language with D-Bus support will do; there is no Glimpse SDK or Rust trait to adopt. If you
write Rust, follow the bundled `glimpse-compositor-niri` and `glimpse-compositor-hyprland` crates:
each implements `glimpse_compositors::Backend` and passes it to `provider::run`. The trait is in
`crates/glimpse-compositors/src/backend.rs`, `provider.rs` serves the D-Bus interface, and
`client.rs` holds the client proxy.

1. Own the bus name and expose the properties and methods below. If another process already owns
   the name, leave it alone. The bundled providers exit 4 on a name clash, and their units don't
   restart after that exit.
2. Publish the compositor's state through the properties and announce each change with
   `PropertiesChanged`. Advertise the capabilities you support, and return `Unsupported` for any
   operation you can't perform.
3. Install a user service with `WantedBy=glimpse-session.target` and enable it explicitly. If your
   provider replaces a bundled one, mask the bundled unit.

The [session target](./systemd) starts both bundled provider units. Each one checks its environment
and makes a real `--probe` request to select its compositor. There is no D-Bus activation file.

## Properties

Every property is read-only. `Name` and `Capabilities` stay fixed for as long as you own the bus
name; for any other property, emit `org.freedesktop.DBus.Properties.PropertiesChanged` when it
changes. A `?` marks an optional dictionary key. Clients ignore keys and capability strings they
don't know.

| Property | D-Bus type | Value or dictionary fields |
| --- | --- | --- |
| `Name` | `s` | Provider name, such as `niri` or `hyprland` |
| `Capabilities` | `as` | The capabilities you support, from the table below |
| `Outputs` | `aa{sv}` | `connector s`, `enabled b`, `description s?`, `make s?`, `model s?`, `mode (uuu)?` (width, height, refresh in mHz), `logical (iiuud)?` (x, y, width, height, scale) |
| `Workspaces` | `aa{sv}` | `id t`, `index y?`, `name s?`, `output s?`, `active b` |
| `Windows` | `aa{sv}` | `id t`, `title s?`, `app_id s?`, `pid i?`, `workspace t?`, `floating b`, `urgent b`, `order q?` |
| `Focus` | `a{sv}` | `output s?`, `window t?` |
| `Keyboard` | `a{sv}` | `names as?`, `codes as?`, `current u?` (zero-based index into the supplied arrays) |
| `Casts` | `aa{sv}` | `id t`, `active b`, `output s?`, `session t?`, `pw_node u?` |
| `SessionLockRestore` | `b` | Optional: whether the compositor lets a session lock be restored. Without it, clients treat the answer as unknown and log no warning |

Leave out an optional value you don't have rather than sending a sentinel.

IDs are opaque unsigned 64-bit values that mean something only while your provider owns the bus
name. Clients must not display or persist them.

Providers and clients both cap text: titles at 512 characters, app IDs and names at 128, with
control characters stripped.

## Capabilities

| String | Enables | `niri` | `hyprland` |
| --- | --- | --- | --- |
| `rename-workspace` | `RenameWorkspace` | Yes | Yes |
| `output-power` | `SetOutputEnabled` | Yes | Yes |
| `blank-screens` | `PowerOffMonitors` | Yes | No |
| `window-screenshot` | `ScreenshotWindow` | Yes | No |
| `window-geometry` | `WindowGeometries` | Yes | Yes |
| `overview-backdrop` | Placing the `glimpse-backdrop` layer behind the overview | Yes | No |

## Methods

A method returns nothing unless the table shows a return type. The step methods follow the
compositor's own ordering, wrapping and output scope, so clients call them instead of working out the
next index from their copy of the state.

| Method | Behavior |
| --- | --- |
| `FocusWorkspace(t id)` | Focus a workspace |
| `StepWorkspace(b forward)` | Step between workspaces |
| `RenameWorkspace(t id, s name)` | Rename a workspace; an empty name clears it |
| `FocusWindow(t id)` | Focus a window |
| `StepWindow(b forward)` | Step between windows |
| `SwitchKeyboardLayout(u index)` | Select a layout by zero-based index |
| `StepKeyboardLayout(b forward) → u` | Step between layouts and return the selected zero-based index |
| `SetOutputEnabled(s connector, b enabled)` | Turn the named output on or off |
| `PowerOffMonitors()` | Blank the screens |
| `StopScreencast(t session)` | Stop the cast whose `session` field matches |
| `ScreenshotWindow(t id, s path)` | Write the screenshot to an absolute UTF-8 path, and reply only once the file is complete |
| `WindowGeometries() → a{ta{sv}}` | Live geometry by window ID: `x d`, `y d`, `width d`, `height d`, in logical pixels relative to the window's output |

If the compositor doesn't report a window's geometry, leave that window out of `WindowGeometries`.
On `niri`, tiled windows have no width or height in `glimpse-shot list`, and capturing one goes
through `ScreenshotWindow`.

## Errors

Error names start with `me.aresa.Glimpse.Compositor1.Error.` and end in one of four words:

| Name | When |
| --- | --- |
| `Unsupported` | The operation isn't supported |
| `Refused` | The request is invalid or rejected |
| `Unavailable` | The connection to the compositor is lost |
| `Failed` | Anything else went wrong |

Answer with `Refused` for a workspace, window, output or session ID you don't know, for a workspace
name over 128 characters or containing control characters, and for a screenshot path that isn't an
absolute UTF-8 path.

## When there is no provider

If nothing has ever owned the bus name, Glimpse treats compositor support as unavailable rather
than broken: commands return `Unsupported` and compositor features stay hidden, but notification
popups still appear. If the owner goes away, clients mark their copy as degraded and keep the last
state. A new owner supplies a fresh snapshot.
