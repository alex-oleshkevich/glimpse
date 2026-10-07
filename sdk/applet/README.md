# glimpse-sdk

Write a [glimpse](https://github.com/alex-oleshkevich/glimpse) panel applet in React. The applet is
a Node program: the panel starts it, and the SDK speaks the panel's NDJSON protocol on
stdin and stdout.

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
glimpsectl applets new
cd <dir> && npm install
glimpsectl applets run --dev .
```

`applets new` writes `index.tsx`, `package.json`, `tsconfig.json`, `config.toml`, `README.md`
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
  `useAppletName` and `useStateDir`. `useStateDir` returns the applet's private, persistent
  directory.
- Host requests: `notify`, `copy`, `openUri`, `launch` and `closePopover`.
- Services, each behind its own grant: `useMpris` with `mpris.*`, `useCompositor` with
  `compositor.*`. `positionNow` advances a playing track's position.

The applet runs in a sandbox. Beyond its state directory, it reaches only the paths the user
grants under `[applets."<id>"] sandbox` in the glimpse config, and it has no network unless
`network = true`. It sees media players only with `mpris = true` and workspaces and windows
only with `compositor = true`, and `launch` starts only the desktop ids listed in
`launch = [...]`.

The wire format is specified in [PROTOCOL.md](PROTOCOL.md).

## Testing

Import `render` from `glimpse-sdk/testing` to run an applet with an in-memory host. Its
`tree()` returns the current root node and `requests()` lists host requests. `click(id)`
activates an ActionRow or clicks a Button; `setOptions(options)` and `setPopoverOpen(open)` send
host messages. Call `close()` before rendering another applet. The test host checks the shape
of each commit, not the panel's nesting and size rules; `glimpsectl applets run --dev` enforces those.
