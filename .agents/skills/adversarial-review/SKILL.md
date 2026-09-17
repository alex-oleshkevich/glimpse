---
name: adversarial-review
description: Review a change for correctness AND completeness with fresh eyes — a criteria table where every verdict names its evidence, a hunt for requirements that have no implementation at all, and permission to declare the spec wrong rather than the code. Use when reviewing a diff, auditing someone else's work, checking whether a task is actually finished, or writing the brief for a reviewer agent.
effort: high
---

# adversarial-review

A review has two jobs. The second is the one people skip.

**JOB 1 — correctness.** Where does the code disagree with its spec, and where is it
wrong on its own terms?

**JOB 2 — completeness.** Which requirements have *no implementation at all*?

Job 2 is harder and matters more. A wrong implementation produces a failing test or a
visible symptom. An absent one produces **silence** — a green suite, no warning, and a
requirement nobody wrote code for. Budget most of the effort there.

## The rules that make a review worth its cost

- **Fresh eyes, every round.** A reviewer who watched the code being written stops
  seeing it. Reviewing your own change, or re-reviewing after your own fix, is a
  different and weaker activity — say which one you are doing.
- **FIX NOTHING.** No edits, no formatters, no git. A reviewer who edits becomes a
  second author and the review is worthless. Make it mechanical rather than
  honour-based: spawn the reviewer with `--permission-mode plan` (Claude Code) or
  `--sandbox read-only` (Codex). Both still allow the verification commands below.
- **"The tests pass" is not evidence.** A requirement nothing asserts passes a green
  suite while being entirely unimplemented. Name the test, the code path you read, or
  the command output you saw.
- **Verify independently, and narrowly.** Run the checks yourself — the narrowest ones
  that prove this change, not a whole-workspace gate that goes red for unrelated
  reasons and rebuilds the world to tell you so. An author reporting green and the
  tree being green are different claims — and an author who verified before a sibling's
  work landed was honest and is now stale.
- **Diff against the committed state — `git diff HEAD`, never bare `git diff`.** The
  author may have staged files, and a bare diff compares against the index, silently
  misreporting what changed. True of anyone's uncommitted work, not only your own.
- **The spec may be what is wrong.** Say so plainly. A spec defect implemented
  faithfully is the most expensive failure there is, because it passes review twice.
  **Review the criteria as well as the code** — an acceptance criterion that cannot be
  satisfied, or that asks for something the codebase makes impossible, is a finding
  against whoever wrote it. Whoever briefed you is the one participant nothing else in
  the loop is checking.
- **Separate defects from taste, and label the taste.** A padded list trains people to
  skim. A short honest review beats a long hedged one.

## Output

**(a) The criteria table.** Every criterion: MET / NOT MET / NOT VERIFIABLE HERE, and
for each, the evidence. "Not verifiable here" is a legitimate verdict — say what would
be needed (a display, real hardware, a live server) and put it on the manual list.

    AC-1  MET             volume_for() returns None with no default sink —
                          audio/device.rs:88, asserted by tests::no_default_sink
    AC-2  NOT MET         nothing caps the stream name; a 4KB title reaches the label
    AC-3  NOT VERIFIABLE  needs a mapped surface to prove the knob claims the press —
                          manual list

**(b) Findings, most severe first.** Each one: the claim in a sentence, file:line, the
concrete failure scenario — what breaks, for whom, when — and the specific fix.

## Where bugs actually hide

Hunt these first. They are the shapes that survive a green suite:

- **Ordering.** Two correct operations in the wrong sequence. Capping a string before
  parsing it; fetching a parent list before the children that reference it; validating
  an id before checking whether the connection exists. Each reads fine in isolation.
- **Wrong source.** Right type, wrong meaning. A predicate used as an icon; a tooltip
  formatter used as a row value; a device's identity used where its label belongs. The
  compiler is happy and the screen is wrong.
- **The transient window.** State reachable only *before the first event*, *during a
  drag*, or *while a resource is briefly gone*. Tests construct an object and
  immediately feed it events, so this window is never exercised. Ask explicitly: what
  does this do before anything has happened to it?
- **Untestable by construction.** A private field with no constructor, a type no test
  can build. Correct "by inspection" means nothing is stopping the next refactor.
- **Environment assumptions.** A library's documented behaviour that is not its actual
  behaviour; a cache that never re-reads; a test asserting the state of the host. These
  need a real run, not a closer read.
- **Missing bounds.** A loop with no upper limit, a list with no cap, a retry with no
  ceiling. Fine until the input is adversarial or merely busy.

## Reviewing your own work

You cannot get fresh eyes, so substitute discipline:

- Re-read the spec first, before the code. Reading the code first anchors you to what
  it does rather than what was asked.
- Ask what you would need to believe for each finding to be false, then check that.
