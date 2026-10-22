# Show the weather for your city

The weather applet can follow where you are or watch a fixed place. The settings are in the
[information applets reference](/config/applets/information#weather) and under
[location](/config/services/appearance#location).

## Follow where you are

This is the default: glimpse asks GeoClue, the system location service, and the applet shows the
weather there. Nothing to write.

## Set your coordinates by hand

To keep location services off, or when GeoClue cannot place you, set this in `config.toml`:

```toml
[geolocation]
provider = "manual"
latitude = 52.37
longitude = 4.90
```

Both are required with `manual`. Latitude runs from -90 to 90 and longitude from -180 to 180;
outside that range the settings service reports the location as unknown and everything that needs
a place goes without, and `glimpsectl config validate` does not catch it. This one location also
drives the night light and automatic dark mode.

## Pin the applet to one place

The applet can watch a place that does not move with you. In `panel.toml`, name a city and its
ISO 3166-1 alpha-2 country code in capitals, and glimpse resolves it through Open-Meteo:

```toml
[applets.weather.place]
at = "location"
name = "Amsterdam, NL"
```

Resolving the name sends a geocoding request to Open-Meteo, and the applet shows the name
Open-Meteo returns, which can differ from your spelling. To skip the request, give the coordinates
directly with `at = "latlon"` and `latitude`/`longitude` in degrees; the place then reads as the
nearest city from an offline city list. Either way, `label` sets the name the applet shows in its
tooltip and at the top of its popover:

```toml
[applets.weather]
label = "Home"
```

::: tip If the applet stays empty
The name must be a city Open-Meteo knows, written as `City, CC`. Check the
[glimpse-weather log](/guide/troubleshooting#read-the-logs) for a warning, or use
`at = "latlon"`.
:::

## Pick the units

`[regional] units` applies to everything glimpse shows: `locale` (the default, follows your system
locale; American English gives imperial), `metric` (Celsius, km/h, millimeters) or `imperial`
(Fahrenheit, mph, inches):

```toml
[regional]
units = "metric"
```

## Pick the forecast source

Forecasts come from Open-Meteo by default; it needs no account and no key. For the Norwegian
Meteorological Institute, the only source that carries official warnings:

```toml
[weather]
provider = "met-no"
```

## Check what is fetched

`glimpsectl weather status` prints the state of the fetch (`available`, `stale`, a `reason` when
it is not available, and when it was last `updated`), then one row per watched place. A place
stays listed for about 30 minutes after the last applet stops watching it, and with nothing
watching, the command prints `nothing is watching a place, so nothing is fetched`. When a fetch
fails, the applet shows a warning icon and its tooltip gives the reason.
`glimpsectl weather refresh` fetches now instead of waiting for the next poll.
