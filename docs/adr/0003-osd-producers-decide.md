# Producers decide when and what the on-screen display shows

Producers decide when an OSD appears and what it shows; `glimpse-agents` only renders `Osd1` messages. Audio and brightness state already lives in the panel, so a second reader duplicates libpulse and contends for DDC/CI i2c access. We accept that there is no built-in OSD while the panel is down and no feedback when a key press does not change a value at its limit.

## Considered Options

- Let agents run its own watchers: duplicates the panel's audio and brightness readers, including libpulse and DDC/CI i2c access.
- Add a provider process: adds another process and another state owner for information the panel already has.
- Let agents cache the last value per kind for a peek: adds state and a peek contract to a renderer that only needs to show each message it receives.
