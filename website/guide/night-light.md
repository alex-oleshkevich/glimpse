# Set up the night light

The night light warms the screen's colors at night. It lives under `[night-light]`; the full setting
list is in the [night light reference](/config/services/night-light).

## Follow the sun

The default needs nothing written: `schedule = "automatic"` warms the screen from sunset to sunrise
where you are. That needs your location, which glimpse gets from GeoClue, the system location
service, or from coordinates you write yourself (see [location](/config/services/appearance#location)).
Without a location the screen stays neutral.

## Keep fixed hours

```toml
[night-light]
schedule = "schedule"
start-time = "21:00"
end-time = "07:00"
```

Times are local, `HH:MM`. Both are required and must differ, and the window can cross midnight.
With only one of them set, the night light stops changing the screen; `glimpsectl night-light status`
says why.

## Pick the warmth

`temperature` is the night color in Kelvin, from 1000 to 6500. Lower is warmer, and 6500 is the
screen's own color. The default is 4200.

```toml
[night-light]
temperature = 3400
```

`transition-minutes` sets how long the change between day and night takes (15 by default). The
change finishes at sunset or at `start-time`, so it starts that many minutes before. `0` changes over a few seconds
instead of a full transition.

## Change it for now

These commands never write the config file, and editing `[night-light]` or restarting the night
light service drops all of them:

| Command | Effect |
| --- | --- |
| `glimpsectl night-light pause` | keeps neutral daylight until the current night ends; refused during the day |
| `glimpsectl night-light resume` | ends a pause early |
| `glimpsectl night-light temperature 3000` | holds that temperature until the next sunset or sunrise, or your `start-time` or `end-time` |
| `glimpsectl night-light temperature --clear` | follows the schedule again |
| `glimpsectl night-light schedule off` | turns it off; `automatic` or `schedule` forces those instead |

## Check what it is doing

```sh
glimpsectl night-light status
```

It prints the mode, the temperature in use and the night target. When the screen is not being
changed, the `serving` line says why.

::: tip If the screen never warms
Your compositor has to let clients set the display gamma, and only one program can do that at a
time. Stop wlsunset, gammastep or hyprsunset if one is running, then check
`glimpsectl night-light status`.
:::
