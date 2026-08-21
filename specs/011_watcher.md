---
state: draft
---

# 011 — Watcher

The service that watches configuration and stylesheets, so no UI process opens an inotify handle.

## Problem

Four binaries read the same `config.toml`, and two of them also read a stylesheet. If each watches
its own files, the session carries four independent inotify setups, four debounce windows, and four
slightly different answers to what an editor's write-then-rename looks like. `fs.inotify.max_user_watches`
is a shared resource, and the panel restarting drops and re-adds its watches every time.

Filesystem watching is an OS integration, and `001_architecture.md` puts those in the daemon. It is
the only piece of the configuration path still living in the UI processes.

## Goals

- One set of watches for the whole session, whoever is running.
- A UI binary learns its files changed without opening a watch or knowing about inotify.
- A binary still starts, and still loads its own configuration, with the daemon dead.
- Adding a `config.d/` drop-in is noticed, not only edits to files that already exist.

## Non-goals

- **No parsing of what it watches.** The watcher publishes that a file changed and what its content
  digests to. It never parses TOML and never parses CSS — glimpsed has no GTK, so it could not
  validate a stylesheet even if it wanted to.
- **No content delivery.** The topic carries digests and paths, never file bodies. A client reads
  its own files.
- No watching outside the glimpse configuration directories. See Registration.

## Tech

### Shape

| Property  | Value                                                                     |
| --------- | ------------------------------------------------------------------------- |
| Kind      | owned — the watch set exists nowhere else                                 |
| Lifecycle | `OnBoot + Never`                                                          |
| Backend   | `notify` with `notify-debouncer-full`                                     |
| Topics    | `watch.config`, `watch.style`                                             |
| Commands  | `watch.register_style`, `watch.unregister_style`                          |

`OnBoot` rather than `OnDemand`, which is the exception to the rule that a service does no work
until something subscribes. The daemon is itself a consumer: it needs a reload trigger for its own
configuration whether or not any UI is running, and `Ctx::subscribe` from inside the daemon counts
as demand. Six inotify watches is not backend work worth deferring.

### The division of labour

The watcher says *something changed*. Every binary still loads its own files.

That split is deliberate and it is what keeps the daemon out of the critical path. A client that
took its configuration from a topic would need two code paths — one for a live daemon and one for a
dead one — and `glimpse-lock` may never depend on the daemon for function at all. With loading kept
local there is one path: read the files at start, and re-read when the topic says the digest moved.

Losing the daemon therefore costs hot reload and nothing else. The panel keeps the configuration it
started with; `SIGHUP` and a restart both still work.

### Topics

Both are state cells, not change events, so a client that starts after a change still learns about
it from its subscription snapshot.

`watch.config` — the resolved configuration stack:

```json
{
  "files": ["/etc/glimpse/config.toml", "/home/u/.config/glimpse/config.d/10-laptop.toml"],
  "digest": "sha256:1f3a…"
}
```

`files` is the stack in merge order, drop-ins included, after symlink resolution. `digest` covers
the ordered contents of every file in it, so adding, removing, editing or reordering a drop-in all
move it. A client compares the digest against the one it last loaded and re-reads only on a
difference — which also means a client that just started and is already current does no work.

`watch.style` — one entry per registered stylesheet:

```json
{ "styles": { "panel": { "path": "/home/u/.config/glimpse/panel.css", "digest": "sha256:9c2b…" } } }
```

### Registration

Stylesheet paths are not the daemon's to know. `--css` and `GLIMPSE_CSS_PATH` make the path a
client's decision, and the path lives in a table the daemon does not own. So a client registers what
it wants watched:

| Command                   | Arguments        | Effect                                    |
| ------------------------- | ---------------- | ----------------------------------------- |
| `watch.register_style`    | `key`, `path`    | Watch this file, publish it on `watch.style` |
| `watch.unregister_style`  | `key`            | Stop watching it                            |

`key` is the client's own label — `panel`, `lock`, and whatever a further client chooses. Re-registering
an existing key replaces its path, which is what a `--css` change across a restart looks like.

Registration is dropped when the registering connection closes, so a panel that dies does not leave
a watch behind. A client that reconnects re-registers, exactly as it resubscribes.

Two limits, because a command that makes the daemon hold a kernel resource is a resource a client
can exhaust:

- A path must resolve inside `$XDG_CONFIG_HOME/glimpse/` or `/etc/glimpse/`. Anything else is
  refused with a non-retryable error. The daemon is unprivileged so this grants no read the client
  lacked, but it keeps the watch set to files that are plausibly configuration.
- At most 8 registered styles per connection, and 32 across all connections.

Path resolution follows `010_configuration.md`: symlinks are followed and the target must be a
regular file.

**A symlinked file needs two watches, not one.** Dotfile managers put `lock.css` in the
configuration directory as a link into a repository, and the two ends break differently: editing the
file produces events in the *target's* directory, while re-stowing or `ln -sfn` replaces the link
and produces events only in the *configuration* directory. Watching either end alone misses half of
what users actually do, so the watch goes on the link's parent and on the resolved target's parent.
When the path is not a symlink the two collapse to one directory and one watch.

### What counts as a change

A watch is on a directory, so most of what arrives is about neighbouring files, and some of it is
not about content at all.

**Only create, remove and modify events are considered.** Access events — open, close-nowrite, read
— fire for every file in a watched directory and describe nothing that changed, this daemon's own
digest reads included. Filtering them is what keeps the directory holding `config.toml` from
generating traffic every time anything reads anything in it.

**An event only costs a digest read when its path names a watched file, or resolves to one.** The
second half matters for the symlinked case: an editor writing through the link produces an event
naming the target, and the registration named the link.

The digest gate is the backstop rather than the filter. It makes a spurious wake-up publish nothing,
but it still has to read and hash the file to find that out, which is the cost these two rules avoid
paying on every unrelated write.

### Directories that do not exist yet

inotify cannot watch a path that is not there, and a watch is bound to an inode rather than to a
name. Four situations follow from that, and all four are ordinary rather than exotic:

| Situation                                  | When it happens                                  |
| ------------------------------------------ | ------------------------------------------------ |
| `config.d/` absent at start                | The common case. Most users never create one     |
| `$XDG_CONFIG_HOME/glimpse/` absent         | First run, before anything has been configured   |
| `/etc/glimpse/` absent                     | Installed without a system layer                 |
| A watched directory deleted and recreated  | `rm -rf ~/.config/glimpse` then re-stow or re-clone |

The last one is the dangerous one. The directory exists again, with the same name and different
inode, so a watcher holding the old watch sees nothing ever again and reports no error. Dotfile
managers do this routinely.

The rule:

- **Watch the nearest existing ancestor.** If `$XDG_CONFIG_HOME/glimpse/config.d/` is missing, the
  watch goes on `$XDG_CONFIG_HOME/glimpse/`; if that is missing too, on `$XDG_CONFIG_HOME/`.
- **Never walk above `$XDG_CONFIG_HOME` or `/etc`.** Both exist on any system that can run a
  session, so the walk always terminates, and neither `$HOME` nor `/` is ever watched — they are
  noisy enough to be a cost of their own.
- **Descend when the missing component appears.** A create event matching the next component moves
  the watch down to it.
- **Re-arm on `IN_DELETE_SELF` and `IN_MOVE_SELF`**, from the nearest ancestor that still exists.
  This is what makes the recreate case work rather than fail silently.
- **Every re-arm is followed by a rescan and a publish.** Anything that happened between losing the
  watch and placing the new one produced no event and is simply gone, so the state has to be read
  rather than inferred. The digest gate makes a rescan that finds nothing new cost one publish that
  never happens.

A missing directory is never an error. `config.d/` absent is the normal case, and `/etc/glimpse/`
absent means there is no system layer, not that something failed.

There is no periodic rescan. Polling every few seconds to catch what inotify missed would wake an
otherwise idle session forever, which is a poor trade for a gap that re-arming already closes. The
backstops for anything that still slips through are `SIGHUP` and a client reconnecting, which
re-registers and gets a fresh snapshot.

### Debounce

An editor saving a file produces several events — a write, a rename over the target, sometimes a
delete and a create. Published without coalescing, each one costs every subscriber a reload of a
file that is about to change again.

Events coalesce over a 250 ms window per watched set, and the topic's equality gate does the rest: a
digest that comes back unchanged is not republished, so `:w` on an unmodified file is silent and an
editor that rewrites a file byte-identically costs nothing downstream.

### Bounds

The configuration watch set is bounded at six directories, specified in `010_configuration.md`.
Registered styles add at most 32 more, and watches are on parent directories, so several stylesheets
in one directory share a watch — and a watch displaced onto an ancestor is still one watch, so
missing directories lower the count rather than raising it.

If the kernel refuses a watch — `max_user_watches` is shared and commonly exhausted — the service
logs once and reports `degraded` on `system.services` rather than failing. Everything else keeps
working; what is lost is automatic reload, which `SIGHUP` and a restart both replace.

### Interaction with `SIGHUP`

`SIGHUP` stays the explicit reload trigger and is what `ExecReload=` sends. The watcher is the
automatic path to the same reload. Neither replaces the other: a user who edits with a tool that
defeats inotify still has a way to apply the change, and a session with no watches left still
reloads on request.

## Alternatives considered

- **Each binary watches its own files** — rejected: four inotify setups for the same two files, four
  debounce implementations to keep consistent, and watches churned on every panel restart. It is
  also the last piece of OS integration outside the daemon, which is the boundary the architecture
  is drawn on.
- **Publishing file contents on the topic** — rejected: it gives every client two code paths, one
  for a live daemon and one for a dead one, and `glimpse-lock` must work with the daemon dead.
  Digests keep loading local and single-path.
- **A `watch.changed` event topic** — rejected: `001_architecture.md` makes topics state cells. A
  client that starts after the change would miss an event, where a digest it can compare against
  tells it what it needs on its first snapshot.
- **A fixed stylesheet path list** — rejected: `--css` and `GLIMPSE_CSS_PATH` make the path a
  client's decision, and the path is configured in a table the daemon does not own.
- **Watching only the resolved target of a symlink** — rejected: it is the obvious reading of
  "follow symlinks", and it silently stops noticing anything for the users most likely to be
  editing configuration, because replacing the link is how dotfile managers apply a change.
- **Ending the watch task when no directory can be watched** — rejected: it is what the previous
  implementation did, and a watcher that has quietly stopped watching is indistinguishable from one
  with nothing to report. The service reports `degraded` and stays up, so `system.services` says so.

## Changelog

- 2026-08-20 — created.
- 2026-08-20 — defined behaviour for directories that are missing at start or recreated later: watch the nearest existing ancestor, descend on create, re-arm and rescan on delete or move.
- 2026-08-20 — specified symlink watching against both parents, the create/remove/modify event filter, and per-path event matching, from what `_old/glimpse-lock`'s watcher got right; recorded that giving up silently when no watch can be placed is not acceptable.
- 2026-08-21 — dropped the alternative about watching Blueprint output for `glimpse-devtools`, which no longer exists; the `key` example no longer names it.
