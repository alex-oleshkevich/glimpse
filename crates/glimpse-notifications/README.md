# glimpse-notifications

The standalone notification owner and popup UI. It runs the existing notification, compositor and
session services in process and exposes notification state and controls over a typed D-Bus interface.

**The provider name is what this process is for.** A process that cannot take
`me.aresa.Glimpse.Notifications` on the session bus exits instead of running on: a popup surface
with no name serves nothing, and the service graph behind it is pure cost. The same goes for a
missing session bus. The unit bounds the resulting restarts through `StartLimitBurst`, so a genuine
duplicate stops rather than loops. With the default application id GTK's single-instance handoff
already exits the second process first; this only surfaces when the ids differ.

**`Post` is how glimpse raises its own notification**, and it is the only way in besides
`org.freedesktop.Notifications.Notify`. A posted record takes its id from the **same counter** the
fdo server allocates from, reached through the exported object rather than a second counter, or two
id spaces would eventually hand out the same number for different records; with no server exported
there is no id to give and the command is refused rather than inventing one. Everything after that
is shared — the same capping, suppression and do-not-disturb gate a notification arriving over fdo
meets.

**Grouping keys on the strongest identity a sender has**: its `desktop-entry` hint, else a desktop
file matching its executable, else **the name it calls itself**, else its bus name. The last two are
the trade: a unique bus name is per-*connection*, so a sender that exits between notifications — a
screenshot script, `notify-send` — would open a fresh group every time and never stack. Falling back
to `app_name` costs the guarantee that two identity-less senders are told apart; one that carries a
desktop identity is still keyed on it and cannot be joined by a sender merely claiming its name.

The first local notification snapshot is a baseline and never opens a popup. Later unread records
appear newest nearest the configured edge; replacing a visible record
updates its existing card and restarts its timer. Do not disturb, lock, privacy, disablement and
service loss clears the transient stack without deleting in-process history, and records received while a
gate is active are never replayed when it opens. History is bounded in memory and resets with this
process.

One layer-shell surface owns the full stack. Every entry uses the shared `NotificationCard`, whose
optional image slot updates without replacing the widget. It reserves a paint gutter around every
card for its shadow and entrance translation, and narrows the Wayland input region to card bounds
so the gutter and inter-card gaps remain click-through. Left click asks the compositor to focus the
sender process where one is known and dismisses the notification; right click hides only the popup;
close dismisses it into history; named actions invoke the sender action and then dismiss.

The card owns the same `34rem` width in this surface and the notifications popover. The paint gutter
is extra transparent window space for the shadow and does not change the card width.

`[notifications]` selects the output, edge, delay and cap. Placement stays fixed while the stack is
non-empty, except when that output disappears. File changes and `SIGHUP` reload both configuration
and styles in place; changing the theme re-arms the theme watcher without replacing the process or
layer surface.

The process-local session service supplies lock and privacy gates from logind and compositor
screencast state. It receives the compositor handle explicitly from this process's composition root.
