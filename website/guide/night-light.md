# Set up the night light

The night light warms the screen's colors at night. It lives under `[night-light]`; the full setting
list is in the [night light reference](/config/services/night-light).

## Follow the sun

The default needs nothing written: `schedule = "automatic"` warms the screen from sunset to sunrise
where you are. That needs your location, which glimpse gets from GeoClue, the system location
service, or from coordinates you write yourself (see [location](/config/services/appearance#location)).

## Keep fixed hours

```toml
[night-light]
schedule = "schedule"
start-time = "21:00"
end-time = "07:00"
```

Times are local, `HH:MM`.

## Pick the warmth

`temperature` is the night color in Kelvin, from 1000 to 6500. Lower is warmer, and 6500 is the
screen's own color. The default is 4200.

```toml
[night-light]
temperature = 3400
```

`transition-minutes` sets how slowly the change between day and night happens (15 by default).

## Pause it for tonight

```sh
glimpsectl sunset pause
```

The pause keeps neutral daylight until the current night ends, and is refused during the day.
`glimpsectl sunset resume` ends it early. To hold one temperature for a while, run
`glimpsectl sunset temperature 3000`, and `glimpsectl sunset temperature clear` to follow the
schedule again. Neither command writes the config file.

::: tip If the screen never warms
Your compositor has to let clients set the display gamma. Where it does not, the night light has no
effect.
:::
