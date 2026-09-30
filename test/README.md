# test

Applet id: `me.test`.

Run `npm install`, then `glimpsectl applets dev .`. The desktop entry runs `node --run start`, which
builds `build/applet.mjs` and starts it, so every panel start runs the current source. After an
edit, `glimpsectl applets dev me.test` restarts it. `npm run check` type-checks.

Add `center = ["me.test"]` to a panel's applet zone.
