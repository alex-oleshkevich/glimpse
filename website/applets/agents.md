# Build an applet with a coding agent

glimpse ships two agent skills, so a coding agent such as Claude Code or Codex can write an applet
for you with the real settings, commands and SDK calls instead of guessing them. One skill writes
command buttons, the other builds exec applets. [Choosing an applet kind](/applets/) explains the
difference.

| Skill | What the agent does with it |
| --- | --- |
| `glimpse-command-applet` | Adds a button to `panel.toml` that runs a program on click or scroll, keeps the other applets in that zone, and checks the file with `glimpsectl config validate`. |
| `glimpse-exec-applet` | Creates a project with `glimpsectl applet new`, writes the applet and its tests in TypeScript, Python, Rust or Go, runs `just lint` and `just test`, and adds it to a panel zone. |

Both live in `/usr/share/glimpse/sdk/skills/`. They come with glimpse, so they always match the
version you have installed.

## Build an exec applet

Create the project first:

```sh
glimpsectl applet new
```

The new project holds the `glimpse-exec-applet` skill in `.agents/skills/`, which Codex and other
agents read, and `.claude/skills` links to the same folder for Claude Code. Open your agent in the
project directory and describe the applet:

```text
Show how many days are left until a date I set in the panel config,
with a popover listing the date.
```

The agent edits the applet, adds tests and runs them. It leaves `just dev` for you to run and installs
the applet only when you ask. Try the result with `just dev` while your panel runs, then keep it with
`just install`. [Develop with live reload](/applets/exec/live-reload) covers `just dev`.

## Add a command button

A command button has no project, so make the skill available to your agent once. Copy the folder
into the skills folder your agent reads, for example for Claude Code:

```sh
mkdir -p ~/.claude/skills
cp -r /usr/share/glimpse/sdk/skills/glimpse-command-applet ~/.claude/skills/
```

Codex reads `~/.agents/skills/` instead. Then ask for the button:

```text
Add a button before the clock that opens foot on click
and changes the volume with wpctl on scroll.
```

The agent writes the button into `~/.config/glimpse/panel.toml`, and the panel picks it up when the
file is saved. To undo it, delete the `[applets.<name>]` table and its name from the zone.

A copied skill does not update with glimpse. Copy it again after an upgrade, or link the folder
instead of copying it.

## What the agent reads

The skills hold the workflow, the rules and the common mistakes. For everything else they point the
agent at the Markdown version of these pages, such as
`https://alex-oleshkevich.github.io/glimpse/applets/exec/reference/elements.md`, and at the SDK
source inside the project. glimpse's test suite loads every configuration example in the skills and
parses every `glimpsectl` command in their shell examples, so a renamed setting or command cannot
ship unnoticed.

To let the agent look at the applet running in a panel, click it and take pictures of its popover,
see [Let a coding agent see glimpse](/developers/agents).
