export type Placement = {
  output: string | null;
  position: string;
  orientation: string;
  zone: string;
  size: number;
};

export type Io = {
  writeLine: (line: string) => void;
  lines: AsyncIterable<string>;
};

type Json = string | number | boolean | null | Json[] | { [key: string]: Json };
type Op = { [key: string]: Json };
type Props = Record<string, unknown>;
type Handler = (...args: unknown[]) => void;

type HostNode = {
  id: number;
  type: string;
  props: Props;
  children: HostNode[];
  attached: boolean;
  handlers: Map<string, Handler>;
  appliedSeq?: number;
  confirmSeq?: number;
};

type Reconciler = {
  createContainer: (...args: unknown[]) => unknown;
  updateContainer: (...args: unknown[]) => void;
  flushSyncFromReconciler: (fn?: () => void) => void;
  "flushPassiveEffects"?: () => boolean;
};

type Session = {
  live: boolean;
  suppress: boolean;
  nextId: number;
  nodes: Map<number, HostNode>;
  ops: Op[];
  orderBefore: Map<number, number[]>;
  reconciler: Reconciler;
  root: unknown;
};

type Store<T> = {
  get: () => T;
  set: (value: T) => void;
  subscribe: (listener: () => void) => () => void;
  clear: () => void;
};

type HostMessage =
  | {
    t: "hello";
    v?: number;
    name?: string;
    options?: Props;
    placement?: Placement;
  }
  | { t: "options"; options?: Props }
  | { t: "placement"; placement?: Placement }
  | { t: "popover"; open?: boolean }
  | {
    t: "event";
    id?: number;
    name?: string;
    args?: unknown[];
    seq?: number | null;
  };

const TEXT = new Set(["label", "indicator", "button", "progress"]);
const SKIP = new Set(["children", "key", "ref"]);
const encoder = new TextEncoder();

const PROP_ORDER: Record<string, readonly string[]> = {
  indicator: [
    "icon",
    "text",
    "tooltip",
    "badge",
    "overlay",
    "dot",
    "severity",
    "attention",
    "notice",
    "className",
    "onPress",
    "onScroll",
  ],
  popover: ["className"],
  hero: ["icon", "title", "subtitle", "className"],
  section: ["title", "count", "className"],
  row: [
    "icon",
    "title",
    "subtitle",
    "value",
    "selected",
    "busy",
    "className",
    "onActivate",
  ],
  switchrow: [
    "icon",
    "title",
    "subtitle",
    "active",
    "busy",
    "className",
    "onToggle",
  ],
  fader: [
    "icon",
    "value",
    "maximum",
    "floor",
    "muted",
    "className",
    "onChange",
    "onMute",
  ],
  entry: ["placeholder", "value", "className", "onChange", "onSubmit"],
  placeholder: ["icon", "title", "description", "className"],
  footer: ["className"],
  box: [
    "orientation",
    "spacing",
    "homogeneous",
    "halign",
    "valign",
    "hexpand",
    "vexpand",
    "className",
  ],
  label: ["text", "wrap", "xalign", "ellipsize", "lines", "className"],
  image: ["icon", "tooltip", "pixelSize", "className"],
  button: ["text", "icon", "tooltip", "sensitive", "className", "onClick"],
  switch: ["active", "sensitive", "className", "onToggle"],
  scale: ["value", "min", "max", "step", "sensitive", "className", "onChange"],
  spinner: ["className"],
  progress: ["fraction", "text", "className"],
  separator: ["orientation", "className"],
};

function createStore<T>(initial: T): Store<T> {
  let value = initial;
  const listeners = new Set<() => void>();
  return {
    get: () => value,
    set(next: T) {
      if (Object.is(next, value)) return;
      value = next;
      for (const listener of [...listeners]) listener();
    },
    subscribe(listener: () => void) {
      listeners.add(listener);
      return () => listeners.delete(listener);
    },
    clear() {
      listeners.clear();
    },
  };
}

export const optionsStore = createStore<Record<string, unknown>>({});
export const placementStore = createStore<Placement | null>(null);
export const nameStore = createStore("");
export const openStore = createStore(false);

let emit = (message: unknown) => {
  Deno.stdout.writeSync(encoder.encode(`${JSON.stringify(message)}\n`));
};

export function setEmit(write: (message: unknown) => void): void {
  emit = write;
}

export function send(message: unknown): void {
  emit(message);
}

let consoleSaved = false;
let originalLog = console.log;
let originalInfo = console.info;
let originalDebug = console.debug;

function writeStderr(...args: unknown[]): void {
  const text = args.map((value) => {
    if (typeof value === "string") return value;
    try {
      const encoded = JSON.stringify(value);
      return encoded ?? String(value);
    } catch {
      return String(value);
    }
  }).join(" ");
  Deno.stderr.writeSync(encoder.encode(`${text}\n`));
}

export function redirectConsole(): void {
  if (!consoleSaved) {
    originalLog = console.log;
    originalInfo = console.info;
    originalDebug = console.debug;
    consoleSaved = true;
  }
  console.log = writeStderr;
  console.info = writeStderr;
  console.debug = writeStderr;
}

export function restoreConsole(): void {
  if (!consoleSaved) return;
  console.log = originalLog;
  console.info = originalInfo;
  console.debug = originalDebug;
}

export function installProcess(): void {
  const global = globalThis as {
    process?: { env?: Record<string, string | undefined> };
  };
  try {
    if (global.process?.env?.NODE_ENV === "production") return;
  } catch {
    global.process = { env: { NODE_ENV: "production" } };
    return;
  }
  if (!global.process) {
    global.process = { env: { NODE_ENV: "production" } };
    return;
  }
  if (!global.process.env) global.process.env = {};
  try {
    if (!global.process.env.NODE_ENV) {
      global.process.env.NODE_ENV = "production";
    }
  } catch {
    global.process = { env: { NODE_ENV: "production" } };
  }
}

let reconcilerFactory: ((config: object) => Reconciler) | null = null;
let eventPriority = 32;

async function loadReconciler(): Promise<(config: object) => Reconciler> {
  if (reconcilerFactory) return reconcilerFactory;
  installProcess();
  const loaded = await import("npm:react-reconciler@0.33.0");
  const constants = await import("npm:react-reconciler@0.33.0/constants.js");
  eventPriority = constants.DefaultEventPriority ?? 32;
  const factory = loaded.default ?? loaded;
  reconcilerFactory = factory as (config: object) => Reconciler;
  return reconcilerFactory;
}

function textOf(children: unknown): string | undefined {
  if (typeof children === "string" || typeof children === "number") {
    return String(children);
  }
  if (
    Array.isArray(children) &&
    children.every((child) =>
      typeof child === "string" || typeof child === "number"
    )
  ) {
    return children.join("");
  }
  return undefined;
}

function orderedKeys(type: string, props: Props): string[] {
  const preferred = PROP_ORDER[type] ?? [];
  const rest = Object.keys(props).filter((key) =>
    !preferred.includes(key) && !SKIP.has(key)
  );
  return [...preferred, ...rest];
}

function wireProps(type: string, props: Props): Props {
  const out: Props = {};
  const text = TEXT.has(type) ? textOf(props.children) : undefined;
  for (const key of orderedKeys(type, props)) {
    if (SKIP.has(key)) continue;
    if (key === "text") {
      if (text !== undefined) out.text = text;
      else if (
        typeof props.text === "string" || typeof props.text === "number"
      ) {
        out.text = String(props.text);
      }
      continue;
    }
    if (!Object.hasOwn(props, key)) continue;
    const value = props[key];
    if (typeof value === "function") {
      out[key] = true;
      continue;
    }
    if (value === undefined) continue;
    out[key] = value;
  }
  return out;
}

function same(left: unknown, right: unknown): boolean {
  if (Object.is(left, right)) return true;
  if (left === null || right === null) return false;
  if (typeof left !== "object" || typeof right !== "object") return false;
  return JSON.stringify(left) === JSON.stringify(right);
}

function diffProps(type: string, prev: Props, next: Props): Props | null {
  const before = wireProps(type, prev);
  const after = wireProps(type, next);
  const seen = new Set<string>();
  const out: Props = {};
  let changed = false;
  for (const key of [...orderedKeys(type, next), ...orderedKeys(type, prev)]) {
    if (seen.has(key)) continue;
    seen.add(key);
    const hasBefore = Object.hasOwn(before, key);
    const hasAfter = Object.hasOwn(after, key);
    if (!hasBefore && !hasAfter) continue;
    if (hasBefore && hasAfter && same(before[key], after[key])) continue;
    out[key] = hasAfter ? after[key] : null;
    changed = true;
  }
  return changed ? out : null;
}

function bindHandlers(node: HostNode, props: Props): void {
  node.handlers = new Map();
  for (const [key, value] of Object.entries(props)) {
    if (typeof value === "function") node.handlers.set(key, value as Handler);
  }
}

function blankNode(type: string): HostNode {
  return {
    id: 0,
    type,
    props: {},
    children: [],
    attached: false,
    handlers: new Map(),
  };
}

function serialize(session: Session, node: HostNode): Json {
  if (node.id === 0) node.id = session.nextId++;
  node.attached = true;
  session.nodes.set(node.id, node);
  return {
    id: node.id,
    type: node.type,
    props: node.props as Json,
    children: node.children.map((child) => serialize(session, child)),
  };
}

function forget(session: Session, node: HostNode): void {
  session.nodes.delete(node.id);
  for (const child of node.children) forget(session, child);
  node.children = [];
  node.handlers.clear();
  node.attached = false;
}

const BODY = new Set([
  "section",
  "row",
  "switchrow",
  "fader",
  "entry",
  "placeholder",
  "box",
  "label",
  "image",
  "button",
  "switch",
  "scale",
  "spinner",
  "progress",
  "separator",
  "unsupported",
]);

function admit(parent: HostNode, child: HostNode): void {
  const body = BODY.has(child.type);
  const allowed = parent.type === "root"
    ? child.type === "indicator" || child.type === "popover"
    : parent.type === "popover"
    ? child.type === "hero" || child.type === "footer" || body
    : parent.type === "section" || parent.type === "box"
    ? body && child.type !== "section"
    : parent.type === "footer"
    ? ["button", "row", "label", "box"].includes(child.type)
    : false;
  if (!allowed) {
    throw new Error(`${child.type} cannot be a child of ${parent.type}`);
  }
  const unique = (parent.type === "root" && child.type === "popover") ||
    (parent.type === "popover" && ["hero", "footer"].includes(child.type));
  if (
    unique &&
    parent.children.some((item) => item !== child && item.type === child.type)
  ) {
    throw new Error(`${parent.type} already has a ${child.type}`);
  }
}

function place(
  session: Session,
  parent: HostNode,
  child: HostNode,
  before: HostNode | null,
): void {
  if (!session.live || session.suppress) return;
  admit(parent, child);
  if (child.attached && !session.orderBefore.has(parent.id)) {
    session.orderBefore.set(parent.id, parent.children.map((item) => item.id));
  }
  const existing = parent.children.indexOf(child);
  if (existing >= 0) parent.children.splice(existing, 1);
  const index = before
    ? parent.children.indexOf(before)
    : parent.children.length;
  parent.children.splice(index < 0 ? parent.children.length : index, 0, child);
  if (child.attached) {
    session.ops.push({
      op: "move",
      parent: parent.id,
      id: child.id,
      before: before ? before.id : null,
    });
    return;
  }
  session.ops.push({
    op: "insert",
    parent: parent.id,
    before: before ? before.id : null,
    node: serialize(session, child),
  });
}

function unplace(session: Session, parent: HostNode, child: HostNode): void {
  if (!session.live || session.suppress) return;
  const index = parent.children.indexOf(child);
  if (index >= 0) parent.children.splice(index, 1);
  if (child.attached && child.id !== 0) {
    session.ops.push({ op: "remove", parent: parent.id, id: child.id });
  }
  forget(session, child);
}

function lisIndices(seq: number[]): number[] {
  const previous = new Array<number>(seq.length).fill(-1);
  const tails: number[] = [];
  for (let index = 0; index < seq.length; index++) {
    let low = 0;
    let high = tails.length;
    while (low < high) {
      const mid = (low + high) >> 1;
      if (seq[tails[mid]] < seq[index]) low = mid + 1;
      else high = mid;
    }
    if (low > 0) previous[index] = tails[low - 1];
    if (low === tails.length) tails.push(index);
    else tails[low] = index;
  }
  const chosen: number[] = [];
  for (
    let index = tails.length === 0 ? -1 : tails[tails.length - 1];
    index >= 0;
    index = previous[index]
  ) {
    chosen.push(index);
  }
  chosen.reverse();
  return chosen;
}

function minimalMoveOps(session: Session): Op[] {
  const parents = [...new Set(session.ops.map((op) => op.parent as number))];
  const next: Op[] = [];
  for (const parent of parents) {
    const before = session.orderBefore.get(parent);
    const node = session.nodes.get(parent);
    if (!before || !node) {
      next.push(...session.ops.filter((op) => op.parent === parent));
      continue;
    }
    const after = node.children.map((child) => child.id);
    const position = new Map(before.map((id, index) => [id, index]));
    if (
      after.length !== before.length || after.some((id) => !position.has(id))
    ) {
      next.push(...session.ops.filter((op) => op.parent === parent));
      continue;
    }
    const stable = new Set(
      lisIndices(after.map((id) => position.get(id) ?? 0)),
    );
    for (let index = 0; index < after.length; index++) {
      if (stable.has(index)) continue;
      let anchor: number | null = null;
      for (let cursor = index + 1; cursor < after.length; cursor++) {
        if (stable.has(cursor)) {
          anchor = after[cursor];
          break;
        }
      }
      next.push({ op: "move", parent, id: after[index], before: anchor });
    }
  }
  return next;
}

function makeHost(session: Session): object {
  let priority = eventPriority;
  return {
    supportsMutation: true,
    supportsPersistence: false,
    supportsHydration: false,
    isPrimaryRenderer: true,
    noTimeout: -1,
    scheduleTimeout: setTimeout,
    cancelTimeout: clearTimeout,
    supportsMicrotasks: true,
    scheduleMicrotask: queueMicrotask,
    getRootHostContext: () => ({}),
    getChildHostContext: (context: unknown) => context,
    getPublicInstance: (instance: unknown) => instance,
    shouldSetTextContent: (type: string, props: Props) =>
      TEXT.has(type) && textOf(props.children) !== undefined,
    createInstance(type: string, props: Props) {
      const node = blankNode(type);
      node.props = wireProps(type, props);
      bindHandlers(node, props);
      return node;
    },
    createTextInstance(text: string) {
      throw new Error(
        `raw text "${text}" is not a child; put it on the text prop`,
      );
    },
    appendInitialChild: (parent: HostNode, child: HostNode) => {
      admit(parent, child);
      parent.children.push(child);
    },
    appendChild: (parent: HostNode, child: HostNode) =>
      place(session, parent, child, null),
    appendChildToContainer: (parent: HostNode, child: HostNode) =>
      place(session, parent, child, null),
    insertBefore: (parent: HostNode, child: HostNode, before: HostNode) =>
      place(session, parent, child, before),
    insertInContainerBefore: (
      parent: HostNode,
      child: HostNode,
      before: HostNode,
    ) => place(session, parent, child, before),
    removeChild: (parent: HostNode, child: HostNode) =>
      unplace(session, parent, child),
    removeChildFromContainer: (parent: HostNode, child: HostNode) =>
      unplace(session, parent, child),
    commitUpdate(node: HostNode, _type: string, prev: Props, next: Props) {
      if (!session.live || session.suppress) return;
      bindHandlers(node, next);
      const diff = diffProps(node.type, prev, next);
      if (!diff) return;
      const op: { [key: string]: Json } = {
        op: "set",
        id: node.id,
        props: diff as Json,
      };
      if (node.confirmSeq !== undefined && Object.hasOwn(diff, "value")) {
        op.seq = node.confirmSeq;
        node.confirmSeq = undefined;
      }
      node.props = wireProps(node.type, next);
      session.ops.push(op);
    },
    finalizeInitialChildren: () => false,
    prepareForCommit: () => null,
    resetAfterCommit() {
      if (!session.live || session.suppress) {
        session.ops = [];
        session.orderBefore.clear();
        return;
      }
      if (
        session.ops.length > 0 && session.ops.every((op) => op.op === "move")
      ) {
        session.ops = minimalMoveOps(session);
      }
      session.orderBefore.clear();
      if (session.ops.length > 0) send({ t: "commit", ops: session.ops });
      session.ops = [];
    },
    clearContainer: () => {},
    detachDeletedInstance: () => {},
    preparePortalMount: () => {},
    getCurrentEventPriority: () => eventPriority,
    setCurrentUpdatePriority: (next: number) => {
      priority = next;
    },
    getCurrentUpdatePriority: () => priority,
    resolveUpdatePriority: () => priority || eventPriority,
    shouldAttemptEagerTransition: () => false,
    maySuspendCommit: () => false,
    maySuspendCommitOnUpdate: () => false,
    maySuspendCommitInSyncRender: () => false,
    preloadInstance: () => true,
    startSuspendingCommit: () => {},
    suspendInstance: () => {},
    waitForCommitToBeReady: () => null,
    requestPostPaintCallback: () => {},
    trackSchedulerEvent: () => {},
    resolveEventType: () => null,
    resolveEventTimeStamp: () => -1.1,
    NotPendingTransition: null,
    HostTransitionContext: {
      $$typeof: Symbol.for("react.context"),
      _currentValue: null,
      _currentValue2: null,
    },
    resetFormInstance: () => {},
    bindToConsole: (method: string, args: unknown[]) =>
      Function.prototype.bind.apply(console[method as "error"], [
        console,
        ...args,
      ]),
    commitMount: () => {},
    commitTextUpdate: () => {},
    resetTextContent: () => {},
    hideInstance: () => {},
    hideTextInstance: () => {},
    unhideInstance: () => {},
    unhideTextInstance: () => {},
    getInstanceFromNode: () => null,
  };
}

let current: Session | null = null;

function flush(session: Session, fn?: () => void): void {
  session.reconciler.flushSyncFromReconciler(fn);
  session.reconciler["flushPassiveEffects"]?.();
}

function retire(session: Session): void {
  session.suppress = true;
  try {
    flush(session, () => {
      session.reconciler.updateContainer(null, session.root, null, null);
    });
  } catch (error) {
    console.error(error);
  }
  session.live = false;
  session.ops = [];
  session.orderBefore.clear();
}

async function mountFresh(element: unknown): Promise<void> {
  if (current) retire(current);
  const factory = await loadReconciler();
  const session: Session = {
    live: true,
    suppress: false,
    nextId: 1,
    nodes: new Map(),
    ops: [],
    orderBefore: new Map(),
    reconciler: null as unknown as Reconciler,
    root: null,
  };
  session.reconciler = factory(makeHost(session));
  const container = blankNode("root");
  container.id = 0;
  container.attached = true;
  session.root = session.reconciler.createContainer(
    container,
    0,
    null,
    false,
    null,
    "applet",
    (error: unknown) => {
      console.error(error);
      Deno.exit(70);
    },
    (error: unknown) => {
      console.error(error);
    },
    (error: unknown) => {
      console.error(error);
    },
    null,
  );
  current = session;
  flush(session, () => {
    session.reconciler.updateContainer(element, session.root, null, null);
  });
}

function dispatchEvent(message: HostMessage): void {
  if (message.t !== "event" || !current) return;
  const id = message.id;
  if (typeof id !== "number") return;
  const node = current.nodes.get(id);
  if (!node) return;
  if (typeof message.name !== "string") return;
  const handler = node.handlers.get(message.name);
  if (!handler) return;
  if (
    message.name === "onChange" &&
    ["entry", "fader", "scale"].includes(node.type) &&
    typeof message.seq === "number" && Number.isFinite(message.seq)
  ) {
    if (node.appliedSeq !== undefined && message.seq < node.appliedSeq) return;
    node.appliedSeq = message.seq;
    node.confirmSeq = message.seq;
  }
  const args = Array.isArray(message.args) ? message.args : [];
  try {
    handler(...args);
  } catch (error) {
    console.error(error);
    Deno.exit(1);
  }
}

function apply(message: HostMessage): void {
  if (!current) return;
  if (message.t === "options") {
    optionsStore.set(message.options ?? {});
    return;
  }
  if (message.t === "placement") {
    placementStore.set(message.placement ?? null);
    return;
  }
  if (message.t === "popover") {
    openStore.set(message.open === true);
    return;
  }
  if (message.t === "event") dispatchEvent(message);
}

export async function session(element: unknown, io: Io): Promise<void> {
  redirectConsole();
  installProcess();
  await loadReconciler();
  setEmit((message) => io.writeLine(JSON.stringify(message)));
  io.writeLine(JSON.stringify({ t: "hello", v: 1 }));
  for await (const line of io.lines) {
    let message: HostMessage;
    try {
      message = JSON.parse(line) as HostMessage;
    } catch (error) {
      console.error(error);
      continue;
    }
    if (
      !message || typeof message !== "object" || typeof message.t !== "string"
    ) continue;
    if (message.t === "hello") {
      if (current) retire(current);
      current = null;
      optionsStore.clear();
      placementStore.clear();
      nameStore.clear();
      openStore.clear();
      optionsStore.set(message.options ?? {});
      placementStore.set(message.placement ?? null);
      nameStore.set(typeof message.name === "string" ? message.name : "");
      openStore.set(false);
      await mountFresh(element);
      continue;
    }
    if (!current) continue;
    const sessionNow = current;
    flush(sessionNow, () => apply(message));
  }
}

export async function* stdinLines(): AsyncIterable<string> {
  const reader = Deno.stdin.readable.pipeThrough(new TextDecoderStream())
    .getReader();
  let buffer = "";
  try {
    while (true) {
      const { value, done } = await reader.read();
      if (done) {
        if (buffer.trim() !== "") yield buffer.replace(/\r$/, "");
        return;
      }
      buffer += value;
      let index = buffer.indexOf("\n");
      while (index >= 0) {
        let line = buffer.slice(0, index);
        buffer = buffer.slice(index + 1);
        if (line.endsWith("\r")) line = line.slice(0, -1);
        if (line.trim() !== "") yield line;
        index = buffer.indexOf("\n");
      }
    }
  } finally {
    reader.releaseLock();
  }
}

export function standardIo(): Io {
  return {
    writeLine(line: string) {
      Deno.stdout.writeSync(encoder.encode(`${line}\n`));
    },
    lines: stdinLines(),
  };
}
