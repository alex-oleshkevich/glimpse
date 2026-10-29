# Let a coding agent see glimpse

Two MCP servers let a coding agent such as Claude Code look at glimpse while it runs.
[gtk-mcp](https://github.com/alex-oleshkevich/gtk-mcp) reads the widgets inside a glimpse program,
and [wayland-mcp](https://github.com/alex-oleshkevich/wayland-mcp) sees the screen and sends real
clicks and keys. The applet skills in [Build an applet with a coding agent](/applets/agents) use
both servers when your agent has them.

## Which one to use

| | gtk-mcp | wayland-mcp |
| --- | --- | --- |
| What it sees | The GTK widgets inside one glimpse program | The screen, as pixels and text read by OCR |
| How it acts | Calls GTK directly: opens popovers, activates buttons, fills entries | Real pointer, keyboard and touch input |
| Builds | Debug builds of glimpse only | Any build |
| Pictures | One widget, window or popover, rendered by GTK | The whole screen |

Use gtk-mcp to find out why something looks or behaves wrong: which widget an element became, what
got cut off, which style applies. Use wayland-mcp to check what a person sees and presses.

## Install the servers

```sh
curl -fsSL https://raw.githubusercontent.com/alex-oleshkevich/gtk-mcp/main/install.sh | sh
curl -fsSL https://raw.githubusercontent.com/alex-oleshkevich/wayland-mcp/main/install.sh | sh
```

Both put their binary in `~/.local/bin`. Register them with your agent, from inside your Wayland
session:

```sh
claude mcp add gtk-mcp -- gtk-mcp
claude mcp add wayland-mcp -- wayland-mcp
```

A glimpse checkout carries a `.mcp.json` that registers both, so Claude Code started in the checkout
finds them once the binaries are on `PATH`.

## Build glimpse with the agent inside

The debug builds of `glimpse-panel`, `glimpse-notifications`, `glimpse-lock`, `glimpse-wallpaper`,
`glimpse-agents` and `glimpse-shot` have the gtk-mcp agent compiled in. Release builds and packages
leave it out, so on a packaged install gtk-mcp finds no glimpse program.

From a checkout set up as in [Build glimpse from source](/contribute/build):

```sh
just build
```

The debug binaries land in `target/debug/`.

## Look at your applet in a debug panel

Run your applet project with the `glimpsectl` from that build:

```sh
<checkout>/target/debug/glimpsectl applet run --dev --isolated
```

`--isolated` starts a fresh test panel at the bottom of the screen and leaves your session bar
alone. It runs the `glimpse-panel` next to that `glimpsectl`, so the test panel is a debug build
with the agent inside. Its app id starts with `me.aresa.GlimpsePanel.Dev.`. Saving a file rebuilds
and restarts the applet as described in [How development mode works](/developers/dev-mode).

Then ask your coding agent to look at the applet. A typical round:

1. `list_apps` finds the test panel.
2. `snapshot` prints its widget tree: your applet's elements as the GTK widgets the panel built
   from them.
3. `audit_layout` reports labels cut off with "…", children outside their parent and icon names
   that do not resolve. A narrow bar and a fixed-width popover are where these show up.
4. `interact` opens the popover and activates a button or fills an entry, then reads the result
   back. `wait_for` waits until a widget reaches the state you describe.
5. `capture` saves a picture of the popover. GTK renders it, so it works on the panel's layer
   surface and while the popover is covered.

`apply_css` tries a style on the running panel and returns its parse errors, which helps when you
[write a theme](/guide/theming). The style goes away when the agent disconnects.

## Check the screen with wayland-mcp

Have the agent run `doctor` first: it lists what works on your compositor and changes nothing.
`screenshot` returns a picture and a `shot_id`. A `click` that passes the same `shot_id` is refused
if the screen changed since, instead of landing somewhere else. `key` presses a shortcut, so the
agent can check a [keybinding](/guide/keys) end to end.

On your own session the clicks and keys are real, and an overlay on screen lists what the agent
does. `sandbox_start` and `target_select` give the agent a nested compositor with its own Wayland
socket and D-Bus session instead, and `app_launch` starts a program inside it.

## What reaches the model

Screenshots and captures go to whatever model your agent uses. gtk-mcp never returns the text of a
password field, not even its length, and refuses to capture one that shows its text. Anything else
on screen can end up in a picture, so close what you would not paste into a chat.
