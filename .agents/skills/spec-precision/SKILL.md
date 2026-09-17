---
name: spec-precision
description: Write a task brief another agent (or your future self) can execute without guessing — exact file list, one named model file, falsifiable acceptance criteria, and a self-check that every symbol the spec names actually exists and is reachable. Use before delegating implementation work, writing a ticket someone else will pick up, or handing a task to a sub-agent.
---

# spec-precision

Spec quality is the dominant cost lever in delegated work. A mid-tier model with a
precise brief beats a frontier model with a vague one, at a fraction of the price —
because vagueness is not paid once, it is paid every round.

Measured on one real epic, September 2026: a task with a clean spec finished in **one
round at 104k tokens**. A task whose spec carried three defects took **three rounds and
494k**. Same model, same reviewer discipline. Each spec defect bought a round.

## The self-check — do this before sending

**For every type, function, module path, file and command the spec names, confirm it
exists and is reachable from where the implementer will sit.** Look each one up with
Codegraph where the repo is indexed, otherwise the LSP, otherwise `sg` — in that order.

Nearly every expensive rework is one of these:

- A **module that is private**, so the import path you wrote does not compile from
  another package.
- A **type with no public constructor**, so the test you asked for cannot be written.
- A **struct you extend whose construction sites you did not list**, so the change does
  not compile and the implementer must go outside your file list to fix it.
- A **method you half-remember** that is not on that trait, or has a different
  signature.
- A **verification command that cannot pass** for a change of this shape.

That is one blast-radius query per named symbol. It costs minutes and saves rounds.

## What a brief must carry

- **The exact files** that may be created or edited. Not "the audio module" — paths.
  If you are unsure whether a file is needed, find out before sending; an incomplete
  list forces the implementer to choose between breaking the build and breaking scope.
- **One named model file, with a line range.** "Follow the existing pattern" makes them
  search. Naming the closest existing example is the single highest-value line in a
  brief.
- **The 2-4 traps most likely to sink it**, each with its consequence. Not "be careful
  with locks" — "callbacks run under the mainloop lock, so awaiting under it deadlocks."
- **Only verification commands that can actually pass.** A gate the change cannot
  satisfy wastes a round and teaches the implementer to distrust the spec. Prefer the
  **narrowest** command that proves the work: one crate, not the workspace.
- **What NOT to do**, where it is non-obvious. The boundary with a sibling task, the
  refactor that must not be mixed in, the dependency that must not be added.

## Acceptance criteria — write them before the code exists

Criteria written after seeing an implementation get shaped by it: you end up describing
what was built rather than what was asked, and the review confirms the code matches
itself.

    AC-1  As <who>, I want <capability>, so that <outcome>.
          GIVEN <state> WHEN <action> THEN <observable result>
          Verify by: <command, test name, or inspection>

- **One per requirement, including every "do not."** A prohibition is a criterion:
  "GIVEN a state-driven write, THEN no command is emitted."
- **Falsifiable only.** "Works correctly" is not a criterion. "Returns None when there
  is no default device, so the caller renders nothing" is.
- **Name how each is verified, honestly.** "By inspection" and "needs a human on real
  hardware" are legitimate — they go on the manual list, not the test list. A criterion
  you cannot check is a criterion you have not written.
- **Mine the spec for concrete data.** A mentioned malformed input is a test case. A
  measured value is a boundary.
- **Add the boundaries the spec implies but never states:** empty input, absent optional
  field, value at and past a cap, two of a thing where the spec says one, the resource
  disappearing mid-operation, and **the state before the first event ever arrives** —
  that last one is reachable in production and never in a test that constructs an object
  and immediately feeds it events.

## When the spec turns out to be wrong

It will. Tell implementers explicitly: **if an instruction contradicts what you find in
the repo, stop and report the contradiction rather than guessing — the spec is more
likely wrong than you are.** Then when a report comes back:

- **Fix the spec, and every sibling task inheriting the same contract**, before those
  are claimed. A downstream agent faithfully implementing a wrong spec is the most
  expensive failure in the loop.
- **Fold the finding into the next brief.** Any review finding that was a trap you could
  have named up front is a round you bought. After each review, ask which finding
  belonged in the spec.
