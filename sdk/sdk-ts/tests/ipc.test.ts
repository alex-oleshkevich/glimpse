import test from "node:test";
import assert from "node:assert/strict";
import { createServer } from "node:net";
import { mkdtempSync, rmSync } from "node:fs";
import { tmpdir } from "node:os";
import { join } from "node:path";

import { Subscriber, IpcError, ipc } from "../src/index.js";

test("ipc() resolves the socket path; shell maps to ipc.sock", () => {
  const prev = process.env.GLIMPSE_IPC_DIR;
  const dir = mkdtempSync(join(tmpdir(), "glimpse-ipc-"));
  try {
    process.env.GLIMPSE_IPC_DIR = dir;
    assert.ok(ipc() instanceof Subscriber);
    assert.ok(ipc("wallpaper") instanceof Subscriber);
  } finally {
    if (prev === undefined) delete process.env.GLIMPSE_IPC_DIR;
    else process.env.GLIMPSE_IPC_DIR = prev;
    rmSync(dir, { recursive: true, force: true });
  }
});

test("dispatch and listen against a fake server", async () => {
  const dir = mkdtempSync(join(tmpdir(), "glimpse-ipc-"));
  const socketPath = join(dir, "ipc.sock");

  const server = createServer((conn) => {
    conn.write("hello version=test\n");
    let buf = "";
    conn.on("data", (chunk) => {
      buf += chunk.toString();
      const nl = buf.indexOf("\n");
      if (nl < 0) return;
      const line = buf.slice(0, nl);
      if (line.startsWith("subscribe ")) {
        assert.equal(line, "subscribe audio.*");
        conn.write("notification.received body=l1\\nl2\\sword ts=7\n");
      } else if (line.startsWith("fail")) {
        conn.write("ack ok=false error=nope\n");
      } else {
        assert.equal(line, "open_uri uri=https://example.com");
        conn.write("ack ok=true echo=done\n");
      }
    });
  });

  await new Promise<void>((resolve) => server.listen(socketPath, resolve));
  try {
    const sub = new Subscriber(socketPath);

    const ack = await sub.dispatch("open_uri", {
      uri: "https://example.com",
    });
    assert.deepEqual(ack, { echo: "done" });

    await assert.rejects(() => sub.dispatch("fail"), /nope/);

    for await (const ev of sub.listen("audio.*")) {
      assert.equal(ev.name, "notification.received");
      assert.equal(ev.ts, 7);
      assert.equal(ev.fields.body, "l1\nl2 word");
      break;
    }
  } finally {
    server.close();
    rmSync(dir, { recursive: true, force: true });
  }
});

test("connect failure rejects with IpcError", async () => {
  const sub = new Subscriber("/nonexistent/glimpse-missing.sock");
  await assert.rejects(() => sub.dispatch("noop"), IpcError);
});

test("dispatch rejects an unsafe action/key before connecting", async () => {
  const sub = new Subscriber("/nonexistent/glimpse-missing.sock");
  await assert.rejects(
    () => sub.dispatch("evil\nsubscribe *"),
    /whitespace/,
  );
  await assert.rejects(
    () => sub.dispatch("ok", { "bad key": "v" }),
    /whitespace/,
  );
});
