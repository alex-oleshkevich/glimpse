# glimpse-weather

`glimpse-weather` owns geolocation and weather fetching independently of the panel. It starts the
existing `Geolocation` service, injects its typed handle into the existing `Weather` service, and
exports only `me.aresa.Glimpse.Weather1` on the session bus.

The interface exposes one coherent `Snapshot` property plus `WatchPlace` and `Refresh`.
`WatchPlace(kind, latitude, longitude, location)` uses kind `0` for here, kind `1` for fixed
coordinates, and kind `2` for a named location. Latitude and longitude are ignored for kinds `0`
and `2`; `location` is ignored for kinds `0` and `1`. A snapshot's positional place tuple retains
the requested kind, coordinates, and location name, then supplies resolved coordinates plus a
canonical city and ISO 3166-1 alpha-2 country code; either canonical name field is an empty string
when it is unavailable. A watch is the existing 30-minute service lease, so consumers renew it
while they need the place and the provider fetches nothing when no lease remains. The process keeps
no client registry and does not own solar state, Night Light, UI theme, or regional configuration
beyond applying the configured weather units.

With the normal layered configuration, the panel and provider read the same `config.toml` on their
own. An explicit `glimpse-panel --config` applies only to the panel process; D-Bus activation cannot
inherit that argument, so an alternate stack starts `glimpse-weather --config <path>` separately
before the panel. The startup log records the provider's explicit configuration path when one is
set.

Glimpse's logs report process, bus-name, service-health, configuration, watch-kind, refresh, and
shutdown transitions without coordinates, provider URLs, or response payloads. A broad dependency
log filter such as `debug` can still include HTTP connection destinations from the HTTP stack.
