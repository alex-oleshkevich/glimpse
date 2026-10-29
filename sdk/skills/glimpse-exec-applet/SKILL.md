---
name: glimpse-exec-applet
description: Load when the user wants a live glimpse panel applet (changing text or icons, a popover, state, timers, data from a program) written with the TypeScript, Python, Rust or Go SDK. A plain button that runs a command on click or scroll needs glimpse-command-applet instead.
---

# Build a glimpse exec applet

An exec applet is a program the panel starts and talks to over a socket. It draws an
`Indicator` on the bar and an optional `Popover`. Applets run unconfined as a systemd user unit, so
treat their code like any program the user installs.

## Decide first

- Already inside a project: if the current directory holds a `justfile` and one `<id>.desktop`
  (made by `glimpsectl applet new`, which also put this skill in `.agents/skills/`), skip Scaffold
  and keep that project's language and id.
- Static button, one command per gesture: stop and load `glimpse-command-applet`.
- Text, icons, badges or a popover that change, state, timers, files, network, options: build an
  exec applet (this skill).
- Language: use the one the user names. If unspecified, use `ts`. Each needs its toolchain
  (`ts`: Node.js 22+, npm, `just`; `py`: `uv`; `rs`: cargo; `go`: Go and golangci-lint).

## Scaffold

Pick a reverse-DNS id (it becomes the desktop-file id, the project name and the zone name). Run:

```sh
glimpsectl applet new --language ts --id org.example.Countdown --name Countdown --icon x-office-calendar-symbolic --yes --dir countdown
```

`--language` is `ts`, `py`, `rs` or `go`. `--description` is also available. The target directory
must be new or empty. The id must not already be installed. Read the generated `README.md` and
`justfile` before editing.

| Language | Source you edit | Dependency / manifest | Entry in the desktop file |
| --- | --- | --- | --- |
| ts | `index.tsx` | `package.json` (`glimpse-sdk`), `tsconfig.json` (`jsxImportSource: "glimpse-sdk"`, keep it) | `Exec=node applet.mjs` (built into `dist/`) |
| py | `applet.py`, `test_applet.py` | `pyproject.toml` (`glimpse-applet-sdk`) | `Exec=uv run applet.py` |
| rs | `src/main.rs` | `Cargo.toml` (`glimpse-sdk`) | the built binary |
| go | `main.go` | `go.mod` (`.../sdk/sdk-go`) | the built binary |

Every project also has `<id>.desktop`, `devconfig.toml` (the options and env `just dev` passes) and
a `justfile`. If the SDK dependency does not resolve from the registry, install it from the glimpse
source tree (for ts: `npm install <glimpse checkout>/sdk/sdk-ts`; for py: `<checkout>/sdk/sdk-py`).
That writes a `file:` path into the manifest, which only works on this machine; tell the user.

The desktop entry must have `Type=Application`, `Implements=me.aresa.Glimpse.Applet1`, an `Exec`
that is parsed without a shell, and must not set `Hidden`, `Terminal` or `DBusActivatable`. The
file name minus `.desktop` is the applet id. Exactly one `.desktop` file may sit in the project.
Details: https://alex-oleshkevich.github.io/glimpse/applets/exec/reference/desktop-entry.md

## Component model

You return a tree from a component (ts: React-style function; py: `BaseApplet.build`; rs:
`App::build`; go: `Build`). The tree has two top-level parts:

- `Indicator`: what shows on the bar (`icon`, `tooltip`, `badge`, `dot`; the text is children in
  ts and the `text=` prop in py).
- `Popover`: what opens on click (`title`, `icon`, `subtitle`), holding `Section`, `Row`,
  `ToggleRow`, `ActionRow`, `Entry`, `Button` and the rest.

Only the panel checks which element may sit in which, so run `just dev` after every structural
change. Read the parent table and each element's props in
https://alex-oleshkevich.github.io/glimpse/applets/exec/reference/elements.md and do not use a prop
or element you cannot find there or in the SDK source. Hooks in ts: `useState`, `useEffect`,
`useOptions`, `usePlacement`, `usePopover`, `useAppletName`, `useStateDir`, `usePersistentState`.
Python and Rust keep state on the applet (`set_state`, `update`) and read `ctx.options`,
`ctx.state_dir`, `ctx.name`; Go uses `ctx.Options`, `ctx.StateDir`, `ctx.Name`.

The SDK source in the project is the offline API reference. Search it before guessing:

- ts: `node_modules/glimpse-sdk/` (`elements.ts`, `hooks.ts`, `desktop.ts`, `testing.ts`)
- py: `uv run python -c "import glimpse_sdk; print(glimpse_sdk.__file__)"`, then read that package
- rs: `cargo doc --open`, or the `glimpse-sdk` crate under `~/.cargo/registry/src/`
- go: `go list -m -f '{{.Dir}}' github.com/alex-oleshkevich/glimpse/sdk/sdk-go`

Per-language pages: https://alex-oleshkevich.github.io/glimpse/applets/exec/reference/sdk.md (ts),
`.../python.md`, `.../rust.md`, `.../go.md` in the same folder.

Python specifics: state is a frozen dataclass subclassing `State`, assigned as `state = …` on the
applet and changed with `self.set_state(field=value)`. Start timers and loops in
`async def start(self, ctx)` with `ctx.spawn(coroutine)`, and run blocking file or network calls
through `asyncio.to_thread`. The SDK dependency is declared twice: in `pyproject.toml` (lint and
tests) and in the `# /// script` block at the top of `applet.py` (what `Exec=uv run applet.py`
uses). Change both together.

## Options, data and host actions

- Options come from the panel config (`options` table below) and arrive as a JSON object. Validate
  every value: the type parameter of `useOptions` is a cast, not a check. Changing options restarts
  the applet.
- Persist data under the applet's state directory (`~/.local/state/glimpse/applets/<id>/`). ts:
  `usePersistentState(key, initial)` or `useStateDir()`; py: `persist = ("field",)` on the applet or
  `ctx.state_dir`; rs: `ctx.load_state()` / `ctx.save_state()`. Write files to a temporary name,
  then rename. Two instances of one id share the directory, so key by the instance name.
  https://alex-oleshkevich.github.io/glimpse/applets/exec/data.md
- Host actions run from an event handler and return nothing: `notify`, `copy` (needs `wl-copy`),
  `openUri` (`http`, `https`, `mailto`, `file`, at most 2048 bytes), `launch` (a desktop-file id, not
  a command line) and closing the popover. https://alex-oleshkevich.github.io/glimpse/applets/exec/host.md
- Text from the network, files, other programs or options is hostile. Cap its length before
  showing it, pass it only as a prop or child value, and never assemble markup or a command line
  from it.

## Example: a countdown (TypeScript)

Scaffolded with the command above, then tested with `just lint` and `just test` (3 tests pass).
`tsconfig.json` `include` was widened to `["*.tsx"]` so the test file is type-checked. Delete the
`build/` directory `just test` leaves before `just lint`, or add `!build` to Biome's ignore list.
`app.tsx` (new):

```tsx
import { Indicator, Popover, Row, Section, useOptions } from "glimpse-sdk";

const ICON = "x-office-calendar-symbolic";
const DAY_MS = 86_400_000;

export function daysLeft(date: string, now: Date): number | null {
    const match = /^(\d{4})-(\d{2})-(\d{2})$/.exec(date);
    if (!match) return null;
    const target = Date.UTC(Number(match[1]), Number(match[2]) - 1, Number(match[3]));
    const today = Date.UTC(now.getFullYear(), now.getMonth(), now.getDate());
    return Math.round((target - today) / DAY_MS);
}

export function App() {
    const options = useOptions();
    // Options are untrusted input: cap the length before showing it.
    const date = typeof options.date === "string" ? options.date.slice(0, 10) : "";
    const left = daysLeft(date, new Date());
    const text = left === null ? "Set date" : `${left}d`;
    return (
        <>
            <Indicator icon={ICON} tooltip={left === null ? "Set options.date" : `Until ${date}`}>
                {text}
            </Indicator>
            <Popover icon={ICON} title="Countdown">
                <Section title="Countdown">
                    <Row title={left === null ? "No valid date" : date} subtitle={text} />
                </Section>
            </Popover>
        </>
    );
}
```

`index.tsx` (replaces the generated body):

```tsx
import { run } from "glimpse-sdk";
import { App } from "./app.tsx";

await run(<App />);
```

`countdown.test.tsx` (`just test` runs every `*.test.tsx`):

```tsx
import assert from "node:assert/strict";
import { test } from "node:test";
import { render } from "glimpse-sdk/testing";
import { App, daysLeft } from "./app.tsx";

type Node = { type: string; props: Record<string, unknown>; children: Node[] };
type Element = Parameters<typeof render>[0];

function find(node: Node, type: string): Node | undefined {
    if (node.type === type) return node;
    for (const child of node.children) {
        const hit = find(child, type);
        if (hit) return hit;
    }
}

test("counts days", () => {
    assert.equal(daysLeft("2026-01-11", new Date(2026, 0, 1)), 10);
    assert.equal(daysLeft("nope", new Date()), null);
});

test("shows a hint without a valid date", async () => {
    const app = await render((<App />) as Element, { options: { date: "garbage" } });
    try {
        assert.equal(find(app.tree() as Node, "row")?.props.title, "No valid date");
    } finally {
        await app.close();
    }
});

test("shows the configured date", async () => {
    const app = await render((<App />) as Element, { options: { date: "2999-01-01" } });
    try {
        assert.equal(find(app.tree() as Node, "row")?.props.title, "2999-01-01");
    } finally {
        await app.close();
    }
});
```

`devconfig.toml` is `options = { date = "2026-12-31" }` for `just dev`. In `render`, the second
argument also takes `placement` and `state`; the returned object has `tree()`, `requests()`,
`click(id)`, `event(id, name, args)`, `setOptions`, `setPopoverOpen` and `close()`.

## Workflow

Run these from the project directory; fix each failure before moving on.

```sh
just lint
just test
just dev
```

- `just lint`: Biome and `tsc` (ts), ruff and mypy (py), clippy (rs), golangci-lint (go).
- `just test`: runs the SDK test session without a panel. Python: `Session(App(), lambda line:
  None)` then `app.build(session.ctx)`. Go and Rust: build a `Session`, send a `hello`, `flush`.
  Samples: https://alex-oleshkevich.github.io/glimpse/applets/exec/testing.md. Tests cannot check
  element nesting.
- `just dev` (`glimpsectl applet run --dev --zone left .`): attaches the applet to the user's
  running panel, rebuilds and restarts on source edits and `env` changes in `devconfig.toml`
  (`options` edits reach the running applet without a restart), and prints stderr. In that terminal
  type `r` + Enter to restart and `t` + Enter to print the tree. Ctrl-C detaches. It needs a
  running panel; if there is none, say so and rely on `just test`. Do not leave it running in the
  background; tell the user to run it.
- `just install` builds the package and installs it for the user. `just package` writes a
  `.glimpse.zip` to share. Run `just install` only when the user asks for it.

## Put it on the bar

Add the id to a zone of a panel in `~/.config/glimpse/panel.toml`. Writing a zone replaces its
default list, so read the current file first and copy every name that must stay; a zone you leave
out keeps its default. The defaults are in `/usr/share/glimpse/panel.commented.toml`. A dotted id
needs no table; the table is only for options, env or a second copy under another name:

```toml
[[panels]]
right = [
    "org.example.Countdown",
    "next-event",
    "tray",
    "removable",
    "clipboard",
    "printing",
    "bluetooth",
    "network",
    "display",
    "audio",
    "idle",
    "power",
    "keyboard",
    "session",
]

[applets."org.example.Countdown"]
options = { date = "2026-12-31" }

[applets.countdown-work]
extends = "exec"
applet = "org.example.Countdown"
options = { date = "2027-06-01" }
```

An exec table has `extends = "exec"`, `applet` (the desktop-file id), `options` and `env`. Run
`glimpsectl --log error config validate`; it prints nothing when the file loads. The panel reloads the file by
itself. Tell the user how to remove the applet: delete the id from the zone, then
`glimpsectl applet uninstall org.example.Countdown`.
https://alex-oleshkevich.github.io/glimpse/applets/exec/configure.md

## Debug

```sh
glimpsectl applet list --installed
glimpsectl applet inspect --installed org.example.Countdown
glimpsectl applet restart org.example.Countdown
journalctl --user -u 'app-glimpse-*.service' -f
```

- No chip on the bar: check the id is in a zone and `inspect` shows the desktop entry accepted.
- The panel kills the applet when it refuses the tree (for example `section cannot hold section`);
  the reason is in the panel log, not the `just dev` terminal. Fix the nesting from the elements
  page.
- Exceptions in handlers end the process and print to stderr. `console` output goes to stderr in
  ts; use it for debugging, not `stdout`.
- `restart` needs running units for that id. In a dev run, save a file instead.
- With the gtk-mcp server available and a glimpse debug build, run the applet with that build's
  `target/debug/glimpsectl applet run --dev --isolated .`: the test panel it starts carries the
  gtk-mcp agent. `list_apps`, `snapshot`, `audit_layout` (labels cut off, children outside their
  parent, missing icons), `interact` and `capture` then show and drive what the panel built from
  the tree. A packaged glimpse is a release build, and gtk-mcp sees nothing in it.
- With the wayland-mcp server available, `screenshot` shows the chip and popover on screen and
  `click` / `key` use them with real input. Run `doctor` first.

https://alex-oleshkevich.github.io/glimpse/applets/exec/debug.md
https://alex-oleshkevich.github.io/glimpse/developers/agents.md

## Report back

Tell the user the project path, the id, how to see it (`just dev` with the panel running), how to
keep it (`just install`, then the zone edit and `glimpsectl config validate`) and how to remove
it. State which checks you ran and which you could not (for example no running panel).
