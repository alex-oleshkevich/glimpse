---
name: work-in-team
description: Orchestrating glimpse work across worker agents — Claude plans, briefs, reviews and merges while cheaper workers (haiku/sonnet subagents, codex, grok, pi with DeepSeek, muse, kimi) write the code; beads claiming, worktrees, batching, live testing on eDP-1 and merge cleanup. Use whenever the user asks to work on beads or an epic with agents, to spawn or dispatch workers, to "orchestrate", or says "you are orchestrator", and whenever more than one task is in flight at once.
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

Pick by complexity, cheapest that will get it right:

- **haiku** (Agent tool, `model: haiku`) — mechanical edits, a one-line config change, a rename.
- **sonnet** (Agent tool, `model: sonnet`) — normal feature and docs work, live tests.
- **codex** — larger Rust work: `codex exec -m gpt-6.1-sol -C <worktree> --sandbox workspace-write
"<brief>" < /dev/null`. Always close stdin.
- **codex astra** — planning and review, read-only: `codex exec -m gpt-6-astra --sandbox read-only
-C <worktree> -o <out.md> - < <brief.md>`. Never a builder.
- **grok**, **muse**, **kimi** — builders of the same level; spread parallel batches across them.
  Run each from the worktree, stdin closed:
  - `grok --cwd <worktree> --prompt-file <brief.md> --always-approve`
  - `env -C <worktree> muse exec --prompt-file <brief.md>`
  - `env -C <worktree> kimi -p "$(cat <brief.md>)"`. `-p` already edits and runs commands; it
    rejects `-y` and `--auto`. Never add a large MCP server (such as rea) to `~/.kimi-code/mcp.json`:
    its tools overflow Moonshot's schema limit (400 "schema exceeds maximum allowed size").
- **pi with DeepSeek** — fact analysis: `pi -p --no-session --model
"openrouter/~deepseek/deepseek-flash-latest" "<question>"`. Live tests: `pi -p --no-session -ne
--no-mcp -ns -np --model openrouter/deepseek/deepseek-v4-flash-latest "<brief>"`.
- **muse** is an executor only: it builds, never plans or reviews. It is the first fallback when
  another builder CLI is broken or out of credits.
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
criteria written (including error paths). Size it by `spec-precision`'s tiers: a trivial bead
needs only its file and verify command. The brief then carries:

- The bead, pasted verbatim from `bd show <id>`. Criteria are never rewritten in the brief; a
  missing one goes onto the bead first.
- First instruction: claim it with `env -C /home/alex/projects/glimpse bd update <id> --claim`.
  Last: append notes with `bd update <id> --append-notes`.
- The worktree path to work in and the branch it is on.
- For normal and risky beads: the architecture, structs, interfaces and file:line links the work
  builds on, so the worker writes best-in-class code instead of rediscovering the tree.
- The repo rules that bite: `just` not raw `cargo`, no comments, tests in a sibling `<module>/tests.rs`
  behind `#[cfg(test)] mod tests;` never inline, `timeout` around every command,
  `env -C` instead of `cd`, never touch `~/.config/glimpse` or the main checkout.
- What to report back, and in what shape.

## Batching and parallelism

- Every task gets a fresh worker session; never hand a second task to a session that finished one.
- A worker session never grows past 300k tokens of context. A fix round may resume the same session
  while it is under that; past it, spawn a fresh worker with the bead and the review findings.
- Batch similar tasks into one worktree, one session per task, then build and test the batch once.
- Run independent batches in parallel; never chain what could run side by side.
- When several workers share a worktree, they only edit and run no builds (they share one cargo
  lock). Claude integrates shared files, runs the checks once, and routes failures back.
- Each worktree has its own `target/`; worktrees never share `CARGO_TARGET_DIR`.

## Beads

- Put the current tasks under one epic and claim the epic.
- Workers claim children; Claude closes a child only after its own review and a passing live test.
- Fix everything found in the same pass. Never file a follow-up bead for a finding unless the user
  asks for one.
- Put review findings into the bead so compacted session can get full context from it.

## Worktrees and merging

- All work happens in worktrees under `.worktrees/<name>`, created from local `master`. The main
  checkout stays on `master` and is never edited.
- Commit only when the user asks for that. User want to review the code first.
- Merge only when the user approves. Merge into `master`; never create another base branch.
- Always close merged branches and worktrees: right after a merge, remove its worktree and delete
  its branch with `git branch -d`, then check `git branch --merged master` for any other leftover.
  A branch with no commits of its own but uncommitted changes in its worktree is work in progress,
  not merged; leave it.
- When a branch changes behavior users see, update the same pages in every open docs branch in that
  pass.

## Verification

- Live-test finished work before reporting it done or asking to merge: nested niri and an isolated
  test panel, per `docs/live-testing.md`. Unit tests and review are not enough.
- All GUI testing goes on eDP-1, never DP-2. Move the nested niri window and test panels there right
  after they start.
- No keyboard or mouse injection on the user's session.
- A test that writes the clipboard first backs up every offered mime type and restores the richest.

## Before reporting back

- [ ] Every diff reviewed by Claude, every finding fixed, not deferred.
- [ ] Each touched crate's checks pass in its worktree; live test passed on eDP-1.
- [ ] Beads have notes; children Claude reviewed are closed.
- [ ] Nothing merged without approval; every merged worktree and branch removed
  (`git worktree list`, `git branch --merged master`).
