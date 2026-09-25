# Todo

A glimpse applet. The chip's badge is the number of open items. A trailing
`YYYY-MM-DD` on a new item is its due date, and a red dot appears when any open
item is overdue. Activate a row to toggle done. The entry adds an item, Show
done reveals finished ones, and Copy all copies every line.

The section title is `options.title` when that option is a non-empty string,
otherwise `Todo`. On a vertical bar the chip drops its text and keeps the badge.

Items are stored in `localStorage` under `glimpse.todo`. Each bar runs its own
applet process, and Deno does not deliver storage events across processes, so a
second bar sees an addition only on its next start.

The installed entry is `me.aresa.GlimpseTodo.desktop`:

```
Exec=glimpse-applet /usr/share/glimpse/sdk/applet/examples/todo/main.tsx
```

GIO refuses an entry whose program is not on `PATH`, and a relative script path
means nothing to the panel. For a live edit, point a desktop file at
`glimpse-applet` on `PATH` and an absolute script path:

```
Exec=glimpse-applet --watch /absolute/path/to/sdk/applet/examples/todo/main.tsx
```
