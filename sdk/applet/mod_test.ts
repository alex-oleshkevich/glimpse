import {
  redirectConsole,
  restoreConsole,
  session,
  setEmit,
} from "./reconciler.ts";
import {
  Box,
  Button,
  closePopover,
  copy,
  createElement,
  Entry,
  Fragment,
  Hero,
  Image,
  Indicator,
  Label,
  notify,
  openUri,
  Popover,
  Row,
  run,
  Section,
  session as hostSession,
  useOptions,
  useState,
} from "./mod.ts";

function assert(condition: unknown, message: string): asserts condition {
  if (!condition) throw new Error(message);
}

function fixture(name: string): URL {
  return new URL(`./fixtures/${name}`, import.meta.url);
}

function assertBytes(actual: string[], path: URL): void {
  const expected = Deno.readTextFileSync(path);
  const got = `${actual.join("\n")}\n`;
  if (got === expected) return;
  const expectedLines = expected.split("\n");
  const actualLines = got.split("\n");
  for (
    let index = 0;
    index < Math.max(expectedLines.length, actualLines.length);
    index++
  ) {
    if (expectedLines[index] !== actualLines[index]) {
      throw new Error(
        `${path.pathname} line ${index + 1}\nexpected: ${
          expectedLines[index]
        }\nactual:   ${actualLines[index]}\n--- actual ---\n${got}`,
      );
    }
  }
}

async function drive(element: unknown, messages: unknown[]): Promise<string[]> {
  const lines: string[] = [];
  await session(element, {
    writeLine: (line) => lines.push(line),
    lines: (async function* () {
      for (const message of messages) {
        yield typeof message === "string" ? message : JSON.stringify(message);
      }
    })(),
  });
  return lines;
}

function hostLine(): string {
  const text = Deno.readTextFileSync(fixture("host.ndjson"));
  const line = text.split("\n")[0];
  assert(line, "host.ndjson has a hello");
  return line;
}

function HelloApp() {
  return createElement(
    Fragment,
    null,
    createElement(
      Indicator,
      { icon: "view-list-symbolic", tooltip: "Todos" },
      "2",
    ),
    createElement(
      Popover,
      null,
      createElement(Hero, {
        icon: "view-list-symbolic",
        title: "Todos",
        subtitle: "2 open",
      }),
      createElement(
        Section,
        { title: "Open", count: "2" },
        createElement(Row, { title: "Buy milk" }),
        createElement(Row, { title: "Ship applets", onActivate: () => {} }),
      ),
    ),
  );
}

function rowProps(key: string, step: number): Record<string, unknown> {
  const props: Record<string, unknown> = { key };
  if (key === "a" && step === 7) props.title = "Sequenced";
  else if (key === "a" && step === 6) props.title = "Updated";
  else if (!(key === "a" && step >= 8)) props.title = key;
  if (key === "a" && step >= 6) props.busy = "nope";
  return props;
}

function OpsApp() {
  const options = useOptions<{ step?: number }>();
  const step = options.step ?? 0;
  const keys = step < 1
    ? []
    : step < 2
    ? ["a", "b", "c"]
    : step < 3
    ? ["c", "a", "b"]
    : ["c", "a", "d", "b"];
  const children: unknown[] = [];
  if (step >= 1) {
    children.push(
      createElement(
        Section,
        { title: "Open" },
        ...keys.map((key) => createElement(Row, rowProps(key, step))),
      ),
    );
  }
  if (step === 4) {
    children.push(
      createElement(
        Box,
        null,
        createElement(Label, null, "one"),
        createElement(Label, null, "two"),
      ),
    );
  }
  return createElement(Popover, null, ...children);
}

function EntryApp() {
  const [value, setValue] = useState("x");
  return createElement(
    Popover,
    null,
    createElement(Entry, {
      value,
      onChange: (next: string) => setValue(next),
    }),
  );
}

function EntryWithSibling() {
  const [value, setValue] = useState("x");
  const [text, setText] = useState("old");
  return createElement(
    Popover,
    null,
    createElement(Label, { text }),
    createElement(Entry, {
      value,
      onChange: (next: string) => {
        setText("new");
        setValue(next);
      },
    }),
  );
}

function Loud() {
  console.log("from-log");
  console.info("from-info");
  console.debug("from-debug");
  return createElement(Indicator, { icon: "view-list-symbolic" });
}

function Boom(): never {
  throw new Error("render boom");
}

function mockExit(
  throwOnExit: boolean,
): { code: () => number | null; restore: () => void } {
  const original = Deno.exit;
  let status: number | null = null;
  Deno.exit = ((code?: number) => {
    status = code ?? 0;
    if (throwOnExit) throw new Error(`deno-exit:${status}`);
  }) as typeof Deno.exit;
  return {
    code: () => status,
    restore: () => {
      Deno.exit = original;
    },
  };
}

Deno.test("applet sdk", async (test) => {
  await test.step("hello fixture matches hello.ndjson", async () => {
    const lines = await drive(createElement(HelloApp), [hostLine()]);
    assertBytes(lines, fixture("hello.ndjson"));
  });

  await test.step("icon tooltips reach the wire", async () => {
    const lines = await drive(
      createElement(
        Popover,
        null,
        createElement(
          Box,
          null,
          createElement(Image, {
            icon: "dialog-warning-symbolic",
            tooltip: "Warning",
          }),
          createElement(Button, {
            icon: "media-playback-start-symbolic",
            tooltip: "Play",
            onClick: () => {},
          }),
        ),
      ),
      [hostLine()],
    );
    assertBytes(lines, fixture("tooltip.ndjson"));
  });

  await test.step("ops fixture matches ops.ndjson", async () => {
    const messages: unknown[] = [{
      t: "hello",
      v: 1,
      name: "ops",
      options: { step: 0 },
      placement: {
        output: null,
        position: "bottom",
        orientation: "horizontal",
        zone: "center",
        size: 48,
      },
    }];
    for (const step of [1, 2, 3, 4, 5, 6]) {
      messages.push({ t: "options", options: { step } });
    }
    messages.push({
      t: "event",
      id: 3,
      name: "onActivate",
      args: [],
      seq: null,
    });
    messages.push({ t: "options", options: { step: 7 } });
    messages.push({ t: "options", options: { step: 8 } });
    const lines = await drive(createElement(OpsApp), messages);
    assert(lines[0] === '{"t":"hello","v":1}', `first line ${lines[0]}`);
    assertBytes(lines.slice(1), fixture("ops.ndjson"));
  });

  await test.step("requests fixture matches requests.ndjson", () => {
    const lines: string[] = [];
    setEmit((message) => lines.push(JSON.stringify(message)));
    notify({
      summary: "Timer",
      body: "The tomato is done",
      icon: "alarm-symbolic",
      urgency: "critical",
    });
    notify({ summary: "Ping", urgency: "low" });
    notify({ summary: "Saved" });
    copy("buy milk");
    openUri("https://example.com/todos");
    for (
      const action of [
        "lock",
        "suspend",
        "hibernate",
        "log-out",
        "reboot",
        "power-off",
      ] as const
    ) {
      hostSession(action);
    }
    closePopover();
    assertBytes(lines, fixture("requests.ndjson"));
  });

  await test.step("console output goes to stderr", async () => {
    const protocol: string[] = [];
    const errors: Uint8Array[] = [];
    const original = Deno.stderr.writeSync;
    Deno.stderr.writeSync = (data: Uint8Array) => {
      errors.push(data);
      return data.byteLength;
    };
    try {
      redirectConsole();
      const lines = await drive(createElement(Loud), [hostLine()]);
      protocol.push(...lines);
    } finally {
      Deno.stderr.writeSync = original;
      restoreConsole();
    }
    const stderr = new TextDecoder().decode(
      errors.reduce((all, chunk) => {
        const next = new Uint8Array(all.length + chunk.length);
        next.set(all);
        next.set(chunk, all.length);
        return next;
      }, new Uint8Array()),
    );
    const joined = protocol.join("\n");
    assert(!joined.includes("from-log"), "console.log reached the protocol");
    assert(!joined.includes("from-info"), "console.info reached the protocol");
    assert(
      !joined.includes("from-debug"),
      "console.debug reached the protocol",
    );
    assert(stderr.includes("from-log"), `stderr missed log: ${stderr}`);
    assert(stderr.includes("from-info"), `stderr missed info: ${stderr}`);
    assert(stderr.includes("from-debug"), `stderr missed debug: ${stderr}`);
  });

  await test.step("mounts only after the host hello", async () => {
    const lines: string[] = [];
    const exit = mockExit(true);
    try {
      await run(createElement(Indicator, { icon: "view-list-symbolic" }), {
        writeLine: (line) => lines.push(line),
        lines: (async function* () {})(),
      });
      throw new Error("run returned without exiting");
    } catch (error) {
      assert(
        error instanceof Error && error.message === "deno-exit:0",
        String(error),
      );
    } finally {
      exit.restore();
    }
    assert(exit.code() === 0, `exit ${exit.code()}`);
    assert(
      lines.length === 1 && lines[0] === '{"t":"hello","v":1}',
      lines.join("\n"),
    );
  });

  await test.step("a second host hello remounts from id 1", async () => {
    const hello = hostLine();
    const lines = await drive(createElement(HelloApp), [hello, hello]);
    const hellos = lines.filter((line) => line === '{"t":"hello","v":1}');
    assert(hellos.length === 1, `hello lines ${hellos.length}`);
    const commits = lines.filter((line) => line.startsWith('{"t":"commit"'));
    assert(
      commits.length === 2,
      `commits ${commits.length}: ${lines.join("\n")}`,
    );
    assert(
      commits[0] === commits[1],
      `remount differed\n${commits[0]}\n${commits[1]}`,
    );
    assert(commits[0].includes('"id":1,"type":"indicator"'), commits[0]);
  });

  await test.step("an older seq does not overwrite the entry", async () => {
    const lines = await drive(createElement(EntryApp), [
      hostLine(),
      { t: "event", id: 2, name: "onChange", args: ["ab"], seq: 5 },
      { t: "event", id: 2, name: "onChange", args: ["a"], seq: 2 },
      { t: "event", id: 2, name: "onChange", args: ["abc"], seq: 6 },
    ]);
    const sets = lines.filter((line) => line.includes('"op":"set"'));
    assert(sets.length === 2, `sets ${sets.length}\n${lines.join("\n")}`);
    assert(
      sets[0].includes('"value":"ab"') && sets[0].includes('"seq":5'),
      sets[0],
    );
    assert(!sets[0].includes('"a"') || sets[0].includes('"ab"'), sets[0]);
    assert(
      !lines.some((line) => line.includes('"value":"a"')),
      lines.join("\n"),
    );
    assert(
      sets[1].includes('"value":"abc"') && sets[1].includes('"seq":6'),
      sets[1],
    );
  });

  await test.step("entry seq stays with its value set", async () => {
    const lines = await drive(createElement(EntryWithSibling), [
      hostLine(),
      { t: "event", id: 3, name: "onChange", args: ["new"], seq: 9 },
    ]);
    const sets = lines.flatMap((line) => {
      const message = JSON.parse(line) as {
        t: string;
        ops?: { op: string; id: number; seq?: number }[];
      };
      return message.t === "commit"
        ? (message.ops ?? []).filter((op) => op.op === "set")
        : [];
    });
    assert(
      sets.some((op) => op.id === 2 && op.seq === undefined),
      JSON.stringify(sets),
    );
    assert(
      sets.some((op) => op.id === 3 && op.seq === 9),
      JSON.stringify(sets),
    );
  });

  await test.step("invalid parent is rejected before a commit", async () => {
    const exit = mockExit(false);
    let lines: string[] = [];
    try {
      lines = await drive(
        createElement(Popover, null, createElement(Indicator, { icon: "x" })),
        [hostLine()],
      );
    } finally {
      exit.restore();
    }
    assert(exit.code() === 70, `exit ${exit.code()}`);
    assert(
      !lines.some((line) => line.includes('"t":"commit"')),
      lines.join("\n"),
    );
  });

  await test.step("a render throw exits 70", async () => {
    const exit = mockExit(false);
    try {
      await drive(createElement(Boom), [hostLine()]);
      await new Promise((resolve) => setTimeout(resolve, 0));
    } finally {
      exit.restore();
    }
    assert(exit.code() === 70, `exit ${exit.code()}`);
  });
});
