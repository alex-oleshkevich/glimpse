# glimpse-weather

`glimpse-weather` owns geolocation and weather fetching independently of the panel. It starts the
existing `Geolocation` service, injects its typed handle into the existing `Weather` service, and
exports only `me.aresa.Glimpse.Weather1` on the session bus.

The interface exposes one coherent `Snapshot` property plus `WatchPlace` and `Refresh`. A watch is
the existing 30-minute service lease, so consumers renew it while they need the place and the
provider fetches nothing when no lease remains. The process keeps no client registry and does not
own solar state, Night Light, UI theme, or regional configuration beyond applying the configured
weather units.

Glimpse's logs report process, bus-name, service-health, configuration, watch-kind, refresh, and
shutdown transitions without coordinates, provider URLs, or response payloads. A broad dependency
log filter such as `debug` can still include HTTP connection destinations from the HTTP stack.
