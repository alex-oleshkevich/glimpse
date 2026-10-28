# Agent skills for applet authors

| | |
| --- | --- |
| Bead | **glimpse-tumwm**; tasks carry `Source: docs/plans/applet-author-skills.md §N` |
| Sibling | **glimpse-dx371**, `docs/plans/llm-docs.md`: these skills link to its `.md` pages for reference |
| Status | Final; decisions in §4, open questions closed in §13. |
| Rule | Every claim is tagged **measured** (run on this machine at `ea2430ab`, 2026-10-28), **source** (`file:line` at `ea2430ab`) or **documented** (URL given). |

## 1. Goal

A user asks a coding agent for a glimpse applet. The agent has a skill that tells it which kind to
build, the real keys and commands, the workflow from scaffold to installed applet, and the traps,
and it links to the site's Markdown pages for full reference. Two skills:
`glimpse-command-applet` (a button in `panel.toml`) and `glimpse-exec-applet` (a program using an
SDK). They ship with glimpse, so they match the installed version. Each is evaluated against a
baseline agent without it before it ships (§10).

## 2. Current state (tree at `ea2430ab`)

| What | Where |
| --- | --- |
| Command applets: `extends = "command"`, gestures `on-click`, `on-middle-click`, `on-scroll-{up,down,left,right}` as argv lists, `env`, `inherit-env`, `icon`, `label`, `tooltip-format` | `crates/glimpse-config/src/schema/applets.rs:170-220` (`Command`) (**source**) |
| Exec applet table: `extends = "exec"`, `applet` (desktop-file id), `options`, `env` | `applets.rs:103-118` (`Exec`) (**source**) |
| A dotted zone name resolves to an exec applet | `crates/glimpse-config/README.md` "Applets" (**source**) |
| `glimpsectl applet new` renders `DATA_DIR/sdk/templates/<lang>` (or `$GLIMPSE_SDK_TEMPLATES_DIR`), `.jinja` files through minijinja, others copied | `crates/glimpsectl/src/commands/applets_author.rs:112-117`, `:144-162`, `:250-305` (**source**) |
| Templates `ts`, `py`, `rs`, `go`; each has a justfile with `dev` (`glimpsectl applet run --dev`), `test`, `install`, `package` | `sdk/templates/*/justfile.jinja`, e.g. `sdk/templates/ts/justfile.jinja` (**source**) |
| `install.sh` installs every file under `sdk/templates` to `DATA_DIR/sdk/templates` | `scripts/install.sh:74-76` (**source**) |
| SDK versions: ts, py, rs all `0.13.0` | `sdk/sdk-ts/package.json`, `sdk/sdk-py/pyproject.toml`, `sdk/sdk-rs/Cargo.toml` (**source**) |
| Published SDKs lag the tree: npm `glimpse-sdk` 0.9.0, PyPI `glimpse-applet-sdk` 0.8.0, while the ts template asks for `^0.13.0` | (**measured**) |
| Human docs: `website/applets/command/*.md`, `website/applets/exec/*.md` and `reference/*.md`, `website/developers/{exec-applets,protocol,systemd,trust,dev-mode,files}.md` | (**source**) |
| Contributor skill `.claude/skills/applet` is for built-in applets, not users | (**source**) |
| Skill discovery: Codex and Warp read `.agents/skills/`; Claude Code reads `.claude/skills/` and `~/.claude/skills/` | https://developers.openai.com/codex/skills, https://docs.warp.dev/agents/capabilities/skills, https://docs.claude.com/en/api/agent-sdk/skills (**documented**) |

## 3. Facts

- F1 A skill is a directory with `SKILL.md` (frontmatter `name`, `description`) plus optional
  `references/` (**documented**, Agent Skills spec via the links above).
- F2 The site serves a Markdown copy of every page at `<page>.md` (**measured**, see
  `docs/plans/llm-docs.md` §2).
- F3 An exec applet's own state lives under `state_dir()`; the panel runs it as a systemd unit,
  unconfined (`AGENTS.md`, `website/developers/trust.md`) (**source**).

## 4. Decisions

- D1 Names: `glimpse-command-applet`, `glimpse-exec-applet`.
- D2 Source: `sdk/skills/<name>/`. One primary path to users: `install.sh` installs them to
  `DATA_DIR/sdk/skills/<name>/`; `glimpsectl applet new` copies `glimpse-exec-applet` into the new
  project as `.agents/skills/glimpse-exec-applet/` and links `.claude/skills` to
  `../.agents/skills`. A command applet has no project; its skill is used from `DATA_DIR` (the user
  copies or links it into their agent's skill directory, which `website/applets/agents.md` says).
- D3 Content split: `SKILL.md` holds the decision (command vs exec), the workflow, the rules and
  the traps, with every key and command spelled out. Full reference stays on the site and is linked
  as `https://alex-oleshkevich.github.io/glimpse/<page>.md`; the skill never copies a reference
  table. The SDK sources in the project (`node_modules/glimpse-sdk`, the Python package, the crate)
  are the offline API reference, and the skill says where they are.
- D4 Staying true: one glimpsectl integration test, `crates/glimpsectl/tests/skills.rs`, reads both
  skills. Every fenced `toml` block is a complete `panel.toml` and is loaded with
  `glimpse_config::load_files` and checked with `unknown_applets`; every line in a fenced `sh`
  block that starts with `glimpsectl` must parse with `glimpsectl::cli::Cli::try_parse_from`. A
  removed or renamed key or subcommand fails `just verify`.
- D5 Versioning: skills ship in the same package as glimpse and the templates; no separate
  version. The SDK dependency in the templates already tracks the SDK version.
- D6 Evaluation is part of done (§10): each skill is run by fresh agents on fixed tasks, with and
  without the skill, graded on the same rubric; the skill ships only when it beats the baseline on
  every rubric line it targets and introduces no regression.

## 5. Architecture

### 5.1 Files

| File | Change |
| --- | --- |
| `sdk/skills/glimpse-command-applet/SKILL.md` | new |
| `sdk/skills/glimpse-exec-applet/SKILL.md` | new |
| `scripts/install.sh`, `scripts/package-binary.sh`, `nix/package.nix`, `crates/glimpse-package/Cargo.toml` (deb and both rpm lists) | install `sdk/skills/**` to `DATA_DIR/sdk/skills/` beside templates; `crates/glimpse-package/tests/packaging.rs` and the docker smoke scripts check it |
| `crates/glimpsectl/src/commands/applets_author.rs` | add the skill's files to the planned writes before any file is written, then create the `.claude/skills` symlink; a missing skill directory is not an error |
| `crates/glimpsectl/tests/applets_author.rs` | tests: the skill lands in a new project and matches the source; absence is tolerated |
| `crates/glimpsectl/tests/skills.rs` | new: the D4 test |
| `website/applets/agents.md` (new), `website/applets/index.md`, `website/applets/command/index.md`, `website/applets/exec/index.md`, `website/applets/exec/start.md`, `website/developers/files.md`, `website/.vitepress/config.mts` | the page on building applets with an agent, links to it, the scaffold file lists, the installed files table and the sidebar |
| `crates/glimpsectl/README.md`, `AGENTS.md` | the skill-copy rule and the `sdk/skills` entry |

### 5.2 How the pieces connect

`install.sh` puts `sdk/skills` beside `sdk/templates` under `DATA_DIR/sdk/`. `applet new` already
resolves the templates root (`template_dir`, `applets_author.rs:112-117`); the skills root is its
sibling `skills/`, so `$GLIMPSE_SDK_TEMPLATES_DIR` set to `sdk/templates` in a checkout finds
`sdk/skills` too. The skill text links to the pages `llms.txt` lists (sibling plan §5.3), and
`llms.txt` names `DATA_DIR/sdk/skills/` (sibling plan §5.4).

```rust
// applets_author.rs
const SKILL: &str = "glimpse-exec-applet";
fn skill_dir(template: &Path) -> Option<PathBuf>; // canonical template/../../skills/glimpse-exec-applet, if a directory
fn plan_skill(skill: &Path, planned: &mut Vec<(PathBuf, Vec<u8>)>) -> Result<()>;
// adds .agents/skills/glimpse-exec-applet/<file> to the planned writes, written like template files
// after the writes: symlink project/.claude/skills → ../.agents/skills
```

### 5.3 What each skill covers

`glimpse-command-applet`: when a command applet is enough; the `[applets.<name>]` table in
`panel.toml` with `extends = "command"`; gestures as argv lists with no shell (and `sh -c` when a
shell is wanted); `env`/`inherit-env`; icons (`-symbolic`, absolute path); naming rules (no dot in
the name, not a built-in name); adding the name to a zone (writing a zone replaces its default
list); `glimpsectl config validate`; `glimpsectl applet click`/`scroll` to try a gesture.

`glimpse-exec-applet`: when exec is needed; `glimpsectl applet new --language …`; project layout
from the template; the desktop entry; the element tree and indicator/popover; state and persistent
data under the state directory; options from `[applets.<name>] options`; host actions (notify,
copy, open, launch); hostile text (cap, no markup); `just dev` / `glimpsectl applet run --dev`;
`just test` with the SDK test session; `just install`; placing the id in a zone; debugging with
`glimpsectl applet inspect`/`restart` and the journal. Links to the `reference/*.md` page of the
chosen SDK.

## 6. Config

No new glimpse setting.

## 9. Flows

- Command: user asks "a button that opens my terminal and scrolls the volume" → agent loads
  `glimpse-command-applet` → writes the table and zone into `panel.toml` → `glimpsectl config
  validate` prints nothing → the panel reloads the file.
- Exec: user asks for a live applet → agent loads `glimpse-exec-applet` → `glimpsectl applet new`
  → edits the entry file → `just test` → `just dev` to see it → `just install` → adds the id to a
  zone.

## 10. Acceptance criteria and evaluation

```
AC-1  As an applet author's agent, I want the command skill, so that the button I write loads first time.
      GIVEN the skill and the task in §10.1 E1 WHEN the agent finishes THEN `glimpsectl config validate` prints nothing and the rubric passes
      Verify by: eval E1, with-skill run
AC-2  As an applet author's agent, I want the exec skill, so that the applet I scaffold builds and its tests pass.
      GIVEN the skill and task E2 WHEN the agent finishes THEN `just lint` and `just test` pass in the project and the rubric passes
      Verify by: eval E2, with-skill run
AC-3  As the maintainer, I want proof the skill helps.
      GIVEN the same task without the skill WHEN graded THEN the with-skill run scores at least as well on every rubric line and better on at least one
      Verify by: eval report attached to the bead
AC-4  (error path) A key removed from the schema fails `just verify` naming the skill file and the block.
      Verify by: the D4 test, run once with a deliberately misspelled key
AC-5  `glimpsectl applet new` writes .agents/skills/glimpse-exec-applet/SKILL.md and .claude/skills -> ../.agents/skills; without an installed skills dir it still succeeds.
      Verify by: applets_author tests
```

### 10.1 Eval tasks

- E1 (command): "Add a button to the right of my bar, before the clock, that opens `foot` on click
  and changes the volume with `wpctl` on scroll." Sandbox: empty `XDG_CONFIG_HOME`.
- E2 (exec): "Build an applet in TypeScript that shows how many days are left until a date set in
  the panel config, with a popover listing the date." Sandbox project directory; the SDK resolved
  from the tree (`sdk/sdk-ts`) because the published package lags (§2).

Rubric (each line pass/fail): validates or builds; uses only real keys/APIs; places the applet in a
zone without dropping the zone's defaults silently; no shell string where an argv list belongs (E1);
reads its option from `options` (E2); has a passing test (E2); caps external text (E2); tells the
user how to see it and how to remove it.

## 11. Slicing

1. Skill texts (§5.3) + eval (§10) — one task per skill.
2. Install and `applet new` copy (§5.1, §5.2, AC-5).
3. D4 test (AC-4).
4. Site sentences (§5.1).

## 12. Rejected

- Publishing via `npx skills add`: a second channel whose version can drift from the installed
  glimpse.
- Copying reference tables into `references/`: a third copy of facts the site and schema hold.
- Putting the skill inside each template directory: four copies in the tree.

## 13. Open questions

- ~~Which directory do agents read?~~ `.agents/skills` for Codex/Warp, `.claude/skills` for Claude
  Code; the project gets the first and a link for the second (D2).
- ~~Published SDK versions lag; does that block the skill?~~ No: the skill names the template's
  dependency; the lag is a release task, and the eval resolves the SDK from the tree.

## 14. Review record

- 2026-10-28: written with plan-precision; template, install and schema code read at `ea2430ab`;
  published SDK versions measured.
- 2026-10-28: evaluation (§10), fresh agents, same sandbox rules, with and without the skill. E3 is
  a held-out task the skill has no example for: "a Python applet counting files in ~/Downloads,
  popover with the 5 newest names, refresh every minute".

  | Task | Run | Tool calls | Web fetches | Result |
  | --- | --- | --- | --- | --- |
  | E1 command | baseline | 10 | 7 (2 were 404) | valid; zone defaults kept |
  | E1 command | skill | 7 | 0 | valid; zone defaults kept; noted the clock is in `center` |
  | E2 ts exec | baseline | 22 | 0 | lint/test pass; `center = ["clock", "countdown"]` silently drops 3 default applets; told the user to reload the panel |
  | E2 ts exec | skill | 11 | 0 | lint/test pass; zone defaults kept (task matches the skill's example) |
  | E3 py exec | baseline | 16 | 0 | lint/test pass; file names shown uncapped; only `pyproject.toml` pointed at the SDK, so `Exec=uv run applet.py` would resolve the outdated registry SDK |
  | E3 py exec | skill | 10 | 1 | lint/test pass; names capped and cleaned; both dependency declarations changed; folder made an option |

  Every rubric line was equal or better with the skill. Changes made from the runs' feedback: the
  default zones listed in the command skill, `--log error` on validate, the Python specifics
  paragraph, a valid-date test in the example. Template problems found are glimpse-160u4,
  glimpse-nkqt2 and glimpse-3fwnj.
