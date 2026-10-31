# The launcher and its external providers stay running for the whole session

`glimpse-launcher` starts with the session, keeps its window built and hidden, and starts every enabled external provider alongside it, so opening the launcher and answering the first query never wait on a process start (any glimpse binary needs about 45 ms just to start, before its first frame). We accept the resident memory this costs and keep it as low as quality allows: no fixed memory cap, but caches are bounded, per-query buffers are freed on hide, the launcher uses no CPU while hidden, and a soak test fails on memory growth across open/type/close cycles.

## Considered Options

- Start on demand and exit after idle minutes: costs nothing while unused, but the first open after idle pays a cold start.
- A small resident index with a GUI spawned per open: every open pays a process start plus the first frame, and there are two processes to keep in sync.
- Start external providers lazily on their first query: the first answer from each waits on a cold start.
