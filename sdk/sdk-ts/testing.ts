import { mkdtemp, rm } from "node:fs/promises";
import { tmpdir } from "node:os";
import { join } from "node:path";
import { createElement, type ReactNode, useEffect, useState } from "react";
import { restoreConsole, session } from "./reconciler.ts";

let active = false;

type Node = {
  id: number;
  type: string;
  props: Record<string, unknown>;
  children: Node[];
};

export type Rendered = {
  tree(): unknown;
  requests(): Array<Record<string, unknown>>;
  click(id: number): Promise<void>;
  event(id: number, name: string, args?: unknown[]): Promise<void>;
  setOptions(options: Record<string, unknown>): Promise<void>;
  setPopoverOpen(open: boolean): Promise<void>;
  setService(name: string, value: unknown): Promise<void>;
  close(): Promise<void>;
};

export async function render(
  element: ReactNode,
  host: {
    options?: Record<string, unknown>;
    placement?: Record<string, unknown>;
    state?: string;
  } = {},
): Promise<Rendered> {
  if (active) throw new Error("close the previous render before starting another");
  active = true;
  const owned = host.state === undefined
    ? await mkdtemp(join(tmpdir(), "glimpse-sdk-testing-")).catch((error) => {
      active = false;
      throw error;
    })
    : undefined;
  const release = async () => {
    active = false;
    if (owned) await rm(owned, { recursive: true, force: true });
  };
  let root: Node = { id: 0, type: "root", props: {}, children: [] };
  let nodes = new Map<number, Node>([[0, root]]);
  const sent: Array<Record<string, unknown>> = [];
  const queue: Array<{ line: string; done: (error?: Error) => void }> = [];
  let wake: (() => void) | undefined;
  let closed = false;
  let violation: Error | undefined;
  let unmount: (() => void) | undefined;
  let didMount = false;
  function Mounted() {
    const [mounted, setMounted] = useState(true);
    unmount = () => setMounted(false);
    useEffect(() => {
      didMount = true;
      return () => { didMount = false; };
    }, []);
    return mounted ? element : null;
  }
  const commit = (ops: Array<Record<string, any>>) => {
    const staged = structuredClone(root);
    const stagedNodes = new Map<number, Node>();
    const parents = new Map<number, Node>();
    const add = (node: Node, parent?: Node) => {
      if (stagedNodes.has(node.id)) throw new Error(`duplicate node ${node.id}`);
      stagedNodes.set(node.id, node);
      if (parent) parents.set(node.id, parent);
      for (const child of node.children) add(child, node);
    };
    const remove = (node: Node) => {
      stagedNodes.delete(node.id);
      parents.delete(node.id);
      for (const child of node.children) remove(child);
    };
    const insert = (parent: Node, node: Node, before: number | null) => {
      const index = before === null ? parent.children.length : parent.children.findIndex((child) => child.id === before);
      if (index < 0) throw new Error(`unknown before ${before} for parent ${parent.id}`);
      parent.children.splice(index, 0, node);
      parents.set(node.id, parent);
    };
    add(staged);
    for (const op of ops) {
      if (op.op === "insert") {
        const parent = stagedNodes.get(op.parent);
        if (!parent) throw new Error(`unknown parent ${op.parent}`);
        if (op.before !== null && !parent.children.some((child) => child.id === op.before)) {
          throw new Error(`unknown before ${op.before} for parent ${op.parent}`);
        }
        add(op.node, parent);
        insert(parent, op.node, op.before);
      } else if (op.op === "move") {
        const node = stagedNodes.get(op.id);
        if (!node || op.id === 0) throw new Error(`unknown node ${op.id}`);
        const parent = stagedNodes.get(op.parent);
        if (!parent) throw new Error(`unknown parent ${op.parent}`);
        if (op.before !== null && !parent.children.some((child) => child.id === op.before && child !== node)) {
          throw new Error(`unknown before ${op.before} for parent ${op.parent}`);
        }
        for (let ancestor: Node | undefined = parent; ancestor; ancestor = parents.get(ancestor.id)) {
          if (ancestor === node) throw new Error(`move would cycle node ${op.id}`);
        }
        const old = parents.get(op.id)!;
        old.children.splice(old.children.indexOf(node), 1);
        insert(parent, node, op.before);
      } else if (op.op === "remove") {
        const node = stagedNodes.get(op.id);
        if (!node || op.id === 0) throw new Error(`unknown node ${op.id}`);
        const parent = stagedNodes.get(op.parent);
        if (!parent) throw new Error(`unknown parent ${op.parent}`);
        if (parents.get(op.id) !== parent) throw new Error(`node ${op.id} is not a child of ${op.parent}`);
        parent.children.splice(parent.children.indexOf(node), 1);
        remove(node);
      } else if (op.op === "set") {
        const node = stagedNodes.get(op.id);
        if (!node || op.id === 0) throw new Error(`unknown node ${op.id}`);
        const props = node.props;
        for (const [key, value] of Object.entries(op.props)) {
          if (value === null) delete props[key];
          else props[key] = value;
        }
      } else {
        throw new Error(`unknown operation ${op.op}`);
      }
    }
    root = staged;
    nodes = stagedNodes;
  };
  const lines = (async function* () {
    while (true) {
      if (closed && violation) {
        for (const pending of queue.splice(0)) pending.done(violation);
        return;
      }
      if (!queue.length) {
        if (closed) return;
        await new Promise<void>((resolve) => { wake = resolve; });
      }
      const next = queue.shift();
      if (!next) continue;
      yield next.line;
      next.done(violation);
    }
  })();
  const running = session(createElement(Mounted), {
    lines,
    writeLine(line) {
      const message = JSON.parse(line) as Record<string, any>;
      if (message.t === "commit") {
        try {
          commit(message.ops);
        } catch (error) {
          violation = error as Error;
          closed = true;
        }
      }
      else if (message.t !== "hello") sent.push(message);
    },
  }, (code) => {
    violation ??= new Error(`applet failed with exit code ${code}`);
    if (didMount) unmount?.();
    restoreConsole();
    closed = true;
    wake?.();
    wake = undefined;
  });
  const send = (message: Record<string, unknown>) => {
    if (closed) return Promise.reject(new Error("rendered session is closed"));
    return new Promise<void>((resolve, reject) => {
      queue.push({ line: JSON.stringify(message), done: (error) => error ? reject(error) : resolve() });
      wake?.();
      wake = undefined;
    });
  };
  try {
    await send({
      t: "hello", v: 1, name: "", state: host.state ?? owned,
      options: host.options ?? {},
      placement: host.placement ?? { output: null, position: "top", orientation: "horizontal", zone: "right", size: 32 },
    });
  } catch (error) {
    closed = true;
    wake?.();
    try {
      await running;
    } finally {
      restoreConsole();
      await release();
    }
    throw error;
  }

  return {
    tree: () => structuredClone(root),
    requests: () => structuredClone(sent),
    click(id) {
      if (closed) return Promise.reject(new Error("rendered session is closed"));
      const node = nodes.get(id);
      const handler = node?.type === "actionrow" ? "onActivate" : node?.type === "button" ? "onClick" : undefined;
      if (!node || !handler || node.props[handler] !== true) {
        return Promise.reject(new Error(`node ${id} has no ${handler ?? "click"} handler`));
      }
      return send({ t: "event", id, name: handler, args: [], seq: null });
    },
    event(id, name, args = []) {
      if (closed) return Promise.reject(new Error("rendered session is closed"));
      const node = nodes.get(id);
      if (!node || !name.startsWith("on") || node.props[name] !== true) {
        return Promise.reject(new Error(`node ${id} has no ${name} handler`));
      }
      return send({ t: "event", id, name, args, seq: null });
    },
    setOptions: (options) => send({ t: "options", options }),
    setPopoverOpen: (open) => send({ t: "popover", open }),
    setService: (name, value) => send({ t: "service", name, value }),
    async close() {
      try {
        if (unmount) {
          unmount();
          if (!closed) await send({ t: "popover", open: false });
        }
      } finally {
        closed = true;
        wake?.();
        try {
          await running;
        } finally {
          restoreConsole();
          await release();
        }
      }
    },
  };
}
