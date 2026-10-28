# glimpse-weather

`glimpse-weather` owns weather fetching independently of the panel. `Here` follows the settings
daemon's geolocation property through an injected watch receiver; fixed coordinates and named
places are independent of it. This process acquires no location, owns `me.aresa.Glimpse.Weather`
and exports only the `me.aresa.Glimpse.Weather1` interface.

The interface exposes one coherent `Snapshot` property plus `WatchPlace` and `Refresh`.
`WatchPlace(kind, latitude, longitude, location)` uses kind `0` for here, kind `1` for fixed
coordinates, and kind `2` for a named location. Latitude and longitude are ignored for kinds `0`
and `2`; `location` is ignored for kinds `0` and `1`. A snapshot's positional place tuple retains
the requested kind, coordinates, and location name, then supplies resolved coordinates plus a
canonical city and ISO 3166-1 alpha-2 country code; either canonical name field is an empty string
when it is unavailable. A watch is the 30-minute service lease, so consumers renew it
while they need the place and the provider fetches nothing when no lease remains. The process keeps
no client registry and does not own solar state, Night Light, UI theme, or regional configuration
beyond applying the configured weather units.

The panel and the provider read the same `config.toml` on their own. An explicit
`glimpse-panel --config-dir` applies only to the panel process; D-Bus activation cannot inherit that
argument, so an alternate stack starts `glimpse-weather --config-dir <dir>` separately before the
panel.

Glimpse's logs report process, bus-name, service-health, configuration, watch-kind, refresh, and
shutdown transitions without coordinates, provider URLs, or response payloads. A broad dependency
log filter such as `debug` can still include HTTP connection destinations from the HTTP stack.
