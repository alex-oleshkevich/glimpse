# glimpse-calendar

`glimpse-calendar` owns calendar feeds independently of the panel and exports
`me.aresa.Glimpse.Calendar1` at `me.aresa.Glimpse.Calendar`. The snapshot contains the
near-window occurrences and current health; the bus name is claimed after every initial source
finishes and its results are expanded. Range queries leave the snapshot unchanged; Refresh rereads feeds.

**A failing source is one notification, not one per poll.** It is posted when the set of failing
sources changes, replaced in place, and closed when every source reads again; a changed reason alone
posts nothing.

The provider reads `config.toml` itself. An alternate configuration starts
`glimpse-calendar --config-dir <dir>` separately, because D-Bus activation cannot inherit a
consumer's command-line arguments.
