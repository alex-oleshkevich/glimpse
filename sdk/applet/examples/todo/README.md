# Todo

A glimpse applet. The chip's badge is the number of open items. A trailing
`YYYY-MM-DD` on a new item is its due date, and a red dot appears when any open
item is overdue. Activate a row to toggle done. The entry adds an item, Show
done reveals finished ones, and Copy all copies every line.

The section title is `options.title` when that option is a non-empty string,
otherwise `Todo`. On a vertical bar the chip drops its text and keeps the badge.

Items are stored in `todo.json` under the state directory the host passes in
its `hello`.

The shipped directory is read-only source. Copy it somewhere writable, then:

```
npm install && npm run build
glimpsectl applets install .
```

`install` copies `build/applet.mjs` and rewrites the entry's `Exec`. For a live
edit, run `npm run dev`, point the entry at `glimpsectl applets run
/absolute/path/to/build/applet.mjs`, and grant that directory in the panel
config: `[applets."me.aresa.GlimpseTodo"] sandbox = { read = ["/absolute/path/to"] }`.
