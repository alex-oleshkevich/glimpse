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

`glimpsectl night-light` checks and overrides the running service without writing the file, through
`status`, `pause`, `resume`, `schedule` and `temperature`. The guide covers them under
[Change it for now](/guide/night-light#change-it-for-now) and
[Check what it is doing](/guide/night-light#check-what-it-is-doing). The night light needs a
compositor that lets clients set the display gamma.
