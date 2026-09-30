import assert from "node:assert/strict";
import { existsSync, statSync } from "node:fs";
import { isAbsolute } from "node:path";
import test from "node:test";
import { createElement, useEffect, useState } from "react";
import { closePopover, copy, notify, openUri, sessionAction, useOptions, usePlacement, usePopoverOpen, useStateDir } from "./index.ts";
import { render } from "./testing.ts";

test("testing host renders, dispatches, updates, and closes", async () => {
  function App() {
    const [clicked, setClicked] = useState(false);
    const options = useOptions<{ title?: string }>();
    const open = usePopoverOpen();
    return createElement("popover", null,
      createElement("row", {
        title: clicked ? "Clicked" : options.title,
        onActivate: () => {
          setClicked(true);
          copy("copied");
          notify({ summary: "done" });
          openUri("https://example.com");
          sessionAction("lock");
          closePopover();
        },
      }),
      createElement("button", { text: open ? "Open" : "Closed", onClick: () => copy("button") }),
      createElement("row", { title: options.title, key: "inert" }),
    );
  }

  const view = await render(createElement(App), { options: { title: "Initial" } });
  const children = () => (view.tree() as { children: Array<{ children: Array<{ id: number; props: Record<string, unknown> }> }> }).children[0].children;
  const [row, button, inert] = children();
  assert.equal(row.props.title, "Initial");
  await assert.rejects(view.click(inert.id), new RegExp(`node ${inert.id}`));
  const popover = (view.tree() as { children: Array<{ id: number }> }).children[0];
  await assert.rejects(view.click(popover.id), new RegExp(`node ${popover.id}`));
  await view.click(row.id);
  assert.equal(children()[0].props.title, "Clicked");
  assert.deepEqual(view.requests(), [
    { t: "copy", text: "copied" },
    { t: "notify", summary: "done" },
    { t: "open-uri", uri: "https://example.com" },
    { t: "session", action: "lock" },
    { t: "close-popover" },
  ]);
  await view.setOptions({ title: "Changed" });
  assert.equal(children()[0].props.title, "Clicked");
  assert.equal(children()[2].props.title, "Changed");
  await view.setPopoverOpen(true);
  assert.equal(children()[1].props.text, "Open");
  await view.click(button.id);
  assert.deepEqual(view.requests().at(-1), { t: "copy", text: "button" });
  await view.close();
  await assert.rejects(view.click(row.id), /closed/);
});

test("testing host applies set, move, and remove commits", async () => {
  function App() {
    const options = useOptions<{ step?: number }>();
    const step = options.step ?? 0;
    return createElement("popover", null,
      ...(step === 0
        ? [createElement("row", { key: "a", title: "A" }), createElement("row", { key: "b", title: "B" })]
        : step === 1
        ? [createElement("row", { key: "b", title: "B2" }), createElement("row", { key: "a", title: "A" })]
        : [createElement("row", { key: "b", title: "B2" })]),
    );
  }
  const view = await render(createElement(App));
  const titles = () => (view.tree() as { children: Array<{ children: Array<{ props: { title: string } }> }> }).children[0].children.map((node) => node.props.title);
  assert.deepEqual(titles(), ["A", "B"]);
  await view.setOptions({ step: 1 });
  assert.deepEqual(titles(), ["B2", "A"]);
  await view.setOptions({ step: 2 });
  assert.deepEqual(titles(), ["B2"]);
  await view.close();
});

test("only one render can own the reconciler session", async () => {
  const first = await render(createElement("popover"));
  await assert.rejects(render(createElement("popover")), /close the previous render/);
  await first.close();
  const second = await render(createElement("popover"));
  await second.close();
});

test("closing a render stops its requests before the next render", async () => {
  function App() {
    useEffect(() => {
      const timer = setInterval(() => copy("first"), 5);
      return () => clearInterval(timer);
    }, []);
    return createElement("popover");
  }
  const first = await render(createElement(App));
  await new Promise((resolve) => setTimeout(resolve, 30));
  assert.ok(first.requests().length > 0);
  await first.close();
  const second = await render(createElement("popover"));
  await new Promise((resolve) => setTimeout(resolve, 30));
  assert.deepEqual(second.requests(), []);
  await second.close();
});

test("a failed interval is unmounted before the next render", async () => {
  function Broken() {
    const [ticks, setTicks] = useState(0);
    useEffect(() => {
      const timer = setInterval(() => {
        copy("first");
        setTicks((value) => value + 1);
      }, 5);
      return () => clearInterval(timer);
    }, []);
    if (ticks > 0) throw new Error("failed interval");
    return createElement("popover");
  }
  const log = console.log;
  const first = await render(createElement(Broken));
  await new Promise((resolve) => setTimeout(resolve, 40));
  assert.ok(first.requests().length > 0);
  await assert.rejects(first.setOptions({}), /closed/);
  assert.equal(console.log, log);
  await first.close();
  const second = await render(createElement("popover"));
  await new Promise((resolve) => setTimeout(resolve, 30));
  assert.deepEqual(second.requests(), []);
  await second.close();
});

test("default hello supplies state and placement", async () => {
  function App() {
    const state = useStateDir();
    const placement = usePlacement();
    return createElement("indicator", { text: `${state}|${placement?.zone}|${placement?.position}` });
  }
  const view = await render(createElement(App));
  const text = (view.tree() as { children: Array<{ props: { text: string } }> }).children[0].props.text;
  const [state, zone, position] = text.split("|");
  assert.ok(isAbsolute(state));
  assert.equal(statSync(state).mode & 0o777, 0o700);
  assert.equal(zone, "right");
  assert.equal(position, "top");
  await view.close();
  assert.equal(existsSync(state), false);
});

test("a failing applet rejects instead of exiting and frees the host", async () => {
  function Broken(): never {
    throw new Error("broken render");
  }
  await assert.rejects(render(createElement(Broken)), /exit code 70/);
  function Clicker() {
    return createElement("button", { label: "Go", onClick: () => { throw new Error("broken handler"); } });
  }
  const view = await render(createElement(Clicker));
  const id = (view.tree() as { children: Array<{ id: number }> }).children[0].id;
  await assert.rejects(view.click(id), /exit code 1/);
  await view.close();
});

test("invalid commit rejects its helper and preserves the last good tree", async () => {
  function App() {
    const options = useOptions<{ add?: boolean }>();
    return createElement("popover", null,
      options.add && createElement("row", { key: "new", title: "New" }),
      createElement("row", { key: "old", title: "Old" }),
    );
  }
  const view = await render(createElement(App));
  const before = view.tree();
  const stringify = JSON.stringify;
  let corrupted = false;
  JSON.stringify = ((value: unknown) => {
    const wire = value as { t?: string; ops?: Array<Record<string, unknown>> };
    if (wire?.t !== "commit") return stringify(value);
    const copy = structuredClone(wire);
    if (copy.ops?.length) {
      copy.ops.push({
        op: "insert", parent: 0, before: 99999,
        node: { id: 99998, type: "row", props: { title: "Bad" }, children: [] },
      });
      corrupted = true;
    }
    return stringify(copy);
  }) as typeof JSON.stringify;
  try {
    const changed = view.setOptions({ add: true });
    const queued = view.setPopoverOpen(true);
    await Promise.all([
      assert.rejects(changed, /unknown before 99999/),
      assert.rejects(queued, /unknown before 99999/),
    ]);
  } finally {
    JSON.stringify = stringify;
  }
  assert.ok(corrupted);
  assert.deepEqual(view.tree(), before);
  await view.close();
});
