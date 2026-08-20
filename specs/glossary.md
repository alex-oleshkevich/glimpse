# Glossary

Terms used across the specs with a meaning narrower than their everyday sense. Use these words
verbatim rather than inventing synonyms.

- **Topic** — a named cell of state owned by exactly one service. Identified by a dotted path
  (`audio.volume`). Every event carries the topic's full value, never a delta.
- **State cell** — the model behind a topic: it has a current value, not a history. Subscribing
  yields a snapshot; reconnecting yields the same snapshot again. There is nothing to replay.
- **Dynamic topic** — a topic claimed at runtime rather than declared at compile time, such as
  `tray.item.{id}`. Dropping its `Publisher` retracts it and subscribers receive a tombstone.
- **Pattern** — a subscription expression. `audio.*` matches one level, `tray.**` matches a subtree.
- **Command** — a client-to-daemon call with a reply, named `domain.verb_object`
  (`audio.set_volume`). Distinct from a topic, which flows the other way.
- **Frame** — one newline-delimited JSON message on the socket: `{id?, type, data}`.
- **Broker** — the single task inside `glimpsed` that stores topic values, matches patterns, and
  fans out to clients. It routes and does nothing else.
- **Service** — one tokio task inside `glimpsed` that owns a set of topics and a set of commands.
  Handlers run serially on `&mut self`.
- **Owned service** — a service whose state exists nowhere else, so `glimpsed` is the source of
  truth. Tray and notifications.
- **Mirror service** — a service that adapts a backend which owns its own state. Restarting it means
  re-enumerating and losing nothing. Network, bluetooth, audio, battery, mpris, brightness.
- **Backend** — the external authority a mirror service adapts: NetworkManager, BlueZ, UPower,
  PipeWire, logind, the compositor's IPC socket.
- **Demand** — the condition that keeps an `OnDemand` service running: a client pattern matches one
  of its topics, an in-process subscription targets one, a command names it, or a dependent service
  is running.
- **Coalescing** — collapsing several pending events for one topic, for one client, into the newest
  value. Lossless because topics are state cells.
- **Stale** — broker metadata on an event meaning the producing service is no longer `Running`. The
  value is the last known good one; subscribers keep rendering and know it is frozen.
- **Degraded** — a service that is running but cannot fully do its job, usually because a backend or
  a Wayland protocol is absent. It keeps publishing what it can.
- **Edge capability** — something obtainable only through a Wayland connection: gamma, idle
  notification, clipboard capture. Reached through `trait WaylandEdge`.
- **Applet** — a panel component that renders one or more topics and sends commands. Holds no state
  beyond its own widget.
- **External applet** — an applet implemented as a separate process spawned by the panel, speaking a
  line protocol over stdin and stdout.
- **Item** — a tray entry, in the StatusNotifierItem sense. Identified by its SNI `Id` property, not
  by its bus name, which changes when the application restarts.
- **Watcher** — `org.kde.StatusNotifierWatcher`, the bus name that holds the tray roster. Owned by
  `glimpsed`.
- **Host** — `org.kde.StatusNotifierHost-*`, the registration that tells applications a tray exists.
  Also owned by `glimpsed`, alongside the watcher.
