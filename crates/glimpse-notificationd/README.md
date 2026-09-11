# glimpse-notificationd

The standalone notification popup UI. It is independent of the panel and talks only to `glimpsed`
over `glimpse-ipc`.

The first `notifications.list` value of every connection generation is a baseline and never opens a
popup. Later unread records appear newest nearest the configured edge; replacing a visible record
updates its existing card and restarts its timer. Do not disturb, lock, privacy, disablement and
reconnect clear the transient stack without deleting daemon history, and records received while a
gate is active are never replayed when it opens.

One layer-shell surface owns the full stack. It uses the shared `NotificationItem`, reserves a paint
gutter around every card for its shadow and entrance translation, and narrows the Wayland input
region to card bounds so the gutter and inter-card gaps remain click-through. Left click asks the
compositor to focus the sender process where one is known and dismisses the notification; right
click hides only the popup; close dismisses it into history; named actions invoke the sender action
and then dismiss.

`[notifications]` selects the output, edge, delay and cap. Placement stays fixed while the stack is
non-empty, except when that output disappears. File changes and `SIGHUP` reload both configuration
and styles in place; changing the theme re-arms the theme watcher without replacing the process or
layer surface.

The daemon-owned `session.status` topic supplies lock and privacy gates from logind, compositor
screencast state and explicit overrides; the popup process never opens those backends itself.
