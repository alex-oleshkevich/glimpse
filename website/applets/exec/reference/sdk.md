# TypeScript SDK

The `glimpse-sdk` package exports React components, hooks, host requests, and supporting types for exec applets. Configure TypeScript with `"jsx": "react-jsx"` and `"jsxImportSource": "glimpse-sdk"`.

::: tip Create an applet

```sh
glimpsectl applet new --language ts --id me.aresa.GlimpseTodo --yes
cd glimpse-todo
just dev
```

The [todo tutorial](/applets/exec/) builds a full applet from this project. `just lint` checks the
code with [Biome](https://biomejs.dev/), which the project's `npm install` brings; its site has the
editor extension and a standalone install.

:::

## Components

Every component is imported from `glimpse-sdk` and documented, with its props and TypeScript usage,
in [Elements](/applets/exec/reference/elements). `React.ComponentProps<typeof Indicator>` gives a
component's props type.

## Hooks

```ts
function useState<T>(initial: T | (() => T)): [T, (action: T | ((prev: T) => T)) => void];
function useEffect(effect: () => void | (() => void), deps?: readonly unknown[]): void;
function useOptions<T extends Record<string, unknown> = Record<string, unknown>>(): T;
function usePlacement(): Placement | null;
function usePopover(): { open: boolean; close: () => void };
function useAppletName(): string;
function useStateDir(): string;
function usePersistentState<T>(key: string, initial: T): [T, (action: T | ((prev: T) => T)) => void];
```

- `useState` and `useEffect` are React's.
- `useOptions` returns the current options. The type parameter is a cast, not validation.
- `usePlacement` returns `null` until the panel sends the placement.
- `usePopover` tells whether the popover is open, and `close` closes it. `close` is the same function on every render.
- `useAppletName` returns this configured instance's name.
- `useStateDir` returns the applet's own directory for files, created with mode `0700`.
- `usePersistentState` works like `useState` and saves `key` to `state.json` in that directory. It loads the saved value on the first render; a missing or unreadable file gives `initial`. Several keys share one file.

## Desktop actions

```ts
function notify(message: {
  summary: string;
  body?: string;
  icon?: string;
  urgency?: "low" | "normal" | "critical";
}): void;
function copy(text: string): void;
function openUri(uri: string): void;
function launch(id: string): void;
```

`notify` defaults to an empty body and normal urgency. It runs `notify-send` from the applet process and `copy` runs `wl-copy`, so libnotify and wl-clipboard must be installed; the notification daemon applies its own limits. `launch` takes an installed desktop-file id with or without `.desktop`. `openUri` accepts only `http`, `https`, `mailto`, and eligible `file` URIs, at most 2,048 bytes. See [Notify, copy, open links and launch apps](/applets/exec/host) for file restrictions.

## Runtime and JSX exports

```ts
function run(element: unknown, io?: Io): Promise<void>;
function createElement(
  type: unknown,
  props?: Record<string, unknown> | null,
  ...children: unknown[]
): unknown;
const Fragment: typeof React.Fragment;
function jsx(type: unknown, props: unknown, key?: unknown): unknown;
function jsxs(type: unknown, props: unknown, key?: unknown): unknown;
function jsxDEV(type: unknown, props: unknown, key?: unknown): unknown;

interface Io {
  writeLine: (line: string) => void;
  lines: AsyncIterable<string | string[]>;
}

interface Placement {
  output: string | null;
  position: string;
  orientation: string;
  zone: string;
  size: number;
}

type Node =
  | string | number | boolean | null | undefined | Node[]
  | { type: unknown; props: unknown; key: unknown };

type FC<P = object> = (props: P & { key?: string | number }) => Node;
```

`run` connects to the socket named by `GLIMPSE_APPLET_SOCKET`. Without that variable and without an
`io`, it prints `GLIMPSE_APPLET_SOCKET is not set: run the applet under the panel` and exits 1. A
supplied `Io` replaces the socket. `run` redirects console methods to stderr and exits 0 when the
session ends. `jsx`, `jsxs` and `jsxDEV` are what the JSX runtime calls; you never call them.

The host's placement values are `top`, `bottom`, `left`, or `right` for `position`; `horizontal` or `vertical` for `orientation`; and `left`, `center`, or `right` for `zone`. `size` is the bar's logical-pixel thickness, and `output` is its connector name or `null`.

The separate `glimpse-sdk/testing` entry point exports `render` and the `Rendered` type. See [Test an applet](/applets/exec/testing) for its methods.
