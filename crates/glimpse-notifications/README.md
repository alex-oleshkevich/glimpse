# glimpse-notifications

The standalone notification owner and popup UI. It runs the existing notification, compositor and
session services in process and exposes notification state and controls over a typed D-Bus interface.

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
