# glimpse-settings-daemon

The headless desktop settings process owns `me.aresa.Glimpse.Settings` on the session bus and
publishes location, solar phase and desktop mode through `me.aresa.Glimpse.Settings1` at `/me/aresa/Glimpse/Settings`.
It acquires the name exclusively before starting its services, so a second instance exits without
acquiring location or writing desktop preferences. SIGTERM and SIGINT stop its services and release the name.

Solar calculation lives in `glimpse-services`; this binary supplies its configured location input
and exports read-only `Solar`, `Geolocation` and `Mode` properties. Weather follows geolocation for `Here`.
GeoClue retries, city accuracy, the last fix kept under `state_dir()` and transient client cleanup belong to the existing
location service. `--config` selects a document, file changes and SIGHUP
reload it, and a rejected reload keeps the last valid configuration. Sunset owns gamma control
and follows solar phase without calculating it or acquiring location itself.

`appearance.auto-dark` selects solar policy (`auto`, the default) or system preference observation
(`system`). The mode service receives an injected writer; GSettings `org.gnome.desktop.interface`
`color-scheme` is the desktop adapter. `SetTemporaryMode` holds light or dark until `Resume`, a policy
change or clean shutdown. System policy restores the preference captured before the hold only while
the hold's last write remains in force. Holds are not persisted, so a crash can leave that preference
applied. `appearance.color-scheme` remains the local Glimpse override.
