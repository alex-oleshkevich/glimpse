# Trust model

An exec applet is a program the user installed, and glimpse runs it the way the session runs any program:
as a transient systemd user unit with the user's full session. There is no confinement layer between an
applet and the rest of the desktop. This page states what that means, what the host still enforces, and
why the line is drawn there.

## What an applet can do

The unit gets the panel's environment (minus systemd-internal names such as `NOTIFY_SOCKET`, `INVOCATION_ID`,
`LISTEN_*` and `WATCHDOG_*`, which would corrupt the applet's own service management if inherited, and
any name that is not a valid identifier)
plus any `env` entries from its configuration table. It can reach the session bus, the user's files, the
network and everything else the user can reach. Its persistent state directory is mode 0700, but that is
housekeeping, not a boundary: nothing stops an applet from reading or writing anywhere else the user can.

Installing an applet is therefore the trust decision, exactly as installing any application is. The
desktop entry decides what runs, and glimpse refuses entries it cannot run honestly (hidden entries,
entries without the applet interface, `DBusActivatable` and `Terminal` entries), but it does not audit
what the program does once started. A malicious applet needs nothing from the panel to do damage: it
already runs as the user.

## What the host still enforces

If confinement is off the table, the remaining host rules protect the shared surface: the bar, the other
applets, and the user actions an applet can ask the host to perform on its behalf. A buggy or hostile
applet should be able to ruin its own chip and nothing else.

| Area | Rule |
| --- | --- |
| Tree size | A commit is rejected as a whole past 300 nodes, 200 children of one parent, or 1,200 operations. Lines are capped at 1 MiB and JSON nesting at 128 levels. |
| Rates | Lines past 120 per second are delayed, so the applet blocks on its socket instead of the panel blocking on the applet. Notifications are capped at one per second, and media-player and compositor calls at ten per second; past the cap the request is dropped. Clipboard copies past 1 MiB are dropped. |
| Memory | Every applet unit carries `MemoryMax` of 256 MiB in its cgroup. |
| Protocol abuse | Invalid JSON, an oversized line, an invalid tree, a commit or request before `hello`, an unsupported protocol version, more than 5 `hello`s in 10 seconds, or a full write queue (an applet not reading its socket) kills only the offending unit. The full list is in [How exec applets work](/developers/exec-applets). |
| Text | Every string an applet puts on screen is cleaned and capped (256 characters for labels and titles, 8 for a badge, and so on), and it reaches widgets as plain text, never as markup. |
| URIs | `open-uri` accepts only `http`, `https`, `mailto` and `file`, at most 2,048 bytes; anything else is dropped. |
| Files | A `file` URI opens only a directory, or a regular file that is neither executable nor of a type that starts a program (a desktop entry, a script, a binary, an installer, an AppImage or similar launcher types). The path is resolved before the check, so a symlink cannot dress an executable up as a document. |
| Launching | `launch` takes a desktop-file id. An applet can of course start programs itself, since it runs as the user; the host's own launch path stays narrow so a click in the bar never becomes "run this file". |

The file rule deserves its reason. Opening a URI is the one host action that turns applet-supplied data
into a program start on the user's click, so it is gated even though the applet could start the same
program on its own. The difference is who appears to have asked: a user who clicks a link in the bar
expects a document or a folder, not an installer.

## Text from other apps

The same suspicion applies in the other direction. Titles, metadata and names that arrive from tray
items, notifications, media players, windows and wireless networks are treated as hostile input wherever
glimpse displays them, including inside exec applets: media-player fields and window titles are capped at
256 characters before they are sent to an applet, and applet labels render them as plain text. An applet
is trusted code, but the strings flowing through it are not.

## Why no confinement

A barrier that still lets an applet show useful things (files, players, workspaces, the network) would
mostly duplicate the session's own permissions, while giving users a reason to install applets they have
not actually decided to trust. glimpse instead keeps the model honest: installation is the decision, the
unit runs as the user under systemd's accounting (its own cgroup, restart curve and journal), and the
host's caps keep one applet from taking the bar, the notification stack or the user's clicks down with
it. Anything stronger belongs to the mechanisms a user can already apply to any program in their
session.
