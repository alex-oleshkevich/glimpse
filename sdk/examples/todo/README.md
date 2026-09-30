# Todo

A glimpse applet. The chip's badge is the number of open items. A trailing
`YYYY-MM-DD` on a new item is its due date, and a red dot appears when any open
item is overdue. Activate a row to toggle done. The entry adds an item, Show
done reveals finished ones, and Copy all copies every line.

The section title is `options.title` when that option is a non-empty string,
otherwise `Todo`. On a vertical bar the chip drops its text and keeps the badge.

Items are stored in `todo.json` under the state directory the host passes in
its `hello`.

From this directory:

```
npm install
glimpsectl applets run --dev .
```

The entry runs `node --run start`, which builds and starts the applet in this
directory, so grant it in the panel config:
`[applets."me.aresa.GlimpseTodo"] sandbox = { read = ["/absolute/path/to/todo"], write = ["/absolute/path/to/todo"] }`.
