# Night light

`[night-light]` warms the screen color in the evening. With the default `automatic` schedule it
follows sunset and sunrise at your [location](./appearance#location).

<ConfigTable path="night-light" />

For fixed hours instead:

```toml
[night-light]
schedule = "schedule"
start-time = "21:00"
end-time = "07:00"
temperature = 3500
```

Two commands change it for now without editing the file: `glimpsectl night-light pause` keeps daylight
color until the current night ends, and `glimpsectl night-light temperature 3000` holds that temperature
until you run `glimpsectl night-light temperature --clear`. The night light needs a
compositor that lets clients set the display gamma.
