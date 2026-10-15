# {{ name }}

Applet id: `{{ id }}`.

Run `npm install`, then `glimpsectl applets run --dev .`: it loads `config.toml` and attaches the
applet with its options and env. Options edits update it; source
edits restart it. The desktop entry runs `node --run start` to build and launch it.
`npm run check` type-checks.

Add `center = ["{{ id }}"]` to a panel's applet zone.
