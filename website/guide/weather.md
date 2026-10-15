# Show the weather for your city

The weather applet can follow where you are or watch a fixed place. The settings are in the
[information applets reference](/config/applets/information#weather) and under
[location](/config/services/appearance#location).

## Follow where you are

This is the default: glimpse asks GeoClue, the system location service, and the applet shows the
weather there. Nothing to write.

## Set your coordinates by hand

To keep location services off, or when GeoClue cannot place you:

```toml
[geolocation]
provider = "manual"
latitude = 52.37
longitude = 4.90
```

Both are required with `manual`. This one location also drives the night light and automatic dark
mode.

## Pin the applet to one place

The applet can watch a place that does not move with you. Name a city and its ISO 3166-1 alpha-2
country code, and glimpse resolves it through Open-Meteo:

```toml
[applets.weather.place]
at = "location"
name = "Amsterdam, NL"
```

Resolving the name sends a geocoding request to Open-Meteo. To skip it, give the coordinates
directly with `at = "latlon"` and `latitude`/`longitude` in degrees. `label` on the applet overrides
the name shown in the bar:

```toml
[applets.weather]
label = "Home"
```

## Pick the units

`[regional] units` applies to everything glimpse shows: `locale` (the default, follows your system
locale), `metric` (Celsius, km/h, millimeters) or `imperial` (Fahrenheit, mph, inches):

```toml
[regional]
units = "metric"
```

`glimpsectl weather status` prints every watched place and its current conditions, and
`glimpsectl weather refresh` fetches now instead of waiting for the next poll.
