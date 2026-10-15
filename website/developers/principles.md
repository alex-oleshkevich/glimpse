# Design principles

A short list of rules decides most design questions in glimpse. They are stated here with the reason each
one exists, because a rule without its reason gets "improved" away by the next person who does not know
what it was holding up.

## One config file

All of glimpse reads one document, `config.toml`, with one top-level table per owner: one per service,
plus the panel, wallpaper and lock tables. A binary acts on the tables it owns plus the shared `[appearance]` and
`[regional]` tables. One cross-owner read exists: while the lock background names neither image, the
locker inherits the wallpaper images, because a lock screen that differs from the desktop by default
reads as a bug.

One file means one place for a user to look, one merge and validation path, and no process acting as a
configuration broker that every other process must ask. Stylesheets stay separate files beside it, since CSS is not configuration
in the same sense, and commands that change state at runtime (forcing a night-light mode, for instance)
never write back into the document: the file says what the user asked for, not what happened to be in
force when a process last ran. The user-facing account of the file is
[How configuration works](/config/).

## Mirror services add no policy

Services that front a system backend (network, bluetooth, audio, battery, MPRIS, brightness, removable
media and the rest) mirror it. They enumerate once at start, then follow change signals, and their
commands are thin pass-throughs. They never reimplement a decision the backend already makes: no
auto-connect policy and no reconnect loops on top of NetworkManager, no retry logic the backend owns.
When the mirror and the backend disagree, the backend is right.

The reason is that every policy glimpse added on top would eventually fight the backend's own policy, and
the user would debug two decision makers for one device. A mirror can also be tested by comparing it to
the backend, while a policy has to be tested against someone's intention.

## The three sanctioned backend exceptions

Exactly three places make a decision a backend left open. Each exists because a pure pass-through fails
in the one case the user actually hit, and nothing else may grow a policy on top of a backend without the
same kind of explicit note.

| Exception | Backend | Why a pass-through fails |
| --- | --- | --- |
| A settled pairing trusts and connects | BlueZ | BlueZ leaves a freshly bonded device bonded and not connected. A user who pressed Pair meant "use this", so when pairing settles, the bluetooth service marks the device trusted and connects it. |
| Eject unmounts first | UDisks2 | `Drive.Eject` refuses with `DeviceBusy` while any filesystem on the drive is mounted, and offers no option to unmount. The removable service unmounts (and locks) only the drive's own volumes first; a failed unmount is reported as itself and the eject never runs. Races such as `NotMounted` are stepped over rather than reported. |
| A bonded device's incoming service is authorized | BlueZ | `Agent1.AuthorizeService` is BlueZ delegating a decision, and it only asks about an untrusted device. A blanket refusal would strand every pairing made before glimpse, with no interface anywhere explaining why. The bond is the whole test; nothing else is consulted. |

## Failures are notifications

A failed command is reported by a notification, never by a banner inside a popover. A popover is open for
seconds and the failure outlives it: a banner would say the same thing the notification already says, and
then leave a stale sentence behind on the next open. There is exactly one allowed banner, a condition a
notification cannot carry: the notification provider itself being down.

The same rule scales down to configuration. One broken document becomes one notification from the panel,
not one per process, and every binary keeps running on its last good configuration or on defaults. A
shell that refuses to start over a typo has failed at its main job.

## Optimistic UI

UI state never waits on a round trip. An applet updates its widget as soon as the user acts, sends the
command, and lets the next service event reconcile the display. A volume slider that waits for the audio
server to confirm every step feels broken even when it is correct.

Optimism has one sharp edge: when a command is rejected, a mirror service publishes nothing, because its
state did not change. Without help, the optimistic flip would stand until something unrelated moved. So a
failed command wakes its own applet, which re-reads the snapshot and falls back to the true state, while
the failure itself arrives as a notification.

## Text from other apps is hostile

Tray titles, notification summaries and bodies, MPRIS metadata, window titles and network names are
chosen by other applications and are unbounded. glimpse treats all of it as attacker-controlled: the text
is cleaned (control characters and bidi overrides dropped, whitespace folded), capped in length,
ellipsized for display, and never handed to a label as markup. Where a notification body allows a small
formatting subset, it passes through a sanitizer that keeps only bold, italic, underline and line breaks,
and drops link and span tags and every attribute (their text stays as plain text), so a sender cannot repaint or resize text inside the shell or
hide a clickable address in a surface the user did not ask to open.

The rule applies at every boundary, including glimpse's own. A reader caps text the writer already
capped, because trusting the earlier cap would mean trusting a bus name rather than a process. The
terminal counts too: provider text is stripped before `glimpsectl` prints it, since an escape sequence
in a terminal can repaint the screen. [Trust model](/developers/trust) covers the same stance for exec
applets.
