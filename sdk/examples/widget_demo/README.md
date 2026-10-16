# Widget demo

A development fixture that fills the popover shell slots. The built-in twin is an applet with `extends = "widget-demo"`.

Build the local SDK first, from the repository root:

```
cd sdk/sdk-ts && npm ci && npm run build
```

`.npmrc` (`install-links=true`) copies that build in.

From this directory:

```
npm install
glimpsectl applet run --dev .
```
