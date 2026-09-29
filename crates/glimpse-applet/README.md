# glimpse-applet

The launcher execs Deno so the applet keeps the same process and exit status. `--watch` uses Deno's watcher.

Only network, read, and environment grants are expressible. The launcher always grants `NODE_ENV`.
