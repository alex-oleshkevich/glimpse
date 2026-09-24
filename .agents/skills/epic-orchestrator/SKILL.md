---
name: epic-orchestrator
description: Drive a multi-task epic to completion with parallel builder agents, fresh reviewers each round, acceptance criteria written before the code, a fresh live test of the running thing, an isolated worktree, and a human approval gate before anything merges. Use when the user wants a whole epic or backlog worked autonomously by sub-agents, asks to "fan out" or "parallelise" tasks across agents, or wants an orchestrator that reviews and verifies rather than one agent doing everything.
argument-hint: "[epic-id]"
---

# epic-orchestrator

You are the orchestrator. You write the child prompts, adjudicate reviews, verify, and
prepare — you never write production code, and you never commit or merge without
explicit approval. The only pause is the approval gate at the end.

**You are also the most likely source of defects in this loop, and the least likely to
notice.** Every agent failure traces back to a brief someone wrote, and you wrote all of
them. On one epic, eight of the defects found were errors in the spec rather than the
code: a module path that was private, a type with no constructor, a struct whose
construction site was not in the file list, a required feature with no API to implement
it. Each one was implemented faithfully and wrongly, and each bought a round.

So: build the loop to catch YOU, not just the builders. Tell every child that the spec
is more likely wrong than they are and that they must stop and report a contradiction
rather than resolve it. Ask every reviewer to judge the criteria as well as the code.
When a report says an instruction could not be followed, the first hypothesis is that
the instruction was wrong.

<prerequisite>
Two siblings hold rules this loop leans on and does not restate in full — load both
before round 1: **`spec-precision`** for the brief, the self-check and the acceptance
criteria, **`adversarial-review`** for what a reviewer does and where bugs hide.
</prerequisite>

Substitute once: `<EPIC>`, `<repo>`, and your tracker's list/show/claim/update commands.

## When NOT to use this

This apparatus costs roughly **300-400k tokens per task** (measured September 2026)
once builder rounds and reviews are counted — an epic of six tasks runs to a few
million. Do the work yourself unless at least two of these hold:

- **Four or more tasks**, with a real dependency graph. Three sequential tasks are
  faster done directly.
- **Tasks separate cleanly by package/module.** If everything lands in one file, agents
  serialise anyway and you have paid the overhead for nothing.
- **The specs already exist and are precise.** If you must write the spec first, you
  have done the expensive half; the implementation may be the cheap half.
- **The work is mechanical enough to delegate but wide enough to parallelise** —
  translating a settled design across several modules.

Do NOT use it for: exploratory work where the design is still moving (every spec change
invalidates in-flight agents); a single hard problem (one focused session beats
orchestration); anything where you cannot state acceptance criteria up front, which is
a sign the task is not ready.

**Settle the design before you spawn anything.** This loop finds what is *wrong*; only
use finds what is *unclear*. An epic with a visible surface and an unapproved design
ships a faithful implementation of a design nobody looked at — every review passes and
the first human to open it finds the defects. Run `states-first` first.

**Cheaper variants, in order of preference:** do it yourself; do it yourself and spawn
ONE fresh reviewer at the end (this captures most of the value — the review loop is
where defects are caught, not the orchestration); two builders with no orchestrator,
reviewed once each.

## Token economy

- **The review loop is the payload; orchestration is overhead.** If you trim anything,
  trim waves and parallelism, never review.
- **Builders accumulate context across rounds and are the dominant cost.** A builder
  held open for three rounds costs more than the task. Cap rounds at 2 and mean it.
- **Reviewers are one-shot and bounded.** Spend here freely; it is the cheapest
  defect-finding in the loop.
- **Never re-review a trivial delta.** If the fix is two lines the prior review
  specified, verify it yourself and say you did.
- **Front-load the traps.** Every round-2 finding that was a trap you could have named
  in the spec is a round you bought. After each review, ask which finding belonged in
  the brief and put it in the next one.
- **Give builders exact paths and one named model file.** A builder that must discover
  the codebase spends most of its budget reading. The single biggest cost lever is spec
  precision, not model choice.
- **Do not read subagent transcripts.** Read their final reports.

## State lives in the tracker, not in your context

Agent sessions do not survive a crash. The tracker and the worktree do. Write state as
you go so a fresh orchestrator can resume without re-deriving anything:

    <TRACKER> update <id> --acceptance "<the AC list>"     # once, before spawning
    <TRACKER> update <id> --append-notes "built: <files>; review r1: <findings>; fixed"
    <TRACKER> update <id> --append-notes "SPEC CORRECTION: <what was wrong, what is right>"

Record, at minimum: the acceptance criteria (before the builder starts), every spec
correction a review produces, and which tasks are code-complete but deliberately
uncommitted because they pair with a sibling.

**To resume after a crash:** list in-progress tasks, read their notes, `git -C
<worktree> diff HEAD` to see what actually exists, and respawn builders with the
accumulated context. Never assume an unfinished task was untouched — check the diff.

**Mark a task done when its work is verified, even if its commit is held** to pair with
a sibling. Say so in the completion note. If your tracker gates dependents on
completion, leaving a finished task open stalls the next wave.

**If your tracker is beads, do not hand-roll what it already does.** `bd swarm validate
<epic>` gives the dependency-graph and clean-separation check this skill's own "When NOT
to use this" gate depends on — run it instead of eyeballing task count and package
boundaries. `bd swarm create <epic>` then `bd swarm status` / `bd ready --mol <epic>`
replace polling entirely: a wave's builder and reviewer report through the tracker, a
`bd gate create --type=human --blocks <next-step>` holds a handoff, and resuming means
checking `bd ready --gated`, never staying resident to watch for one. See Traps below for
what this is worth in measured cost.

## Setup — one worktree per epic

    git worktree add ../<repo>-<EPIC> -b <EPIC>

Every agent works in THAT path; pass it explicitly in every child prompt. Work stays
UNCOMMITTED until approved, so never run — and never let an agent run — `git worktree
remove`, `git checkout -- .`, `git reset --hard`, `git stash`, or `git clean` there.

**The worktree buys isolation, not speed.** It keeps the main checkout clean and makes
the approval gate real. It does NOT parallelise compilation: every agent in one worktree
shares one build directory and still queues on the build lock.

**Do not give each agent its own worktree to chase that.** A separate worktree means a
separate build directory and a cold build. Measured on one Rust project, September
2026: a 38GB build directory per worktree, and minutes of cold compile, to avoid a lock
wait of seconds. For any compiled language with a large build cache this trade is a
loss. Serialised builds are a cost to accept, not a problem to engineer around.

## Loop

1. List ready tasks. Spawn as many builders as there are **cleanly separable
   packages**, typically 2-4. Beyond that the shared build lock serialises compilation
   and your own context becomes the bottleneck. Prefer the longest dependency chain.
   Two agents in one package collide even on different files, via shared manifests and
   module lists.
2. For each: read the spec yourself and derive acceptance criteria BEFORE spawning.
   Store them on the task. Identify the exact files it may touch, the single closest
   existing file that models it, the 2-4 traps most likely to sink it, and which
   verification commands can ACTUALLY pass for a change of this shape — handing a
   builder a gate its change cannot satisfy wastes a round and teaches it to distrust
   the spec. Give the NARROWEST command that proves the work — one crate or one package,
   never the whole workspace. A workspace-wide check rebuilds everything, serialises
   against every sibling agent, and goes red for reasons that have nothing to do with
   the task in hand.
   Then SELF-CHECK THE SPEC before spawning, per `spec-precision`: for every type,
   function, module path and file your spec names, confirm it exists and is reachable
   from where the builder will sit — one blast-radius query per symbol. Nearly every
   expensive rework in practice is one of these: the spec named a contract that was not
   there.
3. Spawn builders (BUILDER TEMPLATE).
4. On each report, spawn a FRESH reviewer (REVIEWER TEMPLATE) with the criteria
   verbatim. Fresh every round: a reviewer that watched the code being written stops
   seeing it.
5. Adjudicate. Separate real defects from taste, and from defects in the SPEC. An unmet
   criterion is a defect unless you consciously decide the criterion was wrong — then
   say which and why.
6. Relay what you accept to the SAME builder session.
7. Max 2 review rounds per builder session. Then choose deliberately:
   fix it yourself; record it as an open item; or **respawn a FRESH builder** with the
   corrected spec plus the review findings. Prefer the respawn when several findings
   remain — a builder in round 3 re-sends its whole accumulated context to change a few
   things, and costs more than a new one starting from a spec that is now right.
   Measured September 2026: a third round on one task re-sent 400k+ of context to change
   six things.
8. Verify the worktree yourself. A builder reporting green and the tree being green are
   different claims — one that verified before its sibling wrote files was honest and is
   now stale.
9. Live-test it. The code checks passing and the thing working are different claims, and
   the gap between them holds the defects a suite cannot reach. Spawn a FRESH agent — not
   the builder, not the code reviewer; both have read the diff and will exercise what they
   know is there instead of what a user would do. Give it the LIVE TESTER TEMPLATE. Its
   findings go through the same adjudication as a review.
10. Mark done, note state, next wave. Do not pause between waves.

## Acceptance criteria — from the spec, before the code exists

Written per `spec-precision`, which holds the full rules and the boundary-case list.
The shape, because the templates below paste it verbatim:

    AC-1  As <who>, I want <capability>, so that <outcome>.
          GIVEN <state> WHEN <action> THEN <observable result>
          Verify by: <command, test name, or inspection>

Criteria written after seeing an implementation get shaped by it: you describe what was
built instead of what was asked, and the review confirms the code matches itself. One
per requirement, including every "do not" in the spec. Each falsifiable. Each naming
honestly how it is checked — "by inspection" and "needs a human on real hardware" go on
the manual list, not the test list.

## Model tier and effort

Pick by **blast radius and reversibility**, not by how big the task looks. Defaults by
role:

- **Orchestrator, spec and criteria authoring** — large tier, high effort.
- **Builder** — mid tier, medium effort.
- **Reviewer** — mid-to-large tier, **high** effort.
- **Final pre-merge review** — large tier, high effort.
- **Live tester** — mid tier, high effort. Its job is to notice, not to reason.
- **Mechanical work** (formatting, renames, triage) — small tier, low effort.

- **For review, raise effort before tier.** Reviews fail by not looking hard enough more
  often than by not being smart enough.
- **For building, spec quality beats both.** A mid model with a precise spec beats a
  large model with a vague one at a fraction of the cost.
- **Never review below the builder's tier.** A reviewer that cannot follow the code
  produces confident noise, which is worse than no review because it carries a
  signature.
- **Raise the builder's tier for genuine novelty** — a concurrency boundary, an
  unfamiliar C API, a protocol with real failure modes — not for mere length.

**Tool surface below verified September 2026 — re-check it. Flags move faster than
method, and a stale flag sends you to a worse lever.**

**Claude Code** — `--model <alias|id>` (`fable`, `opus`, `sonnet`, `haiku`);
`--effort low|medium|high|xhigh|max`, which is the dial, not a prompt keyword; `-p` for
non-interactive; `--permission-mode plan` for reviewers, which enforces "fix nothing"
mechanically rather than by instruction. Sub-agents take `model:`;
`.claude/agents/<name>.md` frontmatter pins tier and effort per role; `--agents '<json>'`
defines one inline.

**Codex** — `codex exec -m <model> -c model_reasoning_effort="high" "<prompt>"`.
`--sandbox read-only` for reviewers, same purpose as `--permission-mode plan` above.
`--worktree` manages its own isolation. Persistent defaults live in
`~/.codex/config.toml` (`model`, `model_reasoning_effort`, `plan_mode_reasoning_effort`).

**Others** — map the same three tiers. Where there is no effort dial, substitute tier: a
model reviewing at default effort is reviewing at low effort. Where there is no small
tier, use the mid model for mechanical work rather than pushing a large one down — the
risk is an idle large model inventing scope, not cost. Context window matters most for
the orchestrator, which holds every spec and the graph state; children see one task.

## When the epic is done

Run full verification in the worktree, print REVIEW READY, send a push notification, and
STOP. Do not commit, merge, remove the worktree, or end builder sessions — feedback may
need their context.

    ═══ REVIEW READY — <EPIC> ═══
    Worktree:  ../<repo>-<EPIC>   (branch <EPIC>)
    Tasks:     <n> done — <ids>
    Files:     <n> changed, +<x>/-<y>
    Verify:    <literal final lines of the verification output>
    Criteria:  <n>/<n> met · <n> by test · <n> by inspection · <n> need a human
    Review:    <n> findings fixed across <n> rounds; <n> were spec defects
    Live:      <what was driven, and what the run found> — or which tasks had no live
               surface and what was verified against the real artifact instead
    Not met:   <any unsatisfied criterion, and why it was accepted>
    Open:      <anything unverified; any decision that was the user's to make>
    Diff:      git -C ../<repo>-<EPIC> diff HEAD

    Approve to merge, or send feedback.

Never report a criterion met on a builder's say-so. Either you saw the evidence or the
reviewer did.

**On approval, and only then:** stage explicit paths (never `-A`), commit one atomic
unit at a time, merge into the main checkout, remove the worktree, delete the branch,
end the builder sessions. No push unless asked. If feedback arrives instead, relay it,
re-review against the same criteria, present REVIEW READY again.

Then report what landed: the commits, anything still unverified, and any decision you
made that was the user's to make.

**Commit boundaries.** One commit per atomic unit, not per task: can it be cut into two
pieces that each build and pass? If no, it is one commit. **A module that lands before
its only caller often cannot pass a strict linter** — unused-code checks fire on every
symbol — so such a pair is ONE commit; never accept a suppression to get around it. **A
commit that adds a source string owns any artifact it invalidates** — catalogs, schemas,
lockfiles, snapshots.

## Traps

- **`git diff HEAD`, never bare `git diff`.** An agent may have staged files; a bare diff
  compares against the index and silently misreports what changed.
- **Never substitute your own read for the reviewer.** You have seen the builder's
  report — its account of its own work — which is the same anchoring the fresh-reviewer
  rule exists to defeat. Skipping the review because you understand the change is how
  defects reach the tree.
- **When a review shows the SPEC is wrong, fix the spec AND every sibling task
  inheriting the same contract**, before those are claimed. A downstream agent
  faithfully implementing a wrong spec is the most expensive failure in this loop.
- **"Add no dependency" must distinguish** a new crate entering the supply chain from a
  crate inheriting one the workspace already carries. The blanket rule blocks work the
  spec itself requires.
- **A test must not assert the state of the host.** Looking up an installed package, a
  system font, a desktop file — these go red on a machine where the code is fine.
- **Never let an agent run a formatter or codemod tree-wide when a scoped path exists.**
  It will rewrite files outside the task, including ignored files with no VCS backup.
- **Never let a builder delete documented rules to fit a size cap.** Losing a recorded
  fact is irreversible; a line count is not. Let it go over and report it.
- **Never poll a condition with repeated individual Bash calls** (`pgrep`,
  `systemctl is-active`, a hand-rolled retry loop). Wrap a real wait in one `Monitor`
  call, or — if the wait is really "has this task finished" — a gate (`bd gate create`,
  or your tracker's equivalent), and resume by checking whether it resolved rather than
  staying resident to watch for it. Measured September 2026: 24 individual poll calls
  spanning 11 minutes, checking one condition a single `Monitor` call would have
  covered, cost $3.12 and 24 turns in one orchestrator run — right next to nine correct
  uses of the same `Monitor` pattern in the same session.
- **A coordinating thread that never resets pays for its own history every turn, the
  same as any other long thread.** State already lives in the tracker (see above), so
  nothing is lost by ending your turn — a fresh orchestrator invocation reading
  `bd swarm status` / `bd ready --mol <epic>` (or your tracker's equivalent) resumes
  exactly where the last one stopped. Measured September 2026: a single 637-turn,
  12.5-hour orchestrator cost $92 coordinating 19 sub-agents that together cost
  $18.62 — the coordination thread, not the sub-agent work, was the expense.
- **Delegate the live pass, never a GUI loop.** Step 9's one bounded run is worth what it
  costs. What is never worth it is a builder iterating against a windowed app — edit,
  launch, screenshot, squint, repeat. Each launch is tens of seconds, the output needs a
  human eye anyway, and incidental screen capture photographs whatever else is on the
  desktop. Builders make the edit and run the headless checks; the live tester drives it
  once; a genuinely visual judgement is the user's. Measured: one blueprint realignment
  cost the largest transcript of an entire epic, for the smallest task in it.
- **A passing suite is not coverage.** Tests pass while a requirement is entirely
  unimplemented, because nothing asserts it.

## BUILDER TEMPLATE

    You are implementing exactly ONE task: <id>. This task only, plus feedback I send.

    WORK IN THIS DIRECTORY AND NOWHERE ELSE: <worktree path>
    A git worktree on branch <EPIC>. Never touch the main checkout. Never run git reset,
    git checkout --, git stash, git clean, or git worktree — the work here is
    uncommitted and those destroy it.

    If any instruction here contradicts what you find in the repo, STOP and report the
    contradiction rather than guessing — the spec is more likely wrong than you are.

    STEP 1 — read your spec in full, notes included; notes override the description:
        <TRACKER SHOW CMD> <id>
        <TRACKER CLAIM CMD> <id>

    STEP 2 — read, in this order:
        <project convention/rules file(s)>
        <THE closest existing model file, named explicitly, with line range>
        <what you build on — "read it, do not redefine it">

    STEP 3 — before touching a file, map each acceptance criterion to the step that
    will satisfy it and the command that will verify it:
        AC-1 → <what you'll build> → verify: <command>
    If a criterion has no step, or a step serves no criterion, that plan is wrong —
    fix it before writing code, not after.

    Minimum diff that satisfies the acceptance criteria. No speculative abstraction,
    no config or flexibility nobody asked for, no error handling for a state this
    change cannot produce. If you could delete half of what you wrote without losing
    a criterion, delete it.

    No comments unless the spec asks for one or an existing file's convention is to
    write them. Well-named code does not need a comment restating what it does; write
    one only for a non-obvious WHY — a hidden constraint, a workaround, something that
    would surprise a reader — and never reference this task or its ticket ID in it.

    Inside every file you touch, touch only the lines your task requires. Do not
    reformat, rename, or "improve" adjacent code, and match existing style even
    where you would choose differently. If you notice unrelated dead code, name it
    in your report — do not remove it. The only symbols you may delete are ones
    YOUR OWN edit made unused.

    If the spec supports more than one reasonable reading, or a simpler approach
    exists than the one specified, do not silently pick one: implement your best
    judgment call but flag it explicitly in STEP 6, naming the reading you rejected
    and why. If the ambiguity is large enough that a wrong guess wastes the whole
    task, STOP and report it instead — same bar as a contradiction.

    STEP 3 (cont.) — implement. Hard scope:
    - Create/edit ONLY: <exact paths>
    - Do NOT touch <other package>. Another agent is live there in this same worktree.
    - Dependencies: <none / this one is approved, default settings>
    - <the 2-4 traps, as imperatives, each with its consequence>

    STEP 4 — acceptance criteria. Satisfy every one; where a criterion names a test,
    write it:
    <the AC list, verbatim>

    STEP 5 — verify with EXACTLY these:
        <only commands that can actually pass for this change>
    Do not run <the broad command>; a sibling's in-flight work makes it red for
    unrelated reasons. Do not regenerate <shared artifact>.

    STEP 6 — report. NO COMMIT, no push, do not mark the task done.
    Files changed; LITERAL command output; a line per criterion saying met / not met /
    not verifiable and how you know; anything in the spec you could not do; every
    judgement call the spec did not settle, including any interpretation or
    simplification you chose over an alternative; every line you left out of a
    "minimum diff" because it wasn't needed for a criterion.
    If something fails, show the failure — a truthful red beats a claimed green.

## REVIEWER TEMPLATE

    You are a code reviewer. Fresh eyes. You did not write this.

    WORK IN THIS DIRECTORY AND NOWHERE ELSE: <worktree path>
    SCOPE — review ONLY: <exact paths>
    IGNORE ENTIRELY: <other agents' paths>, and <any known-unrelated failure>.

    THE SPECIFICATION: <TRACKER SHOW CMD> <id>   — read it first, in full.

    THREE JOBS. JOB 2 AND JOB 3 ARE THE ONES PEOPLE SKIP.

    JOB 1 — CORRECTNESS. Where do the code and the spec disagree, and where is the code
    wrong on its own terms? Compare directly against <model file, line range>.

    JOB 2 — COMPLETENESS. Take each criterion ONE AT A TIME: MET, NOT MET, or NOT
    VERIFIABLE HERE (say what would be needed). For every MET, name the evidence — the
    test, the code path you read, the output you saw. "The tests pass" is not evidence:
    a requirement nothing asserts passes a green suite while being unimplemented. Hunt
    for requirements with NO code and NO test.

    JOB 3 — SCOPE. Read the full diff, not just the lines a criterion points at. Flag
    anything present that no criterion required: an abstraction, a config knob, a
    dependency, a comment restating what the code already says, error handling for a
    state that cannot occur, or an edit outside <exact paths>. Flag any line changed
    inside an in-scope file that no criterion touches — a reformat, a rename, an
    "improvement" to code the task didn't need. "It doesn't hurt" is not a defense:
    every line added without a criterion behind it is a line nobody asked for and
    nobody is testing.

    ACCEPTANCE CRITERIA:
    <the AC list, verbatim>

    ALSO VERDICT ON:
    <each judgement call the builder flagged, with the consequence spelled out — not
     "is this ok" but "what breaks, for whom, when">

    If the SPEC or a CRITERION is wrong rather than the code, say so — both were written
    by the orchestrator and would rather be fixed than papered over.

    Look hard. A requirement with no implementation produces no failing test and no
    warning — only silence. Budget most of your effort on JOB 2.

    VERIFY INDEPENDENTLY, do not trust the author's claims: <commands>

    RULES: FIX NOTHING — no edits, no formatters, no git. A reviewer who edits becomes a
    second author and the review is worthless. Do not touch the tracker or commit.

    OUTPUT: (a) the criteria table — criterion, verdict, evidence. (b) findings,
    numbered, most severe first: the claim in one sentence, file:line, the concrete
    consequence, the specific fix. Separate defects from taste and label the taste ones.
    If it is correct, say so briefly rather than padding.

## LIVE TESTER TEMPLATE

Fresh agent, after the code checks pass. Never the builder, never the code reviewer.

**Decide what "live" means here first**, and put the answer in the prompt. Not every
project has a screen:

- **HTTP API** — curl and jq the running server with real credentials. An in-process test client
  is not a live check; it shares the process and hides everything the wire would show.
- **Web UI** — drive a real browser via playwright-mcp, stop browser and test servers when task is done.
- **TUI or CLI** — run it in a virtui (suggest installation) or pty. Keyboard only; assume no mouse.
- **Desktop (GTK, Qt, native)** — launch it and drive it through the accessibility layer
  where one exists. Screenshots last: they cost the most and need a human eye anyway.
- **No surface yet** — a model, a migration, a library, a component with no caller. Say so
  and verify against the real artifact instead: the applied DDL on a real database, the
  generated schema, the built binary. Record it as unclicked rather than faking a pass.

Cut any bullet below that the surface cannot have — a CLI has no layout, a library has no
forms. Keep the rest; they apply everywhere something renders.

    You are live-testing <id>. You did not write it, and you are not reading the diff.

    RUN IT: <exact launch command, port, how to reach a signed-in or loaded state>
    CLEAN UP: <how to undo what you create — leave the environment as you found it>

    FIX NOTHING. No edits, no formatters, no git, no tracker. Change no state you did not
    create yourself.

    Drive it the way a user would, not the way the tests do. Report only what you SAW:
    the request and the status, the text on screen, the key you pressed and what happened.
    "It works" is not a finding. Neither is "looks fine".

    WALK THE PATHS
    - Every path the task claims, reached from a cold start — no deep link, no hand-edited
      state. A path you cannot get to from the front door is unreachable, however correct.
    - The happy path, end to end, once.
    - The error paths: rejected input, a record that is missing, a permission refusal, the
      backend gone. Each must say what went wrong in terms the user can act on.
    - After every error, is the thing still usable? A stuck spinner, a dead session or a
      form that will not resubmit is a defect even when the message was right.

    JUDGE WHAT YOU SEE
    - Layout at the smallest and largest size the project supports. Nothing clipped,
      overlapped or off-screen; nothing makes the whole view scroll sideways.
    - Keyboard alone reaches every control, focus is always visible, nothing traps it, and
      every control has a name the platform's accessibility layer can read.
    - Feedback arrives in the channel THIS project already uses for that kind of message —
      toast, inline field error, status line, dialog, stderr. Find the existing convention
      and hold the new work to it. Inventing a second channel for one screen is a defect.
    - Forms validate before they submit and name the offending field, next to the field,
      in words that say how to fix it.
    - Motion earns its place by explaining a change or showing progress. Gratuitous
      animation is a defect, and so is motion that ignores the platform's reduced-motion
      setting.
    - Nothing is present that need not be: a label restating the field beside it, a
      heading over a single item, help text repeating the control, a confirmation for
      something harmless. Flag every one.
    - If the project is localised, every string is translated — no raw keys, no fallback
      language, no untranslated new string. If it is not localised, say you skipped this.
    - Common sense, last and hardest: would someone who did not build this know what to do
      here, and know what happened after they did it?

    REPORT: what you ran and what you drove, then findings most severe first — what you
    did, what you expected, what you saw, and where. Separate defects from taste and label
    the taste. If it is genuinely good, say so in a line rather than padding.
