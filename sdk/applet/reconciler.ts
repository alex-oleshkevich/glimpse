import fs from "node:fs";
import { createInterface } from "node:readline";
import type { Readable } from "node:stream";
import { inspect } from "node:util";
import Reconciler from "react-reconciler";
import { DefaultEventPriority } from "react-reconciler/constants.js";

export type Placement = {
  output: string | null;
  position: string;
  orientation: string;
  zone: string;
  size: number;
};

export type Io = {
  writeLine: (line: string) => void;
  lines: AsyncIterable<string | string[]>;
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
  nextId: number;
  nodes: Map<number, HostNode>;
  ops: Op[];
  pending: Op[];
  container: HostNode;
  reconciler: Reconciler;
  root: unknown;
};

type Store<T> = {
  get: () => T;
  set: (value: T) => void;
  subscribe: (listener: () => void) => () => void;
};

type HostMessage =
  | {
    t: "hello";
    v?: number;
    name?: string;
    state?: string;
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

const TEXT = new Set(["label", "indicator", "button"]);
const SKIP = new Set(["children", "key", "ref"]);
const encoder = new TextEncoder();

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
  };
}

export const optionsStore = createStore<Record<string, unknown>>({});
export const placementStore = createStore<Placement | null>(null);
export const nameStore = createStore("");
export const stateStore = createStore("");
export const openStore = createStore(false);

class ProtocolLimitError extends Error {
  override name = "ProtocolLimitError";
}

function writeAll(fd: number, text: string): void {
  const bytes = encoder.encode(text);
  const pause = new Int32Array(new SharedArrayBuffer(4));
  for (let offset = 0; offset < bytes.length;) {
    try {
      const written = fs.writeSync(fd, bytes, offset, bytes.length - offset);
      if (written === 0) throw new Error("zero-byte write");
      offset += written;
    } catch (error) {
      if ((error as NodeJS.ErrnoException).code !== "EAGAIN") throw error;
      Atomics.wait(pause, 0, 0, 2);
    }
  }
}

function lineOf(message: unknown): string {
  const line = JSON.stringify(message);
  if (encoder.encode(line).byteLength > 1_048_576) {
    throw new ProtocolLimitError("outgoing line exceeds 1 MiB");
  }
  return line;
}

let emit = (message: unknown) => writeAll(1, `${lineOf(message)}\n`);
let helloWritten = false;
const queued: unknown[] = [];

export function setEmit(write: (message: unknown) => void): void {
  emit = write;
}

export function send(message: unknown): void {
  if (helloWritten) emit(message);
  else queued.push(message);
}

const consoleMethods = [
  "log",
  "info",
  "debug",
  "dir",
  "dirxml",
  "table",
  "count",
  "countReset",
  "time",
  "timeLog",
  "timeEnd",
  "group",
  "groupCollapsed",
  "groupEnd",
  "trace",
] as const;
const originalConsole = new Map<string, (...args: unknown[]) => void>();

function writeStderr(...args: unknown[]): void {
  const text = args.map((value) => {
    if (typeof value === "string") return value;
    return inspect(value);
  }).join(" ");
  writeAll(2, `${text}\n`);
}

export function redirectConsole(): void {
  const target = console as unknown as Record<
    string,
    (...args: unknown[]) => void
  >;
  if (originalConsole.size) return;
  for (const method of consoleMethods) {
    originalConsole.set(method, target[method]);
    target[method] = writeStderr;
  }
}

export function restoreConsole(): void {
  const target = console as unknown as Record<
    string,
    (...args: unknown[]) => void
  >;
  for (const [method, original] of originalConsole) target[method] = original;
  originalConsole.clear();
}

const reconcilerFactory = Reconciler as unknown as (config: object) => Reconciler;
const eventPriority = DefaultEventPriority ?? 32;

async function loadReconciler(): Promise<(config: object) => Reconciler> {
  return reconcilerFactory;
}

function textOf(children: unknown): string | undefined {
  const values = [children].flat(Infinity).filter((child) =>
    child !== null && child !== undefined && typeof child !== "boolean"
  );
  return values.length &&
      values.every((child) =>
        typeof child === "string" || typeof child === "number"
      )
    ? values.join("")
    : undefined;
}

function capped(value: string, limit: number): string {
  return [...value].slice(0, limit).join("");
}

function wireProps(type: string, props: Props): Props {
  const out: Props = {};
  const text = TEXT.has(type) ? textOf(props.children) : undefined;
  for (const key of Object.keys(props)) {
    if (SKIP.has(key)) continue;
    if (key === "text") {
      if (text !== undefined) out.text = capped(text, 256);
      else if (
        typeof props.text === "string" || typeof props.text === "number"
      ) {
        out.text = capped(String(props.text), 256);
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
    out[key] = typeof value === "string"
      ? capped(
        value,
        key === "badge" ? 8 : type === "entry" && key === "value" ? 4096 : 256,
      )
      : value;
  }
  if (text !== undefined && !Object.hasOwn(out, "text")) {
    out.text = capped(text, 256);
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
  for (const key of [...Object.keys(after), ...Object.keys(before)]) {
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

function place(
  session: Session,
  parent: HostNode,
  child: HostNode,
  before: HostNode | null,
): void {
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
  const index = parent.children.indexOf(child);
  if (index >= 0) parent.children.splice(index, 1);
  if (child.attached && child.id !== 0) {
    session.ops.push({ op: "remove", parent: parent.id, id: child.id });
  }
  forget(session, child);
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
      const node = blankNode("label");
      node.props = { text: capped(text, 256) };
      return node;
    },
    appendInitialChild: (parent: HostNode, child: HostNode) => {
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
      bindHandlers(node, next);
      const diff = diffProps(node.type, prev, next);
      if (!diff) return;
      const op: { [key: string]: Json } = {
        op: "set",
        id: node.id,
        props: diff as Json,
      };
      const controlled = CONTROLLED[node.type]?.[1];
      if (
        node.confirmSeq !== undefined && controlled &&
        Object.hasOwn(diff, controlled)
      ) {
        op.seq = node.confirmSeq;
        node.confirmSeq = undefined;
      }
      node.props = wireProps(node.type, next);
      session.ops.push(op);
    },
    finalizeInitialChildren: () => false,
    prepareForCommit: () => null,
    resetAfterCommit() {
      session.pending.push(...session.ops);
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
    commitMount: () => {},
    commitTextUpdate(node: HostNode, _previous: string, next: string) {
      const text = capped(next, 256);
      if (node.props.text === text) return;
      node.props.text = text;
      session.ops.push({ op: "set", id: node.id, props: { text } });
    },
    resetTextContent: () => {},
    hideInstance: () => {},
    hideTextInstance: () => {},
    unhideInstance: () => {},
    unhideTextInstance: () => {},
  };
}

const CONTROLLED: Record<string, [string, string]> = {
  entry: ["onChange", "value"],
  scale: ["onChange", "value"],
  switch: ["onToggle", "active"],
  togglerow: ["onToggle", "active"],
  togglebutton: ["onToggle", "active"],
  checkbutton: ["onToggle", "active"],
  expanderrow: ["onToggle", "open"],
  sliderrow: ["onChange", "value"],
  dropdown: ["onSelect", "selected"],
  choicelist: ["onSelect", "selected"],
};

let current: Session | null = null;
let fail: (code: number) => void = (code) => process.exit(code);

const SERIES_KEYS = ["values", "tooltips"];

function oversized(value: unknown, limit = 64): boolean {
  if (Array.isArray(value)) {
    return value.length > limit || value.some((item) => oversized(item));
  }
  if (value !== null && typeof value === "object") {
    return Object.values(value).some((field) => oversized(field));
  }
  return false;
}

function propsOversized(props: Record<string, unknown>): boolean {
  return Object.entries(props).some(([key, value]) =>
    oversized(value, SERIES_KEYS.includes(key) ? 366 : 64)
  );
}

function flush(session: Session, fn?: () => void, emitCommit = true): void {
  session.reconciler.flushSyncFromReconciler(fn);
  session.reconciler["flushPassiveEffects"]?.();
  for (const node of session.nodes.values()) {
    const controlled = CONTROLLED[node.type]?.[1];
    if (node.confirmSeq === undefined || !controlled) continue;
    session.pending.push({
      op: "set",
      id: node.id,
      props: { [controlled]: (node.props[controlled] ?? null) as Json },
      seq: node.confirmSeq,
    });
    node.confirmSeq = undefined;
  }
  let count = 0;
  const visit = (node: HostNode): void => {
    if (++count > 300) throw new ProtocolLimitError("tree exceeds 300 nodes");
    if (propsOversized(node.props)) {
      throw new ProtocolLimitError("array prop exceeds 64 items");
    }
    if (node.children.length > 200) {
      throw new ProtocolLimitError("parent exceeds 200 children");
    }
    for (const child of node.children) visit(child);
  };
  visit(session.container);
  if (session.pending.length > 1200) {
    throw new ProtocolLimitError("commit exceeds 1200 operations");
  }
  if (emitCommit && session.pending.length) {
    send({ t: "commit", ops: session.pending });
    session.pending = [];
  }
}

async function mountFresh(element: unknown): Promise<void> {
  const factory = await loadReconciler();
  const container = blankNode("root");
  container.id = 0;
  container.attached = true;
  const session: Session = {
    nextId: 1,
    nodes: new Map(),
    ops: [],
    pending: [],
    container,
    reconciler: null as unknown as Reconciler,
    root: null,
  };
  session.reconciler = factory(makeHost(session));
  session.root = session.reconciler.createContainer(
    container,
    0,
    null,
    false,
    null,
    "applet",
    (error: unknown) => {
      console.error(error);
      fail(70);
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
  }, false);
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
    CONTROLLED[node.type]?.[0] === message.name &&
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
    fail(1);
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

export async function session(
  element: unknown,
  io: Io,
  onFail: (code: number) => void = (code) => process.exit(code),
): Promise<void> {
  fail = onFail;
  redirectConsole();
  await loadReconciler();
  current = null;
  helloWritten = false;
  setEmit((message) => io.writeLine(lineOf(message)));
  io.writeLine(lineOf({ t: "hello", v: 1 }));
  helloWritten = true;
  for (const message of queued.splice(0)) emit(message);
  for await (const chunk of io.lines) {
    try {
      const buffered: HostMessage[] = [];
      for (const line of Array.isArray(chunk) ? chunk : [chunk]) {
        let message: HostMessage;
        try {
          message = JSON.parse(line) as HostMessage;
        } catch (error) {
          console.error(error);
          continue;
        }
        if (
          !message || typeof message !== "object" ||
          typeof message.t !== "string"
        ) continue;
        if (message.t === "hello") {
          if (current) continue;
          if (message.v !== 1) {
            console.error(`unsupported host protocol version: ${message.v}`);
            fail(1);
            return;
          }
          optionsStore.set(message.options ?? {});
          placementStore.set(message.placement ?? null);
          nameStore.set(typeof message.name === "string" ? message.name : "");
          stateStore.set(typeof message.state === "string" ? message.state : "");
          openStore.set(false);
          await mountFresh(element);
          continue;
        }
        if (current) buffered.push(message);
      }
      if (current) {
        flush(current, () => {
          for (const message of buffered) apply(message);
        });
      }
    } catch (error) {
      if (!(error instanceof ProtocolLimitError)) throw error;
      console.error(error);
      fail(70);
      return;
    }
  }
}

export async function* stdinLines(input: Readable = process.stdin): AsyncIterable<string[]> {
  const reader = createInterface({ input, crlfDelay: Infinity });
  const pending: string[] = [];
  const batches: string[][] = [];
  let wake: (() => void) | undefined;
  let closed = false;
  reader.on("line", (line) => {
    if (line.trim() === "") return;
    pending.push(line);
    if (pending.length === 1) {
      setImmediate(() => {
        batches.push(pending.splice(0));
        wake?.();
        wake = undefined;
      });
    }
  });
  reader.on("close", () => {
    closed = true;
    wake?.();
    wake = undefined;
  });
  try {
    while (!closed || pending.length || batches.length) {
      if (!batches.length) await new Promise<void>((resolve) => { wake = resolve; });
      while (batches.length) yield batches.shift()!;
    }
  } finally {
    reader.close();
  }
}

export function standardIo(): Io {
  return {
    writeLine(line: string) {
      writeAll(1, `${line}\n`);
    },
    lines: stdinLines(),
  };
}
