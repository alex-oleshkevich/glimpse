---
name: docs
description: Writing and reviewing pages for the glimpse documentation site in website/ — the voice, tone and style the site is written in, which audience each section serves, how every fact is checked against the code, the components and screenshots a page may use, and the lint, build and review loop a page passes before it is done. Use for any new or changed page under website/, for a docs brief handed to another agent, and for reviewing docs someone else wrote. Trigger on the location, not the wording.
---

# docs

The site is a VitePress project in `website/`, served at `/glimpse/`. `docs/` is agent
documentation and is not part of the site.

## The voice

The site talks to one person who wants something done on their desktop. Write the way a friend who
knows the code would explain it across a table: direct, plain, specific, never selling.

- **Second person, present tense, active voice.** "The history holds the last 100 notifications",
  not "Notifications will be retained by the history service".
- **Say what happens, with the real number.** "Popups stay for 4 seconds", not "popups are shown
  briefly". A vague sentence is usually a fact you have not looked up yet.
- **One idea per sentence; short paragraphs.** A paragraph longer than five lines is two paragraphs
  or a list.
- **Give the reason in one clause when it changes what the reader does.** "Writing a zone replaces
  its whole default list, so keep the names you want to stay on the bar." No reason for its own sake.
- **Lead with the outcome.** Headings in guides are what the reader wants ("Keep more history",
  "Move the bar to another edge"), not the mechanism ("The `keep` setting").
- **No hedging, no hype, no apology.** Not "simply", "just", "easily", "powerful", "seamless". If a
  step needs an apology, fix the step.
- **Warn only about real damage.** A `::: warning` is for losing data, a locked-out session or a
  stopped lock screen. Everything else is plain prose or a `::: tip`.

Calibration, from the site:

> The bar shows the time, your workspaces, what is playing, and the state of your Wi-Fi, Bluetooth,
> sound and battery. Click any of them and a small panel opens with the controls you need.

> Without `--until` it stands until you turn it off. Popups are held back while the history keeps
> the notifications for you.

Rewrite, not:

> glimpse provides a powerful, seamless notification experience that lets you effortlessly manage
> your incoming alerts.

## Audience decides the register

| Section | Reader | Register |
| --- | --- | --- |
| `start/`, `guide/` | Someone using a desktop, not reading code | Everyday words. "The bar", "a small panel opens", "your compositor". No D-Bus, units, crates or internals unless the step needs a command. |
| `config/` | Someone editing `config.toml` | Precise and complete. Generated tables (`<ConfigTable>`) carry the settings; prose explains what tables cannot. |
| `applets/` | Someone building an applet | A tutorial that works start to finish, then task pages, then reference. Every code sample type-checks against `sdk/`. |
| `developers/`, `contribute/` | Contributors | Exact names: binaries, bus names, units, files. Explanation of why the design is the way it is. |

Each page is one kind: a tutorial (learning by doing, one path, every step works), a how-to (steps to
a goal, conditionals allowed), reference (austere facts) or explanation (why and how it fits). Do not
mix them and do not label them.

## Shape of a page

- `# Title` in sentence case, then a lead paragraph of one to three sentences: what the page gets
  you, and a link to the reference page that lists every setting.
- `##` sections in the order the reader needs them. No "Introduction", "Overview" or "Conclusion",
  no closing send-off.
- Link instead of repeating: settings live in `/config/…`, command options in `/config/cli/…`,
  screenshot tools in `/ecosystem/`. Absolute site links without `.md`.
- A TOML sample shows only the keys that matter. When it writes a list that replaces a default
  (panel zones, `[[panels]]`), show the full default list with the change, or say what it removes.

## Mechanics the linter enforces

`website/scripts/lint.mjs` runs inside `npm run build`; `cd website && npm run -s lint` prints
nothing when a page is clean.

- US English. No em or en dashes: use a period, comma, colon or parentheses. Straight quotes only.
- Sentence-case headings. No emojis. No bold-label bullet lists.
- The words niri and Hyprland appear only in code. Prose says "your compositor"; a page that
  differs per compositor uses the `<Compositors />` badge.
- Real examples only: no foo, bar, hello world, Pomodoro, `example.com`, `me.example.*`. The example
  applet id is `me.aresa.GlimpseTodo` (`sdk/examples/todo`).
- There is one version: the current one. No history, no earlier versions, nothing "removed",
  "renamed", "no longer" or "since 0.x"; removed features such as the exec-applet sandbox
  (`[sandbox]`, Landlock, `unconfined`) and the `[power]` table never appear.

## Every fact comes from the tree

A setting name, default, range, command, flag, limit, path, unit name or behavior is written only
after it is read in the source. If it cannot be confirmed, it is left out.

| Fact | Source |
| --- | --- |
| Settings, defaults, ranges, descriptions | `data/config.schema.json`, `crates/glimpse-config/src/schema/` |
| Commands and flags | `target/debug/<binary> --help`, the generated `website/config/cli/*.md` |
| Units and bus names | `data/systemd/`, `data/dbus-1/services/` |
| Design rules | the crate `README.md` files |
| Applet SDK and wire format | `sdk/applet/`, `crates/glimpse-services/src/services/exec/`, `crates/glimpse-panel/src/applets/exec/` |
| UI strings the reader will see | the `_("…")` strings in the UI crates |

Validate every complete TOML sample:
`env -u GLIMPSE_CONFIG_PATH target/debug/glimpsectl config validate <file>`. A fragment is validated
assembled into the file it belongs to.

Never use `_old/` or `var/glimpse2` as a source.

## Components

Registered in `website/.vitepress/theme/`:

- `<ConfigTable path="notifications" />`: the settings table for a schema path, generated from the
  schema. Add `describe` to show the section description.
- `<AppletList />`, `<Compositors />`.
- `<AppletShot name="audio" />`: a collapsed light and dark screenshot of an applet's popover.
- `<ThemeImage light="…" dark="…" alt="…" />`: any image with light and dark variants. Every image
  has alt text saying what is on it.
- Mermaid diagrams in fenced `mermaid` blocks; `::: tip`, `::: warning`, `::: code-group`.

## Screenshots

Screenshots come only from `just docs-screenshots` and the home page video only from
`just docs-video`; both run `scripts/docs-screenshots.py`: a
nested compositor, private session and system buses, mocked NetworkManager, BlueZ, UPower, logind
and UDisks2, fake media players, calendar events and notifications, and a scratch home. Never capture
the real desktop. A real screen shows names, networks, devices, places and messages that are not
the project's to publish.

Before an image goes in, look at it: no real person, place, network, device, file or account, and no
window from the host. New mock data is invented and plausible ("Lighthouse", "AirPods Pro",
"Lunch with Maya").

## Owning the sidebar

The nav and sidebar in `website/.vitepress/config.mts` are edited by whoever integrates pages. A
brief that delegates pages tells the writer not to touch `.vitepress/` and to report the sidebar
entries it wants.

## Done means

1. `cd website && npm run -s lint` prints nothing.
2. `just docs-build` passes; it fails on any dead link.
3. The page renders in a browser in light and dark with no console errors and no broken image.
   An HTTP 200 alone proves nothing.
4. A reviewer who did not write the page checks every factual claim against the code and reports
   each problem with the file and line that contradicts it. The `adversarial-review` skill shapes
   that brief. Fix every finding, including the ones on pages you did not write.
5. A delegated writer's final report lists its files and, for each page, its key claims with the
   `file:line` each came from.
