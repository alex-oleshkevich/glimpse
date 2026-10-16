# glimpse-sdk

Write a [glimpse](https://github.com/alex-oleshkevich/glimpse) panel applet in React. The applet is
a Node program: the panel starts it, and the SDK speaks the panel's NDJSON protocol on the
unix socket named by `GLIMPSE_APPLET_SOCKET`, which the panel sets (without it the applet exits with code 1).

```tsx
import { Indicator, Popover, Row, run, Section } from "glimpse-sdk";

await run(
  <>
    <Indicator icon="face-smile-symbolic" tooltip="Hello">Hello</Indicator>
    <Popover icon="face-smile-symbolic" title="Hello">
      <Section title="Hello">
        <Row title="It works" />
      </Section>
    </Popover>
  </>,
);
```

## Start

```
glimpsectl applet new
cd <dir> && npm install
glimpsectl applet run --dev .
```

`applet new` writes `index.tsx`, `package.json`, `tsconfig.json`, `devconfig.toml`, `README.md`
and the desktop entry. The entry runs `node --run start`, which bundles everything into
`build/applet.mjs` and starts it.

Set `"jsxImportSource": "glimpse-sdk"` in `tsconfig.json`.

## API

- Components: `Indicator`, `Popover`, `Addon`, `Container`, `Section`, `Row`, `ActionRow`,
  `ToggleRow`, `MeterRow`, `ExpanderRow`, `SliderRow`, `Start`, `End`, `Entry`, `Placeholder`,
  `Box`, `Label`, `Image`, `Button`, `Switch`, `Scale`, `Spinner`, `Separator`, `FactList`,
  `ChoiceList`, `Readout`, `Notice`, `Calendar`, `Dots`, `DropDown`, `ToggleButton`,
  `CheckButton`, `Revealer`, `Stack`, `RangeBar`, `Bars` and `Sparkline`.
- Hooks: `useState`, `useEffect`, `useOptions`, `usePlacement`, `usePopoverOpen`,
  `useAppletName`, `useStateDir` and `usePersistentState`. `useStateDir` returns the applet's
  private, persistent directory. `usePersistentState(key, initial)` works like `useState`: it
  loads `key` from `state.json` in that directory on the first render (a missing or corrupt
  file falls back to `initial`) and writes the whole JSON object back when the value changes,
  so several keys share one file. It persists nothing when the state dir is empty.
- Host requests: `notify`, `copy`, `openUri`, `launch` and `closePopover`.
- Services: `useMpris` with `mpris.*`, `useCompositor` with
  `compositor.*`. `positionNow` advances a playing track's position.

An exec applet runs unconfined as a systemd user unit and has the user's full session.

The wire format is specified in [PROTOCOL.md](PROTOCOL.md).

## Testing

Import `render` from `glimpse-sdk/testing` to run an applet with an in-memory host. Its
`tree()` returns the current root node and `requests()` lists host requests. `click(id)`
activates an ActionRow or clicks a Button; `setOptions(options)` and `setPopoverOpen(open)` send
host messages. Call `close()` before rendering another applet. The test host checks the shape
of each commit, not the panel's nesting and size rules; `glimpsectl applet run --dev` enforces those.
