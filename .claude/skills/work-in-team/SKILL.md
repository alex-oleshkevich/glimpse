---
name: work-in-team
description: Orchestrate glimpse work across worker agents: plan, brief, review, merge while cheaper workers code. Use whenever the user wants beads or an epic worked with agents, to spawn or dispatch workers, "orchestrate", or more than one task is in flight.
---

# work-in-team

Claude is the orchestrator: it plans, writes briefs, reviews every diff and decides. Workers write
the code. Claude's context is the scarce resource — spend it on briefs, review and decisions, never
on typing code or grepping for facts.

## Roles

- Claude writes no production code. A fix found in review goes back to the worker that owns it.
  The one exception is a delta of a line or two, which Claude may fix itself.
- Claude owns every review verdict: read the diff, adjudicate, relay fixes. The only reviewer
  Claude may spawn is codex astra, which also drafts plans; Claude still reads the diff and decides.
- Writing docs (new prose, a page or section rewrite) goes to an opus subagent (Agent tool,
  `model: opus`) whose brief tells it to load the `writing-documentation-with-diataxis` and `docs`
  skills first. Every page has one Diataxis type. Mechanical docs work (formatting samples,
  applying listed fixes, removing sections, repointing links, running lint and builds) goes to
  sonnet or haiku, never opus.
- Shell fact-finding (grep, sed, reading source to answer a question) goes codegraph, and if not available, to pi, so the answer
  lands in Claude's context without the file dumps.

## Workers

Live tests, shell fact-finding and unit-test runs go to sonnet, haiku, luna or pi with DeepSeek,
never opus, and never Claude itself. Subagents get Ashkenazi names (abram, rabi, ...). At most 6
workers are live at once. When the live-test queue is long, spread it over up to five
Hetzner servers (`hcloud`, 4 cores, 8 GB) and delete every one when the queue is done.

Pick by complexity, cheapest that will get it right:

- **haiku** (Agent tool, `model: haiku`) — mechanical edits, a one-line config change, a rename.
- **sonnet** (Agent tool, `model: sonnet`) — normal feature and docs work, live tests.
- **codex** — larger Rust work.
- **codex astra** — planning and review, read-only. Never a builder.
- **grok**, **muse**, **kimi** — builders of the same level; spread parallel batches across them. muse only builds.
- **pi with DeepSeek** — builds, live tests and fact analysis. A 403 "Key limit exceeded" means pi is out until the monthly OpenRouter cap resets.
- Launch a CLI worker (pi, gridr, codex, kimi) as one background tool call that runs the CLI in the
  foreground of that call, or `setsid <cli ...> < /dev/null > <out> 2>&1 &` and confirm within 30 s
  that the pid is alive or the output file is non-empty; never a nested `( ... ) &`.
- Command lines and quirks per CLI: `${CLAUDE_SKILL_DIR}/references/workers.md` (codex, codex astra, gridr,
  grok, muse, kimi, pi build form against live-test form).
- Briefs, outputs, scripts and the spend log live in the main checkout's `var/orchestration/<epic>/`
  (a worktree is removed at merge), never the harness
  scratchpad (Claude Code deletes it). Log OpenRouter spend for pi DeepSeek per run.
- When a planning or review CLI (codex astra) is unavailable, a Claude subagent takes that role.
  When any CLI is broken or out of credits, say so.
- Workers economize too: they hand simple steps (running tests, reading or grepping files) to a
  cheaper agent (a haiku subagent or pi) and navigate with the codegraph CLI, never grep/Read
  loops: it saves most of their tokens. Each worktree gets its own index (`codegraph init --yes
  <worktree>` once; the orchestrator does it before dispatch), and workers run `codegraph explore
  "<symbols or question>"`, `codegraph node <symbol|file>`, `codegraph callers|impact <symbol>`
  from the worktree root, with `codegraph sync` after their edits. Every brief says this.

## Briefs

A worker has none of Claude's context and must not guess; it only codes. Before dispatch, make the
bead self-contained: every decision settled, files and symbols named and checked, acceptance
criteria written (including error paths). Dry-run every grep/rg acceptance pattern against the
base tree and the expected result before dispatch; a pattern that matches by accident is a spec
defect. Size it by `spec-precision`'s tiers: a trivial bead
needs only its file and verify command. The brief then carries:

- The bead, pasted verbatim from `bd show <id>`. Criteria are never rewritten in the brief; a
  missing one goes onto the bead first.
- The worker does not claim. Its last step is `bd update <id> --append-notes`.
- The worktree path to work in and the branch it is on.
- For normal and risky beads: the architecture, structs, interfaces and file:line links the work
  builds on, so the worker writes best-in-class code instead of rediscovering the tree.
- The repo rules that bite: `just` not raw `cargo`, no comments, tests in a sibling `<module>/tests.rs`
  behind `#[cfg(test)] mod tests;` never inline, `timeout` around every command,
  `env -C` instead of `cd`, never touch `~/.config/glimpse` or the main checkout.
- What to report back, and in what shape. A live-tester brief states whether the tester can read
  images; if not, it asserts on pixels (`magick ... txt:`), the accessibility tree or log lines.

Sample worker brief:

```
Bead glimpse-abc.3 (pasted verbatim below). The orchestrator already claimed this; do not claim.
Worktree: /home/alex/projects/glimpse/.claude/worktrees/<epic>, branch <epic>. Work only there.
Files: crates/glimpse-foo/src/bar.rs, crates/glimpse-foo/src/bar/tests.rs.
Verify: `timeout 900 just check-crate glimpse-foo`, `timeout 900 just test-crate glimpse-foo`.
Navigate with `codegraph explore "<symbols>"` from the worktree root; `codegraph sync` after edits.
Never open a window: GTK tests run on broadway. Never touch ~/.config/glimpse or the main checkout.
Write no comments. Tests go in the sibling tests.rs.
Report: files changed, verify results, anything you could not follow.
Last step: `bd update glimpse-abc.3 --append-notes "<what changed, verify results>"`.
<bd show output>
```

Sample WORKER bead note:

```
WORKER: abram | pi, openrouter/deepseek/deepseek-v4-flash-latest | pid 48213
brief: var/orchestration/<epic>/brief-abc3.md
output: var/orchestration/<epic>/out-abc3.md
resume: re-run the same command with the findings appended to the brief (fresh session under 300k tokens).
```

## Batching and parallelism

- Every task gets a fresh worker session; never hand a second task to a session that finished one.
- A worker session never grows past 300k tokens of context. A fix round may resume the same session
  while it is under that; past it, spawn a fresh worker with the bead and the review findings.
- Batch similar tasks into one worktree, one session per task, then build and test the batch once.
- Run independent batches in parallel; never chain what could run side by side.
- When several workers share a worktree, each edits only its own crate's files and runs only
  crate-scoped `just check/test/lint-crate`; a failure in a file it did not touch means a sibling is
  mid-edit, so wait and retry.
- Each worktree has its own `target/`; worktrees never share `CARGO_TARGET_DIR`.

## Beads

- Put the current tasks under one epic and claim the epic.
- The orchestrator claims each child at dispatch with assignee = the worker's name (retry while bd
  is locked) and appends a `WORKER:` note (name, tool and model, agentId or CLI session, brief and
  output paths, how to resume); the epic gets one `ORCHESTRATOR SESSION` note. Claude closes a child only after its own review and a passing live test, which may be one epic-level live acceptance task when beads share a surface.
- Fix everything found in the same pass. Never file a follow-up bead for a finding unless the user
  asks for one.
- Put review findings into the bead so compacted session can get full context from it.

## Worktrees and merging

- All work happens in worktrees under `.claude/worktrees/<name>`, created from local `master`. The main
  checkout stays on `master` and is never edited.
- Commit only when the user asks for that. User want to review the code first.
- Merge only when the user approves. Merge into `master` as one squashed commit (`git merge --squash
  <branch>`, then one commit); never create another base branch.
- Always close merged branches and worktrees: right after a merge, remove its worktree and delete
  its branch with `git branch -D` (a squashed branch never shows as merged), then check
  `git worktree list` for any other leftover.
  A branch with no commits of its own but uncommitted changes in its worktree is work in progress,
  not merged; leave it.
- When a branch changes behavior users see, update the same pages in every open docs branch in that
  pass.

## Verification

- Live-test finished work before reporting it done or asking to merge, per `docs/live-testing.md`.
  Unit tests and review are not enough.
- Builders and GTK tests never open a window: GTK runs on broadway (`timeout 1500 gtk4-broadwayd
  :<n> & BP=$!; trap 'kill $BP' EXIT` in the same shell command, long-lived, never `timeout 20`). A live tester may run one nested niri whose window is
  moved to eDP-1 immediately, never DP-2. Anything needing real outputs (gamma, multi-monitor) runs
  in the `var/vm` QEMU box.
- Inject input only with the nested `WAYLAND_DISPLAY` set in the same command, never on the user's
  session.
- A test that writes the clipboard first backs up every offered mime type and restores the richest.

## Before reporting back

- [ ] Every diff reviewed by Claude, every finding fixed, not deferred.
- [ ] Each touched crate's checks pass in its worktree; live test passed.
- [ ] Beads have notes; children Claude reviewed and live-tested (or covered by the epic-level acceptance) are closed.
- [ ] Nothing merged without approval; every merged worktree and branch removed
  (`git worktree list`, `git branch`).
